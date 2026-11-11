use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{StorageInvalidation, StoragePath, StoragePlan, StorageProjection};
use bray_symbols::{DeclaredStorageShape, NamedTypeSymbolId, TypeData, TypeId};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

use bray_bound_tree::StorageObservationKind as Observation;

#[derive(Debug)]
struct Shape {
    observations: Vec<(Vec<StorageProjection>, Observation)>,
    complete_value: bool,
    inline_only: bool,
    complete_references: bool,
    // Inline ownership differs from observable referents and protected contents.
    // None means that this type's possible inline ownership remains unknown.
    inline_carriers: Option<BTreeSet<TypeId>>,
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
        // Opaque reachable state does not expose this value's inline storage.
        Self {
            observations: vec![(Vec::new(), Observation::Identity), (Vec::new(), Observation::Mutable)],
            complete_value: false,
            inline_only: false,
            complete_references: false,
            inline_carriers: None,
        }
    }
}

pub(crate) fn carries_no_borrows<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
    let shape = paths(request, ty, &mut BTreeSet::new(), &mut BTreeMap::new())?;

    Ok(shape.complete_references && !shape.observations.iter().any(|(_, observation)| {
        matches!(observation, Observation::Reference(ty)
            if matches!(request.semantic_values().type_data(*ty).as_ref(), TypeData::Borrow { .. }))
    }))
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
            inline_only: true,
            complete_references: true,
            inline_carriers: Some(BTreeSet::new()),
        });
    }

    let scalar = crate::representation::is_scalar(role);

    let observation = if scalar {
        Observation::Value
    } else if crate::representation::is_address(role) {
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
        inline_only: scalar,
        complete_references: scalar || matches!(observation, Observation::Address),
        inline_carriers: (scalar || matches!(observation, Observation::Address))
            .then(BTreeSet::new),
    };

    if matches!(
        role,
        RepresentationRole::Atomic | RepresentationRole::Uninit
    ) {
        let element = request
            .available_compiler_known_symbols()
            .unary_representation_argument(request.semantic_values(), role, ty)
            .expect("checked protected storage retains its payload type");

        let element = paths(request, element, visiting, complete_reference_types)?;

        shape.complete_references = element.is_reference_free();
        shape.inline_carriers = element.inline_carriers;

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
    pub(crate) owned_observations: BTreeMap<bray_bound_tree::StorageIdentityId, Vec<(StoragePath, Observation)>>,
    pub(crate) borrowed_observations: BTreeMap<StoragePath, (bray_symbols::BorrowKind, TypeId, Vec<(StoragePath, Observation)>)>,
    pub(crate) empty_markers: BTreeMap<StoragePath, TypeId>,
    pub(crate) discriminants: BTreeMap<bray_bound_tree::StorageIdentityId, BTreeSet<StoragePath>>,
    shapes: BTreeMap<TypeId, Shape>,
}

