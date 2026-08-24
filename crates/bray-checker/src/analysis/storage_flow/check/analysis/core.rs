// rust-style: allow(module-too-large, reason = "storage flow collection is one transition engine whose diagnostics require the same plan, state, and decision context")

use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BorrowCapabilityId, BoundDependencySubject, BoundExpressionId,
    CheckedMemoryOperations, CheckedRefinements, CheckedSemanticSelections, Liveness,
    MemoryOperationStatus, Refinement, StorageAccessId, StorageAccessPlan, StorageAccessPurpose,
    StorageAccessRoot, StorageBinding, StorageExitDecision, StorageFlow, StorageIdentity,
    StorageOperationDecision, StorageOperationStatus, StoragePlan, StorageProjection,
    StorageRelationship, StorageSuspensionState,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticRelatedLocation, DiagnosticRelatedLocationKind,
    DiagnosticStorageAccess, DiagnosticStorageAccessPurpose, DiagnosticStorageProjection,
    DiagnosticStorageRoot, SeverityKind,
};
use bray_symbols::{AnySymbolId, BorrowKind, CallableSignatureQuery};

use crate::storage::StorageScopeOwners;
use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerQueryError, CheckerRequestContext,
    CheckerSemanticQueryProvider, CheckerUnitView,
};
use crate::unit::semantic_inputs_match;

