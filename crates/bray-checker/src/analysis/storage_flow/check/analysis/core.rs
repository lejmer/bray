use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BorrowCapabilityId, BoundDependencySubject, BoundExpressionId,
    CheckedMemoryOperations, CheckedRefinementFacts, LivenessFacts, MemoryOperationStatus,
    RefinementFact, StorageAccessId, StorageAccessPlan, StorageAccessPurpose, StorageAccessRoot,
    StorageExitDecision, StorageFlowFacts, StorageOperationDecision, StorageOperationStatus,
    StoragePlan, StorageProjection, StorageRelationship, StorageSuspensionState,
};
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};
use bray_symbols::CallableSignatureFact;

use crate::{
    CheckerFactError, CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext,
    CheckerSemanticFactProvider, CheckerUnitView,
};

use super::super::availability::storage_is_recovered;
use crate::analysis::build::{ControlFlowGraphBuildOutcome, build_storage_control_flow_graph};
use crate::analysis::fixed_point::{FixedPointOutcome, solve_fixed_point};
use crate::analysis::model::{AnalysisOperation, AnalysisOperationKind, AnalysisScopeExitPhase};
use crate::analysis::reachability::analyze_reachability;
use crate::analysis::storage_flow::authority::mutable_storage;
use crate::analysis::storage_flow::copyability::CopyabilityResolver;
use crate::analysis::storage_flow::decision::{diagnostic_kind, more_conservative};
use crate::analysis::storage_flow::model::{StorageFlowDomain, StorageFlowInput, StorageFlowState};

pub(crate) fn check_storage_flow<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    liveness: &LivenessFacts,
    refinements: &CheckedRefinementFacts,
    memory: &CheckedMemoryOperations,
) -> CheckerOutcome<StorageFlowFacts>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    if storage.unit() != request.unit().unit()
        || storage.kind() != request.unit().key().kind()
        || liveness.unit() != request.unit().unit()
        || liveness.kind() != request.unit().key().kind()
        || refinements.unit() != request.unit().unit()
        || refinements.kind() != request.unit().key().kind()
        || memory.unit() != request.unit().unit()
        || memory.kind() != request.unit().key().kind()
    {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidStorageFlowFacts,
        );
    }

    let graph = match build_storage_control_flow_graph(request, storage) {
        ControlFlowGraphBuildOutcome::Complete(graph) => graph,
        ControlFlowGraphBuildOutcome::Cancelled => return CheckerOutcome::Cancelled,
    };

    let Some(reachability) = analyze_reachability(&graph, request) else {
        return CheckerOutcome::Cancelled;
    };

    let mut copyability = CopyabilityResolver::new(request);

    for plan in storage.access_plans().iter().copied().filter(|plan| {
        matches!(
            plan.purpose(),
            StorageAccessPurpose::Copy | StorageAccessPurpose::ValueTransfer
        )
    }) {
        let Some(access) = storage.access(plan.access()) else {
            continue;
        };

        match copyability.resolve(access.reached_type()) {
            Ok(_) => {}
            Err(CheckerFactError::Cancelled) => return CheckerOutcome::Cancelled,
            Err(CheckerFactError::Infrastructure(error)) => {
                return CheckerOutcome::InfrastructureFailure(error);
            }
        }
    }

    let (copyable_types, copyability_diagnostics) = copyability.into_parts();

    let (mutable_storage, authority_diagnostics) = match mutable_storage(request, storage) {
        Ok(result) => result,
        Err(CheckerFactError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(CheckerFactError::Infrastructure(error)) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
    };

    let input = StorageFlowInput::new(storage, copyable_types, mutable_storage);

    let domain = StorageFlowDomain::new(
        &graph,
        &reachability,
        storage,
        liveness,
        refinements,
        memory,
        &input,
        request,
    );

    let result = match solve_fixed_point(&graph, &domain, &request) {
        FixedPointOutcome::Complete(result) => result,
        FixedPointOutcome::Cancelled => return CheckerOutcome::Cancelled,
        FixedPointOutcome::ConvergenceInvariantViolated => {
            panic!("finite storage-flow analysis exceeded its convergence bound")
        }
    };

    let mut collector =
        StorageFlowCollector::new(request, storage, liveness, refinements, memory, &input);

    collector.diagnostics.add_range(copyability_diagnostics);
    collector.diagnostics.add_range(authority_diagnostics);

    for block in graph.blocks() {
        // Publication evaluates operations against an independent final block-entry state.
        let Some(mut state) = result.state(block.id()).cloned() else {
            continue;
        };

        if !state.reachable {
            continue;
        }

        for operation in block.operations() {
            let Some(operation) = graph.operation(*operation) else {
                continue;
            };

            collector.apply_operation(&mut state, operation);
        }
    }

    let decisions = collector.decisions().collect::<Vec<_>>();
    let memory_decisions = collector.memory_decisions().collect::<Vec<_>>();

    let facts = match StorageFlowFacts::try_new(
        storage.unit(),
        storage.kind(),
        decisions,
        collector.suspensions,
        collector.exits,
        collector.is_recovered
            || storage_is_recovered(storage)
            || liveness.is_recovered()
            || refinements.is_recovered(),
    )
    .and_then(|facts| facts.with_memory_operations(memory, memory_decisions))
    {
        Ok(facts) => facts,
        Err(_) => {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidStorageFlowFacts,
            );
        }
    };

    CheckerOutcome::complete(facts, collector.diagnostics)
}

