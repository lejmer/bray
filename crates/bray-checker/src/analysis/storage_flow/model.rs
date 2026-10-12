use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BorrowCapabilityId, BoundDependencySubject, BoundExpressionId,
    CheckedMemoryOperations, Liveness, StorageAccessId, StorageAccessPlan, StorageAccessPurpose,
    StorageIdentity, StorageIdentityId, StoragePlan,
};
use bray_symbols::BorrowKind;

use crate::storage::{StorageScopeOwners, local_initialization_destinations};
use crate::{CheckerRequestContext, CheckerUnitView};

use super::super::fixed_point::{FixedPointDomain, FlowDirection};
use super::super::id::AnalysisEdgeId;
use super::super::model::{AnalysisBlock, AnalysisEdge, AnalysisEdgeKind, ControlFlowGraph};
use super::super::reachability::ReachabilityResult;
use super::check::StorageFlowCollector;

#[derive(Debug, Default)]
pub(super) struct StorageFlowInput {
    expression_places: BTreeMap<BoundExpressionId, Vec<super::memory::Place>>,
    definitions: BTreeMap<AnyBoundNodeId, Vec<StorageIdentityId>>,
    initialization_destinations: BTreeMap<BoundExpressionId, StorageIdentityId>,
    mutable_storage: BTreeSet<StorageIdentityId>,
    immutable_field_accesses: BTreeSet<StorageAccessId>,
    storage_last_uses: BTreeMap<bray_bound_tree::BoundExecutionSite, Vec<StorageIdentityId>>,
    unused_entry_storage: BTreeSet<StorageIdentityId>,
    cleanup_free_storage: BTreeSet<StorageIdentityId>,
    pub(super) suspension_borrows: BTreeSet<BorrowCapabilityId>,
}