impl Footprint {
    fn observe_accesses<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
    ) -> BTreeMap<bray_bound_tree::StorageIdentityId, BTreeSet<TypeId>> {
        self.atomic = Some(StorageInvalidation::Observed {
            scope: bray_bound_tree::StorageMutationScope::Unbounded,
            accesses: BTreeMap::new(),
            snapshots: BTreeMap::new(),
            referents: BTreeSet::new(),
            stable_descriptors: BTreeSet::new(),
            stable_values: BTreeMap::new(),
            normal_snapshots: Default::default(),
        });

        let mut static_types = BTreeMap::<_, BTreeSet<_>>::new();

        let entry_owners = request
            .trusted_contracts()
            .into_iter()
            .flat_map(|contracts| contracts.required_witnesses.iter().flat_map(|(_, owner, containing_value)|
                owner.carrier_inputs(request.semantic_values(), dependencies, *containing_value)))
            .collect::<BTreeSet<_>>();

        for (access_id, access) in storage.access_entries() {
            if let bray_bound_tree::StorageAccessRoot::Storage(root) = access.root()
                && access.projections().is_empty()
                && !self.owned_observations.contains_key(&root)
                && storage
                    .identity(root)
                    .is_some_and(|identity| identity.is_invocation_owned(storage.kind()))
                && storage.storage_type(root).is_some_and(|ty| {
                    crate::representation::reference_kind(request, ty)
                        == Some(bray_symbols::DependencyReferenceKind::Address)
                })
                && crate::ExecutionPlace::storage_identity(storage, root)
                    .is_some_and(|owner| entry_owners.contains(&owner))
            {
                // This owned slot is a protocol witness for the current entry.
                // Keeping its address value grants no unowned pointee permission.
                self.owned_observations.entry(root).or_default().push((StoragePath::root(root), Observation::Identity));
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

    pub(crate) fn prepare_stores<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
        memory: &bray_bound_tree::CheckedMemoryOperations,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        lifecycle_paths: &BTreeMap<(bray_symbols::CallableInstanceData, bray_symbols::DependencySubject), (TypeId, bray_bound_tree::StorageReferencePath)>,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        let mut atomic = false;

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

        // Prepare every typed transfer in the existing effect walk, including
        // helper Consume/Write summaries whose target is not a source local.
        for call in dependencies.calls().values() {
            atomic |= prepare_effects(request, call.effects().iter().chain(call.deferred_effects().into_iter().flatten()),
                &mut self.shapes, complete_reference_types, |subject| call.reference_type(subject))?;
        }

        for (callable, contract) in dependencies.lifecycle_dependencies() {
            atomic |= prepare_effects(request, contract.effects(), &mut self.shapes, complete_reference_types,
                |subject| lifecycle_paths.get(&(*callable, subject.clone())).map(|(ty, _)| *ty))?;
        }

        if !atomic {
            self.atomic = None;
        }

        Ok(())
    }

    /// Borrows the original classification from the nearest owning or loan context.
    pub(crate) fn observations<'a>(&'a self, path: &'a StoragePath) -> impl Iterator<Item = (&'a StoragePath, Observation)> {
        let borrowed = (0..=path.projections.len()).rev().find_map(|length| {
            self.borrowed_observations.get(&StoragePath {
                root: path.root,
                projections: path.projections[..length].into(),
            })
        });

        let values = borrowed.map(|(_, _, values)| values)
            .or_else(|| self.owned_observations.get(&path.root));

        values.into_iter().flatten()
            .filter(|(observed, _)| path.contains(observed))
            .map(|(path, kind)| (path, *kind))
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
                    matches!(observation, Observation::Value | Observation::Address)
                }),
        )
    }

    /// Nominal carriers transported inline by this already classified value type.
    /// Reachability and mutation authority remain with the resolved operation.
    pub(crate) fn inline_carriers(&self, ty: TypeId) -> Option<&BTreeSet<TypeId>> {
        self.shapes.get(&ty)?.inline_carriers.as_ref()
    }

}

