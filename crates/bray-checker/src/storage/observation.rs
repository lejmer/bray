use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{StorageInvalidation, StoragePath, StoragePlan, StorageProjection};
use bray_symbols::{DeclaredStorageShape, NamedTypeSymbolId, TypeData, TypeId};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Observation {
    Mutable,
    Scalar,
    Address,
    Identity,
    Marker,
    Discriminant,
    Reference(TypeId),
}

#[derive(Debug)]
struct Shape {
    observations: Vec<(Vec<StorageProjection>, Observation)>,
    complete_value: bool,
    complete_references: bool,
}

impl Shape {
    fn is_reference_free(&self) -> bool {
        self.complete_references
            && !self
                .observations
                .iter()
                .any(|(_, observation)| matches!(observation, Observation::Reference(_)))
    }

    fn unknown() -> Self {
        Self {
            observations: vec![(Vec::new(), Observation::Mutable)],
            complete_value: false,
            complete_references: false,
        }
    }
}

fn known_shape<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
    role: bray_compiler_known::RepresentationRole,
    reference: bool,
    visiting: &mut BTreeSet<NamedTypeSymbolId>,
    complete_reference_types: &mut BTreeMap<TypeId, bool>,
) -> Result<Shape, CheckerQueryError<C::UpstreamError>> {
    use bray_compiler_known::RepresentationRole;

    if matches!(role, RepresentationRole::Unit | RepresentationRole::Never) {
        return Ok(Shape {
            observations: Vec::new(),
            complete_value: !reference,
            complete_references: true,
        });
    }

    let scalar = role.numeric_kind().is_some()
        || matches!(
            role,
            RepresentationRole::ScalarBool | RepresentationRole::ScalarChar
        );

    let observation = if scalar && !reference {
        Observation::Scalar
    } else if matches!(
        role,
        RepresentationRole::RawPointer
            | RepresentationRole::FunctionPointer
            | RepresentationRole::DevicePointer
    ) {
        Observation::Address
    } else if matches!(role, RepresentationRole::Future | RepresentationRole::Task) {
        // Runtime progress changes the protected frame, not the owned handle's identity.
        Observation::Identity
    } else {
        Observation::Mutable
    };

    let mut shape = Shape {
        observations: vec![(Vec::new(), observation)],
        complete_value: scalar && !reference,
        complete_references: scalar || matches!(observation, Observation::Address),
    };

    if matches!(
        role,
        RepresentationRole::Atomic | RepresentationRole::Uninit
    ) {
        let element = request
            .available_compiler_known_symbols()
            .unary_representation_argument(request.semantic_values(), role, ty)
            .expect("checked protected storage retains its payload type");

        shape.complete_references =
            paths(request, element, visiting, complete_reference_types)?.is_reference_free();

        // Protected storage exposes no ordinary payload value. A reference-free
        // slot retains its opaque snapshot while its inline bytes are unreachable.
        shape.complete_value =
            !reference && role == RepresentationRole::Uninit && shape.complete_references;
    }

    Ok(shape)
}

#[derive(Debug, Default)]
pub(crate) struct Footprint {
    pub(crate) atomic: Option<StorageInvalidation>,
    pub(crate) references: Vec<(StoragePath, TypeId)>,
    pub(crate) complete_reference_types: std::sync::Arc<BTreeMap<TypeId, bool>>,
    pub(crate) scalars: BTreeMap<bray_bound_tree::StorageIdentityId, Vec<StoragePath>>,
    pub(crate) borrowed_identities: BTreeMap<StoragePath, Vec<StoragePath>>,
    pub(crate) identities: BTreeMap<bray_bound_tree::StorageIdentityId, Vec<StoragePath>>,
    pub(crate) empty_markers: BTreeSet<StoragePath>,
    pub(crate) discriminants: BTreeMap<bray_bound_tree::StorageIdentityId, BTreeSet<StoragePath>>,
    shapes: BTreeMap<TypeId, Shape>,
}