pub(crate) struct StorageFlowCollector<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) request: CheckerUnitView<'analysis, C>,
    pub(super) storage: &'analysis StoragePlan,
    pub(super) liveness: &'analysis LivenessFacts,
    pub(super) refinements: &'analysis CheckedRefinementFacts,
    pub(super) memory: &'analysis CheckedMemoryOperations,
    pub(super) input: &'analysis StorageFlowInput,
    pub(super) statuses: BTreeMap<StorageAccessPlan, StorageOperationStatus>,
    pub(super) suspensions: Vec<StorageSuspensionState>,
    pub(super) exits: Vec<StorageExitDecision>,
    pub(super) memory_decisions: BTreeMap<BoundExpressionId, MemoryOperationStatus>,
    pub(super) diagnostics: DiagnosticBag,
    pub(super) reported_diagnostics: BTreeSet<(DiagnosticKind, StorageAccessId)>,
    pub(super) reported_memory_diagnostics: BTreeSet<(DiagnosticKind, BoundExpressionId)>,
    pub(super) publish: bool,
    pub(super) is_recovered: bool,
}

impl<'analysis, C> StorageFlowCollector<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    fn new(
        request: CheckerUnitView<'analysis, C>,
        storage: &'analysis StoragePlan,
        liveness: &'analysis LivenessFacts,
        refinements: &'analysis CheckedRefinementFacts,
        memory: &'analysis CheckedMemoryOperations,
        input: &'analysis StorageFlowInput,
    ) -> Self {
        Self {
            request,
            storage,
            liveness,
            refinements,
            memory,
            input,
            statuses: BTreeMap::new(),
            suspensions: Vec::new(),
            exits: Vec::new(),
            memory_decisions: BTreeMap::new(),
            diagnostics: DiagnosticBag::new(),
            reported_diagnostics: BTreeSet::new(),
            reported_memory_diagnostics: BTreeSet::new(),
            publish: true,
            is_recovered: false,
        }
    }

    pub(in crate::analysis::storage_flow) fn without_publication(
        request: CheckerUnitView<'analysis, C>,
        storage: &'analysis StoragePlan,
        liveness: &'analysis LivenessFacts,
        refinements: &'analysis CheckedRefinementFacts,
        memory: &'analysis CheckedMemoryOperations,
        input: &'analysis StorageFlowInput,
    ) -> Self {
        Self {
            publish: false,
            ..Self::new(request, storage, liveness, refinements, memory, input)
        }
    }

    pub(in crate::analysis::storage_flow) fn apply_operation(
        &mut self,
        state: &mut StorageFlowState,
        operation: &AnalysisOperation,
    ) {
        if let AnalysisOperationKind::DirectAwait(expression) = operation.kind() {
            self.record_suspension(state, expression);
        }

        if matches!(operation.kind(), AnalysisOperationKind::Recovery(_)) {
            state.recovered = true;
            self.is_recovered = true;
        }

        self.initialize_operation_storage(state, operation.kind().node());

        let refinements = self.refinements.facts_before(operation.kind().node());

        for plan in self.input.plans(operation.kind().node()) {
            self.apply_plan(state, *plan, refinements);
        }

        self.transfer_raw_pointer_state(state, operation.kind().node());
        self.apply_memory_operation(state, operation.kind().node(), refinements);

        self.end_last_use_borrows(state, operation.kind().node());

        if let AnalysisOperationKind::ScopeExit {
            block,
            phase: AnalysisScopeExitPhase::LifecycleResolution,
        } = operation.kind()
        {
            self.record_exit(state, block);
            self.end_scope(state, block);
        }
    }

    fn apply_plan(
        &mut self,
        state: &mut StorageFlowState,
        plan: StorageAccessPlan,
        refinements: &[RefinementFact],
    ) {
        let purpose = self.effective_purpose(plan);
        let status = self.operation_status(state, plan, purpose, refinements);
        let borrow = self.input.borrow(plan);

        if matches!(status, StorageOperationStatus::Valid) {
            self.apply_valid_operation(state, plan, purpose, borrow);
        } else if matches!(status, StorageOperationStatus::Recovered) {
            self.is_recovered = true;
        }

        if !self.publish {
            return;
        }

        self.statuses
            .entry(plan)
            .and_modify(|current| *current = more_conservative(*current, status))
            .or_insert(status);

        if let Some(kind) = diagnostic_kind(status) {
            self.add_diagnostic(kind, plan.access());
        }
    }

    fn operation_status(
        &self,
        state: &StorageFlowState,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
        refinements: &[RefinementFact],
    ) -> StorageOperationStatus {
        let Some(access) = self.storage.access(plan.access()) else {
            return StorageOperationStatus::Recovered;
        };

        if access.is_recovered() {
            return StorageOperationStatus::Recovered;
        }

        if !self.refinements_allow_access(plan.access(), refinements) {
            return StorageOperationStatus::InactiveProjection;
        }

        let Some(root) = self.storage.root_identity(plan.access()) else {
            return StorageOperationStatus::Recovered;
        };

        let requires_value = matches!(
            purpose,
            StorageAccessPurpose::Read
                | StorageAccessPurpose::Copy
                | StorageAccessPurpose::Move
                | StorageAccessPurpose::Borrow(_)
        );

        if requires_value && !state.initialized.contains(&root) {
            return StorageOperationStatus::Uninitialized;
        }

        if requires_value && self.access_is_moved(state, plan.access()) {
            return StorageOperationStatus::Moved;
        }

        let operation_access = self.operation_access(plan, purpose);

        if purpose == StorageAccessPurpose::Move
            && self.access_uses_borrow(operation_access)
            && !self.type_is_borrow(access.reached_type())
        {
            return StorageOperationStatus::MissingOwnership;
        }

        if self.has_borrow_conflict(state, plan, purpose) {
            return StorageOperationStatus::ConflictingBorrow;
        }

        if let Some(authority_access) = self.mutation_authority_access(plan, purpose)
            && (!self.has_mutation_authority(authority_access)
                || !self.fields_allow_mutation(authority_access))
        {
            return StorageOperationStatus::MissingMutationAuthority;
        }

        if purpose == StorageAccessPurpose::Copy
            && !self.input.type_is_copyable(access.reached_type())
        {
            return StorageOperationStatus::NotCopyable;
        }

        StorageOperationStatus::Valid
    }

    fn apply_valid_operation(
        &self,
        state: &mut StorageFlowState,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
        borrow: Option<BorrowCapabilityId>,
    ) {
        match purpose {
            StorageAccessPurpose::Move => {
                state.moved.insert(plan.access());

                if self.move_consumes_complete_union_payload(plan.access())
                    && let Some(root) = self.root_access(plan.access())
                {
                    state.moved.insert(root);
                }
            }
            StorageAccessPurpose::Initialize | StorageAccessPurpose::Assignment => {
                if self.storage.is_root_access(plan.access())
                    && let Some(root) = self.storage.root_identity(plan.access())
                {
                    state.live.insert(root);
                    state.initialized.insert(root);
                }

                state
                    .moved
                    .retain(|moved| !self.storage.access_contains(plan.access(), *moved));
            }
            StorageAccessPurpose::Write => {}
            StorageAccessPurpose::Borrow(_) => {
                if let Some(borrow) = borrow {
                    state.active_borrows.insert(borrow);
                    state.definitely_active_borrows.insert(borrow);
                }
            }
            StorageAccessPurpose::Read
            | StorageAccessPurpose::Copy
            | StorageAccessPurpose::ValueTransfer
            | StorageAccessPurpose::Member
            | StorageAccessPurpose::Index
            | StorageAccessPurpose::Slice
            | StorageAccessPurpose::Projection => {}
        }
    }

    fn move_consumes_complete_union_payload(&self, access: StorageAccessId) -> bool {
        let Some(StorageProjection::ActiveUnionPayloadField { variant, .. }) = self
            .storage
            .resolved_projections(access)
            .and_then(|projections| projections.last())
        else {
            return false;
        };

        self.request
            .symbols()
            .union_variant(*variant)
            .is_some_and(|variant| variant.payload_fields().len() == 1)
    }

    fn root_access(&self, access: StorageAccessId) -> Option<StorageAccessId> {
        let identity = self.storage.root_identity(access)?;

        self.storage.access_entries().find_map(|(candidate, _)| {
            (self.storage.root_identity(candidate) == Some(identity)
                && self.storage.is_root_access(candidate))
            .then_some(candidate)
        })
    }

    fn initialize_operation_storage(&self, state: &mut StorageFlowState, node: AnyBoundNodeId) {
        let definitions = self.input.definitions(node);

        state.live.extend(definitions.iter().copied());
        state.initialized.extend(definitions.iter().copied());
    }

    fn access_is_moved(&self, state: &StorageFlowState, access: StorageAccessId) -> bool {
        state
            .moved
            .iter()
            .any(|moved| self.storage.relationship(*moved, access) != StorageRelationship::Disjoint)
    }

    pub(super) fn effective_purpose(&self, plan: StorageAccessPlan) -> StorageAccessPurpose {
        if plan.purpose() != StorageAccessPurpose::ValueTransfer {
            return plan.purpose();
        }

        let Some(access) = self.storage.access(plan.access()) else {
            return StorageAccessPurpose::ValueTransfer;
        };

        if self.input.type_is_copyable(access.reached_type()) {
            StorageAccessPurpose::Copy
        } else {
            StorageAccessPurpose::Move
        }
    }

    fn end_last_use_borrows(&self, state: &mut StorageFlowState, operation: AnyBoundNodeId) {
        let moved_borrows = state
            .moved
            .iter()
            .filter_map(|access| match self.storage.access(*access)?.root() {
                StorageAccessRoot::Borrow(borrow) => Some(borrow),
                StorageAccessRoot::Storage(_)
                | StorageAccessRoot::OwnedIndirection { .. }
                | StorageAccessRoot::Recovery(_) => None,
            })
            .collect::<BTreeSet<_>>();

        let ended = state
            .active_borrows
            .iter()
            .copied()
            .filter(|borrow| {
                let Some(capability) = self.storage.borrow_capability(*borrow) else {
                    return false;
                };

                let subject = BoundDependencySubject::BorrowCapability(*borrow);

                capability.entry_binding().is_none()
                    && (moved_borrows.contains(borrow)
                        || self.liveness.is_last_use(operation, subject)
                        || capability.expression().is_some_and(|expression| {
                            self.liveness
                                .is_last_use(AnyBoundNodeId::Expression(expression), subject)
                        }))
            })
            .collect::<BTreeSet<_>>();

        state
            .active_borrows
            .retain(|borrow| !ended.contains(borrow));

        state
            .definitely_active_borrows
            .retain(|borrow| !ended.contains(borrow));

        state.moved.retain(|access| {
            self.storage.access(*access).is_none_or(|access| {
                !matches!(access.root(), StorageAccessRoot::Borrow(borrow) if ended.contains(&borrow))
            })
        });
    }

    fn end_scope(&self, state: &mut StorageFlowState, block: bray_bound_tree::BoundBlockId) {
        state.live.retain(|storage| {
            self.liveness
                .is_live_across_scope(block, BoundDependencySubject::Storage(*storage))
        });

        state
            .initialized
            .retain(|storage| state.live.contains(storage));

        state.moved.retain(|access| {
            self.storage
                .root_identity(*access)
                .is_some_and(|storage| state.live.contains(&storage))
        });

        state.active_borrows.retain(|borrow| {
            self.borrow_is_entry(*borrow)
                || self
                    .liveness
                    .is_live_across_scope(block, BoundDependencySubject::BorrowCapability(*borrow))
        });

        state.definitely_active_borrows.retain(|borrow| {
            self.borrow_is_entry(*borrow)
                || self
                    .liveness
                    .is_live_across_scope(block, BoundDependencySubject::BorrowCapability(*borrow))
        });
    }

    fn borrow_is_entry(&self, borrow: BorrowCapabilityId) -> bool {
        self.storage
            .borrow_capability(borrow)
            .is_some_and(|capability| capability.entry_binding().is_some())
    }

    fn record_exit(&mut self, state: &StorageFlowState, block: bray_bound_tree::BoundBlockId) {
        if !self.publish {
            return;
        }

        self.exits.push(StorageExitDecision::new(
            block,
            state.initialized.iter().copied(),
            state.moved.iter().copied(),
            state.active_borrows.iter().copied(),
            state.recovered,
        ));
    }

    fn record_suspension(&mut self, state: &StorageFlowState, expression: BoundExpressionId) {
        if !self.publish {
            return;
        }

        self.suspensions.push(StorageSuspensionState::new(
            expression,
            state.live.iter().copied(),
            state.initialized.iter().copied(),
            state.moved.iter().copied(),
            state.definitely_active_borrows.iter().copied(),
        ));
    }

    fn add_diagnostic(&mut self, kind: DiagnosticKind, access: StorageAccessId) {
        if !self.reported_diagnostics.insert((kind, access)) {
            return;
        }

        let Some(access) = self.storage.access(access) else {
            return;
        };

        let Ok(source) = self.request.source(access.source()) else {
            return;
        };

        let id = u32::try_from(self.diagnostics.len()).unwrap_or(u32::MAX);

        self.diagnostics.add(
            Diagnostic::new(DiagnosticId::new(id), kind, SeverityKind::Error)
                .with_primary_span(source.span()),
        );
    }

    fn decisions(&self) -> impl Iterator<Item = StorageOperationDecision> + '_ {
        self.storage.access_plans().iter().copied().map(|plan| {
            StorageOperationDecision::new(
                plan.expression(),
                self.effective_purpose(plan),
                plan.access(),
                self.input.borrow(plan),
                self.statuses
                    .get(&plan)
                    .copied()
                    .unwrap_or(StorageOperationStatus::Unreachable),
            )
        })
    }
}