use super::super::availability::storage_is_recovered;
use crate::analysis::build::{ControlFlowGraphBuildOutcome, build_storage_control_flow_graph};
use crate::analysis::fixed_point::{FixedPointOutcome, solve_fixed_point};
use crate::analysis::model::{AnalysisOperation, AnalysisOperationKind, AnalysisScopeExitPhase};
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
) -> CheckerOutcome<StorageFlow>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    if !semantic_inputs_match(
        request,
        [
            (selections.unit(), selections.kind()),
            (storage.unit(), storage.kind()),
            (liveness.unit(), liveness.kind()),
            (refinements.unit(), refinements.kind()),
            (memory.unit(), memory.kind()),
        ],
    ) {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidStorageFlow,
        );
    }

    let graph = match build_storage_control_flow_graph(request, storage, selections) {
        ControlFlowGraphBuildOutcome::Complete(graph) => graph,
        ControlFlowGraphBuildOutcome::Cancelled => return CheckerOutcome::Cancelled,
    };

    check_storage_flow_with_graph(
        request,
        storage,
        liveness,
        refinements,
        memory,
        &graph,
    )
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
) -> CheckerOutcome<StorageFlow>
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
        }
    }

    let (copyable_types, copyability_diagnostics) = copyability.into_parts();

    let (mutable_storage, authority_diagnostics) = match mutable_storage(request, storage) {
        Ok(result) => result,
        Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Infrastructure(error)) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
    };

    let input = StorageFlowInput::new(request, storage, copyable_types, mutable_storage);

    let owners = match StorageScopeOwners::collect(request) {
        Ok(owners) => owners,
        Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Infrastructure(error)) => {
            return CheckerOutcome::InfrastructureFailure(error);
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

    if let Some(error) = collector.infrastructure_failure.take() {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    let decisions = collector.decisions().collect::<Vec<_>>();
    let memory_decisions = collector.memory_decisions().collect::<Vec<_>>();

    let analysis = match StorageFlow::try_new(
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
    .and_then(|analysis| analysis.with_memory_operations(memory, memory_decisions))
    {
        Ok(analysis) => analysis,
        Err(_) => {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidStorageFlow,
            );
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
    pub(super) exits: Vec<StorageExitDecision>,
    pub(super) memory_decisions: BTreeMap<BoundExpressionId, MemoryOperationStatus>,
    pub(super) diagnostics: DiagnosticBag,
    pub(super) reported_diagnostics: BTreeSet<(DiagnosticKind, StorageAccessId)>,
    pub(super) reported_memory_diagnostics: BTreeSet<(DiagnosticKind, BoundExpressionId)>,
    pub(super) publish: bool,
    pub(super) is_recovered: bool,
    pub(super) infrastructure_failure: Option<CheckerInfrastructureError>,
}

impl<'analysis, C> StorageFlowCollector<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
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
        if let AnalysisOperationKind::Suspension { expression, .. } = operation.kind() {
            self.record_suspension(state, expression);
        }

        if matches!(operation.kind(), AnalysisOperationKind::Recovery(_)) {
            state.recovered = true;
            self.is_recovered = true;
        }

        self.initialize_operation_storage(state, operation.kind().node());

        let refinements = self.refinements.refinements_before(operation.kind().node());

        for plan in self.input.plans(operation.kind().node()) {
            self.apply_plan(state, *plan, refinements);
        }

        self.transfer_raw_pointer_state(state, operation.kind().node());
        self.apply_memory_operation(state, operation.kind().node(), refinements);
        self.transfer_memory_result_state(state, operation.kind().node());

        self.end_last_use_borrows(state, operation.kind().node());

        if let AnalysisOperationKind::ScopeExit {
            block,
            exit,
            phase: AnalysisScopeExitPhase::LifecycleResolution,
        } = operation.kind()
        {
            self.record_exit(state, block, exit);
            self.end_scope(state, block);
        }
    }

    fn apply_plan(
        &mut self,
        state: &mut StorageFlowState,
        plan: StorageAccessPlan,
        refinements: &[Refinement],
    ) {
        let purpose = self.effective_purpose(plan);
        let outcome = self.operation_status(state, plan, purpose, refinements);
        let status = outcome.status;
        let borrow = self.input.borrow(plan);

        if matches!(
            status,
            StorageOperationStatus::Uninitialized
                | StorageOperationStatus::InactiveProjection
                | StorageOperationStatus::NotCopyable
        ) {
            self.infrastructure_failure = Some(CheckerInfrastructureError::InvalidStorageFlow);

            return;
        }

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
            self.add_diagnostic(kind, plan, purpose, outcome.origin);
        }
    }

    fn operation_status(
        &self,
        state: &StorageFlowState,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
        refinements: &[Refinement],
    ) -> StorageOperationOutcome {
        let Some(access) = self.storage.access(plan.access()) else {
            return StorageOperationOutcome::status(StorageOperationStatus::Recovered);
        };

        if access.is_recovered() {
            return StorageOperationOutcome::status(StorageOperationStatus::Recovered);
        }

        if !self.pattern_establishes_projection(plan.access())
            && !self.refinements_allow_access(plan.access(), refinements)
        {
            return StorageOperationOutcome::status(StorageOperationStatus::InactiveProjection);
        }

        let Some(root) = self.storage.root_identity(plan.access()) else {
            return StorageOperationOutcome::status(StorageOperationStatus::Recovered);
        };

        let requires_value = matches!(
            purpose,
            StorageAccessPurpose::Read
                | StorageAccessPurpose::Copy
                | StorageAccessPurpose::Move
                | StorageAccessPurpose::Borrow(_)
        );

        if requires_value && !state.initialized.contains(&root) {
            return StorageOperationOutcome::status(StorageOperationStatus::Uninitialized);
        }

        if requires_value && !self.pattern_establishes_projection(plan.access()) {
            let origins = self.moved_origins(state, plan.access());

            if !origins.is_empty() {
                return StorageOperationOutcome::moved(origins);
            }
        }

        let operation_access = self.operation_access(plan, purpose);

        if purpose == StorageAccessPurpose::Move
            && self.access_uses_borrow(operation_access)
            && !self.type_is_borrow(access.reached_type())
        {
            return StorageOperationOutcome::status(StorageOperationStatus::MissingOwnership);
        }

        if let Some(conflict) = self.borrow_conflict(state, plan, purpose) {
            return StorageOperationOutcome::borrow_conflict(conflict);
        }

        if let Some(authority_access) = self.mutation_authority_access(plan, purpose)
            && (!self.has_mutation_authority(authority_access)
                || !self.fields_allow_mutation(authority_access))
        {
            return StorageOperationOutcome::status(
                StorageOperationStatus::MissingMutationAuthority,
            );
        }

        if purpose == StorageAccessPurpose::Copy
            && !self.input.type_is_copyable(access.reached_type())
        {
            return StorageOperationOutcome::status(StorageOperationStatus::NotCopyable);
        }

        StorageOperationOutcome::status(StorageOperationStatus::Valid)
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
                state.moved.insert(plan.access(), plan.expression());

                if self.move_consumes_complete_union_payload(plan.access())
                    && let Some(root) = self.root_access(plan.access())
                {
                    state.moved.insert(root, plan.expression());
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
                    .retain(|moved, _| !self.storage.access_contains(plan.access(), *moved));
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

        state.moved.retain(|access, _| {
            self.storage
                .root_identity(*access)
                .is_none_or(|storage| !definitions.contains(&storage))
        });

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
                self.storage.relationship(**moved, access) != StorageRelationship::Disjoint
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

                capability.entry_binding().is_none()
                    && ((moved_borrows.contains(borrow)
                        && (!retained_for_suspension || completed_retaining_suspension))
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

        state.moved.retain(|access, _| {
            self.storage.access(*access).is_none_or(|access| {
                access
                    .root()
                    .borrow_capability()
                    .is_none_or(|borrow| !ended.contains(&borrow))
            })
        });
    }

    fn end_scope(&self, state: &mut StorageFlowState, block: bray_bound_tree::BoundBlockId) {
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

    fn record_exit(
        &mut self,
        state: &StorageFlowState,
        block: bray_bound_tree::BoundBlockId,
        exit: AnyBoundNodeId,
    ) {
        if !self.publish {
            return;
        }

        self.exits.push(StorageExitDecision::new(
            block,
            exit,
            state.initialized.iter().copied(),
            state.moved.keys().copied(),
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
            self.infrastructure_failure = Some(CheckerInfrastructureError::InvalidStorageFlow);
            return;
        };

        let source = match self.request.source(access.source()) {
            Ok(source) => source,
            Err(error) => {
                self.infrastructure_failure = Some(error);

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

        let id = u32::try_from(self.diagnostics.len()).unwrap_or(u32::MAX);

        let mut diagnostic = Diagnostic::new(DiagnosticId::new(id), kind, SeverityKind::Error)
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
    ) -> Result<DiagnosticStorageAccess, CheckerInfrastructureError> {
        let access = self
            .storage
            .access(access_id)
            .ok_or(CheckerInfrastructureError::InvalidStorageFlow)?;

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
            None => return Err(CheckerInfrastructureError::InvalidStorageFlow),
        };

        Ok(root)
    }

    fn diagnostic_storage_projection(
        &self,
        projection: StorageProjection,
    ) -> Result<DiagnosticStorageProjection, CheckerInfrastructureError> {
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
    ) -> Result<String, CheckerInfrastructureError> {
        self.request
            .context()
            .symbols()
            .member_name(symbol)
            .map(|name| name.as_str().to_owned())
            .ok_or(CheckerInfrastructureError::InvalidStorageFlow)
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