impl Footprint {
    fn observe_accesses<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
    ) -> BTreeMap<bray_bound_tree::StorageIdentityId, BTreeSet<TypeId>> {
        self.atomic = Some(StorageInvalidation::Observed {
            scope: bray_bound_tree::StorageMutationScope::Unbounded,
            accesses: BTreeMap::new(),
            snapshots: BTreeMap::new(),
            referents: BTreeSet::new(),
            stable_descriptors: BTreeSet::new(),
            stable_values: BTreeSet::new(),
        });

        let mut static_types = BTreeMap::<_, BTreeSet<_>>::new();

        let entry_owners = request
            .trusted_contracts()
            .into_iter()
            .flat_map(|contracts| contracts.required_witnesses.iter().map(|(_, owner)| owner))
            .collect::<BTreeSet<_>>();

        for (access_id, access) in storage.access_entries() {
            if let bray_bound_tree::StorageAccessRoot::Storage(root) = access.root()
                && access.projections().is_empty()
                && !self.scalars.contains_key(&root)
                && storage
                    .identity(root)
                    .is_some_and(|identity| identity.is_invocation_owned(storage.kind()))
                && storage.storage_type(root).is_some_and(|ty| {
                    crate::representation::reference_kind(request, ty)
                        == Some(bray_symbols::DependencyReferenceKind::Address)
                })
                && crate::ExecutionPlace::storage_identity(storage, root)
                    .is_some_and(|owner| entry_owners.contains(&&owner))
            {
                // This owned slot is a protocol witness for the current entry.
                // Keeping its address value grants no unowned pointee permission.
                self.scalars.insert(root, vec![StoragePath::root(root)]);
            }

            let ty = request
                .semantic_values()
                .unborrowed_type(access.reached_type());

            if matches!(
                request.semantic_values().type_data(ty).as_ref(),
                TypeData::Nullable(_)
                    | TypeData::Named {
                        definition: NamedTypeSymbolId::Union(_),
                        ..
                    }
            ) && let Some(path) = storage.logical_path(access_id)
            {
                self.discriminants
                    .entry(path.root)
                    .or_insert_with(BTreeSet::new)
                    .insert(path.clone());
            }

            if let bray_bound_tree::StorageAccessRoot::Storage(root) = access.root()
                && access.projections().is_empty()
                && matches!(
                    storage.identity(root),
                    Some(bray_bound_tree::StorageIdentity::Static(_))
                )
            {
                static_types
                    .entry(root)
                    .or_default()
                    .insert(access.reached_type());
            }
        }

        static_types
    }

    fn prepare_stores<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
        memory: &bray_bound_tree::CheckedMemoryOperations,
        mut atomic: bool,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        let complete_reference_types = std::sync::Arc::get_mut(&mut self.complete_reference_types)
            .expect("a new footprint owns its type cache before publication");

        for plan in storage
            .access_plans()
            .iter()
            .filter(|plan| plan.purpose() == bray_bound_tree::StorageAccessPurpose::Assignment)
        {
            let ty = storage
                .access(plan.access())
                .expect("assignment retains its checked destination")
                .reached_type();

            cached_shape(request, ty, &mut self.shapes, complete_reference_types)?;
        }

        // Extend the existing closed-operation preparation. Each concrete pointee
        // shares the same cached shape used by ordinary assignment observations.
        for operation in memory.operations() {
            use bray_bound_tree::CheckedMemoryOperationKind as Operation;

            atomic |= matches!(
                operation.kind(),
                Operation::AtomicStore { .. }
                    | Operation::AtomicExchange { .. }
                    | Operation::AtomicCompareExchange { .. }
                    | Operation::AtomicFetch { .. }
            );

            let pointee = match operation.kind() {
                Operation::Read {
                    pointee,
                    kind: bray_bound_tree::MemoryReadKind::Move,
                }
                | Operation::VolatileRead {
                    pointee,
                    kind: bray_bound_tree::MemoryReadKind::Move,
                    ..
                }
                | Operation::Write { pointee }
                | Operation::Copy { pointee, .. }
                | Operation::VolatileWrite { pointee, .. } => *pointee,
                Operation::UninitMove { element }
                | Operation::UninitAssumeInitialized { element }
                | Operation::UninitWrite { element } => *element,
                _ => continue,
            };

            cached_shape(request, pointee, &mut self.shapes, complete_reference_types)?;
        }

        if !atomic {
            self.atomic = None;
        }

        Ok(())
    }

    /// Tag-free stores, with whether they also preserve unrelated identities.
    pub(crate) fn tag_free_store(&self, ty: TypeId) -> Option<bool> {
        let shape = self.shapes.get(&ty)?;

        (shape.is_reference_free()
            && !shape
                .observations
                .iter()
                .any(|(_, observation)| *observation == Observation::Discriminant))
        .then_some(
            shape.complete_value
                && shape.observations.iter().all(|(_, observation)| {
                    matches!(observation, Observation::Scalar | Observation::Address)
                }),
        )
    }

    /// A typed data transfer cannot move an empty ownership marker.
    pub(crate) fn marker_free_transfer(&self, ty: TypeId) -> bool {
        self.shapes.get(&ty).is_some_and(|shape| {
            shape.is_reference_free()
                && shape.observations.iter().all(|(_, observation)| {
                    matches!(
                        observation,
                        Observation::Scalar | Observation::Discriminant | Observation::Address
                    )
                })
        })
    }
}