impl StorageFlowInput {
    pub(super) fn new<C>(
        cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
        mutable_storage: BTreeSet<StorageIdentityId>,
        liveness: &Liveness,
    ) -> crate::CheckerQueryResult<(Self, bray_diagnostics::DiagnosticBag), C::UpstreamError>
    where
        C: CheckerRequestContext + ?Sized,
    {
        let initialization_destinations = local_initialization_destinations(request, storage);

        let mut input = Self {
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

        for (storage, identity) in storage.identity_entries() {
            let Some(node) = identity_definition_node(identity) else {
                continue;
            };

            input.definitions.entry(node).or_default().push(storage);
        }

        input.immutable_field_accesses =
            super::authority::immutable_field_accesses(request, storage, &input)?;

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

    pub(super) fn expression_places(
        &self,
        expression: BoundExpressionId,
    ) -> &[super::memory::Place] {
        self.expression_places
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

    pub(super) fn borrowed_access(
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

    pub(super) fn mutation_authority_access(
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

    pub(super) fn fields_allow_mutation(&self, access: StorageAccessId) -> bool {
        !self.immutable_field_accesses.contains(&access)
    }

    pub(super) fn storage_is_mutable(&self, storage: StorageIdentityId) -> bool {
        self.mutable_storage.contains(&storage)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::analysis) enum StorageAbsence {
    Moved,
    NoPayload,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::analysis) struct StorageFlowState {
    pub(super) reachable: bool,
    pub(super) execution: Option<crate::analysis::guarantee::ExecutionState>,
    pub(super) refinements: crate::analysis::refinement::RefinementSet,
    pub(in crate::analysis) live: BTreeSet<StorageIdentityId>,
    pub(in crate::analysis) initialized: BTreeSet<StorageIdentityId>,
    pub(super) observed_pattern_bindings: BTreeSet<StorageIdentityId>,
    pub(in crate::analysis) moved:
        BTreeMap<bray_bound_tree::StoragePath, BTreeMap<BoundExpressionId, bray_symbols::TypeId>>,
    pub(in crate::analysis) definitely_absent:
        BTreeMap<bray_bound_tree::StoragePath, StorageAbsence>,
    pub(in crate::analysis) fully_moved: BTreeSet<StorageIdentityId>,
    pub(super) active_borrows: BTreeSet<BorrowCapabilityId>,
    pub(super) definitely_active_borrows: BTreeSet<BorrowCapabilityId>,
    pub(super) memory: super::memory::State,
    pub(super) recovered: bool,
}

impl StorageFlowState {
    pub(super) fn record_move(
        &mut self,
        path: bray_bound_tree::StoragePath,
        origin: BoundExpressionId,
        ty: bray_symbols::TypeId,
        definite: bool,
    ) {
        if definite && path.is_exact() {
            if path.projections.is_empty() {
                self.move_complete_storage(path.root);
            }

            // Both indexes own their physical path, sharing its immutable projections.
            self.definitely_absent
                .insert(path.clone(), StorageAbsence::Moved);
        }

        self.moved.entry(path).or_default().insert(origin, ty);
    }

    pub(super) fn initialize_path(&mut self, path: &bray_bound_tree::StoragePath) {
        if path.is_exact() {
            self.fully_moved.remove(&path.root);

            if path.projections.is_empty() {
                self.live.insert(path.root);
                self.initialized.insert(path.root);
            }

            self.moved.retain(|moved, _| !path.contains(moved));
        }

        // An uncertain write cannot prove initialization, but can make prior absence uncertain.
        self.definitely_absent.retain(|absent, _| {
            path.root != absent.root
                || bray_bound_tree::StorageProjection::relationship(
                    &path.projections,
                    &absent.projections,
                ) == bray_bound_tree::StorageRelationship::Disjoint
        });
    }

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

        self.forget_storage(identity);
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
            access
                .access_id(storage)
                .and_then(|access| storage.access(access))
                .is_none_or(|access| {
                    access
                        .root()
                        .borrow_capability()
                        .is_none_or(|borrow| !ended.contains(&borrow))
                })
        });

        self.retain_absent_storage();
    }

    pub(super) fn retain_absent_storage(&mut self) {
        self.definitely_absent
            .retain(|path, _| self.live.contains(&path.root));
    }

    pub(super) fn mark_absent(
        &mut self,
        access: StorageAccessId,
        reason: StorageAbsence,
        storage: &StoragePlan,
    ) {
        if let Some(path) = storage.logical_path(access).cloned() {
            self.definitely_absent
                .entry(path)
                .and_modify(|current| {
                    if reason == StorageAbsence::Moved {
                        *current = reason;
                    }
                })
                .or_insert(reason);
        }
    }

    pub(super) fn invalidate_observed_absence(
        &mut self,
        invalidation: &bray_bound_tree::StorageInvalidation,
        storage: &StoragePlan,
    ) {
        self.definitely_absent.retain(|access, reason| {
            // Opaque mutation can change an observed nullable value, but cannot restore moved ownership.
            *reason == StorageAbsence::Moved || !invalidation.invalidates_path(access, storage)
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
            execution: None,
            refinements: Default::default(),
            live: initialized.clone(),
            initialized,
            observed_pattern_bindings: BTreeSet::new(),
            moved: BTreeMap::new(),
            definitely_absent: BTreeMap::new(),
            fully_moved: BTreeSet::new(),
            definitely_active_borrows: active_borrows.clone(),
            active_borrows,
            memory: super::memory::State::default(),
            recovered: false,
        }
    }

    pub(super) fn merge(
        &mut self,
        incoming: &Self,
        block: Option<crate::analysis::id::AnalysisBlockId>,
    ) -> bool {
        if !incoming.reachable {
            return false;
        }

        if !self.reachable {
            // Each block owns its task-local state after propagation.
            *self = incoming.clone();

            return true;
        }

        let execution_changed = crate::analysis::guarantee::merge_execution(
            &mut self.execution,
            &incoming.execution,
            block,
        );

        let refinements_changed = self.refinements.intersect(&incoming.refinements);
        let live_count = self.live.len();
        let initialized_count = self.initialized.len();
        let observed_count = self.observed_pattern_bindings.len();
        let moved_count = self.moved.len();
        let mut absence_changed = false;
        let fully_moved_count = self.fully_moved.len();
        let borrow_count = self.active_borrows.len();
        let definite_borrow_count = self.definitely_active_borrows.len();
        let was_recovered = self.recovered;

        self.live.extend(incoming.live.iter().copied());

        self.initialized
            .retain(|storage| incoming.initialized.contains(storage));

        self.observed_pattern_bindings
            .retain(|storage| incoming.observed_pattern_bindings.contains(storage));

        let mut moved_changed = false;

        for (path, origins) in &incoming.moved {
            // Each joined state owns its cause set and shares the immutable path.
            let current = self.moved.entry(path.clone()).or_default();
            let count = current.len();

            current.extend(origins.iter().map(|(origin, ty)| (*origin, *ty)));
            moved_changed |= current.len() != count;
        }

        self.definitely_absent.retain(|access, reason| {
            let Some(incoming) = incoming.definitely_absent.get(access) else {
                absence_changed = true;

                return false;
            };

            if *reason == StorageAbsence::Moved && *incoming == StorageAbsence::NoPayload {
                *reason = StorageAbsence::NoPayload;
                absence_changed = true;
            }

            true
        });

        self.fully_moved
            .retain(|storage| incoming.fully_moved.contains(storage));

        self.active_borrows
            .extend(incoming.active_borrows.iter().copied());

        self.definitely_active_borrows
            .retain(|borrow| incoming.definitely_active_borrows.contains(borrow));

        let memory_changed = self.memory.merge(&incoming.memory);

        self.recovered |= incoming.recovered;

        execution_changed
            || refinements_changed
            || self.live.len() != live_count
            || self.initialized.len() != initialized_count
            || self.observed_pattern_bindings.len() != observed_count
            || self.moved.len() != moved_count
            || moved_changed
            || absence_changed
            || self.fully_moved.len() != fully_moved_count
            || self.active_borrows.len() != borrow_count
            || self.definitely_active_borrows.len() != definite_borrow_count
            || memory_changed
            || self.recovered != was_recovered
    }

    pub(super) fn forget_storage(&mut self, identity: StorageIdentityId) {
        self.live.remove(&identity);
        self.initialized.remove(&identity);
        self.fully_moved.remove(&identity);
        self.observed_pattern_bindings.remove(&identity);
        self.memory.forget(identity);

        self.moved.retain(|access, _| access.root != identity);

        self.retain_absent_storage();
    }

    pub(super) fn retain_live_storage(&mut self) {
        self.initialized
            .retain(|identity| self.live.contains(identity));

        self.observed_pattern_bindings
            .retain(|identity| self.live.contains(identity));

        self.fully_moved
            .retain(|identity| self.live.contains(identity));

        self.memory.retain_live(&self.live);

        self.moved.retain(|path, _| self.live.contains(&path.root));

        self.retain_absent_storage();
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
    refinements: &'analysis crate::analysis::refinement::RefinementUniverse,
    memory: &'analysis CheckedMemoryOperations,
    input: &'analysis StorageFlowInput,
    owners: &'analysis StorageScopeOwners,
    request: CheckerUnitView<'analysis, C>,
    execution: &'analysis crate::analysis::guarantee::ExecutionFlowDomain<'analysis, 'analysis, C>,
    dependencies: &'analysis bray_bound_tree::CheckedDependencyContracts,
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
        refinements: &'analysis crate::analysis::refinement::RefinementUniverse,
        memory: &'analysis CheckedMemoryOperations,
        input: &'analysis StorageFlowInput,
        owners: &'analysis StorageScopeOwners,
        request: CheckerUnitView<'analysis, C>,
        execution: &'analysis crate::analysis::guarantee::ExecutionFlowDomain<
            'analysis,
            'analysis,
            C,
        >,
        dependencies: &'analysis bray_bound_tree::CheckedDependencyContracts,
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
            execution,
            dependencies,
        }
    }

    fn transfer_block(&self, block: &AnalysisBlock, source: &StorageFlowState) -> StorageFlowState {
        // Transfer owns an independent task-local successor state.
        let mut state = source.clone();

        if !state.reachable {
            return state;
        }

        let mut collector = StorageFlowCollector::without_publication(
            self.request,
            self.storage,
            self.liveness,
            self.refinements,
            self.memory,
            self.input,
            self.owners,
            self.execution,
            self.dependencies,
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
        let mut state = StorageFlowState::entry(self.storage, self.input);

        state.execution = self.execution.boundary();
        state.reachable = state.execution.is_some();

        state.refinements =
            crate::analysis::refinement::RefinementSet::empty(self.refinements.len());

        state.recovered |= self.refinements.is_recovered();

        state
    }

    fn merge_boundary(&self, target: &mut Self::State, boundary: &Self::State) -> bool {
        target.merge(boundary, None)
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

        let single_predecessor = self
            .graph
            .block(edge.target())
            .expect("flow edge has a target")
            .predecessors()
            .len()
            == 1;

        // Each edge owns one correlated state before refining and joining it.
        let mut incoming = source.clone();

        if !incoming.reachable
            || !incoming
                .execution
                .as_mut()
                .is_some_and(|state| self.execution.refine_edge(state, edge))
        {
            if single_predecessor && target.reachable {
                *target = Self::State::default();

                return true;
            }

            return false;
        }

        if edge.kind() == AnalysisEdgeKind::Recovery {
            incoming.recovered = true;
            incoming.refinements.clear();
        } else if let Some(refinement) = edge.refinement() {
            self.refinements
                .insert_refinement(&mut incoming.refinements, refinement);
        }

        let ends = self
            .edge_lifetime_ends
            .get(&edge.id())
            .map(Vec::as_slice)
            .unwrap_or_default();

        if !ends.is_empty() && !source.recovered && !self.liveness.is_recovered() {
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
        }

        if single_predecessor {
            if *target == incoming {
                return false;
            }

            *target = incoming;

            return true;
        }

        target.merge(&incoming, Some(edge.target()))
    }

    fn convergence_bound(&self, graph: &ControlFlowGraph) -> usize {
        let domain_size = self
            .storage
            .identities()
            .len()
            .saturating_add(self.storage.accesses().len().saturating_mul(2))
            .saturating_add(self.storage.borrow_capabilities().len())
            .saturating_add(self.storage.identities().len().saturating_mul(3))
            .saturating_add(self.refinements.len())
            .saturating_add(2);

        graph
            .blocks()
            .len()
            .saturating_mul(domain_size)
            .saturating_add(self.execution.convergence_bound(graph))
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
            &mut crate::asynchronous::CleanupShapeResolver::new(request),
            request,
            &storage,
            BTreeSet::new(),
            &liveness,
        )
        .expect("test storage indexes must build");

        assert_eq!(input.definitions(expressions[0].into()), &[first, second]);

        assert_eq!(
            input.expression_places(expressions[1]),
            &[
                super::super::memory::Place::root(first),
                super::super::memory::Place::root(second)
            ]
        );

        let mut state = reachable_state();

        state.memory.fresh(
            super::super::memory::Place::root(second),
            Some(expressions[0]),
        );

        assert_eq!(
            input
                .expression_places(expressions[1])
                .iter()
                .find(|place| state.memory.has_reference(place)),
            Some(&super::super::memory::Place::root(second))
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

        assert!(!merged.merge(&observed, None));
        assert!(merged.observed_pattern_bindings.contains(&identity));
        assert!(merged.merge(&reachable_state(), None));
        assert!(merged.observed_pattern_bindings.is_empty());
        assert!(!merged.merge(&observed, None));
    }

    #[test]
    fn fully_moved_storage_survives_only_when_every_incoming_path_moves_it() {
        let (identity, _) = storage_and_expressions(79);

        let mut left = reachable_state();
        let mut moved = reachable_state();

        left.fully_moved.insert(identity);
        moved.fully_moved.insert(identity);

        assert!(!left.merge(&moved, None));
        assert_eq!(left.fully_moved, [identity].into_iter().collect());

        assert!(left.merge(&reachable_state(), None));
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

        assert!(initialized.merge(&reachable_state(), None));
        assert_eq!(initialized.live, [identity].into_iter().collect());
        assert!(initialized.initialized.is_empty());
    }

    #[test]
    fn absence_provenance_survives_mutation_and_joins_conservatively() {
        use super::StorageAbsence;
        use bray_bound_tree::{StorageAccess, StorageAccessRoot, StorageInvalidation};

        let (unit, expressions) = expression_unit(BoundUnitId::new(82), |tree, origin| {
            (0..2)
                .map(|_| {
                    push_expression(
                        tree,
                        BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
                    )
                })
                .collect::<Vec<_>>()
        });

        let mut builder = StoragePlanBuilder::new(unit.unit(), unit.key().kind());

        let accesses = expressions
            .iter()
            .map(|expression| {
                let identity = builder
                    .push_identity(StorageIdentity::Allocation(*expression))
                    .unwrap();

                let origin = unit
                    .tree()
                    .expression(*expression)
                    .unwrap()
                    .origin()
                    .source_anchor();

                builder
                    .push_access(StorageAccess::new(
                        StorageAccessRoot::Storage(identity),
                        [],
                        error_type(),
                        origin,
                        false,
                    ))
                    .unwrap()
            })
            .collect::<Vec<_>>();

        let [moved, observed] = accesses.as_slice() else {
            panic!("test builds two accesses");
        };

        let (moved, observed) = (*moved, *observed);

        let storage = builder.finish();
        let moved = storage.logical_path(moved).cloned().unwrap();
        let observed = storage.logical_path(observed).cloned().unwrap();
        let mut state = reachable_state();

        state.definitely_absent.extend([
            (moved.clone(), StorageAbsence::Moved),
            (observed, StorageAbsence::NoPayload),
        ]);

        state.invalidate_observed_absence(&StorageInvalidation::All, &storage);

        assert_eq!(
            state.definitely_absent.keys().cloned().collect::<Vec<_>>(),
            [moved.clone()]
        );

        let mut incoming = reachable_state();

        incoming
            .definitely_absent
            .insert(moved, StorageAbsence::NoPayload);

        assert!(state.merge(&incoming, None));
        assert!(!state.merge(&incoming, None));
        state.invalidate_observed_absence(&StorageInvalidation::All, &storage);
        assert!(state.definitely_absent.is_empty());
    }

    #[test]
    fn uncertain_writes_do_not_reconstruct_a_definitely_moved_component() {
        let (root, expressions) = storage_and_expressions(125);

        let exact = bray_bound_tree::StoragePath {
            root,
            projections: [bray_bound_tree::StorageProjection::ElementFromStart(
                bray_symbols::SymbolOrdinal::new(0),
            )]
            .into(),
        };

        let dynamic = bray_bound_tree::StoragePath {
            root,
            projections: [bray_bound_tree::StorageProjection::Element(expressions[1])].into(),
        };

        let mut state = reachable_state();

        state.live.insert(root);
        state.initialized.insert(root);

        state.record_move(
            exact.clone(),
            expressions[0],
            crate::test_support::error_type(),
            true,
        );

        state.initialize_path(&dynamic);
        assert!(state.moved.contains_key(&exact));
        assert!(state.definitely_absent.is_empty());
        state.initialize_path(&exact);
        assert!(state.moved.is_empty());
    }

    fn reachable_state() -> StorageFlowState {
        StorageFlowState {
            reachable: true,
            ..StorageFlowState::default()
        }
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
