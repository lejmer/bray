use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BorrowCapabilityId, BoundDependencySubject, BoundExpressionId,
    CheckedMemoryOperations, CheckedRefinements, Liveness, StorageAccessId, StorageAccessPlan,
    StorageAccessPurpose, StorageIdentity, StorageIdentityId, StoragePlan,
};
use bray_symbols::{BorrowKind, TypeId};

use crate::storage::{StorageScopeOwners, local_initialization_destinations};
use crate::{CheckerRequestContext, CheckerUnitView};

use super::super::fixed_point::{FixedPointDomain, FlowDirection};
use super::super::id::AnalysisEdgeId;
use super::super::model::{AnalysisBlock, AnalysisEdge, AnalysisEdgeKind, ControlFlowGraph};
use super::super::reachability::ReachabilityResult;
use super::check::StorageFlowCollector;

#[derive(Debug, Default)]
pub(super) struct StorageFlowInput {
    expression_roots: BTreeMap<BoundExpressionId, Vec<StorageIdentityId>>,
    borrows: BTreeMap<StorageAccessPlan, BorrowCapabilityId>,
    definitions: BTreeMap<AnyBoundNodeId, Vec<StorageIdentityId>>,
    initialization_destinations: BTreeMap<BoundExpressionId, StorageIdentityId>,
    copyable_types: BTreeSet<TypeId>,
    mutable_storage: BTreeSet<StorageIdentityId>,
    immutable_field_accesses: BTreeSet<StorageAccessId>,
    storage_last_uses: BTreeMap<bray_bound_tree::BoundExecutionSite, Vec<StorageIdentityId>>,
    unused_entry_storage: BTreeSet<StorageIdentityId>,
    cleanup_free_storage: BTreeSet<StorageIdentityId>,
    pub(super) suspension_borrows: BTreeSet<BorrowCapabilityId>,
}