/// Observations that may include protected atomic payloads, including unknown raw reachability.
/// Ordinary fields remain live: an atomic update neither replaces nor retires their owner.
pub(crate) fn prepare<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    memory: &bray_bound_tree::CheckedMemoryOperations,
    atomic: bool,
) -> Result<Footprint, CheckerQueryError<C::UpstreamError>> {
    let mut footprint = Footprint::default();

    let static_types = footprint.observe_accesses(request, storage);

    let complete_reference_types = std::sync::Arc::get_mut(&mut footprint.complete_reference_types)
        .expect("a new footprint owns its type cache before publication");

    let Some(StorageInvalidation::Observed {
        accesses,
        stable_descriptors,
        ..
    }) = &mut footprint.atomic
    else {
        unreachable!("footprint construction starts with its complete observation domain");
    };

    for (root, identity) in storage.identity_entries() {
        let access = storage.root_access(root);

        let Some(ty) = storage.storage_type(root) else {
            if let Some(access) = access {
                accesses.insert(access, vec![StoragePath::root(root)]);
            }

            continue;
        };

        // Logical clauses observe their enclosing context without owning or
        // mutating its backing. Keep reference slots, and resolve only fields
        // actually projected by the checked expression. Expanding every pointee
        // here would demand the callable contracts of unrelated sibling fields.
        if matches!(
            storage.kind(),
            bray_bound_tree::BoundUnitKind::ContractClause
                | bray_bound_tree::BoundUnitKind::PredicateDefinition
        ) {
            let path = StoragePath::root(root);

            if matches!(
                request.semantic_values().type_data(ty).as_ref(),
                TypeData::Borrow { .. }
            ) {
                footprint.references.push((path.clone(), ty));
            }

            if let Some(access) = access {
                accesses.insert(access, vec![path]);
            }

            continue;
        }

        let shared = match request.semantic_values().type_data(ty).as_ref() {
            TypeData::Borrow {
                kind: bray_symbols::BorrowKind::Shared,
                target,
            } => Some(*target),
            _ => None,
        };

        let ty = shared.unwrap_or(ty);

        let shape = cached_shape(request, ty, &mut footprint.shapes, complete_reference_types)?;

        let mut observations = shape.observations.iter().cloned().collect::<BTreeSet<_>>();

        let mut complete_value = shape.complete_value;

        // Aliasing groups static specializations by declaration. Preserve a marker
        // only when every selected storage shape proves that same observation.
        for ty in static_types.get(&root).into_iter().flatten() {
            let shape = cached_shape(
                request,
                *ty,
                &mut footprint.shapes,
                complete_reference_types,
            )?;

            complete_value &= shape.complete_value;

            observations.retain(|observation| {
                !matches!(observation.1, Observation::Identity | Observation::Marker)
                    || shape.observations.contains(observation)
            });

            observations.extend(
                shape
                    .observations
                    .iter()
                    .filter(|(_, observation)| {
                        !matches!(observation, Observation::Identity | Observation::Marker)
                    })
                    .cloned(),
            );
        }

        if complete_value {
            let path = StoragePath::root(root);

            if shared.is_some() {
                // Borrowed owners retain their whole observation only while the
                // existing referent and exposure checks preserve this exact value.
                footprint
                    .borrowed_identities
                    .entry(path.clone())
                    .or_default()
                    .push(path);
            } else if identity.is_invocation_owned(storage.kind()) {
                footprint.scalars.entry(root).or_default().push(path);
            }
        }

        let mut borrowed_paths = observations
            .iter()
            .filter_map(|(path, observation)| {
                let Observation::Reference(ty) = observation else {
                    return None;
                };

                let data = request.semantic_values().type_data(*ty);

                let TypeData::Borrow { kind, .. } = data.as_ref() else {
                    return None;
                };

                Some((path.as_slice(), *kind))
            })
            .collect::<BTreeMap<_, _>>();

        if shared.is_some() {
            borrowed_paths.insert(&[], bray_symbols::BorrowKind::Shared);
        }

        let paths: Vec<_> = observations
            .iter()
            .filter_map(|(projections, observation)| {
                let path = StoragePath {
                    root,
                    projections: projections.clone().into(),
                };

                match observation {
                    Observation::Reference(ty) => {
                        footprint.references.push((path, *ty));

                        None
                    }
                    Observation::Identity | Observation::Marker => {
                        if shared.is_some()
                            || identity.is_invocation_owned(storage.kind())
                            || matches!(identity, bray_bound_tree::StorageIdentity::Static(_))
                        {
                            let borrowed = (0..=projections.len()).rev().find_map(|length| {
                                borrowed_paths.get(&projections[..length]).map(|kind| {
                                    (
                                        StoragePath {
                                            root,
                                            projections: projections[..length].into(),
                                        },
                                        *kind,
                                    )
                                })
                            });

                            if *observation == Observation::Marker {
                                // Typed payload stores cannot replace empty markers,
                                // including markers reached through mutable loans.
                                footprint.empty_markers.insert(path.clone());
                            }

                            if let Some((reference, kind)) = borrowed {
                                // An arbitrary mutable call can replace its owner.
                                // Only shared loans protect identity across calls.
                                if kind == bray_symbols::BorrowKind::Shared {
                                    footprint
                                        .borrowed_identities
                                        .entry(reference)
                                        .or_default()
                                        .push(path);
                                }
                            } else {
                                footprint.identities.entry(root).or_default().push(path);
                            }
                        }

                        None
                    }
                    Observation::Discriminant => {
                        if shared.is_some() || identity.is_invocation_owned(storage.kind()) {
                            footprint
                                .discriminants
                                .entry(root)
                                .or_default()
                                .insert(path);
                        }

                        None
                    }
                    Observation::Scalar => {
                        if shared.is_some() {
                            footprint
                                .borrowed_identities
                                .entry(StoragePath::root(root))
                                .or_insert_with(Vec::new)
                                .push(path.clone());
                        } else if identity.is_invocation_owned(storage.kind()) {
                            footprint
                                .scalars
                                .entry(root)
                                .or_insert_with(Vec::new)
                                .push(path.clone());
                        }

                        Some(path)
                    }
                    Observation::Address => {
                        stable_descriptors.insert(path.clone());

                        Some(path)
                    }
                    Observation::Mutable => Some(path),
                }
            })
            .collect();

        if !paths.is_empty()
            && let Some(access) = access
        {
            accesses.insert(access, paths);
        }
    }

    footprint.prepare_stores(request, storage, memory, atomic)?;

    Ok(footprint)
}

