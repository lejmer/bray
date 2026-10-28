use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{StorageInvalidation, StoragePath, StoragePlan, StorageProjection};
use bray_symbols::{DeclaredStorageShape, NamedTypeSymbolId, TypeData, TypeId};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Observation {
    Mutable,
    Scalar,
    Address,
    Identity,
    Discriminant,
    Reference(TypeId),
}

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

fn known_shape(role: bray_compiler_known::RepresentationRole, reference: bool) -> Shape {
    use bray_compiler_known::RepresentationRole;

    if matches!(role, RepresentationRole::Unit | RepresentationRole::Never) {
        return Shape {
            observations: Vec::new(),
            complete_value: !reference,
            complete_references: true,
        };
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
    } else {
        Observation::Mutable
    };

    Shape {
        observations: vec![(Vec::new(), observation)],
        complete_value: scalar && !reference,
        complete_references: scalar || matches!(observation, Observation::Address),
    }
}

pub(crate) struct Footprint {
    pub(crate) atomic: StorageInvalidation,
    pub(crate) references: Vec<(StoragePath, TypeId)>,
    pub(crate) complete_reference_types: BTreeSet<TypeId>,
    pub(crate) scalars: BTreeMap<bray_bound_tree::StorageIdentityId, Vec<StoragePath>>,
    pub(crate) borrowed_identities: BTreeMap<bray_bound_tree::StorageIdentityId, Vec<StoragePath>>,
    pub(crate) identities: BTreeMap<bray_bound_tree::StorageIdentityId, Vec<StoragePath>>,
    pub(crate) discriminants: BTreeMap<bray_bound_tree::StorageIdentityId, BTreeSet<StoragePath>>,
    pub(crate) tag_free_values: BTreeSet<TypeId>,
}

