use crate::{CheckerRequestContext, CheckerSemanticQueryProvider, CheckerUnitView};
use bray_bound_tree::{
    BorrowCapabilityId, BoundDependencySubject, BoundExpressionId, Liveness, StorageAccessId,
    StorageAccessPlan, StorageAccessPurpose, StorageIdentity, StorageIdentityId, StoragePlan,
};
use bray_symbols::{BorrowKind, CallableSignatureQuery};
use std::collections::{BTreeMap, BTreeSet};
use crate::analysis::liveness::OperationEffects;

#[derive(Debug, Default)]
pub(crate) struct StorageFlowInput {
    pub(in crate::analysis::storage_flow) footprint: crate::storage::observation::Footprint,
    pub(in crate::analysis::storage_flow) variant_payloads:
        BTreeMap<bray_bound_tree::StoragePath, BTreeSet<bray_bound_tree::StoragePath>>,
    expression_places:
        BTreeMap<BoundExpressionId, Vec<crate::analysis::storage_flow::memory::Place>>,
    definitions: BTreeMap<bray_bound_tree::BoundExecutionSite, Vec<StorageIdentityId>>,
    mutable_storage: BTreeSet<StorageIdentityId>,
    immutable_field_accesses: BTreeSet<StorageAccessId>,
    storage_last_uses: BTreeMap<bray_bound_tree::BoundExecutionSite, Vec<StorageIdentityId>>,
    /// Assignments preserving tags, with whether their write domain is limited to contents.
    pub(in crate::analysis::storage_flow) tag_preserving_assignments:
        BTreeMap<bray_bound_tree::BoundExecutionSite, bool>,
    pub(in crate::analysis::storage_flow) opaque_values: BTreeMap<bray_bound_tree::StoragePath, bray_bound_tree::StorageObservationKind>,
    /// Inline owner slots, including independently typed nested carriers.
    pub(in crate::analysis::storage_flow) opaque_descriptors: BTreeSet<bray_bound_tree::StoragePath>,
    pub(in crate::analysis::storage_flow) opaque_candidates: Vec<bray_bound_tree::StoragePath>,
    pub(super) unused_entry_storage: BTreeSet<StorageIdentityId>,
    pub(super) cleanup_requirements: std::sync::Arc<crate::asynchronous::StorageCleanupRequirements>,
    pub(in crate::analysis::storage_flow) terminal_dependencies: bool,
    pub(in crate::analysis::storage_flow) terminal_receiver: Option<bray_bound_tree::StoragePath>,
    pub(in crate::analysis::storage_flow) graceful_types: BTreeSet<bray_symbols::TypeId>,
    pub(in crate::analysis::storage_flow) value_dependencies:
        BTreeMap<StorageIdentityId, BTreeSet<BoundDependencySubject>>,
    pub(super) cleanup_dependency_owners:
        BTreeMap<BoundDependencySubject, BTreeSet<StorageIdentityId>>,
    pub(in crate::analysis::storage_flow) lifecycle_storage:
        BTreeMap<bray_symbols::CallableInstanceData, bray_bound_tree::BoundCallStorage>,
    pub(in crate::analysis::storage_flow) allocation_pointer:
        Option<(bray_symbols::StructFieldSymbolId, bray_symbols::TypeId)>,
    pub(in crate::analysis::storage_flow) raw_buffers:
        BTreeMap<bray_symbols::TypeId, crate::representation::RawBufferFields>,
    pub(in crate::analysis::storage_flow) buffer_cleanup_effects:
        BTreeMap<bray_symbols::TypeId, Vec<bray_symbols::DependencyStorageEffect>>,
    pub(in crate::analysis::storage_flow) buffer_cleanup_storage:
        BTreeMap<bray_symbols::TypeId, bray_bound_tree::BoundCallStorage>,
    pub(in crate::analysis::storage_flow) direct_observation_accesses:
        BTreeMap<StorageIdentityId, Vec<StorageAccessId>>,
    pub(in crate::analysis::storage_flow) indirect_observation_accesses: Vec<StorageAccessId>,
    pub(in crate::analysis::storage_flow) value_references: BTreeMap<
        StorageIdentityId,
        BTreeMap<crate::analysis::storage_flow::memory::Place, bray_symbols::TypeId>,
    >,
    pub(in crate::analysis::storage_flow) entry_references: BTreeMap<
        (crate::analysis::storage_flow::memory::Place, usize),
        (
            bray_bound_tree::StorageReferencePath,
            Option<bray_symbols::TypeId>,
            bray_symbols::DependencyReferenceKind,
        ),
    >,
    pub(in crate::analysis::storage_flow) owned_backings: BTreeMap<
        crate::analysis::storage_flow::memory::Place,
        (bray_bound_tree::StorageReferencePath, bray_symbols::TypeId),
    >,
    pub(in crate::analysis::storage_flow) entry_whole_sources: Vec<(StorageIdentityId, bray_bound_tree::StorageReferencePath)>,
    pub(in crate::analysis::storage_flow) reference_constructions: BTreeMap<
        BoundExpressionId,
        Vec<(
            bray_bound_tree::BoundExecutionSite,
            bray_symbols::TypeId,
            crate::analysis::storage_flow::memory::Place,
            bray_symbols::TypeId,
        )>,
    >,
    pub(in crate::analysis::storage_flow) suspension_borrows: BTreeSet<BorrowCapabilityId>,
}