fn cached_shape<'a, C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
    shapes: &'a mut BTreeMap<TypeId, Shape>,
    complete_reference_types: &mut BTreeMap<TypeId, bool>,
) -> Result<&'a Shape, CheckerQueryError<C::UpstreamError>> {
    Ok(match shapes.entry(ty) {
        std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
        std::collections::btree_map::Entry::Vacant(entry) => entry.insert(paths(
            request,
            ty,
            &mut BTreeSet::new(),
            complete_reference_types,
        )?),
    })
}

fn paths<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
    visiting: &mut BTreeSet<NamedTypeSymbolId>,
    complete_reference_types: &mut BTreeMap<TypeId, bool>,
) -> Result<Shape, CheckerQueryError<C::UpstreamError>> {
    let mut shape = value_paths(request, ty, visiting, complete_reference_types)?;

    // Record the slot even when its referent has no observable fields. Terminal
    // ownership includes retained dependencies that the body never names.
    if crate::representation::reference_kind(request, ty).is_some() {
        shape
            .observations
            .push((Vec::new(), Observation::Reference(ty)));

        // A reference value has one explicit pointer slot, independently of its pointee shape.
        shape.complete_references = true;
    }

    if shape.complete_references {
        complete_reference_types.insert(ty, shape.is_reference_free());
    }

    Ok(shape)
}

