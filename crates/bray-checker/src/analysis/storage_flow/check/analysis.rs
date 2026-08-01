use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BorrowCapabilityId, BoundDependencySubject, BoundExpressionId,
    CheckedMemoryOperationKind, CheckedMemoryOperations, CheckedRefinementFacts, LivenessFacts,
    MemoryOperationDecision, MemoryOperationStatus, RefinementFact, RefinementFactKind,
    StorageAccessId, StorageAccessPlan, StorageAccessPurpose, StorageAccessRoot,
    StorageExitDecision, StorageFlowFacts, StorageIdentity, StorageIdentityId,
    StorageOperationDecision, StorageOperationStatus, StoragePlan, StorageProjection,
    StorageRelationship, StorageSuspensionState,
};
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};
use bray_symbols::{BorrowKind, CallableSignatureFact};

use crate::{
    CheckerFactError, CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext,
    CheckerSemanticFactProvider, CheckerUnitView,
};

use super::availability::{projection_is_available, storage_is_recovered};
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

const fn operation_requires_trust(kind: CheckedMemoryOperationKind) -> bool {
    matches!(
        kind,
        CheckedMemoryOperationKind::Reinterpret { .. }
            | CheckedMemoryOperationKind::Read { .. }
            | CheckedMemoryOperationKind::Write { .. }
            | CheckedMemoryOperationKind::Copy { .. }
            | CheckedMemoryOperationKind::RawAllocate
            | CheckedMemoryOperationKind::RawDeallocate
            | CheckedMemoryOperationKind::Allocate
            | CheckedMemoryOperationKind::Deallocate
            | CheckedMemoryOperationKind::ByteBufferFill
            | CheckedMemoryOperationKind::ByteBufferCopy
            | CheckedMemoryOperationKind::ByteBufferRead
            | CheckedMemoryOperationKind::ByteBufferRelease
    )
}

fn copy_raw_state(
    state: &mut StorageFlowState,
    source: StorageIdentityId,
    destination: StorageIdentityId,
) {
    if let Some(initialized) = state.raw_initialized.get(&source).cloned() {
        state.raw_initialized.insert(destination, initialized);
    }

    if state.active_allocations.contains(&source) {
        state.active_allocations.insert(destination);
    }

    if state.invalidated_allocations.contains(&source) {
        state.invalidated_allocations.insert(destination);
    }
}

const fn conservative_memory_status(
    current: MemoryOperationStatus,
    candidate: MemoryOperationStatus,
) -> MemoryOperationStatus {
    if memory_status_rank(candidate) > memory_status_rank(current) {
        candidate
    } else {
        current
    }
}

const fn memory_status_rank(status: MemoryOperationStatus) -> u8 {
    match status {
        MemoryOperationStatus::Unreachable => 0,
        MemoryOperationStatus::Valid => 1,
        MemoryOperationStatus::Recovered => 2,
        MemoryOperationStatus::MissingTrustedFacts => 3,
        MemoryOperationStatus::InvalidatedAllocation => 4,
        MemoryOperationStatus::UninitializedRawStorage => 5,
        MemoryOperationStatus::OutstandingObligations => 6,
    }
}

const fn memory_diagnostic_kind(status: MemoryOperationStatus) -> Option<DiagnosticKind> {
    match status {
        MemoryOperationStatus::MissingTrustedFacts => {
            Some(DiagnosticKind::CheckingMissingTrustedMemoryFacts)
        }
        MemoryOperationStatus::InvalidatedAllocation => {
            Some(DiagnosticKind::CheckingMemoryOperationAfterDeallocation)
        }
        MemoryOperationStatus::UninitializedRawStorage => {
            Some(DiagnosticKind::CheckingUninitializedRawStorage)
        }
        MemoryOperationStatus::OutstandingObligations => {
            Some(DiagnosticKind::CheckingDeallocationWithOutstandingObligations)
        }
        MemoryOperationStatus::Unreachable
        | MemoryOperationStatus::Valid
        | MemoryOperationStatus::Recovered => None,
    }
}

