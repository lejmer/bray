// rust-style: allow(module-too-large, reason = "storage flow collection is one transition engine whose diagnostics require the same plan, state, and decision context")

use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BorrowCapabilityId, BoundDependencySubject, BoundExpressionId,
    CheckedMemoryOperations, CheckedRefinements, CheckedSemanticSelections, Liveness,
    MemoryOperationStatus, Refinement, StorageAccessId, StorageAccessPlan, StorageAccessPurpose,
    StorageAccessRoot, StorageBinding, StorageExitDecision, StorageExitPoint, StorageFlow,
    StorageIdentity, StorageOperationDecision, StorageOperationStatus, StoragePlan,
    StorageProjection, StorageRelationship, StorageSuspensionState,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, DiagnosticStorageAccess,
    DiagnosticStorageAccessPurpose, DiagnosticStorageProjection, DiagnosticStorageRoot,
    SeverityKind,
};
use bray_symbols::{AnySymbolId, BorrowKind, CallableSignatureQuery};

use crate::diagnostic::diagnostic_id;
use crate::storage::{StorageScopeOwners, storage_scope_owners};
use crate::unit::assert_unit_inputs;
use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerQueryError, CheckerRequestContext,
    CheckerSemanticQueryProvider, CheckerStorageFlowFailure, CheckerUnitView,
};

use super::super::availability::storage_is_recovered;
use crate::analysis::build::{ControlFlowGraphBuildOutcome, build_storage_control_flow_graph};
use crate::analysis::fixed_point::{FixedPointOutcome, solve_fixed_point};
use crate::analysis::model::{
    AnalysisCallPhase, AnalysisOperation, AnalysisOperationKind, AnalysisScopeExitPhase,
};
use crate::analysis::reachability::analyze_reachability;
use crate::analysis::storage_flow::authority::mutable_storage;
use crate::analysis::storage_flow::copyability::CopyabilityResolver;
use crate::analysis::storage_flow::decision::{diagnostic_kind, more_conservative};
use crate::analysis::storage_flow::model::{StorageFlowDomain, StorageFlowInput, StorageFlowState};

use super::access::BorrowConflict;

pub(crate) fn check_storage_flow<C>(
    request: CheckerUnitView<'_, C>,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
    liveness: &Liveness,
    refinements: &CheckedRefinements,
    memory: &CheckedMemoryOperations,
) -> CheckerOutcome<StorageFlow, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    assert_unit_inputs(
        request,
        [
            (
                "semantic selections",
                (selections.unit(), selections.kind()),
            ),
            ("storage plan", (storage.unit(), storage.kind())),
            ("liveness", (liveness.unit(), liveness.kind())),
            ("refinements", (refinements.unit(), refinements.kind())),
            ("memory operations", (memory.unit(), memory.kind())),
        ],
    );

    let graph = match build_storage_control_flow_graph(request, storage, selections, None) {
        ControlFlowGraphBuildOutcome::Complete(graph) => graph,
        ControlFlowGraphBuildOutcome::Cancelled => return CheckerOutcome::Cancelled,
        ControlFlowGraphBuildOutcome::InfrastructureFailure(error) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        ControlFlowGraphBuildOutcome::UpstreamFailure(error) => {
            return CheckerOutcome::UpstreamFailure(error);
        }
    };

    check_storage_flow_with_graph(request, storage, liveness, refinements, memory, &graph)
}