fn value_paths<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
    visiting: &mut BTreeSet<NamedTypeSymbolId>,
    complete_reference_types: &mut BTreeMap<TypeId, bool>,
) -> Result<Shape, CheckerQueryError<C::UpstreamError>> {
    if request.is_cancelled() {
        return Err(CheckerQueryError::Cancelled);
    }

    let reference = matches!(
        request.semantic_values().type_data(ty).as_ref(),
        TypeData::Borrow { .. }
    );

    let borrowed = matches!(
        request.semantic_values().type_data(ty).as_ref(),
        TypeData::Borrow {
            kind: bray_symbols::BorrowKind::Mutable,
            ..
        }
    );

    let ty = request.semantic_values().unborrowed_type(ty);
    let data = request.semantic_values().type_data(ty);
    let mut fields = Vec::new();
    let mut complete_references = true;
    let mut nominal = false;

    let definition = match data.as_ref() {
        TypeData::Callable(_) => {
            // Callable values carry code and checked contracts, never captured storage.
            return known_shape(
                request,
                ty,
                bray_compiler_known::RepresentationRole::FunctionPointer,
                reference,
                visiting,
                complete_reference_types,
            );
        }
        TypeData::Named {
            definition,
            substitution,
        } => {
            let symbols = request.available_compiler_known_symbols();

            let role = match definition {
                NamedTypeSymbolId::Struct(id) => symbols.symbol_representation(*id),
                NamedTypeSymbolId::Union(id) => symbols.symbol_representation(*id),
            };

            nominal = role.is_none();

            if let Some(role) = role.filter(|role| {
                !matches!(
                    role,
                    bray_compiler_known::RepresentationRole::Result
                        | bray_compiler_known::RepresentationRole::RunResult
                )
            }) {
                return known_shape(
                    request,
                    ty,
                    role,
                    reference,
                    visiting,
                    complete_reference_types,
                );
            }

            let buffer_element = request
                .context()
                .raw_buffer_element(*definition, *substitution)?;

            // A recursive reference widens only that component, not its siblings.
            if !visiting.insert(*definition) {
                return Ok(Shape::unknown());
            }

            if let Some(element) = buffer_element {
                // Inline pointer fields cover backing with no embedded references.
                // Reference-bearing or opaque elements still require the retained-loan bound.
                let element = paths(request, element, visiting, complete_reference_types)?;

                complete_references = element.is_reference_free();
            }

            let representation = request.declared_type_representation(*definition)?;

            complete_references &= !representation.value().is_recovered()
                && !representation.diagnostics().has_errors();

            let members: Vec<_> = match representation.value().storage() {
                DeclaredStorageShape::Structure(members) => members
                    .iter()
                    .map(|member| {
                        (
                            member.field().map(StorageProjection::ProductField),
                            member.ty(),
                        )
                    })
                    .collect(),
                DeclaredStorageShape::Union(variants) => variants
                    .iter()
                    .flat_map(|variant| {
                        variant.members().iter().map(move |member| {
                            (
                                member.field().map(|field| {
                                    StorageProjection::ActiveUnionPayloadField {
                                        variant: variant.variant(),
                                        field,
                                    }
                                }),
                                member.ty(),
                            )
                        })
                    })
                    .collect(),
            };

            for (projection, template) in members {
                let Some(projection) = projection else {
                    visiting.remove(definition);

                    return Ok(Shape::unknown());
                };

                let checked =
                    crate::constant::checked_substituted_type(request, template, *substitution)?;

                let Some(ty) = *checked.value() else {
                    visiting.remove(definition);

                    return Ok(Shape::unknown());
                };

                fields.push((projection, ty));
            }

            Some(*definition)
        }
        TypeData::Array { element, .. } => {
            let element = paths(request, *element, visiting, complete_reference_types)?;
            let mut shape = Shape::unknown();

            // Element shape proves this without enumerating the array extent.
            shape.complete_references = element.is_reference_free();
            shape.complete_value = !reference && element.complete_value;

            return Ok(shape);
        }
        TypeData::Nullable(element) => {
            fields.push((StorageProjection::NullableValue, *element));

            None
        }
        TypeData::Tuple(elements) => {
            fields.extend(elements.iter().enumerate().map(|(index, ty)| {
                (
                    StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(
                        index
                            .try_into()
                            .expect("tuple ordinal fits its checked type"),
                    )),
                    *ty,
                )
            }));

            None
        }
        _ => return Ok(Shape::unknown()),
    };

    let mut result = if matches!(definition, Some(NamedTypeSymbolId::Struct(_)))
        && fields.is_empty()
        && !borrowed
    {
        vec![(Vec::new(), Observation::Marker)]
    } else {
        Vec::new()
    };

    if matches!(
        data.as_ref(),
        TypeData::Nullable(_)
            | TypeData::Named {
                definition: NamedTypeSymbolId::Union(_),
                ..
            }
    ) {
        result.push((Vec::new(), Observation::Discriminant));
    }

    let mut complete_value = !reference;

    for (field, ty) in fields {
        let shape = paths(request, ty, visiting, complete_reference_types)?;

        // A nominal carrier retains inline addresses and code entries. Their
        // pointees and providers remain governed by the carried dependency contract.
        let address = nominal
            && (crate::representation::reference_kind(request, ty)
                == Some(bray_symbols::DependencyReferenceKind::Address)
                || matches!(
                    request.semantic_values().type_data(ty).as_ref(),
                    TypeData::Callable(_)
                ));

        complete_value &= shape.complete_value || address;
        complete_references &= shape.complete_references;

        result.extend(
            shape.observations.into_iter().map(|(path, descriptor)| {
                (std::iter::once(field).chain(path).collect(), descriptor)
            }),
        );
    }

    if reference {
        result.retain(|(_, observation)| *observation != Observation::Scalar);
    }

    if borrowed {
        result.retain(|(_, observation)| {
            !matches!(
                observation,
                Observation::Identity | Observation::Discriminant
            )
        });
    }

    if let Some(definition) = definition {
        visiting.remove(&definition);
    }

    Ok(Shape {
        observations: result,
        complete_value,
        complete_references,
    })
}

