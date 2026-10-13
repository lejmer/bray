use crate::storage::local_initialization_destinations;
use crate::{CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{
    AnyBoundNodeId, BorrowCapabilityId, BoundDependencySubject, BoundExpressionId, Liveness,
    StorageAccessId, StorageAccessPlan, StorageAccessPurpose, StorageIdentity, StorageIdentityId,
    StoragePlan,
};
use bray_symbols::BorrowKind;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Default)]
pub(in crate::analysis::storage_flow) struct StorageFlowInput {
    expression_places:
        BTreeMap<BoundExpressionId, Vec<crate::analysis::storage_flow::memory::Place>>,
    definitions: BTreeMap<AnyBoundNodeId, Vec<StorageIdentityId>>,
    initialization_destinations: BTreeMap<BoundExpressionId, StorageIdentityId>,
    mutable_storage: BTreeSet<StorageIdentityId>,
    immutable_field_accesses: BTreeSet<StorageAccessId>,
    storage_last_uses: BTreeMap<bray_bound_tree::BoundExecutionSite, Vec<StorageIdentityId>>,
    pub(super) unused_entry_storage: BTreeSet<StorageIdentityId>,
    pub(super) cleanup_free_storage: BTreeSet<StorageIdentityId>,
    pub(in crate::analysis::storage_flow) lifecycle_paths: BTreeMap<
        (
            bray_symbols::CallableInstanceData,
            bray_symbols::DependencySubject,
        ),
        bray_bound_tree::StorageReferencePath,
    >,
    pub(in crate::analysis::storage_flow) lifecycle_references:
        BTreeMap<bray_symbols::CallableInstanceData, Vec<bray_bound_tree::BoundCallReference>>,
    pub(in crate::analysis::storage_flow) reference_paths:
        BTreeMap<StorageAccessId, bray_bound_tree::StorageReferencePath>,
    pub(in crate::analysis::storage_flow) entry_references: BTreeMap<
        crate::analysis::storage_flow::memory::Place,
        (
            bray_bound_tree::StorageReferencePath,
            Option<bray_symbols::TypeId>,
        ),
    >,
    pub(in crate::analysis::storage_flow) reference_constructions: BTreeMap<
        BoundExpressionId,
        Vec<(
            BoundExpressionId,
            crate::analysis::storage_flow::memory::Place,
            bray_symbols::TypeId,
        )>,
    >,
    pub(in crate::analysis::storage_flow) retained_storage: BTreeSet<StorageAccessId>,
    pub(in crate::analysis::storage_flow) suspension_borrows: BTreeSet<BorrowCapabilityId>,
}

impl StorageFlowInput {
    pub(in crate::analysis::storage_flow) fn new<C>(
        cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
        mutable_storage: BTreeSet<StorageIdentityId>,
        liveness: &Liveness,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        values: &crate::dependency::ValueInputs,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        cleanup: &crate::analysis::guarantee::CleanupPlan,
    ) -> crate::CheckerQueryResult<(Self, bray_diagnostics::DiagnosticBag), C::UpstreamError>
    where
        C: CheckerRequestContext + ?Sized,
    {
        let initialization_destinations = local_initialization_destinations(request, storage);

        let mut retained_storage = BTreeSet::new();

        for call in dependencies.calls().values() {
            collect_retained_storage(call.result().requirements(), &mut retained_storage);
        }

        let mut input = Self {
            retained_storage,
            initialization_destinations,
            mutable_storage,
            suspension_borrows: liveness
                .live_across_suspensions()
                .iter()
                .filter_map(|entry| match entry.subject() {
                    bray_bound_tree::BoundDependencySubject::BorrowCapability(borrow) => {
                        Some(borrow)
                    }
                    _ => None,
                })
                .collect(),
            ..Self::default()
        };

        for plan in storage.access_plans().iter().copied() {
            if let Some(place) = storage.logical_path(plan.access()).cloned() {
                input
                    .expression_places
                    .entry(plan.expression())
                    .or_default()
                    .push(place);
            }
        }

        // A simple local receives its value when the initializer completes. Its
        // storage incarnation must begin there, before the value is installed.
        let initialized_at = input
            .initialization_destinations
            .iter()
            .map(|(expression, identity)| (*identity, AnyBoundNodeId::Expression(*expression)))
            .collect::<BTreeMap<_, _>>();

        for (storage, identity) in storage.identity_entries() {
            let Some(node) = initialized_at
                .get(&storage)
                .copied()
                .or_else(|| identity_definition_node(identity))
            else {
                continue;
            };

            input.definitions.entry(node).or_default().push(storage);
        }

        input.prepare_reference_constructions(request, storage, values)?;
        input.prepare_reference_paths(request, storage)?;
        input.prepare_summary_references(request, storage, selections, dependencies)?;
        input.prepare_lifecycle_references(request, cleanup, dependencies)?;

        input.immutable_field_accesses =
            crate::analysis::storage_flow::authority::immutable_field_accesses(
                request, storage, &input,
            )?;

        let (cleanup_free, diagnostics) =
            crate::asynchronous::cleanup_free_storage(cleanup_shapes, request, storage)?;

        input.prepare_storage_retirement(storage, liveness, &cleanup_free);
        input.cleanup_free_storage = cleanup_free;

        Ok((input, diagnostics))
    }

    fn prepare_storage_retirement(
        &mut self,
        storage: &StoragePlan,
        liveness: &Liveness,
        cleanup_free: &BTreeSet<StorageIdentityId>,
    ) {
        if liveness.is_recovered() {
            return;
        }

        for last_use in liveness.last_uses() {
            if let bray_bound_tree::BoundDependencySubject::Storage(identity) = last_use.subject()
                && cleanup_free.contains(&identity)
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
            .extend(cleanup_free.iter().copied().filter(|identity| {
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
                if borrow.expression() == Some(plan.expression()) {
                    borrow.access()
                } else {
                    // Reusing an input capability still accesses the reached field or element.
                    plan.access()
                }
            })
    }

    pub(in crate::analysis::storage_flow) fn definitions(
        &self,
        node: AnyBoundNodeId,
    ) -> &[StorageIdentityId] {
        self.definitions
            .get(&node)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub(in crate::analysis::storage_flow) fn initialization_destination(
        &self,
        expression: BoundExpressionId,
    ) -> Option<StorageIdentityId> {
        self.initialization_destinations.get(&expression).copied()
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

fn identity_definition_node(identity: StorageIdentity) -> Option<AnyBoundNodeId> {
    match identity {
        StorageIdentity::Result(_) => None,
        _ => identity.definition_node(),
    }
}

fn collect_retained_storage(
    requirements: &[bray_bound_tree::BoundDependencyRequirement],
    output: &mut BTreeSet<StorageAccessId>,
) {
    for requirement in requirements {
        match requirement {
            bray_bound_tree::BoundDependencyRequirement::Direct {
                subject: BoundDependencySubject::StorageAccess(access),
                kind,
            } if *kind != bray_bound_tree::BoundDependencyRequirementKind::ValueDependencies => {
                output.insert(*access);
            }
            bray_bound_tree::BoundDependencyRequirement::Guarded(guard) => {
                collect_retained_storage(guard.requirements(), output)
            }
            _ => {}
        }
    }
}