/// Observations that may include protected atomic payloads, including unknown raw reachability.
/// Ordinary fields remain live: an atomic update neither replaces nor retires their owner.
pub(crate) fn prepare<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    dependencies: &bray_bound_tree::CheckedDependencyContracts,
) -> Result<Footprint, CheckerQueryError<C::UpstreamError>> {
    let mut footprint = Footprint::default();

    let static_types = footprint.observe_accesses(request, storage, dependencies);

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
                !matches!(observation.1, Observation::Value | Observation::Identity | Observation::Marker(_))
                    || shape.observations.contains(observation)
            });

            observations.extend(
                shape
                    .observations
                    .iter()
                    .filter(|(_, observation)| {
                        !matches!(observation, Observation::Value | Observation::Identity | Observation::Marker(_))
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
                    .borrowed_observations
                    .entry(path.clone())
                    .or_insert_with(|| (bray_symbols::BorrowKind::Shared, ty, Vec::new()))
                    .2.push((path, Observation::Mutable));
            } else if identity.is_invocation_owned(storage.kind()) {
                footprint.owned_observations.entry(root).or_default().push((path, Observation::Mutable));
            }
        }

        let mut borrowed_paths = observations
            .iter()
            .filter_map(|(path, observation)| {
                let Observation::Reference(ty) = observation else {
                    return None;
                };

                let data = request.semantic_values().type_data(*ty);

                let TypeData::Borrow { kind, target } = data.as_ref() else {
                    return None;
                };

                Some((path.as_slice(), (*kind, *target)))
            })
            .collect::<BTreeMap<_, _>>();

        if shared.is_some() {
            borrowed_paths.insert(&[], (bray_symbols::BorrowKind::Shared, ty));
        }

        let paths: Vec<_> = observations
            .iter()
            .filter_map(|(projections, observation)| {
                let path = StoragePath {
                    root,
                    projections: projections.clone().into(),
                };

                let borrowed = match observation {
                    Observation::Value | Observation::Address | Observation::Identity | Observation::Marker(_) | Observation::Discriminant =>
                        (0..=projections.len()).rev().find_map(|length| {
                            borrowed_paths.get(&projections[..length]).map(|(kind, ty)| {
                                (StoragePath {
                                    root,
                                    projections: projections[..length].into(),
                                }, *kind, *ty)
                            })
                        }),
                    _ => None,
                };

                match observation {
                    Observation::Reference(ty) => {
                        footprint.references.push((path, *ty));

                        None
                    }
                    Observation::Identity | Observation::Marker(_) => {
                        if shared.is_some()
                            || identity.is_invocation_owned(storage.kind())
                            || matches!(identity, bray_bound_tree::StorageIdentity::Static(_))
                        {
                            if let Observation::Marker(ty) = observation
                                && borrowed.is_none()
                            {
                                // Borrowed markers use their loan's physical referents
                                // below. A logical reference slot is not that storage.
                                footprint.empty_markers.insert(path.clone(), *ty);
                            }

                            if let Some((reference, kind, ty)) = borrowed {
                                footprint
                                    .borrowed_observations
                                    .entry(reference)
                                    .or_insert_with(|| (kind, ty, Vec::new()))
                                    .2.push((path, *observation));
                            } else {
                                footprint.owned_observations.entry(root).or_default().push((path, *observation));
                            }
                        }

                        None
                    }
                    Observation::Discriminant => {
                        if borrowed.is_none() && identity.is_invocation_owned(storage.kind()) {
                            footprint.owned_observations.entry(root).or_default()
                                .push((path.clone(), Observation::Discriminant));
                        }

                        if let Some((reference, kind, ty)) = borrowed {
                            footprint.borrowed_observations.entry(reference)
                                .or_insert_with(|| (kind, ty, Vec::new()))
                                .2.push((path.clone(), Observation::Discriminant));
                        }

                        if shared.is_some() || identity.is_invocation_owned(storage.kind()) {
                            footprint
                                .discriminants
                                .entry(root)
                                .or_default()
                                .insert(path);
                        }

                        None
                    }
                    Observation::Value | Observation::Address => {
                        if *observation == Observation::Address {
                            stable_descriptors.insert(path.clone());
                        }

                        if let Some((reference, kind, ty)) = borrowed {
                            footprint.borrowed_observations.entry(reference)
                                .or_insert_with(|| (kind, ty, Vec::new()))
                                .2.push((path.clone(), *observation));
                        } else if identity.is_invocation_owned(storage.kind()) && *observation == Observation::Value {
                            footprint.owned_observations.entry(root).or_default().push((path.clone(), *observation));
                        }

                        // Atomic operations cannot change ordinary inline values
                        // or address descriptors. Their protected payload remains
                        // a separate mutable observation.
                        None
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

    Ok(footprint)
}

fn prepare_effects<'a, C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    effects: impl IntoIterator<Item = &'a bray_symbols::DependencyStorageEffect>,
    shapes: &mut BTreeMap<TypeId, Shape>,
    complete_reference_types: &mut BTreeMap<TypeId, bool>,
    reference_type: impl Fn(&bray_symbols::DependencySubject) -> Option<TypeId>,
) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
    use bray_symbols::{DependencyMutationKind as Mutation, DependencyStorageEffect as Effect};

    let mut atomic = false;

    'effects: for effect in effects {
        let (mut ty, projections) = match effect {
            Effect::Atomic | Effect::Direct { kind: Mutation::Atomic, .. }
            | Effect::Indirect { kind: Mutation::Atomic, .. } => {
                atomic = true;
                continue;
            }
            Effect::Direct { kind: Mutation::Replace(ty), .. }
            | Effect::Indirect { kind: Mutation::Replace(ty), .. }
            | Effect::UnboundedMutation { kind: Mutation::Replace(ty), .. } => (*ty, &[][..]),
            Effect::Direct { access_type: Some(ty), kind: Mutation::Write | Mutation::Consume | Mutation::Clear, .. }
            | Effect::Indirect { access_type: Some(ty), kind: Mutation::Write | Mutation::Consume | Mutation::Clear, .. }
            | Effect::UnboundedMutation { access_type: Some(ty), kind: Mutation::Write | Mutation::Consume | Mutation::Clear, .. }
                if crate::representation::reference_kind(request, *ty) == Some(bray_symbols::DependencyReferenceKind::Address) => {
                let Some(ty) = crate::representation::reference_target_type(request, *ty) else { continue; };

                (ty, &[][..])
            }
            Effect::Direct { subject, kind: Mutation::Write | Mutation::Consume | Mutation::Clear, .. } => {
                let Some(ty) = reference_type(subject) else { continue; };

                (ty, &[][..])
            }
            Effect::Indirect { reference, projections, kind: Mutation::Write | Mutation::Consume | Mutation::Clear, .. } => {
                let Some(ty) = reference_type(reference) else { continue; };

                // A lifecycle root already denotes its physical receiver.
                (crate::representation::reference_target_type(request, ty).unwrap_or(ty), projections.as_ref())
            }
            _ => continue,
        };

        for projection in projections {
            let projected = crate::storage::projected_value_type(request, ty, *projection)?;

            let Some(projected) = *projected.value() else { continue 'effects; };

            ty = projected;
        }

        // A backing region's mutation type is its element, independently of extent.
        if projections.last() == Some(&bray_symbols::DependencyProjection::OwnedTarget)
            && let TypeData::Slice(element) = request.semantic_values().type_data(ty).as_ref() {
            ty = *element;
        }

        cached_shape(request, ty, shapes, complete_reference_types)?;
    }

    Ok(atomic)
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
        shape.inline_carriers = Some(BTreeSet::new());
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

    let data = request.semantic_values().type_data(ty);
    let reference = matches!(data.as_ref(), TypeData::Borrow { .. });

    let nested_borrow = matches!(data.as_ref(), TypeData::Borrow { target, .. }
        if matches!(request.semantic_values().type_data(*target).as_ref(), TypeData::Borrow { .. }));

    let ty = request.semantic_values().unborrowed_type(ty);
    let data = request.semantic_values().type_data(ty);
    let mut fields = Vec::new();
    let mut complete_references = true;
    let mut inline_carriers = Some(BTreeSet::new());

    let role = match data.as_ref() {
        TypeData::Callable(_) => Some(bray_compiler_known::RepresentationRole::FunctionPointer),
        TypeData::Named { definition, .. } => {
            let symbols = request.available_compiler_known_symbols();

            match definition {
                NamedTypeSymbolId::Struct(id) => symbols.symbol_representation(*id),
                NamedTypeSymbolId::Union(id) => symbols.symbol_representation(*id),
            }
        }
        _ => None,
    };

    let nominal = matches!(data.as_ref(), TypeData::Named { .. }) && role.is_none();

    if let Some(role) = role.filter(|role| {
        !matches!(
            role,
            bray_compiler_known::RepresentationRole::Result
                | bray_compiler_known::RepresentationRole::RunResult
        )
    }) {
        let mut shape = known_shape(
            request,
            ty,
            role,
            reference,
            visiting,
            complete_reference_types,
        )?;

        shape.inline_only &= !nested_borrow;

        return Ok(shape);
    }

    let definition = match data.as_ref() {
        TypeData::Named {
            definition,
            substitution,
        } => {
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

            if representation.value().is_recovered() || representation.diagnostics().has_errors() {
                complete_references = false;
                inline_carriers = None;
            }

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
            shape.inline_only = !nested_borrow && element.inline_only && element.is_reference_free();
            shape.inline_carriers = element.inline_carriers;

            if shape.inline_only {
                shape.observations.push((Vec::new(), Observation::Value));
            }

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

    let mut result = Vec::new();

    if definition.is_some() && let Some(carriers) = &mut inline_carriers {
        carriers.insert(ty);
    }

    if matches!(definition, Some(NamedTypeSymbolId::Struct(_)))
        && fields.is_empty()
    {
        result.push((Vec::new(), Observation::Marker(ty)));
    }

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
    let mut inline_only = !nested_borrow && inline_carriers.is_some();

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
        inline_only &= shape.inline_only && shape.is_reference_free();
        complete_references &= shape.complete_references;

        match (&mut inline_carriers, shape.inline_carriers) {
            (Some(markers), Some(field_markers)) => markers.extend(field_markers),
            (_, None) => inline_carriers = None,
            (None, Some(_)) => {},
        }

        result.extend(
            shape.observations.into_iter().map(|(path, descriptor)| {
                (std::iter::once(field).chain(path).collect(), descriptor)
            }),
        );
    }

    if let Some(definition) = definition {
        visiting.remove(&definition);
    }

    if inline_only && !result.iter().any(|(path, observation)| path.is_empty()
        && matches!(observation, Observation::Value | Observation::Marker(_))) {
        result.push((Vec::new(), Observation::Value));
    }

    Ok(Shape {
        observations: result,
        complete_value,
        inline_only,
        complete_references,
        inline_carriers,
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

    let (mut accesses, mut referents, mut snapshots, mut stable_descriptors, stable_values, normal_snapshots) =
        match invalidation {
            StorageInvalidation::Observed {
                accesses,
                referents,
                snapshots,
                stable_descriptors,
                stable_values,
                normal_snapshots,
                ..
            } => (
                accesses,
                referents,
                snapshots,
                stable_descriptors,
                stable_values,
                normal_snapshots,
            ),
            StorageInvalidation::All {
                stable_values,
                mut stable_descriptors,
                normal_snapshots,
            } => {
                let mut accesses = frame_paths.clone();

                accesses.retain(|_, paths| {
                    paths.retain(|path| !stable_values.iter().any(|(stable, kind)| *kind != Observation::Discriminant && stable.contains(path)));

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
                    normal_snapshots,
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
        normal_snapshots,
    }
}