#[expect(
    clippy::too_many_arguments,
    reason = "the analysis consumes correlated durable inputs and one shared control-flow graph"
)]
pub(crate) fn check_storage_flow_with_graph<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    liveness: &Liveness,
    refinements: &CheckedRefinements,
    memory: &CheckedMemoryOperations,
    graph: &crate::analysis::model::ControlFlowGraph,
) -> CheckerOutcome<StorageFlow, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    let Some(reachability) = analyze_reachability(graph, request) else {
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
            Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
            Err(CheckerQueryError::Infrastructure(error)) => {
                return CheckerOutcome::InfrastructureFailure(error);
            }
            Err(CheckerQueryError::Upstream(error)) => {
                return CheckerOutcome::UpstreamFailure(error);
            }
        }
    }

    let (copyable_types, copyability_diagnostics) = copyability.into_parts();

    let (input, authority_diagnostics) =
        match mutable_storage(request, storage).and_then(|(mutable, diagnostics)| {
            StorageFlowInput::new(request, storage, copyable_types, mutable)
                .map(|input| (input, diagnostics))
        }) {
            Ok(result) => result,
            Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
            Err(CheckerQueryError::Infrastructure(error)) => {
                return CheckerOutcome::InfrastructureFailure(error);
            }
            Err(CheckerQueryError::Upstream(error)) => {
                return CheckerOutcome::UpstreamFailure(error);
            }
        };

    let owners = match storage_scope_owners(request).map_err(CheckerQueryError::with_upstream) {
        Ok(owners) => owners,
        Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Infrastructure(error)) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        Err(CheckerQueryError::Upstream(error)) => {
            return CheckerOutcome::UpstreamFailure(error);
        }
    };

    let domain = StorageFlowDomain::new(
        &graph,
        &reachability,
        storage,
        liveness,
        refinements,
        memory,
        &input,
        &owners,
        request,
    );

    let result = match solve_fixed_point(&graph, &domain, &request) {
        FixedPointOutcome::Complete(result) => result,
        FixedPointOutcome::Cancelled => return CheckerOutcome::Cancelled,
        FixedPointOutcome::ConvergenceInvariantViolated => {
            panic!("finite storage-flow analysis exceeded its convergence bound")
        }
    };

    let mut collector = StorageFlowCollector::new(
        request,
        storage,
        liveness,
        refinements,
        memory,
        &input,
        &owners,
    );

    collector.diagnostics.add_range(copyability_diagnostics);
    collector.diagnostics.add_range(authority_diagnostics);

    let mut reachable_exits = BTreeSet::new();

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

            if let AnalysisOperationKind::ScopeExit {
                block,
                exit,
                phase: AnalysisScopeExitPhase::LifecycleResolution,
            } = operation.kind()
            {
                reachable_exits.insert(StorageExitPoint::new(block, exit));
            }

            collector.apply_operation(&mut state, operation);
        }
    }

    if let Some(error) = collector.infrastructure_failure.take() {
        return match error {
            CheckerQueryError::Cancelled => CheckerOutcome::Cancelled,
            CheckerQueryError::Infrastructure(error) => {
                CheckerOutcome::InfrastructureFailure(error)
            }
            CheckerQueryError::Upstream(error) => CheckerOutcome::UpstreamFailure(error),
        };
    }

    let decisions = collector.decisions().collect::<Vec<_>>();
    let memory_decisions = collector.memory_decisions().collect::<Vec<_>>();
    let exits = collector.exit_decisions().collect::<Vec<_>>();
    let replacements = collector.replacement_decisions().collect::<Vec<_>>();

    let analysis = match StorageFlow::try_new(
        storage.unit(),
        storage.kind(),
        decisions,
        collector.suspensions,
        reachable_exits,
        exits,
        collector.is_recovered
            || storage_is_recovered(storage)
            || liveness.is_recovered()
            || refinements.is_recovered(),
    )
    .and_then(|analysis| analysis.with_memory_operations(memory, memory_decisions))
    .and_then(|analysis| analysis.with_replacements(replacements))
    {
        Ok(analysis) => analysis,
        Err(error) => {
            return CheckerOutcome::InfrastructureFailure(CheckerInfrastructureError::StorageFlow(
                CheckerStorageFlowFailure::FlowConstruction(error),
            ));
        }
    };

    CheckerOutcome::complete(analysis, collector.diagnostics)
}

struct StorageOperationOutcome {
    status: StorageOperationStatus,
    origin: Option<StorageOperationOrigin>,
}

enum StorageOperationOrigin {
    Borrows(Vec<BorrowCapabilityId>),
    Moves(Vec<BoundExpressionId>),
}

impl StorageOperationOutcome {
    fn status(status: StorageOperationStatus) -> Self {
        Self {
            status,
            origin: None,
        }
    }

    fn borrow_conflict(conflict: BorrowConflict) -> Self {
        let origin = match conflict {
            BorrowConflict::Unlocated => None,
            BorrowConflict::Borrows(borrows) => Some(StorageOperationOrigin::Borrows(borrows)),
        };

        Self {
            status: StorageOperationStatus::ConflictingBorrow,
            origin,
        }
    }

    fn moved(origins: Vec<BoundExpressionId>) -> Self {
        Self {
            status: StorageOperationStatus::Moved,
            origin: Some(StorageOperationOrigin::Moves(origins)),
        }
    }
}

pub(crate) struct StorageFlowCollector<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) request: CheckerUnitView<'analysis, C>,
    pub(super) storage: &'analysis StoragePlan,
    pub(super) liveness: &'analysis Liveness,
    pub(super) refinements: &'analysis CheckedRefinements,
    pub(super) memory: &'analysis CheckedMemoryOperations,
    pub(super) input: &'analysis StorageFlowInput,
    pub(super) owners: &'analysis StorageScopeOwners,
    pub(super) statuses: BTreeMap<StorageAccessPlan, StorageOperationStatus>,
    pub(super) suspensions: Vec<StorageSuspensionState>,
    exits: Vec<(
        bray_bound_tree::BoundBlockId,
        AnyBoundNodeId,
        StorageFlowState,
    )>,
    exit_indices: BTreeMap<(bray_bound_tree::BoundBlockId, AnyBoundNodeId), usize>,
    replacements: BTreeMap<StorageAccessPlan, StorageFlowState>,
    pub(super) memory_decisions: BTreeMap<BoundExpressionId, MemoryOperationStatus>,
    pub(super) diagnostics: DiagnosticBag,
    pub(super) reported_diagnostics: BTreeSet<(DiagnosticKind, StorageAccessId)>,
    pub(super) reported_memory_diagnostics: BTreeSet<(DiagnosticKind, BoundExpressionId)>,
    pub(super) publish: bool,
    pub(super) is_recovered: bool,
    pub(super) infrastructure_failure: Option<CheckerQueryError<C::UpstreamError>>,
}