/// Observations that may include protected atomic payloads, including unknown raw reachability.
/// Ordinary fields remain live: an atomic update neither replaces nor retires their owner.
pub(crate) fn prepare<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> Result<Footprint, CheckerQueryError<C::UpstreamError>> {
    let mut accesses = BTreeMap::new();
    let mut references = Vec::new();
    let mut shapes = BTreeMap::new();
    let mut complete_reference_types = BTreeSet::new();
    let mut stable_descriptors = BTreeSet::new();
    let mut identities = BTreeMap::new();
    let mut scalars = BTreeMap::new();
    let mut discriminants = BTreeMap::new();
    let mut borrowed_identities = BTreeMap::new();
    let mut static_types = BTreeMap::<_, BTreeSet<_>>::new();

    for (access_id, access) in storage.access_entries() {
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
            discriminants
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

    for (root, identity) in storage.identity_entries() {
        let access = storage.root_access(root);

        let Some(ty) = storage.storage_type(root) else {
            if let Some(access) = access {
                accesses.insert(access, vec![StoragePath::root(root)]);
            }

            continue;
        };

        let shared = match request.semantic_values().type_data(ty).as_ref() {
            TypeData::Borrow {
                kind: bray_symbols::BorrowKind::Shared,
                target,
            } => Some(*target),
            _ => None,
        };

        let ty = shared.unwrap_or(ty);

        if let std::collections::btree_map::Entry::Vacant(e) = shapes.entry(ty) {
            e.insert(paths(
                request,
                ty,
                &mut BTreeSet::new(),
                &mut complete_reference_types,
            )?);
        }

        let mut observations = shapes[&ty]
            .observations
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();

        let mut complete_value = shapes[&ty].complete_value;

        // Aliasing groups static specializations by declaration. Preserve a marker
        // only when every selected storage shape proves that same observation.
        for ty in static_types.get(&root).into_iter().flatten() {
            if !shapes.contains_key(ty) {
                shapes.insert(
                    *ty,
                    paths(
                        request,
                        *ty,
                        &mut BTreeSet::new(),
                        &mut complete_reference_types,
                    )?,
                );
            }

            complete_value &= shapes[ty].complete_value;

            observations.retain(|observation| {
                observation.1 != Observation::Identity
                    || shapes[ty].observations.contains(observation)
            });

            observations.extend(
                shapes[ty]
                    .observations
                    .iter()
                    .filter(|(_, observation)| *observation != Observation::Identity)
                    .cloned(),
            );
        }

        if shared.is_none() && identity.is_invocation_owned(storage.kind()) && complete_value {
            // A call that cannot reach any part of this value cannot change its
            // whole-value contract. Scalar fields alone do not retain that contract.
            scalars
                .entry(root)
                .or_insert_with(Vec::new)
                .push(StoragePath::root(root));
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
                        references.push((path, *ty));

                        None
                    }
                    Observation::Identity => {
                        if shared.is_some() {
                            borrowed_identities
                                .entry(root)
                                .or_insert_with(Vec::new)
                                .push(path);
                        } else if identity.is_invocation_owned(storage.kind())
                            || matches!(identity, bray_bound_tree::StorageIdentity::Static(_))
                        {
                            identities.entry(root).or_insert_with(Vec::new).push(path);
                        }

                        None
                    }
                    Observation::Discriminant => {
                        if shared.is_some() || identity.is_invocation_owned(storage.kind()) {
                            discriminants.entry(root).or_default().insert(path);
                        }

                        None
                    }
                    Observation::Scalar => {
                        if shared.is_none() && identity.is_invocation_owned(storage.kind()) {
                            scalars
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

    for plan in storage
        .access_plans()
        .iter()
        .filter(|plan| plan.purpose() == bray_bound_tree::StorageAccessPurpose::Assignment)
    {
        let ty = storage
            .access(plan.access())
            .expect("assignment retains its checked destination")
            .reached_type();

        if let std::collections::btree_map::Entry::Vacant(e) = shapes.entry(ty) {
            e.insert(paths(
                request,
                ty,
                &mut BTreeSet::new(),
                &mut complete_reference_types,
            )?);
        }
    }

    let tag_free_values = shapes
        .iter()
        .filter_map(|(ty, shape)| {
            (shape.is_reference_free()
                && !shape
                    .observations
                    .iter()
                    .any(|(_, observation)| *observation == Observation::Discriminant))
            .then_some(*ty)
        })
        .collect();

    Ok(Footprint {
        references,
        complete_reference_types,
        atomic: StorageInvalidation::Observed {
            scope: bray_bound_tree::StorageMutationScope::Unbounded,
            accesses,
            snapshots: BTreeMap::new(),
            referents: BTreeSet::new(),
            stable_descriptors,
        },
        identities,
        scalars,
        borrowed_identities,
        discriminants,
        tag_free_values,
    })
}

fn paths<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
    visiting: &mut BTreeSet<NamedTypeSymbolId>,
    complete_reference_types: &mut BTreeSet<TypeId>,
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
        complete_reference_types.insert(ty);
    }

    Ok(shape)
}

fn value_paths<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
    visiting: &mut BTreeSet<NamedTypeSymbolId>,
    complete_reference_types: &mut BTreeSet<TypeId>,
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

    let definition = match data.as_ref() {
        TypeData::Named {
            definition,
            substitution,
        } => {
            let symbols = request.available_compiler_known_symbols();

            let role = match definition {
                NamedTypeSymbolId::Struct(id) => symbols.symbol_representation(*id),
                NamedTypeSymbolId::Union(id) => symbols.symbol_representation(*id),
            };

            if let Some(role) = role.filter(|role| {
                !matches!(
                    role,
                    bray_compiler_known::RepresentationRole::Result
                        | bray_compiler_known::RepresentationRole::RunResult
                )
            }) {
                return Ok(known_shape(role, reference));
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
        vec![(Vec::new(), Observation::Identity)]
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

    let mut complete_value = !reference
        && !result
            .iter()
            .any(|(_, observation)| *observation == Observation::Discriminant);

    for (field, ty) in fields {
        let shape = paths(request, ty, visiting, complete_reference_types)?;

        complete_value &= shape.complete_value;
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

    let (mut accesses, mut referents, mut snapshots, mut stable_descriptors) = match invalidation {
        StorageInvalidation::Observed {
            accesses,
            referents,
            snapshots,
            stable_descriptors,
            ..
        } => (accesses, referents, snapshots, stable_descriptors),
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
    }
}