impl StorageFlowInput {
    pub(in crate::analysis::storage_flow) fn entry_buffer_pointer(
        &self,
        path: &bray_bound_tree::StorageReferencePath,
        pointee: Option<bray_symbols::TypeId>,
    ) -> Option<crate::representation::RawBufferFields> {
        self.raw_buffers.values().find_map(|fields| {
            (pointee == Some(fields.element)
                && path.reads().all(|(_, index, _, _)| index < path.projections().len())
                && path.projections().last() == Some(&bray_bound_tree::StorageProjection::ProductField(fields.pointer)))
                .then_some(*fields)
        })
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "storage-flow preparation consumes independently checked type, dependency, cleanup and memory inputs"
    )]
    pub(crate) fn prepare<C>(
        cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        effects: &OperationEffects,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        types: &bray_bound_tree::CheckedExpressionTypes,
        cleanup: &crate::analysis::guarantee::CleanupPlan,
        memory: &bray_bound_tree::CheckedMemoryOperations,
    ) -> crate::CheckerQueryResult<(Self, bray_diagnostics::DiagnosticBag), C::UpstreamError>
    where
        C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
    {
        let (mutable_storage, diagnostics) =
            crate::analysis::storage_flow::authority::mutable_storage(request, storage)?;

        // Returned reference origins are needed even when a body has no memory effects.
        // Share the same type-shape preparation with mutation observations.
        let footprint = crate::storage::observation::prepare(request, storage, dependencies)?;

        let mut demands = BTreeMap::<_, BTreeSet<_>>::new();

        for (path, ty) in &footprint.references {
            let owner = storage
                .storage_type(path.root)
                .expect("prepared observation retains its storage type");

            demands
                .entry(request.semantic_values().unborrowed_type(owner))
                .or_default()
                .insert((std::sync::Arc::clone(&path.projections), *ty));
        }

        for (path, ty) in &footprint.whole_sources {
            let owner = storage.storage_type(path.root).expect("whole-value source retains its storage type");

            demands.entry(request.semantic_values().unborrowed_type(owner)).or_default()
                .insert((std::sync::Arc::clone(&path.projections), *ty));
        }

        let owned_descriptors = storage
            .identity_entries()
            .filter_map(|(root, identity)| {
                identity.is_invocation_owned(storage.kind()).then_some(root)
            })
            .map(bray_bound_tree::StoragePath::root);

        let mut opaque_descriptors = owned_descriptors.collect::<BTreeSet<_>>();
        let mut opaque_values = BTreeMap::new();

        for (path, kind) in footprint.owned_observations.values().flatten() {
            kind.record_preservation(path.clone(), &mut opaque_descriptors, &mut opaque_values);
        }

        let opaque_candidates = opaque_descriptors.iter().cloned()
            .chain(footprint.borrowed_observations.keys().cloned())
            .chain(
                footprint
                    .discriminants
                    .keys()
                    .copied()
                    .map(bray_bound_tree::StoragePath::root),
            )
            .chain(opaque_values.keys().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();

        let mut input = Self {
            footprint,
            opaque_descriptors,
            opaque_values,
            opaque_candidates,
            mutable_storage,
            ..Self::default()
        };

        input.prepare_locations(storage, cleanup);
        input.prepare_report_dependencies(request, storage, effects, dependencies, cleanup, selections);

        input.index_observation_accesses(request, storage);

        let lifecycle_demands =
            input.prepare_lifecycle_references(request, cleanup, dependencies)?;

        input.prepare_reference_constructions(request, storage, effects.value_inputs(), selections, types)?;

        for (ty, paths) in lifecycle_demands {
            demands.entry(ty).or_default().extend(paths);
        }

        let buffer_types = storage
            .identity_entries()
            .filter_map(|(identity, _)| storage.storage_type(identity))
            .chain(
                storage
                    .access_entries()
                    .map(|(_, access)| access.reached_type()),
            )
            .chain(cleanup.opaque_types())
            .chain(
                memory
                    .operations()
                    .iter()
                    .flat_map(|operation| {
                        operation
                            .arguments()
                            .iter()
                            .copied()
                            .chain([operation.expression()])
                    })
                    .filter_map(|expression| types.expression(expression).map(|ty| ty.ty())),
            );

        input.prepare_buffer_fields(request, buffer_types, &mut demands)?;

        let released_buffers = memory.operations().iter()
            .filter(|operation| crate::memory::execution::commits_buffer_cleanup(operation.kind()))
            .filter_map(|operation| types.expression(operation.arguments()[0]))
            .map(|ty| request.semantic_values().unborrowed_type(ty.ty()));

        input.prepare_buffer_cleanup(request, cleanup.opaque_types().chain(released_buffers), &mut demands)?;
        input.prepare_allocation_pointer(request, memory, types)?;

        input.prepare_effect_observations(request, storage, memory, dependencies)?;

        input.tag_preserving_assignments = Self::prepare_assignment_observations(
            &cleanup.requirements, request, storage, selections, &input.footprint,
        );

        input.prepare_entry_references(request, storage, selections, dependencies, demands)?;

        for (path, ty) in &input.footprint.whole_sources {
            let root = storage.storage_type(path.root).expect("whole-value source retains its entry type");

            if let Some((_, prepared)) = crate::storage::reference_path(request, root, std::sync::Arc::clone(&path.projections), Some(*ty))? {
                input.entry_whole_sources.push((path.root, prepared));
            }
        }

        input.prepare_terminal_ownership(cleanup_shapes, request, storage, dependencies, memory)?;

        input.immutable_field_accesses =
            crate::analysis::storage_flow::authority::immutable_field_accesses(
                request, storage, &input,
            )?;

        input.prepare_cleanup_retention(request, storage, effects, cleanup);
        input.cleanup_requirements = std::sync::Arc::clone(&cleanup.requirements);

        Ok((input, diagnostics))
    }

    fn prepare_effect_observations<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
        memory: &bray_bound_tree::CheckedMemoryOperations,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
        let mut atomic = self.footprint.prepare_stores(request, storage, memory)?;

        for call in dependencies.calls().values() {
            let effects = call.effects().iter()
                .chain(call.deferred_effects().into_iter().flatten())
                .chain(call.deferred_capture_cleanup().into_iter().flatten())
                .chain(call.deferred_result_cleanup().into_iter().flatten());

            atomic |= self.footprint.prepare_effects(request, effects, |subject| call.storage().subject(subject).map(|(ty, _)| ty))?;
        }

        for (callable, contract) in dependencies.lifecycle_dependencies() {
            atomic |= self.footprint.prepare_effects(request, contract.effects(),
                |subject| self.lifecycle_storage.get(callable)?.subject(subject).map(|(ty, _)| ty))?;
        }

        for (ty, effects) in &self.buffer_cleanup_effects {
            atomic |= self.footprint.prepare_effects(request, effects,
                |subject| self.buffer_cleanup_storage[ty].subject(subject).map(|(ty, _)| ty))?;
        }

        if !atomic {
            self.footprint.atomic = None;
        }

        Ok(())
    }

    fn prepare_assignment_observations<C: CheckerRequestContext + ?Sized>(
        cleanup: &crate::asynchronous::StorageCleanupRequirements,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        footprint: &crate::storage::observation::Footprint,
    ) -> BTreeMap<bray_bound_tree::BoundExecutionSite, bool> {
        let mut assignments = BTreeMap::<_, (bool, bool)>::new();

        for plan in storage
            .access_plans()
            .iter()
            .filter(|plan| plan.purpose() == StorageAccessPurpose::Assignment)
        {
            let builtin = matches!(request.view().expression(plan.expression()), Some(bray_bound_tree::BoundExpression::Assignment(assignment)) if assignment.operator() == bray_bound_tree::BoundAssignmentOperator::Assign)
                || matches!(selections.expression(plan.expression()), Some(bray_bound_tree::SemanticSelection::Operation(bray_bound_tree::SelectedOperation::CompoundAssignment(assignment))) if matches!(assignment.target(), bray_bound_tree::OperatorTarget::BuiltIn(_)));

            let ty = storage
                .access(plan.access())
                .expect("assignment retains its destination type")
                .reached_type();

            let content_only = footprint.tag_free_store(ty);

            let preserves = builtin
                && content_only.is_some()
                && cleanup.replacement(plan.access())
                    == bray_bound_tree::AsyncStorageCleanupRequirement::None;

            assignments
                .entry(plan.occurrence())
                .and_modify(|(tags, contents)| {
                    *tags &= preserves;
                    *contents &= content_only == Some(true);
                })
                .or_insert((preserves, content_only == Some(true)));
        }

        assignments
            .into_iter()
            .filter_map(|(site, (tags, contents))| tags.then_some((site, contents)))
            .collect()
    }

    pub(super) fn prepare_locations(
        &mut self,
        storage: &StoragePlan,
        cleanup: &crate::analysis::guarantee::CleanupPlan,
    ) {
        for plan in storage.access_plans().iter().copied().filter(|plan| {
            matches!(
                plan.occurrence(),
                bray_bound_tree::BoundExecutionSite::Node(_)
            ) && !matches!(
                storage
                    .root_identity(plan.access())
                    .and_then(|identity| storage.identity(identity)),
                Some(StorageIdentity::InvocationInput { .. })
            )
        }) {
            if let Some(place) = storage.logical_path(plan.access()).cloned() {
                self.expression_places
                    .entry(plan.expression())
                    .or_default()
                    .push(place);
            }
        }

        for (storage, identity) in storage.identity_entries() {
            let Some(node) = identity_definition_occurrence(identity)
            else {
                continue;
            };

            self.definitions.entry(node).or_default().push(storage);
        }

        for path in storage
            .access_entries()
            .filter_map(|(access, _)| storage.logical_path(access))
            .chain(cleanup.represented_paths())
            .chain(self.footprint.references.iter().map(|(path, _)| path))
        {
            for (depth, projection) in path.projections.iter().enumerate() {
                if matches!(
                    projection,
                    bray_bound_tree::StorageProjection::ActiveUnionPayloadField { .. }
                ) {
                    let parent = bray_bound_tree::StoragePath {
                        root: path.root,
                        projections: path.projections[..depth].into(),
                    };

                    let payload = bray_bound_tree::StoragePath {
                        root: path.root,
                        projections: path.projections[..=depth].into(),
                    };

                    self.variant_payloads
                        .entry(parent)
                        .or_default()
                        .insert(payload);
                }
            }
        }
    }

    fn prepare_cleanup_retention<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
        effects: &OperationEffects,
        cleanup: &crate::analysis::guarantee::CleanupPlan,
    ) {
        // Cleanup still uses retained loans after the last ordinary read of its owner.
        // Index established value retention, never physical referents or invocation inputs.
        let owns_cleanup = |requirement: &bray_bound_tree::AsyncStorageRequirement| {
            !requirement.transfers()
                && matches!(
                    requirement.cleanup(),
                    bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(_)
                )
        };

        for requirement in cleanup.requirements.entries().iter().filter(|row| owns_cleanup(row)) {
            let owner = requirement.identity();

            self.cleanup_dependency_owners
                .entry(BoundDependencySubject::Storage(owner))
                .or_default()
                .insert(owner);
        }

        let bindings = crate::storage::value_transfer_destinations(request, storage);

        let occurrences = bindings
            .iter()
            .flat_map(|(initializer, bindings)| {
                bindings.iter().map(|identity| (*identity, *initializer))
            })
            .chain(
                storage
                    .identity_entries()
                    .filter_map(|(identity, definition)| {
                        Some((identity, definition.definition_occurrence()?))
                    }),
            );

        for (owner, occurrence) in occurrences {
            if !cleanup.requirements.get(owner).is_some_and(owns_cleanup) {
                continue;
            }

            for subject in effects.retained_by(occurrence) {
                self.cleanup_dependency_owners
                    .entry(subject)
                    .or_default()
                    .insert(owner);
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn update_liveness(
        &mut self,
        storage: &StoragePlan,
        liveness: &Liveness,
    ) {
        self.suspension_borrows = liveness.live_across_suspensions().iter().filter_map(|entry| {
            match entry.subject() {
                BoundDependencySubject::BorrowCapability(borrow) => Some(borrow),
                _ => None,
            }
        }).collect();

        self.storage_last_uses.clear();
        self.unused_entry_storage.clear();

        if liveness.is_recovered() {
            return;
        }

        for last_use in liveness.last_uses() {
            if let bray_bound_tree::BoundDependencySubject::Storage(identity) = last_use.subject()
                && self.cleanup_requirements.ends_at_last_use(identity)
            {
                self.storage_last_uses
                    .entry(last_use.occurrence())
                    .or_default()
                    .push(identity);
            }
        }

        let entry_borrowed_storage = storage
            .borrow_capability_entries()
            .filter(|(_, capability)| capability.entry_binding().is_some())
            .filter_map(|(_, capability)| storage.root_identity(capability.access()))
            .collect::<BTreeSet<_>>();

        self.unused_entry_storage
            .extend(self.cleanup_requirements.early_retirement_slots().filter(|identity| {
                storage
                    .identity(*identity)
                    .is_some_and(|identity| identity.is_initialized_at_entry())
                    && liveness
                        .storage_live_at_entry()
                        .binary_search(identity)
                        .is_err()
                    && !entry_borrowed_storage.contains(identity)
            }));
    }

    pub(in crate::analysis::storage_flow) fn expression_places(
        &self,
        expression: BoundExpressionId,
    ) -> &[crate::analysis::storage_flow::memory::Place] {
        self.expression_places
            .get(&expression)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub(in crate::analysis::storage_flow) fn storage_last_uses(
        &self,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) -> &[StorageIdentityId] {
        self.storage_last_uses
            .get(&occurrence)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub(in crate::analysis::storage_flow) fn borrowed_access(
        &self,
        plan: StorageAccessPlan,
        storage: &StoragePlan,
    ) -> Option<StorageAccessId> {
        storage
            .borrow_for_plan(plan)
            .and_then(|borrow| storage.borrow_capability(borrow))
            .map(|borrow| {
                if borrow.occurrence() == Some(plan.occurrence()) {
                    borrow.access()
                } else {
                    // Reusing an input capability still accesses the reached field or element.
                    plan.access()
                }
            })
    }

    pub(in crate::analysis::storage_flow) fn definitions(
        &self,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) -> &[StorageIdentityId] {
        self.definitions
            .get(&occurrence)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub(in crate::analysis::storage_flow) fn mutation_authority_access(
        &self,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
        storage: &StoragePlan,
    ) -> Option<StorageAccessId> {
        match purpose {
            StorageAccessPurpose::Write | StorageAccessPurpose::Assignment => Some(plan.access()),
            StorageAccessPurpose::Borrow(BorrowKind::Mutable) => {
                self.borrowed_access(plan, storage)
            }
            StorageAccessPurpose::Read
            | StorageAccessPurpose::Initialize
            | StorageAccessPurpose::Copy
            | StorageAccessPurpose::Move
            | StorageAccessPurpose::Borrow(BorrowKind::Shared)
            | StorageAccessPurpose::ValueTransfer
            | StorageAccessPurpose::Member
            | StorageAccessPurpose::Index
            | StorageAccessPurpose::Slice
            | StorageAccessPurpose::Projection => None,
        }
    }

    pub(in crate::analysis::storage_flow) fn fields_allow_mutation(
        &self,
        access: StorageAccessId,
    ) -> bool {
        !self.immutable_field_accesses.contains(&access)
    }

    pub(in crate::analysis::storage_flow) fn storage_is_mutable(
        &self,
        storage: StorageIdentityId,
    ) -> bool {
        self.mutable_storage.contains(&storage)
    }
}

fn identity_definition_occurrence(
    identity: StorageIdentity,
) -> Option<bray_bound_tree::BoundExecutionSite> {
    match identity {
        StorageIdentity::Result(_) => None,
        _ => identity.definition_occurrence(),
    }
}