/// Intersects physical may-alias observations with the atomic operation's closed write domain.
pub(crate) fn restrict(
    invalidation: StorageInvalidation,
    frame: &StorageInvalidation,
) -> StorageInvalidation {
    let StorageInvalidation::Observed {
        accesses: frame_paths,
        stable_descriptors: descriptors,
        ..
    } = frame
    else {
        unreachable!("prepared atomic frame contains resolved observations");
    };

    let (mut accesses, mut referents, mut snapshots, mut stable_descriptors, stable_values) =
        match invalidation {
            StorageInvalidation::Observed {
                accesses,
                referents,
                snapshots,
                stable_descriptors,
                stable_values,
                ..
            } => (
                accesses,
                referents,
                snapshots,
                stable_descriptors,
                stable_values,
            ),
            StorageInvalidation::All {
                stable_values,
                mut stable_descriptors,
            } => {
                let mut accesses = frame_paths.clone();

                accesses.retain(|_, paths| {
                    paths.retain(|path| !stable_values.iter().any(|stable| stable.contains(path)));

                    !paths.is_empty()
                });

                stable_descriptors.extend(descriptors.iter().cloned());

                return StorageInvalidation::Observed {
                    scope: bray_bound_tree::StorageMutationScope::Unbounded,
                    accesses,
                    referents: BTreeSet::new(),
                    snapshots: BTreeMap::new(),
                    stable_descriptors,
                    stable_values,
                };
            }
            StorageInvalidation::Paths(_) => return frame.clone(),
        };

    let intersect = |path: &StoragePath| {
        frame_paths
            .values()
            .flatten()
            .filter_map(|candidate| {
                if path.contains(candidate) {
                    Some(candidate.clone())
                } else if candidate.contains(path) {
                    Some(path.clone())
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>()
    };

    accesses.retain(|_, paths| {
        *paths = paths
            .iter()
            .flat_map(&intersect)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        !paths.is_empty()
    });

    referents = referents.iter().flat_map(intersect).collect();
    snapshots.retain(|path, _| accesses.values().flatten().any(|changed| changed == path));
    stable_descriptors.extend(descriptors.iter().cloned());

    StorageInvalidation::Observed {
        scope: bray_bound_tree::StorageMutationScope::Unbounded,
        accesses,
        referents,
        snapshots,
        stable_descriptors,
        stable_values,
    }
}