impl StorageFlowInput {
    pub(super) fn new<C>(
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
        copyable_types: BTreeSet<TypeId>,
        mutable_storage: BTreeSet<StorageIdentityId>,
        liveness: &Liveness,
    ) -> crate::CheckerQueryResult<(Self, bray_diagnostics::DiagnosticBag), C::UpstreamError>
    where
        C: CheckerRequestContext + ?Sized,
    {
        let initialization_destinations = local_initialization_destinations(request, storage);

        let mut input = Self {
            copyable_types,
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

        let planned_borrows = storage
            .borrow_capability_entries()
            .filter_map(|(id, capability)| {
                capability
                    .expression()
                    .map(|expression| ((expression, capability.kind(), capability.access()), id))
            })
            .collect::<BTreeMap<_, _>>();

        for plan in storage.access_plans().iter().copied() {
            if let Some(root) = storage.root_identity(plan.access()) {
                input
                    .expression_roots
                    .entry(plan.expression())
                    .or_default()
                    .push(root);
            }

            let StorageAccessPurpose::Borrow(kind) = plan.purpose() else {
                continue;
            };

            let direct = storage
                .access(plan.access())
                .and_then(|access| access.root().borrow_capability());

            let capability = planned_borrows
                .get(&(plan.expression(), kind, plan.access()))
                .copied()
                .or_else(|| {
                    direct.filter(|capability| {
                        storage
                            .borrow_capability(*capability)
                            .is_some_and(|capability| capability.kind() == kind)
                    })
                });

            if let Some(capability) = capability {
                input.borrows.insert(plan, capability);
            }
        }

        for (storage, identity) in storage.identity_entries() {
            let Some(node) = identity_definition_node(identity) else {
                continue;
            };

            input.definitions.entry(node).or_default().push(storage);
        }

        input.immutable_field_accesses =
            super::authority::immutable_field_accesses(request, storage, &input)?;

        let (cleanup_free, diagnostics) =
            crate::asynchronous::cleanup_free_storage(request, storage)?;

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

    pub(super) fn expression_roots(&self, expression: BoundExpressionId) -> &[StorageIdentityId] {
        self.expression_roots
            .get(&expression)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub(super) fn storage_last_uses(
        &self,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) -> &[StorageIdentityId] {
        self.storage_last_uses
            .get(&occurrence)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub(super) fn borrow(&self, plan: StorageAccessPlan) -> Option<BorrowCapabilityId> {
        self.borrows.get(&plan).copied()
    }

    pub(super) fn definitions(&self, node: AnyBoundNodeId) -> &[StorageIdentityId] {
        self.definitions
            .get(&node)
            .map(Vec::as_slice)
            .unwrap_or_default()
    }

    pub(super) fn initialization_destination(
        &self,
        expression: BoundExpressionId,
    ) -> Option<StorageIdentityId> {
        self.initialization_destinations.get(&expression).copied()
    }

    pub(super) fn type_is_copyable(&self, ty: TypeId) -> bool {
        self.copyable_types.contains(&ty)
    }

    pub(super) fn mutation_authority_access(
        &self,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
        storage: &StoragePlan,
    ) -> Option<StorageAccessId> {
        match purpose {
            StorageAccessPurpose::Write | StorageAccessPurpose::Assignment => Some(plan.access()),
            StorageAccessPurpose::Borrow(BorrowKind::Mutable) => self
                .borrow(plan)
                .and_then(|borrow| storage.borrow_capability(borrow))
                .map(|borrow| borrow.access()),
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

    pub(super) fn fields_allow_mutation(&self, access: StorageAccessId) -> bool {
        !self.immutable_field_accesses.contains(&access)
    }

    pub(super) fn storage_is_mutable(&self, storage: StorageIdentityId) -> bool {
        self.mutable_storage.contains(&storage)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct StorageFlowState {
    pub(super) reachable: bool,
    pub(super) live: BTreeSet<StorageIdentityId>,
    pub(super) initialized: BTreeSet<StorageIdentityId>,
    pub(super) observed_pattern_bindings: BTreeSet<StorageIdentityId>,
    pub(super) moved: BTreeMap<StorageAccessId, BoundExpressionId>,
    pub(super) definitely_moved: BTreeSet<StorageAccessId>,
    pub(super) fully_moved: BTreeSet<StorageIdentityId>,
    pub(super) active_borrows: BTreeSet<BorrowCapabilityId>,
    pub(super) definitely_active_borrows: BTreeSet<BorrowCapabilityId>,
    pub(super) raw_initialized:
        BTreeMap<StorageIdentityId, BTreeMap<TypeId, BTreeSet<BoundExpressionId>>>,
    pub(super) active_allocations: BTreeMap<StorageIdentityId, BTreeSet<BoundExpressionId>>,
    pub(super) allocation_origins: BTreeMap<StorageIdentityId, BTreeSet<BoundExpressionId>>,
    pub(super) invalidated_allocations: BTreeMap<StorageIdentityId, BTreeSet<BoundExpressionId>>,
    pub(super) recovered: bool,
}

impl StorageFlowState {
    pub(super) fn end_storage_lifetime(
        &mut self,
        identity: StorageIdentityId,
        storage: &StoragePlan,
    ) {
        if self.recovered
            || self.active_borrows.iter().any(|borrow| {
                storage
                    .borrow_capability(*borrow)
                    .is_some_and(|borrow| storage.root_identity(borrow.access()) == Some(identity))
            })
        {
            return;
        }

        self.forget_storage(identity, storage);
    }

    pub(super) fn end_borrows(
        &mut self,
        ended: &BTreeSet<BorrowCapabilityId>,
        storage: &StoragePlan,
    ) {
        self.active_borrows.retain(|borrow| !ended.contains(borrow));

        self.definitely_active_borrows
            .retain(|borrow| !ended.contains(borrow));

        self.moved.retain(|access, _| {
            storage.access(*access).is_none_or(|access| {
                access
                    .root()
                    .borrow_capability()
                    .is_none_or(|borrow| !ended.contains(&borrow))
            })
        });

        self.retain_definite_moves(storage);
    }

    pub(super) fn retain_definite_moves(&mut self, storage: &StoragePlan) {
        self.definitely_moved.retain(|definite| {
            self.moved.keys().any(|possible| {
                storage.relationship(*possible, *definite)
                    == bray_bound_tree::StorageRelationship::Identical
            })
        });
    }

    fn entry(storage: &StoragePlan, input: &StorageFlowInput) -> Self {
        let initialized = storage
            .identity_entries()
            .filter_map(|(id, identity)| {
                (identity.is_initialized_at_entry() && !input.unused_entry_storage.contains(&id))
                    .then_some(id)
            })
            .collect::<BTreeSet<_>>();

        let active_borrows = storage
            .borrow_capability_entries()
            .filter_map(|(id, capability)| capability.entry_binding().is_some().then_some(id))
            .collect::<BTreeSet<_>>();

        Self {
            reachable: true,
            live: initialized.clone(),
            initialized,
            observed_pattern_bindings: BTreeSet::new(),
            moved: BTreeMap::new(),
            definitely_moved: BTreeSet::new(),
            fully_moved: BTreeSet::new(),
            definitely_active_borrows: active_borrows.clone(),
            active_borrows,
            raw_initialized: BTreeMap::new(),
            active_allocations: BTreeMap::new(),
            allocation_origins: BTreeMap::new(),
            invalidated_allocations: BTreeMap::new(),
            recovered: false,
        }
    }

    pub(super) fn merge(&mut self, incoming: &Self) -> bool {
        if !incoming.reachable {
            return false;
        }

        if !self.reachable {
            // Each block owns its task-local state after propagation.
            *self = incoming.clone();

            return true;
        }

        let live_count = self.live.len();
        let initialized_count = self.initialized.len();
        let observed_count = self.observed_pattern_bindings.len();
        let moved_count = self.moved.len();
        let definite_move_count = self.definitely_moved.len();
        let fully_moved_count = self.fully_moved.len();
        let borrow_count = self.active_borrows.len();
        let definite_borrow_count = self.definitely_active_borrows.len();
        let raw_storage_count = self.raw_initialized.len();

        let raw_initialized_count = self
            .raw_initialized
            .values()
            .map(BTreeMap::len)
            .sum::<usize>();

        let active_allocation_count = self.active_allocations.len();
        let allocation_origin_count = self.allocation_origins.len();
        let invalidated_allocation_count = self.invalidated_allocations.len();
        let was_recovered = self.recovered;

        self.live.extend(incoming.live.iter().copied());

        self.initialized
            .retain(|storage| incoming.initialized.contains(storage));

        self.observed_pattern_bindings
            .retain(|storage| incoming.observed_pattern_bindings.contains(storage));

        let mut moved_changed = false;

        for (&access, &origin) in &incoming.moved {
            match self.moved.get_mut(&access) {
                Some(current) if origin < *current => {
                    *current = origin;
                    moved_changed = true;
                }
                Some(_) => {}
                None => {
                    self.moved.insert(access, origin);
                    moved_changed = true;
                }
            }
        }

        self.definitely_moved
            .retain(|access| incoming.definitely_moved.contains(access));

        self.fully_moved
            .retain(|storage| incoming.fully_moved.contains(storage));

        self.active_borrows
            .extend(incoming.active_borrows.iter().copied());

        self.definitely_active_borrows
            .retain(|borrow| incoming.definitely_active_borrows.contains(borrow));

        let mut memory_origins_changed = false;

        self.raw_initialized.retain(|storage, initialized| {
            let Some(incoming) = incoming.raw_initialized.get(storage) else {
                return false;
            };

            initialized.retain(|ty, origins| {
                let Some(incoming_origins) = incoming.get(ty) else {
                    return false;
                };

                let count = origins.len();

                origins.extend(incoming_origins.iter().copied());
                memory_origins_changed |= origins.len() != count;

                true
            });

            !initialized.is_empty()
        });

        self.active_allocations.retain(|storage, origins| {
            let Some(incoming_origins) = incoming.active_allocations.get(storage) else {
                return false;
            };

            let count = origins.len();

            origins.extend(incoming_origins.iter().copied());
            memory_origins_changed |= origins.len() != count;

            true
        });

        for (&storage, incoming_origins) in &incoming.allocation_origins {
            let origins = self.allocation_origins.entry(storage).or_default();
            let count = origins.len();

            origins.extend(incoming_origins.iter().copied());
            memory_origins_changed |= origins.len() != count;
        }

        for (&storage, incoming_origins) in &incoming.invalidated_allocations {
            let origins = self.invalidated_allocations.entry(storage).or_default();
            let count = origins.len();

            origins.extend(incoming_origins.iter().copied());
            memory_origins_changed |= origins.len() != count;
        }

        self.recovered |= incoming.recovered;

        self.live.len() != live_count
            || self.initialized.len() != initialized_count
            || self.observed_pattern_bindings.len() != observed_count
            || self.moved.len() != moved_count
            || moved_changed
            || self.definitely_moved.len() != definite_move_count
            || self.fully_moved.len() != fully_moved_count
            || self.active_borrows.len() != borrow_count
            || self.definitely_active_borrows.len() != definite_borrow_count
            || self.raw_initialized.len() != raw_storage_count
            || self
                .raw_initialized
                .values()
                .map(BTreeMap::len)
                .sum::<usize>()
                != raw_initialized_count
            || self.active_allocations.len() != active_allocation_count
            || self.allocation_origins.len() != allocation_origin_count
            || self.invalidated_allocations.len() != invalidated_allocation_count
            || memory_origins_changed
            || self.recovered != was_recovered
    }

    pub(super) fn forget_storage(&mut self, identity: StorageIdentityId, storage: &StoragePlan) {
        self.live.remove(&identity);
        self.initialized.remove(&identity);
        self.fully_moved.remove(&identity);
        self.observed_pattern_bindings.remove(&identity);
        self.raw_initialized.remove(&identity);
        self.active_allocations.remove(&identity);
        self.allocation_origins.remove(&identity);
        self.invalidated_allocations.remove(&identity);

        self.moved
            .retain(|access, _| storage.root_identity(*access) != Some(identity));

        self.retain_definite_moves(storage);
    }

    pub(super) fn retain_live_storage(&mut self, storage: &StoragePlan) {
        self.initialized
            .retain(|identity| self.live.contains(identity));

        self.observed_pattern_bindings
            .retain(|identity| self.live.contains(identity));

        self.fully_moved
            .retain(|identity| self.live.contains(identity));

        self.raw_initialized
            .retain(|identity, _| self.live.contains(identity));

        self.active_allocations
            .retain(|identity, _| self.live.contains(identity));

        self.allocation_origins
            .retain(|identity, _| self.live.contains(identity));

        self.invalidated_allocations
            .retain(|identity, _| self.live.contains(identity));

        self.moved.retain(|access, _| {
            storage
                .root_identity(*access)
                .is_some_and(|identity| self.live.contains(&identity))
        });

        self.retain_definite_moves(storage);
    }

    pub(super) fn move_complete_storage(&mut self, storage: StorageIdentityId) {
        self.fully_moved.insert(storage);
        self.initialized.remove(&storage);
    }
}

pub(super) struct StorageFlowDomain<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    graph: &'analysis ControlFlowGraph,
    edge_lifetime_ends: &'analysis BTreeMap<AnalysisEdgeId, Vec<BoundDependencySubject>>,
    reachability: &'analysis ReachabilityResult,
    storage: &'analysis StoragePlan,
    liveness: &'analysis Liveness,
    refinements: &'analysis CheckedRefinements,
    memory: &'analysis CheckedMemoryOperations,
    input: &'analysis StorageFlowInput,
    owners: &'analysis StorageScopeOwners,
    request: CheckerUnitView<'analysis, C>,
}

impl<'analysis, C> StorageFlowDomain<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    #[expect(
        clippy::too_many_arguments,
        reason = "the domain borrows each independently validated analysis input"
    )]
    pub(super) fn new(
        graph: &'analysis ControlFlowGraph,
        edge_lifetime_ends: &'analysis BTreeMap<AnalysisEdgeId, Vec<BoundDependencySubject>>,
        reachability: &'analysis ReachabilityResult,
        storage: &'analysis StoragePlan,
        liveness: &'analysis Liveness,
        refinements: &'analysis CheckedRefinements,
        memory: &'analysis CheckedMemoryOperations,
        input: &'analysis StorageFlowInput,
        owners: &'analysis StorageScopeOwners,
        request: CheckerUnitView<'analysis, C>,
    ) -> Self {
        Self {
            graph,
            edge_lifetime_ends,
            reachability,
            storage,
            liveness,
            refinements,
            memory,
            input,
            owners,
            request,
        }
    }

    fn transfer_block(&self, block: &AnalysisBlock, source: &StorageFlowState) -> StorageFlowState {
        // Transfer owns an independent task-local successor state.
        let mut state = source.clone();

        let mut collector = StorageFlowCollector::without_publication(
            self.request,
            self.storage,
            self.liveness,
            self.refinements,
            self.memory,
            self.input,
            self.owners,
        );

        for operation in block.operations() {
            let Some(operation) = self.graph.operation(*operation) else {
                continue;
            };

            collector.apply_operation(&mut state, operation);
        }

        state
    }
}

impl<C> FixedPointDomain for StorageFlowDomain<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    type State = StorageFlowState;

    fn direction(&self) -> FlowDirection {
        FlowDirection::Forward
    }

    fn bottom(&self) -> Self::State {
        StorageFlowState::default()
    }

    fn boundary(&self) -> Self::State {
        StorageFlowState::entry(self.storage, self.input)
    }

    fn merge_boundary(&self, target: &mut Self::State, boundary: &Self::State) -> bool {
        target.merge(boundary)
    }

    fn transfer(&self, block: &AnalysisBlock, source: &Self::State) -> Self::State {
        self.transfer_block(block, source)
    }

    fn propagate(
        &self,
        source: &Self::State,
        edge: &AnalysisEdge,
        target: &mut Self::State,
    ) -> bool {
        if !self.reachability.is_edge_reachable(edge.id()) {
            return false;
        }

        let ends = self
            .edge_lifetime_ends
            .get(&edge.id())
            .map(Vec::as_slice)
            .unwrap_or_default();

        if edge.kind() == AnalysisEdgeKind::Recovery {
            let mut incoming = source.clone();

            incoming.recovered = true;

            return target.merge(&incoming);
        }

        if !ends.is_empty() && !source.recovered && !self.liveness.is_recovered() {
            // Each outgoing continuation owns independently retired availability facts.
            let mut incoming = source.clone();

            let ended_borrows = ends
                .iter()
                .filter_map(|end| {
                    let bray_bound_tree::BoundDependencySubject::BorrowCapability(id) = *end else {
                        return None;
                    };

                    (self
                        .storage
                        .borrow_capability(id)
                        .is_some_and(|borrow| borrow.entry_binding().is_none()))
                    .then_some(id)
                })
                .collect::<BTreeSet<_>>();

            if !ended_borrows.is_empty() {
                incoming.end_borrows(&ended_borrows, self.storage);
            }

            for end in ends {
                if let bray_bound_tree::BoundDependencySubject::Storage(id) = *end
                    && self.input.cleanup_free_storage.contains(&id)
                {
                    incoming.end_storage_lifetime(id, self.storage);
                }
            }

            return target.merge(&incoming);
        }

        target.merge(source)
    }

    fn convergence_bound(&self, graph: &ControlFlowGraph) -> usize {
        let domain_size = self
            .storage
            .identities()
            .len()
            .saturating_add(self.storage.accesses().len().saturating_mul(2))
            .saturating_add(self.storage.borrow_capabilities().len())
            .saturating_add(self.storage.identities().len().saturating_mul(3))
            .saturating_add(2);

        graph.blocks().len().saturating_mul(domain_size)
    }
}

fn identity_definition_node(identity: StorageIdentity) -> Option<AnyBoundNodeId> {
    match identity {
        StorageIdentity::Result(_) => None,
        _ => identity.definition_node(),
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundErrorExpression, BoundExpression, BoundUnitId, BoundUnitKind, StorageIdentity,
        StoragePlanBuilder,
    };

    use super::StorageFlowState;
    use crate::test_support::{error_type, expression_unit, push_expression};

    #[test]
    fn raw_expression_indexes_preserve_first_result_and_later_alias_order() {
        use crate::CheckerUnitView;
        use crate::test_support::{TestCheckerContext, callable_entry};
        use bray_bound_tree::{Liveness, StorageAccess, StorageAccessPurpose, StorageAccessRoot};
        use std::collections::BTreeSet;

        let (unit, expressions) = expression_unit(BoundUnitId::new(92), |tree, origin| {
            (0..2)
                .map(|_| {
                    push_expression(
                        tree,
                        BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
                    )
                })
                .collect()
        });

        let mut builder = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

        let first = builder
            .push_identity(StorageIdentity::Temporary(expressions[0]))
            .expect("first result must build");

        let second = builder
            .push_identity(StorageIdentity::Allocation(expressions[0]))
            .expect("second result must build");

        let source = unit
            .tree()
            .expression(expressions[1])
            .expect("test expression must exist")
            .origin()
            .source_anchor();

        for (identity, recovered) in [(first, true), (first, false), (second, false)] {
            let access = builder
                .push_access(StorageAccess::new(
                    StorageAccessRoot::Storage(identity),
                    [],
                    error_type(),
                    source,
                    recovered,
                ))
                .expect("test access must build");

            builder
                .plan_access(
                    expressions[1].into(),
                    expressions[1],
                    StorageAccessPurpose::Read,
                    access,
                )
                .expect("test access plan must build");
        }

        let storage = builder.finish();

        let liveness = Liveness::try_new(unit.unit(), unit.key().kind(), [], [], [], [], [], false)
            .expect("test liveness must build");

        let context = TestCheckerContext::new(false);
        let semantic_context = callable_entry(unit.key());
        let request = CheckerUnitView::new(&unit, &semantic_context, &context);

        let (input, _) = super::StorageFlowInput::new(
            request,
            &storage,
            BTreeSet::new(),
            BTreeSet::new(),
            &liveness,
        )
        .expect("test storage indexes must build");

        assert_eq!(input.definitions(expressions[0].into()), &[first, second]);
        assert_eq!(input.expression_roots(expressions[1]), &[first, second]);

        let mut state = reachable_state();

        state
            .allocation_origins
            .insert(second, BTreeSet::from([expressions[0]]));

        assert_eq!(
            input
                .expression_roots(expressions[1])
                .iter()
                .copied()
                .find(|identity| state.allocation_origins.contains_key(identity)),
            Some(second)
        );
    }

    #[test]
    fn guard_observations_do_not_create_ownership_and_join_by_intersection() {
        let (identity, _) = storage_and_expressions(78);

        let mut observed = reachable_state();

        observed.observed_pattern_bindings.insert(identity);

        assert!(observed.live.is_empty());
        assert!(observed.initialized.is_empty());

        let mut merged = observed.clone();

        assert!(!merged.merge(&observed));
        assert!(merged.observed_pattern_bindings.contains(&identity));
        assert!(merged.merge(&reachable_state()));
        assert!(merged.observed_pattern_bindings.is_empty());
        assert!(!merged.merge(&observed));
    }

    #[test]
    fn raw_memory_merge_preserves_conservative_state_and_branch_origin() {
        let (identity, expressions) = storage_and_expressions(77);

        let ty = error_type();
        let mut left = reachable_state();

        left.raw_initialized
            .entry(identity)
            .or_default()
            .entry(ty)
            .or_default()
            .insert(expressions[1]);

        left.active_allocations
            .entry(identity)
            .or_default()
            .insert(expressions[0]);

        assert!(left.merge(&reachable_state()));
        assert!(!left.raw_initialized.contains_key(&identity));
        assert!(!left.active_allocations.contains_key(&identity));

        let mut invalidated = reachable_state();

        invalidated
            .invalidated_allocations
            .entry(identity)
            .or_default()
            .insert(expressions[2]);

        assert!(left.merge(&invalidated));

        assert_eq!(
            left.invalidated_allocations[&identity],
            [expressions[2]].into_iter().collect()
        );
    }

    #[test]
    fn raw_memory_merge_unions_and_deduplicates_all_causative_origins() {
        let (identity, expressions) = storage_and_expressions(78);

        let ty = error_type();
        let mut merged = memory_state(identity, ty, expressions[0], expressions[1]);
        let incoming = memory_state(identity, ty, expressions[2], expressions[1]);

        assert!(merged.merge(&incoming));

        assert_eq!(
            merged.raw_initialized[&identity][&ty],
            [expressions[0], expressions[2]].into_iter().collect()
        );

        assert_eq!(
            merged.invalidated_allocations[&identity],
            [expressions[1]].into_iter().collect()
        );

        assert_eq!(
            merged.allocation_origins[&identity],
            [expressions[0], expressions[2]].into_iter().collect()
        );
    }

    #[test]
    fn fully_moved_storage_survives_only_when_every_incoming_path_moves_it() {
        let (identity, _) = storage_and_expressions(79);

        let mut left = reachable_state();
        let mut moved = reachable_state();

        left.fully_moved.insert(identity);
        moved.fully_moved.insert(identity);

        assert!(!left.merge(&moved));
        assert_eq!(left.fully_moved, [identity].into_iter().collect());

        assert!(left.merge(&reachable_state()));
        assert!(left.fully_moved.is_empty());
    }

    #[test]
    fn complete_moves_end_definite_initialization() {
        let (identity, _) = storage_and_expressions(81);

        let mut state = reachable_state();

        state.initialized.insert(identity);
        state.move_complete_storage(identity);

        assert_eq!(state.fully_moved, [identity].into_iter().collect());
        assert!(state.initialized.is_empty());
    }

    #[test]
    fn control_flow_joins_retain_maybe_live_storage_and_intersect_initialization() {
        let (identity, _) = storage_and_expressions(80);

        let mut initialized = reachable_state();

        initialized.live.insert(identity);
        initialized.initialized.insert(identity);

        assert!(initialized.merge(&reachable_state()));
        assert_eq!(initialized.live, [identity].into_iter().collect());
        assert!(initialized.initialized.is_empty());
    }

    fn reachable_state() -> StorageFlowState {
        StorageFlowState {
            reachable: true,
            ..StorageFlowState::default()
        }
    }

    fn memory_state(
        identity: bray_bound_tree::StorageIdentityId,
        ty: bray_symbols::TypeId,
        initialization: bray_bound_tree::BoundExpressionId,
        invalidation: bray_bound_tree::BoundExpressionId,
    ) -> StorageFlowState {
        let mut state = reachable_state();

        state
            .raw_initialized
            .entry(identity)
            .or_default()
            .entry(ty)
            .or_default()
            .insert(initialization);

        state
            .invalidated_allocations
            .entry(identity)
            .or_default()
            .insert(invalidation);

        state
            .allocation_origins
            .entry(identity)
            .or_default()
            .insert(initialization);

        state
    }

    fn storage_and_expressions(
        raw_unit: u32,
    ) -> (
        bray_bound_tree::StorageIdentityId,
        Vec<bray_bound_tree::BoundExpressionId>,
    ) {
        let unit = BoundUnitId::new(raw_unit);

        let (_, expressions) = expression_unit(unit, |tree, origin| {
            (0..3)
                .map(|_| {
                    push_expression(
                        tree,
                        BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
                    )
                })
                .collect::<Vec<_>>()
        });

        let mut storage = StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);

        let identity = storage
            .push_identity(StorageIdentity::Allocation(expressions[0]))
            .unwrap_or_else(|error| panic!("allocation identity must validate: {error:?}"));

        (identity, expressions)
    }
}