impl<'analysis, C> StorageFlowCollector<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn record_infrastructure_failure(&mut self, error: CheckerInfrastructureError) {
        self.infrastructure_failure = Some(CheckerQueryError::Infrastructure(error));
    }

    fn new(
        request: CheckerUnitView<'analysis, C>,
        storage: &'analysis StoragePlan,
        liveness: &'analysis Liveness,
        refinements: &'analysis CheckedRefinements,
        memory: &'analysis CheckedMemoryOperations,
        input: &'analysis StorageFlowInput,
        owners: &'analysis StorageScopeOwners,
    ) -> Self {
        Self {
            request,
            storage,
            liveness,
            refinements,
            memory,
            input,
            owners,
            statuses: BTreeMap::new(),
            suspensions: Vec::new(),
            exits: Vec::new(),
            exit_indices: BTreeMap::new(),
            replacements: BTreeMap::new(),
            memory_decisions: BTreeMap::new(),
            diagnostics: DiagnosticBag::new(),
            reported_diagnostics: BTreeSet::new(),
            reported_memory_diagnostics: BTreeSet::new(),
            publish: true,
            is_recovered: false,
            infrastructure_failure: None,
        }
    }

    pub(in crate::analysis::storage_flow) fn without_publication(
        request: CheckerUnitView<'analysis, C>,
        storage: &'analysis StoragePlan,
        liveness: &'analysis Liveness,
        refinements: &'analysis CheckedRefinements,
        memory: &'analysis CheckedMemoryOperations,
        input: &'analysis StorageFlowInput,
        owners: &'analysis StorageScopeOwners,
    ) -> Self {
        Self {
            publish: false,
            ..Self::new(
                request,
                storage,
                liveness,
                refinements,
                memory,
                input,
                owners,
            )
        }
    }

    pub(in crate::analysis::storage_flow) fn apply_operation(
        &mut self,
        state: &mut StorageFlowState,
        operation: &AnalysisOperation,
    ) {
        if let AnalysisOperationKind::PatternObservation(pattern) = operation.kind() {
            state
                .observed_pattern_bindings
                .extend(self.input.definitions(pattern.into()));

            return;
        }

        if let AnalysisOperationKind::Suspension { expression, .. } = operation.kind() {
            self.record_suspension(state, expression);
        }

        if matches!(operation.kind(), AnalysisOperationKind::Recovery(_)) {
            state.recovered = true;
            self.is_recovered = true;
        }

        if matches!(
            operation.kind(),
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Attempt,
                ..
            }
        ) {
            return;
        }

        self.initialize_operation_storage(state, operation.kind().node());

        let refinements = self.refinements.refinements_before(operation.kind().node());

        for plan in self.input.plans(operation.kind().node()) {
            self.apply_plan(state, *plan, refinements);
        }

        self.transfer_raw_pointer_state(state, operation.kind().node());
        self.apply_memory_operation(state, operation.kind().node(), refinements);
        self.transfer_memory_result_state(state, operation.kind().node());

        if let AnalysisOperationKind::ScopeExit {
            block,
            exit,
            phase: AnalysisScopeExitPhase::LifecycleResolution,
        } = operation.kind()
        {
            self.report_escaping_storage_dependencies(state, block, exit);
        }

        if let crate::CheckerUnitRoot::Expression(root) = self.request.root()
            && operation.kind().node() == root.into()
            && matches!(
                self.request.semantic_context(),
                crate::SemanticUnitContext::RuntimeDefault(_)
            )
        {
            self.report_escaping_default_storage(state, root);
        }

        self.end_last_use_borrows(state, operation.kind().node());

        if let AnalysisOperationKind::ScopeExit {
            block,
            exit,
            phase: AnalysisScopeExitPhase::LifecycleResolution,
        } = operation.kind()
        {
            self.record_exit(state, block, exit);
            self.end_scope(state, block, exit);
        }
    }

    fn apply_plan(
        &mut self,
        state: &mut StorageFlowState,
        plan: StorageAccessPlan,
        refinements: &[Refinement],
    ) {
        let purpose = self.effective_purpose(plan);

        let outcome = match self.operation_status(state, plan, purpose, refinements) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.infrastructure_failure = Some(CheckerQueryError::Infrastructure(error));
                return;
            }
        };

        let status = outcome.status;
        let borrow = self.input.borrow(plan);

        if matches!(
            status,
            StorageOperationStatus::Uninitialized
                | StorageOperationStatus::InactiveProjection
                | StorageOperationStatus::NotCopyable
        ) {
            self.infrastructure_failure = Some(CheckerQueryError::Infrastructure(
                CheckerInfrastructureError::InvalidStorageOperation {
                    expression: plan.expression(),
                    access: plan.access(),
                    status,
                },
            ));

            return;
        }

        if matches!(status, StorageOperationStatus::Valid) {
            if self.publish && purpose == StorageAccessPurpose::Assignment {
                self.replacements
                    .entry(plan)
                    .and_modify(|previous| {
                        previous.merge(state);
                    })
                    // Publication owns the pre-installation state independently of later writes.
                    .or_insert_with(|| state.clone());
            }

            if let Err(error) = self.apply_valid_operation(state, plan, purpose, borrow) {
                self.infrastructure_failure = Some(error);

                return;
            }
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
            self.add_diagnostic(kind, plan, purpose, outcome.origin);
        }
    }

    fn operation_status(
        &self,
        state: &StorageFlowState,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
        refinements: &[Refinement],
    ) -> Result<StorageOperationOutcome, CheckerInfrastructureError> {
        let Some(access) = self.storage.access(plan.access()) else {
            return Ok(StorageOperationOutcome::status(
                StorageOperationStatus::Recovered,
            ));
        };

        if access.is_recovered() {
            return Ok(StorageOperationOutcome::status(
                StorageOperationStatus::Recovered,
            ));
        }

        if !self.pattern_establishes_projection(plan.access())
            && !self.refinements_allow_access(plan.access(), refinements)
        {
            return Ok(StorageOperationOutcome::status(
                StorageOperationStatus::InactiveProjection,
            ));
        }

        let Some(root) = self.storage.root_identity(plan.access()) else {
            return Ok(StorageOperationOutcome::status(
                StorageOperationStatus::Recovered,
            ));
        };

        let requires_value = matches!(
            purpose,
            StorageAccessPurpose::Read
                | StorageAccessPurpose::Copy
                | StorageAccessPurpose::Move
                | StorageAccessPurpose::Borrow(_)
        );

        let pattern_establishes_projection = self.pattern_establishes_projection(plan.access());

        if requires_value && !pattern_establishes_projection {
            let origins = self.moved_origins(state, plan.access());

            if !origins.is_empty() {
                return Ok(StorageOperationOutcome::moved(origins));
            }
        }

        if requires_value
            && !pattern_establishes_projection
            && !state.initialized.contains(&root)
            && !state.observed_pattern_bindings.contains(&root)
        {
            return Ok(StorageOperationOutcome::status(
                StorageOperationStatus::Uninitialized,
            ));
        }

        let operation_access = self.operation_access(plan, purpose);

        if purpose == StorageAccessPurpose::Move
            && self.access_uses_borrow(operation_access)?
            && !self.type_is_borrow(access.reached_type())
        {
            return Ok(StorageOperationOutcome::status(
                StorageOperationStatus::MissingOwnership,
            ));
        }

        if let Some(conflict) = self.borrow_conflict(state, plan, purpose)? {
            return Ok(StorageOperationOutcome::borrow_conflict(conflict));
        }

        if let Some(authority_access) =
            self.input
                .mutation_authority_access(plan, purpose, self.storage)
            && (!self.has_mutation_authority(authority_access)?
                || !self.input.fields_allow_mutation(authority_access))
        {
            return Ok(StorageOperationOutcome::status(
                StorageOperationStatus::MissingMutationAuthority,
            ));
        }

        if purpose == StorageAccessPurpose::Copy
            && !self.input.type_is_copyable(access.reached_type())
        {
            return Ok(StorageOperationOutcome::status(
                StorageOperationStatus::NotCopyable,
            ));
        }

        Ok(StorageOperationOutcome::status(
            StorageOperationStatus::Valid,
        ))
    }

    fn apply_valid_operation(
        &self,
        state: &mut StorageFlowState,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
        borrow: Option<BorrowCapabilityId>,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        match purpose {
            StorageAccessPurpose::Move => {
                state.moved.insert(plan.access(), plan.expression());

                if let (Some(identity), Some(path)) = (
                    self.storage.root_identity(plan.access()),
                    self.storage.resolved_projections(plan.access()),
                ) && let Some(access) = self.storage.access_at(identity, path)
                {
                    state.definitely_moved.insert(access);
                }

                if self.storage.is_root_access(plan.access())
                    && self
                        .storage
                        .access(plan.access())
                        .is_some_and(|access| access.root().borrow_capability().is_none())
                    && let Some(root) = self.storage.root_identity(plan.access())
                {
                    state.move_complete_storage(root);
                }
            }
            StorageAccessPurpose::Initialize | StorageAccessPurpose::Assignment => {
                if let Some(root) = self.storage.root_identity(plan.access()) {
                    state.fully_moved.remove(&root);
                }

                if self.storage.is_root_access(plan.access())
                    && let Some(root) = self.storage.root_identity(plan.access())
                {
                    state.live.insert(root);
                    state.initialized.insert(root);
                }

                state
                    .moved
                    .retain(|moved, _| !self.storage.access_contains(plan.access(), *moved));

                state.retain_definite_moves(self.storage);
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

        Ok(())
    }

    fn pattern_establishes_projection(&self, access: StorageAccessId) -> bool {
        let source_is_pattern = |access: StorageAccessId| {
            self.storage.access(access).is_some_and(|access| {
                matches!(
                    access.source().syntax().syntax_kind(),
                    bray_syntax::SyntaxKind::IrrefutablePattern
                        | bray_syntax::SyntaxKind::IrrefutablePatternEntry
                        | bray_syntax::SyntaxKind::CasePattern
                        | bray_syntax::SyntaxKind::CasePatternEntry
                )
            })
        };

        source_is_pattern(access)
            || self.storage.bindings().iter().any(|(_, binding)| {
                let StorageBinding::Access(binding) = binding else {
                    return false;
                };

                source_is_pattern(*binding)
                    && self.storage.relationship(*binding, access) == StorageRelationship::Identical
            })
    }

    fn initialize_operation_storage(&self, state: &mut StorageFlowState, node: AnyBoundNodeId) {
        let definitions = self.input.definitions(node);

        state
            .observed_pattern_bindings
            .retain(|identity| !definitions.contains(identity));

        state.moved.retain(|access, _| {
            self.storage
                .root_identity(*access)
                .is_none_or(|storage| !definitions.contains(&storage))
        });

        state.retain_definite_moves(self.storage);

        state
            .fully_moved
            .retain(|storage| !definitions.contains(storage));

        state.live.extend(definitions.iter().copied());
        state.initialized.extend(definitions.iter().copied());
    }

    fn moved_origins(
        &self,
        state: &StorageFlowState,
        access: StorageAccessId,
    ) -> Vec<BoundExpressionId> {
        let mut origins = state
            .moved
            .iter()
            .filter(|(moved, _)| {
                let moves_borrow_value = self
                    .storage
                    .access(**moved)
                    .is_some_and(|record| self.type_is_borrow(record.reached_type()));

                let relationship = if moves_borrow_value {
                    self.storage.value_relationship(**moved, access)
                } else {
                    self.storage.relationship(**moved, access)
                };

                relationship != StorageRelationship::Disjoint
            })
            .map(|(_, origin)| *origin)
            .collect::<Vec<_>>();

        origins.sort_unstable();
        origins.dedup();

        origins
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
            .keys()
            .filter_map(|access| match self.storage.access(*access)?.root() {
                root => root.borrow_capability(),
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

                let retained_for_suspension = self
                    .liveness
                    .live_across_suspensions()
                    .iter()
                    .any(|entry| entry.subject() == subject);

                let completed_retaining_suspension = match operation {
                    AnyBoundNodeId::Expression(expression) => {
                        self.liveness.is_live_across_suspension(expression, subject)
                    }
                    AnyBoundNodeId::Pattern(_)
                    | AnyBoundNodeId::Block(_)
                    | AnyBoundNodeId::CallableBody(_) => false,
                };

                let reaches_last_use = self.liveness.is_last_use(operation, subject)
                    || capability.expression().is_some_and(|expression| {
                        self.liveness
                            .is_last_use(AnyBoundNodeId::Expression(expression), subject)
                    });

                capability.entry_binding().is_none()
                    && ((moved_borrows.contains(borrow)
                        && !self.liveness.is_owner_retained(subject)
                        && (!retained_for_suspension || completed_retaining_suspension))
                        || reaches_last_use)
            })
            .collect::<BTreeSet<_>>();

        state
            .active_borrows
            .retain(|borrow| !ended.contains(borrow));

        state
            .definitely_active_borrows
            .retain(|borrow| !ended.contains(borrow));

        state.moved.retain(|access, _| {
            self.storage.access(*access).is_none_or(|access| {
                access
                    .root()
                    .borrow_capability()
                    .is_none_or(|borrow| !ended.contains(&borrow))
            })
        });

        state.retain_definite_moves(self.storage);
    }

    fn end_scope(
        &self,
        state: &mut StorageFlowState,
        block: bray_bound_tree::BoundBlockId,
        exit: AnyBoundNodeId,
    ) {
        state
            .live
            .retain(|storage| self.owners.identity_scope(self.storage, *storage) != Some(block));

        state
            .initialized
            .retain(|storage| state.live.contains(storage));

        state.moved.retain(|access, _| {
            self.storage
                .root_identity(*access)
                .is_some_and(|storage| state.live.contains(&storage))
        });

        state.retain_definite_moves(self.storage);

        state
            .fully_moved
            .retain(|storage| state.live.contains(storage));

        state.active_borrows.retain(|borrow| {
            self.borrow_is_entry(*borrow)
                || self.liveness.is_live_across_scope(
                    block,
                    exit,
                    BoundDependencySubject::BorrowCapability(*borrow),
                )
        });

        state.definitely_active_borrows.retain(|borrow| {
            self.borrow_is_entry(*borrow)
                || self.liveness.is_live_across_scope(
                    block,
                    exit,
                    BoundDependencySubject::BorrowCapability(*borrow),
                )
        });
    }

    fn borrow_is_entry(&self, borrow: BorrowCapabilityId) -> bool {
        self.storage
            .borrow_capability(borrow)
            .is_some_and(|capability| capability.entry_binding().is_some())
    }

    fn record_exit(
        &mut self,
        state: &StorageFlowState,
        block: bray_bound_tree::BoundBlockId,
        exit: AnyBoundNodeId,
    ) {
        if !self.publish {
            return;
        }

        let key = (block, exit);

        if let Some(index) = self.exit_indices.get(&key).copied() {
            self.exits[index].2.merge(state);
            return;
        }

        self.exit_indices.insert(key, self.exits.len());

        // Publication owns this snapshot so later operations can mutate their task-local state.
        self.exits.push((block, exit, state.clone()));
    }

    pub(super) fn exit_decisions(&self) -> impl Iterator<Item = StorageExitDecision> + '_ {
        self.exits.iter().map(|(block, exit, state)| {
            StorageExitDecision::new(
                *block,
                *exit,
                state.live.iter().copied(),
                state.initialized.iter().copied(),
                state.moved.keys().copied(),
                state.definitely_moved.iter().copied(),
                state.fully_moved.iter().copied(),
                state.active_borrows.iter().copied(),
                state.recovered,
            )
        })
    }

    fn replacement_decisions(
        &self,
    ) -> impl Iterator<Item = bray_bound_tree::StorageReplacementDecision> + '_ {
        self.replacements.iter().map(|(plan, state)| {
            let root = self.storage.root_identity(plan.access());

            let moved = state
                .moved
                .keys()
                .copied()
                .filter(|access| self.storage.root_identity(*access) == root);

            let initialization = match root {
                Some(root) if !state.live.contains(&root) || state.fully_moved.contains(&root) => {
                    bray_bound_tree::StorageReplacementState::Absent
                }
                Some(root)
                    if state.initialized.contains(&root)
                        && !state.moved.keys().any(|access| {
                            self.storage.access_contains(plan.access(), *access)
                                || self.storage.access_contains(*access, plan.access())
                        }) =>
                {
                    bray_bound_tree::StorageReplacementState::Present
                }
                _ => bray_bound_tree::StorageReplacementState::Conditional,
            };

            bray_bound_tree::StorageReplacementDecision::new(
                plan.expression(),
                plan.access(),
                initialization,
                moved,
                state.recovered || root.is_none(),
            )
        })
    }

    fn record_suspension(&mut self, state: &StorageFlowState, expression: BoundExpressionId) {
        if !self.publish {
            return;
        }

        self.suspensions.push(StorageSuspensionState::new(
            expression,
            state.live.iter().copied(),
            state.initialized.iter().copied(),
            state.moved.keys().copied(),
            state.definitely_active_borrows.iter().copied(),
        ));
    }

    fn add_diagnostic(
        &mut self,
        kind: DiagnosticKind,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
        origin: Option<StorageOperationOrigin>,
    ) {
        if self.infrastructure_failure.is_some() {
            return;
        }

        let access_id = plan.access();

        let Some(access) = self.storage.access(access_id) else {
            self.infrastructure_failure = Some(CheckerQueryError::Infrastructure(
                CheckerInfrastructureError::StorageFlow(
                    CheckerStorageFlowFailure::MissingStorageAccess { access: access_id },
                ),
            ));

            return;
        };

        let source = match self.request.source(access.source()) {
            Ok(source) => source,
            Err(error) => {
                self.infrastructure_failure = Some(CheckerQueryError::Infrastructure(error));

                return;
            }
        };

        let access_argument = match self.diagnostic_storage_access(access_id, purpose) {
            Ok(access) => access,
            Err(error) => {
                self.infrastructure_failure = Some(error);

                return;
            }
        };

        if !self.reported_diagnostics.insert((kind, access_id)) {
            return;
        }

        let mut diagnostic = Diagnostic::new(
            diagnostic_id(self.diagnostics.len()),
            kind,
            SeverityKind::Error,
        )
        .with_primary_span(source.span())
        .with_label(DiagnosticLabel::primary(
            storage_diagnostic_label(kind),
            source.span(),
        ))
        .with_arg(DiagnosticArg::storage_access(access_argument));

        if let Some(origin) = origin {
            diagnostic = self.with_operation_origins(diagnostic, source.span(), origin);
        }

        self.diagnostics.add(diagnostic);
    }

    fn diagnostic_storage_access(
        &self,
        access_id: StorageAccessId,
        purpose: StorageAccessPurpose,
    ) -> Result<DiagnosticStorageAccess, CheckerQueryError<C::UpstreamError>> {
        let access =
            self.storage
                .access(access_id)
                .ok_or(CheckerInfrastructureError::StorageFlow(
                    CheckerStorageFlowFailure::MissingStorageAccess { access: access_id },
                ))?;

        let root = match access.root() {
            StorageAccessRoot::Storage(storage) => self.diagnostic_storage_identity(storage)?,
            StorageAccessRoot::Borrow(_) => DiagnosticStorageRoot::Borrow,
            StorageAccessRoot::BorrowedStorage { .. } => DiagnosticStorageRoot::BorrowedStorage,
            StorageAccessRoot::OwnedIndirection { .. } => DiagnosticStorageRoot::OwnedIndirection,
            StorageAccessRoot::Recovery(_) => DiagnosticStorageRoot::Recovery,
        };

        let projections = access
            .projections()
            .iter()
            .map(|projection| self.diagnostic_storage_projection(*projection))
            .collect::<Result<Vec<_>, _>>()?;

        let reached_type =
            crate::diagnostic::diagnostic_type(self.request.context(), access.reached_type())?;

        Ok(DiagnosticStorageAccess::new(
            diagnostic_storage_purpose(purpose),
            root,
            projections,
            reached_type,
        ))
    }

    fn diagnostic_storage_identity(
        &self,
        identity: bray_bound_tree::StorageIdentityId,
    ) -> Result<DiagnosticStorageRoot, CheckerInfrastructureError> {
        let root = match self.storage.identity(identity) {
            Some(StorageIdentity::LocalOwned(_)) => DiagnosticStorageRoot::Local,
            Some(StorageIdentity::Parameter(_)) => DiagnosticStorageRoot::Parameter,
            Some(StorageIdentity::Receiver(_)) => DiagnosticStorageRoot::Receiver,
            Some(StorageIdentity::Static(_)) => DiagnosticStorageRoot::Static,
            Some(StorageIdentity::AnonymousParameter(_)) => {
                DiagnosticStorageRoot::AnonymousParameter
            }
            Some(StorageIdentity::PredicateParameter(_)) => {
                DiagnosticStorageRoot::PredicateParameter
            }
            Some(StorageIdentity::PostconditionResult(_)) => {
                DiagnosticStorageRoot::PostconditionResult
            }
            Some(StorageIdentity::Result(_)) => DiagnosticStorageRoot::Result,
            Some(StorageIdentity::Temporary(_)) => DiagnosticStorageRoot::Temporary,
            Some(StorageIdentity::CustomIndexBorrow(_)) => DiagnosticStorageRoot::CustomIndexBorrow,
            Some(StorageIdentity::IterationCursor(_)) => DiagnosticStorageRoot::IterationCursor,
            Some(StorageIdentity::IterationElement(_)) => DiagnosticStorageRoot::IterationElement,
            Some(StorageIdentity::Allocation(_)) => DiagnosticStorageRoot::Allocation,
            Some(StorageIdentity::CompilerCreated(_)) => DiagnosticStorageRoot::CompilerCreated,
            Some(StorageIdentity::Alternative { .. }) => DiagnosticStorageRoot::Alternative,
            Some(StorageIdentity::Error(_)) => DiagnosticStorageRoot::Recovery,
            None => {
                return Err(CheckerInfrastructureError::StorageFlow(
                    CheckerStorageFlowFailure::MissingStorageIdentity { identity },
                ));
            }
        };

        Ok(root)
    }

    fn diagnostic_storage_projection(
        &self,
        projection: StorageProjection,
    ) -> Result<DiagnosticStorageProjection, CheckerQueryError<C::UpstreamError>> {
        let projection = match projection {
            StorageProjection::ProductField(field) => DiagnosticStorageProjection::ProductField(
                self.diagnostic_symbol_name(field.into())?,
            ),
            StorageProjection::TupleElement(ordinal) => {
                DiagnosticStorageProjection::TupleElement(u64::from(ordinal.raw()))
            }
            StorageProjection::ElementFromStart(ordinal) => {
                DiagnosticStorageProjection::ElementFromStart(u64::from(ordinal.raw()))
            }
            StorageProjection::ElementFromEnd(ordinal) => {
                DiagnosticStorageProjection::ElementFromEnd(u64::from(ordinal.raw()))
            }
            StorageProjection::ActiveUnionPayloadField { variant, field } => {
                DiagnosticStorageProjection::ActiveUnionPayloadField {
                    variant: self.diagnostic_symbol_name(variant.into())?,
                    field: self.diagnostic_symbol_name(field.into())?,
                }
            }
            StorageProjection::Element(_) => DiagnosticStorageProjection::IndexedElement,
            StorageProjection::SliceRange { start, end } => {
                DiagnosticStorageProjection::SliceRange {
                    has_start: start.is_some(),
                    has_end: end.is_some(),
                }
            }
            StorageProjection::NullableValue => DiagnosticStorageProjection::NullableValue,
            StorageProjection::OwnedTarget => DiagnosticStorageProjection::OwnedTarget,
        };

        Ok(projection)
    }

    fn diagnostic_symbol_name(
        &self,
        symbol: AnySymbolId,
    ) -> Result<String, CheckerQueryError<C::UpstreamError>> {
        self.request
            .member_name(symbol)?
            .map(|name| name.as_str().to_owned())
            .ok_or(CheckerInfrastructureError::StorageFlow(
                CheckerStorageFlowFailure::MissingStorageSymbolName { symbol },
            ))
            .map_err(CheckerQueryError::Infrastructure)
    }

    fn with_operation_origins(
        &self,
        mut diagnostic: Diagnostic,
        primary: bray_source::SourceSpan,
        origin: StorageOperationOrigin,
    ) -> Diagnostic {
        let (kind, sources) = match origin {
            StorageOperationOrigin::Borrows(borrows) => (
                DiagnosticRelatedLocationKind::BorrowOrigin,
                borrows
                    .into_iter()
                    .filter_map(|borrow| self.storage.borrow_capability(borrow)?.source().into())
                    .collect::<Vec<_>>(),
            ),
            StorageOperationOrigin::Moves(expressions) => (
                DiagnosticRelatedLocationKind::MoveOrigin,
                expressions
                    .into_iter()
                    .filter_map(|expression| {
                        Some(
                            self.request
                                .view()
                                .expression(expression)?
                                .origin()
                                .source_anchor(),
                        )
                    })
                    .collect::<Vec<_>>(),
            ),
        };

        for related in sources
            .into_iter()
            .filter_map(|source| self.request.source(source).ok())
            .filter(|related| related.span() != primary)
        {
            diagnostic = diagnostic
                .with_related_location(DiagnosticRelatedLocation::new(kind, related.span()));
        }

        diagnostic
    }

    fn decisions(&self) -> impl Iterator<Item = StorageOperationDecision> + '_ {
        self.storage.access_plans().iter().copied().map(|plan| {
            StorageOperationDecision::new(
                plan.node(),
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

fn storage_diagnostic_label(kind: DiagnosticKind) -> DiagnosticLabelKind {
    match kind {
        DiagnosticKind::CheckingUseOfMovedStorage => DiagnosticLabelKind::MovedStorageUse,
        DiagnosticKind::CheckingConflictingBorrow => {
            DiagnosticLabelKind::ConflictingBorrowOperation
        }
        DiagnosticKind::CheckingMissingMutationAuthority => {
            DiagnosticLabelKind::MissingMutationAuthority
        }
        DiagnosticKind::CheckingMissingStorageOwnership => {
            DiagnosticLabelKind::MissingStorageOwnership
        }
        _ => unreachable!("storage-flow diagnostics must describe a storage operation"),
    }
}

fn diagnostic_storage_purpose(purpose: StorageAccessPurpose) -> DiagnosticStorageAccessPurpose {
    match purpose {
        StorageAccessPurpose::Read => DiagnosticStorageAccessPurpose::Read,
        StorageAccessPurpose::Initialize => DiagnosticStorageAccessPurpose::Initialize,
        StorageAccessPurpose::Write => DiagnosticStorageAccessPurpose::Write,
        StorageAccessPurpose::Move => DiagnosticStorageAccessPurpose::Move,
        StorageAccessPurpose::Copy | StorageAccessPurpose::ValueTransfer => {
            DiagnosticStorageAccessPurpose::Copy
        }
        StorageAccessPurpose::Borrow(BorrowKind::Shared) => {
            DiagnosticStorageAccessPurpose::SharedBorrow
        }
        StorageAccessPurpose::Borrow(BorrowKind::Mutable) => {
            DiagnosticStorageAccessPurpose::MutableBorrow
        }
        StorageAccessPurpose::Assignment => DiagnosticStorageAccessPurpose::Assignment,
        StorageAccessPurpose::Member => DiagnosticStorageAccessPurpose::MemberSelection,
        StorageAccessPurpose::Index => DiagnosticStorageAccessPurpose::IndexSelection,
        StorageAccessPurpose::Slice => DiagnosticStorageAccessPurpose::SliceSelection,
        StorageAccessPurpose::Projection => DiagnosticStorageAccessPurpose::Projection,
    }
}