pub(crate) struct StorageFlowCollector<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    request: CheckerUnitView<'analysis, C>,
    storage: &'analysis StoragePlan,
    liveness: &'analysis LivenessFacts,
    refinements: &'analysis CheckedRefinementFacts,
    memory: &'analysis CheckedMemoryOperations,
    input: &'analysis StorageFlowInput,
    statuses: BTreeMap<StorageAccessPlan, StorageOperationStatus>,
    suspensions: Vec<StorageSuspensionState>,
    exits: Vec<StorageExitDecision>,
    memory_decisions: BTreeMap<BoundExpressionId, MemoryOperationStatus>,
    diagnostics: DiagnosticBag,
    reported_diagnostics: BTreeSet<(DiagnosticKind, StorageAccessId)>,
    reported_memory_diagnostics: BTreeSet<(DiagnosticKind, BoundExpressionId)>,
    publish: bool,
    is_recovered: bool,
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

    fn transfer_raw_pointer_state(&self, state: &mut StorageFlowState, node: AnyBoundNodeId) {
        let mut sources = Vec::new();
        let mut destinations = Vec::new();

        for plan in self.input.plans(node) {
            let Some(root) = self.storage.root_identity(plan.access()) else {
                continue;
            };

            match self.effective_purpose(*plan) {
                StorageAccessPurpose::Move
                | StorageAccessPurpose::Copy
                | StorageAccessPurpose::ValueTransfer => sources.push((root, plan.purpose())),
                StorageAccessPurpose::Initialize | StorageAccessPurpose::Assignment => {
                    destinations.push(root);
                }
                StorageAccessPurpose::Read
                | StorageAccessPurpose::Write
                | StorageAccessPurpose::Borrow(_)
                | StorageAccessPurpose::Member
                | StorageAccessPurpose::Index
                | StorageAccessPurpose::Slice
                | StorageAccessPurpose::Projection => {}
            }
        }

        let ([(source, purpose)], [destination]) = (sources.as_slice(), destinations.as_slice())
        else {
            return;
        };

        copy_raw_state(state, *source, *destination);

        if *purpose == StorageAccessPurpose::Move {
            state.raw_initialized.remove(source);
            state.active_allocations.remove(source);
            state.invalidated_allocations.remove(source);
        }
    }

    fn apply_memory_operation(
        &mut self,
        state: &mut StorageFlowState,
        node: AnyBoundNodeId,
        refinements: &[RefinementFact],
    ) {
        let AnyBoundNodeId::Expression(expression) = node else {
            return;
        };

        let Some(operation) = self.memory.operation(expression) else {
            return;
        };

        let status = if !state.reachable {
            MemoryOperationStatus::Unreachable
        } else if state.recovered {
            MemoryOperationStatus::Recovered
        } else if operation_requires_trust(operation.kind())
            && !refinements
                .iter()
                .any(|fact| matches!(fact.kind(), RefinementFactKind::TrustBoundary(_)))
        {
            MemoryOperationStatus::MissingTrustedFacts
        } else {
            self.apply_valid_memory_operation(state, operation)
        };

        self.is_recovered |= status == MemoryOperationStatus::Recovered;

        if self.publish {
            self.memory_decisions
                .entry(expression)
                .and_modify(|current| *current = conservative_memory_status(*current, status))
                .or_insert(status);

            if let Some(kind) = memory_diagnostic_kind(status) {
                self.add_memory_diagnostic(kind, expression);
            }
        }
    }

    fn apply_valid_memory_operation(
        &self,
        state: &mut StorageFlowState,
        operation: &bray_bound_tree::CheckedMemoryOperation,
    ) -> MemoryOperationStatus {
        let arguments = operation.arguments();

        match operation.kind() {
            CheckedMemoryOperationKind::Address { pointee, .. } => {
                let Some(result) = self.operation_result_storage(operation.expression()) else {
                    return MemoryOperationStatus::Recovered;
                };

                state
                    .raw_initialized
                    .entry(result)
                    .or_default()
                    .insert(pointee);
            }
            CheckedMemoryOperationKind::Null { .. } => {}
            CheckedMemoryOperationKind::IsNull { .. }
            | CheckedMemoryOperationKind::LayoutQuery { .. } => {}
            CheckedMemoryOperationKind::Offset { .. }
            | CheckedMemoryOperationKind::Reinterpret { .. } => {
                let Some(source) = arguments
                    .first()
                    .and_then(|argument| self.argument_storage(*argument))
                else {
                    return MemoryOperationStatus::Recovered;
                };

                let Some(result) = self.operation_result_storage(operation.expression()) else {
                    return MemoryOperationStatus::Recovered;
                };

                copy_raw_state(state, source, result);
            }
            CheckedMemoryOperationKind::Read { pointee, kind } => {
                let Some(pointer) = arguments
                    .first()
                    .and_then(|argument| self.argument_storage(*argument))
                else {
                    return MemoryOperationStatus::Recovered;
                };

                return apply_raw_read(state, pointer, pointee, kind);
            }
            CheckedMemoryOperationKind::Write { pointee } => {
                let Some(pointer) = arguments
                    .first()
                    .and_then(|argument| self.argument_storage(*argument))
                else {
                    return MemoryOperationStatus::Recovered;
                };

                return apply_raw_write(state, pointer, pointee);
            }
            CheckedMemoryOperationKind::Copy { pointee, .. } => {
                let [source, destination, ..] = arguments else {
                    return MemoryOperationStatus::Recovered;
                };

                let Some(source) = self.argument_storage(*source) else {
                    return MemoryOperationStatus::Recovered;
                };

                let Some(destination) = self.argument_storage(*destination) else {
                    return MemoryOperationStatus::Recovered;
                };

                return apply_raw_copy(state, source, destination, pointee);
            }
            CheckedMemoryOperationKind::RawAllocate | CheckedMemoryOperationKind::Allocate => {
                let Some(result) = self.operation_result_storage(operation.expression()) else {
                    return MemoryOperationStatus::Recovered;
                };

                state.active_allocations.insert(result);
                state.invalidated_allocations.remove(&result);
            }
            CheckedMemoryOperationKind::RawDeallocate | CheckedMemoryOperationKind::Deallocate => {
                let Some(pointer) = arguments
                    .first()
                    .and_then(|argument| self.argument_storage(*argument))
                else {
                    return MemoryOperationStatus::Recovered;
                };

                return apply_deallocation(state, pointer);
            }
            CheckedMemoryOperationKind::ByteBufferFill
            | CheckedMemoryOperationKind::ByteBufferCopy
            | CheckedMemoryOperationKind::ByteBufferRead
            | CheckedMemoryOperationKind::ByteBufferRelease => {}
        }

        MemoryOperationStatus::Valid
    }

    fn argument_storage(&self, expression: BoundExpressionId) -> Option<StorageIdentityId> {
        self.storage
            .access_plans()
            .iter()
            .filter(|plan| plan.expression() == expression)
            .find_map(|plan| self.storage.root_identity(plan.access()))
    }

    fn operation_result_storage(&self, expression: BoundExpressionId) -> Option<StorageIdentityId> {
        self.storage.identity_entries().find_map(|(id, identity)| {
            matches!(
                identity,
                StorageIdentity::Temporary(candidate) | StorageIdentity::Allocation(candidate)
                    if candidate == expression
            )
            .then_some(id)
        })
    }

    fn add_memory_diagnostic(&mut self, kind: DiagnosticKind, expression: BoundExpressionId) {
        if !self.reported_memory_diagnostics.insert((kind, expression)) {
            return;
        }

        let Some(node) = self.request.view().expression(expression) else {
            self.is_recovered = true;

            return;
        };

        let span = match self.request.source(node.origin().source_anchor()) {
            Ok(source) => source.span(),
            Err(_) => {
                self.is_recovered = true;

                return;
            }
        };

        self.diagnostics.add(
            Diagnostic::new(
                DiagnosticId::from_index(self.diagnostics.len()),
                kind,
                SeverityKind::Error,
            )
            .with_primary_span(span),
        );
    }

    pub(super) fn memory_decisions(&self) -> impl Iterator<Item = MemoryOperationDecision> + '_ {
        self.memory_decisions
            .iter()
            .map(|(expression, status)| MemoryOperationDecision::new(*expression, *status))
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

    fn effective_purpose(&self, plan: StorageAccessPlan) -> StorageAccessPurpose {
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

    fn has_borrow_conflict(
        &self,
        state: &StorageFlowState,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
    ) -> bool {
        let requested = match purpose {
            StorageAccessPurpose::Borrow(kind) => Some(kind),
            StorageAccessPurpose::Write
            | StorageAccessPurpose::Move
            | StorageAccessPurpose::Assignment => Some(BorrowKind::Mutable),
            StorageAccessPurpose::Read
            | StorageAccessPurpose::Initialize
            | StorageAccessPurpose::Copy
            | StorageAccessPurpose::ValueTransfer => Some(BorrowKind::Shared),
            StorageAccessPurpose::Member
            | StorageAccessPurpose::Index
            | StorageAccessPurpose::Slice
            | StorageAccessPurpose::Projection => None,
        };

        let Some(requested) = requested else {
            return false;
        };

        let access = self.operation_access(plan, purpose);

        let Some(authorizing_borrows) = self.borrow_chain(access) else {
            return true;
        };

        if authorizing_borrows
            .iter()
            .any(|borrow| !state.active_borrows.contains(borrow))
        {
            return true;
        }

        state.active_borrows.iter().copied().any(|active| {
            if authorizing_borrows.contains(&active) {
                return false;
            }

            let Some(capability) = self.storage.borrow_capability(active) else {
                return true;
            };

            if requested == BorrowKind::Shared && capability.kind() == BorrowKind::Shared {
                return false;
            }

            self.storage.relationship(capability.access(), access) != StorageRelationship::Disjoint
        })
    }

    fn has_mutation_authority(&self, access: StorageAccessId) -> bool {
        let Some(storage_access) = self.storage.access(access) else {
            return false;
        };

        match storage_access.root() {
            StorageAccessRoot::Borrow(_) => self.borrow_chain(access).is_some_and(|borrows| {
                !borrows.is_empty()
                    && borrows.iter().all(|borrow| {
                        self.storage
                            .borrow_capability(*borrow)
                            .is_some_and(|borrow| borrow.kind() == BorrowKind::Mutable)
                    })
            }),
            StorageAccessRoot::Recovery(_) => false,
            StorageAccessRoot::Storage(storage)
            | StorageAccessRoot::OwnedIndirection { storage, .. } => {
                self.owned_storage_is_mutable(storage)
            }
        }
    }

    fn fields_allow_mutation(&self, access: StorageAccessId) -> bool {
        let Some(access) = self.storage.access(access) else {
            return false;
        };

        access
            .projections()
            .iter()
            .all(|projection| match projection {
                StorageProjection::ProductField(field) => self
                    .request
                    .symbols()
                    .struct_field(*field)
                    .is_some_and(bray_symbols::StructFieldSymbol::allows_mutation),
                StorageProjection::ActiveUnionPayloadField { field, .. } => self
                    .request
                    .symbols()
                    .union_payload_field(*field)
                    .is_some_and(bray_symbols::UnionPayloadFieldSymbol::allows_mutation),
                StorageProjection::TupleElement(_)
                | StorageProjection::ElementFromStart(_)
                | StorageProjection::ElementFromEnd(_)
                | StorageProjection::Element(_)
                | StorageProjection::SliceRange { .. }
                | StorageProjection::NullableValue
                | StorageProjection::OwnedTarget => true,
            })
    }

    fn operation_access(
        &self,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
    ) -> StorageAccessId {
        match purpose {
            StorageAccessPurpose::Borrow(_) => self
                .input
                .borrow(plan)
                .and_then(|borrow| self.storage.borrow_capability(borrow))
                .map(|borrow| borrow.access())
                .unwrap_or_else(|| plan.access()),
            StorageAccessPurpose::Read
            | StorageAccessPurpose::Write
            | StorageAccessPurpose::Initialize
            | StorageAccessPurpose::Assignment
            | StorageAccessPurpose::Copy
            | StorageAccessPurpose::Move
            | StorageAccessPurpose::ValueTransfer
            | StorageAccessPurpose::Member
            | StorageAccessPurpose::Index
            | StorageAccessPurpose::Slice
            | StorageAccessPurpose::Projection => plan.access(),
        }
    }

    fn access_uses_borrow(&self, access: StorageAccessId) -> bool {
        self.storage
            .access(access)
            .is_some_and(|access| matches!(access.root(), StorageAccessRoot::Borrow(_)))
    }

    fn type_is_borrow(&self, ty: bray_symbols::TypeId) -> bool {
        self.request
            .semantic_values()
            .type_data(ty)
            .is_ok_and(|data| matches!(data.as_ref(), bray_symbols::TypeData::Borrow { .. }))
    }

    fn refinements_allow_access(
        &self,
        access: StorageAccessId,
        refinements: &[RefinementFact],
    ) -> bool {
        let Some(storage_access) = self.storage.access(access) else {
            return false;
        };

        for projection in storage_access.projections() {
            if !projection_is_available(self.storage, access, *projection, refinements) {
                return false;
            }
        }

        true
    }

    fn borrow_chain(&self, access: StorageAccessId) -> Option<Vec<BorrowCapabilityId>> {
        let mut capability = match self.storage.access(access)?.root() {
            StorageAccessRoot::Borrow(capability) => Some(capability),
            StorageAccessRoot::Storage(_)
            | StorageAccessRoot::OwnedIndirection { .. }
            | StorageAccessRoot::Recovery(_) => None,
        };

        let mut chain = Vec::new();

        while let Some(current) = capability {
            let borrow = self.storage.borrow_capability(current)?;

            chain.push(current);
            capability = borrow.parent();
        }

        Some(chain)
    }

    fn mutation_authority_access(
        &self,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
    ) -> Option<StorageAccessId> {
        match purpose {
            StorageAccessPurpose::Write | StorageAccessPurpose::Assignment => Some(plan.access()),
            StorageAccessPurpose::Borrow(BorrowKind::Mutable) => self
                .input
                .borrow(plan)
                .and_then(|borrow| self.storage.borrow_capability(borrow))
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

    fn owned_storage_is_mutable(&self, storage: StorageIdentityId) -> bool {
        match self.storage.identity(storage) {
            Some(StorageIdentity::LocalOwned(AnyBoundNodeId::Pattern(pattern))) => self
                .request
                .view()
                .pattern(pattern)
                .is_some_and(bray_bound_tree::BoundPattern::is_mutable),
            Some(StorageIdentity::Alternative { pattern, .. }) => self
                .request
                .view()
                .pattern(pattern)
                .is_some_and(bray_bound_tree::BoundPattern::is_mutable),
            Some(
                StorageIdentity::Result(_)
                | StorageIdentity::Temporary(_)
                | StorageIdentity::IterationCursor(_)
                | StorageIdentity::IterationElement(_)
                | StorageIdentity::Allocation(_)
                | StorageIdentity::CompilerCreated(_),
            ) => true,
            Some(StorageIdentity::Parameter(_) | StorageIdentity::Receiver(_)) => {
                self.input.storage_is_mutable(storage)
            }
            Some(
                StorageIdentity::LocalOwned(_)
                | StorageIdentity::AnonymousParameter(_)
                | StorageIdentity::PredicateParameter(_)
                | StorageIdentity::Error(_),
            )
            | None => false,
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
            self.liveness
                .is_live_across_scope(block, BoundDependencySubject::BorrowCapability(*borrow))
        });

        state.definitely_active_borrows.retain(|borrow| {
            self.liveness
                .is_live_across_scope(block, BoundDependencySubject::BorrowCapability(*borrow))
        });
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

fn apply_raw_read(
    state: &mut StorageFlowState,
    pointer: StorageIdentityId,
    pointee: bray_symbols::TypeId,
    kind: bray_bound_tree::MemoryReadKind,
) -> MemoryOperationStatus {
    if state.invalidated_allocations.contains(&pointer) {
        return MemoryOperationStatus::InvalidatedAllocation;
    }

    let Some(initialized) = state.raw_initialized.get_mut(&pointer) else {
        return MemoryOperationStatus::UninitializedRawStorage;
    };

    if !initialized.contains(&pointee) {
        return MemoryOperationStatus::UninitializedRawStorage;
    }

    if kind == bray_bound_tree::MemoryReadKind::Move {
        initialized.remove(&pointee);
    }

    MemoryOperationStatus::Valid
}

fn apply_raw_write(
    state: &mut StorageFlowState,
    pointer: StorageIdentityId,
    pointee: bray_symbols::TypeId,
) -> MemoryOperationStatus {
    if state.invalidated_allocations.contains(&pointer) {
        return MemoryOperationStatus::InvalidatedAllocation;
    }

    state
        .raw_initialized
        .entry(pointer)
        .or_default()
        .insert(pointee);

    MemoryOperationStatus::Valid
}

fn apply_raw_copy(
    state: &mut StorageFlowState,
    source: StorageIdentityId,
    destination: StorageIdentityId,
    pointee: bray_symbols::TypeId,
) -> MemoryOperationStatus {
    if state.invalidated_allocations.contains(&source)
        || state.invalidated_allocations.contains(&destination)
    {
        return MemoryOperationStatus::InvalidatedAllocation;
    }

    if !state
        .raw_initialized
        .get(&source)
        .is_some_and(|initialized| initialized.contains(&pointee))
    {
        return MemoryOperationStatus::UninitializedRawStorage;
    }

    state
        .raw_initialized
        .entry(destination)
        .or_default()
        .insert(pointee);

    MemoryOperationStatus::Valid
}

fn apply_deallocation(
    state: &mut StorageFlowState,
    pointer: StorageIdentityId,
) -> MemoryOperationStatus {
    if state.invalidated_allocations.contains(&pointer) {
        return MemoryOperationStatus::InvalidatedAllocation;
    }

    if state
        .raw_initialized
        .get(&pointer)
        .is_some_and(|initialized| !initialized.is_empty())
    {
        return MemoryOperationStatus::OutstandingObligations;
    }

    state.active_allocations.remove(&pointer);
    state.raw_initialized.remove(&pointer);
    state.invalidated_allocations.insert(pointer);

    MemoryOperationStatus::Valid
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundErrorExpression, BoundExpression, BoundUnitId, BoundUnitKind, MemoryOperationStatus,
        MemoryReadKind, StorageIdentity, StoragePlanBuilder,
    };

    use super::{
        StorageFlowState, apply_deallocation, apply_raw_copy, apply_raw_read, apply_raw_write,
    };
    use crate::test_support::{error_type, expression_unit, push_expression};

    #[test]
    fn raw_memory_transitions_preserve_initialization_and_invalidation() {
        let unit = BoundUnitId::new(41);

        let (_, expressions) = expression_unit(unit, |tree, origin| {
            (0..2)
                .map(|_| {
                    push_expression(
                        tree,
                        BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
                    )
                })
                .collect::<Vec<_>>()
        });

        let mut storage = StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);

        let source = storage
            .push_identity(StorageIdentity::Temporary(expressions[0]))
            .unwrap_or_else(|error| panic!("source storage must validate: {error:?}"));

        let destination = storage
            .push_identity(StorageIdentity::Temporary(expressions[1]))
            .unwrap_or_else(|error| panic!("destination storage must validate: {error:?}"));

        let ty = error_type();
        let mut state = StorageFlowState::default();

        assert_eq!(
            apply_raw_write(&mut state, source, ty),
            MemoryOperationStatus::Valid
        );

        assert_eq!(
            apply_raw_copy(&mut state, source, destination, ty),
            MemoryOperationStatus::Valid
        );

        assert_eq!(
            apply_raw_read(&mut state, source, ty, MemoryReadKind::Copy),
            MemoryOperationStatus::Valid
        );

        assert_eq!(
            apply_raw_read(&mut state, source, ty, MemoryReadKind::Move),
            MemoryOperationStatus::Valid
        );

        assert_eq!(
            apply_raw_read(&mut state, source, ty, MemoryReadKind::Move),
            MemoryOperationStatus::UninitializedRawStorage
        );

        assert_eq!(
            apply_deallocation(&mut state, destination),
            MemoryOperationStatus::OutstandingObligations
        );

        state.raw_initialized.remove(&destination);

        assert_eq!(
            apply_deallocation(&mut state, destination),
            MemoryOperationStatus::Valid
        );

        assert_eq!(
            apply_raw_write(&mut state, destination, ty),
            MemoryOperationStatus::InvalidatedAllocation
        );
    }
}
