use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BorrowCapabilityId, BoundDependencySubject, CheckedRefinementFacts,
    LivenessFacts, StorageAccessId, StorageAccessPlan, StorageAccessPurpose, StorageAccessRoot,
    StorageExitDecision, StorageFlowFacts, StorageIdentity, StorageIdentityId,
    StorageOperationDecision, StorageOperationStatus, StoragePlan, StorageRelationship,
};
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};
use bray_symbols::{BorrowKind, CallableSignatureFact};

use crate::{
    CheckerFactError, CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext,
    CheckerSemanticFactProvider, CheckerUnitView,
};

use super::super::build::{ControlFlowGraphBuildOutcome, build_control_flow_graph};
use super::super::fixed_point::{FixedPointOutcome, solve_fixed_point};
use super::super::model::{AnalysisOperation, AnalysisOperationKind, AnalysisScopeExitPhase};
use super::super::reachability::analyze_reachability;
use super::authority::mutable_storage;
use super::copyability::CopyabilityResolver;
use super::model::{StorageFlowDomain, StorageFlowInput, StorageFlowState};

pub(crate) fn check_storage_flow<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    liveness: &LivenessFacts,
    refinements: &CheckedRefinementFacts,
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
    {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidStorageFlowFacts,
        );
    }

    let graph = match build_control_flow_graph(request) {
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

    let domain = StorageFlowDomain::new(&graph, &reachability, storage, liveness, &input, request);

    let result = match solve_fixed_point(&graph, &domain, &request) {
        FixedPointOutcome::Complete(result) => result,
        FixedPointOutcome::Cancelled => return CheckerOutcome::Cancelled,
        FixedPointOutcome::ConvergenceInvariantViolated => {
            panic!("finite storage-flow analysis exceeded its convergence bound")
        }
    };

    let mut collector = StorageFlowCollector::new(request, storage, liveness, &input);

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

    let facts = match StorageFlowFacts::try_new(
        storage.unit(),
        storage.kind(),
        decisions,
        collector.exits,
        collector.is_recovered
            || storage_is_recovered(storage)
            || liveness.is_recovered()
            || refinements.is_recovered(),
    ) {
        Ok(facts) => facts,
        Err(_) => {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidStorageFlowFacts,
            );
        }
    };

    CheckerOutcome::complete(facts, collector.diagnostics)
}

pub(super) struct StorageFlowCollector<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    request: CheckerUnitView<'analysis, C>,
    storage: &'analysis StoragePlan,
    liveness: &'analysis LivenessFacts,
    input: &'analysis StorageFlowInput,
    statuses: BTreeMap<StorageAccessPlan, StorageOperationStatus>,
    borrows: BTreeMap<StorageAccessPlan, BorrowCapabilityId>,
    exits: Vec<StorageExitDecision>,
    diagnostics: DiagnosticBag,
    reported_diagnostics: BTreeSet<(DiagnosticKind, StorageAccessId)>,
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
        input: &'analysis StorageFlowInput,
    ) -> Self {
        Self {
            request,
            storage,
            liveness,
            input,
            statuses: BTreeMap::new(),
            borrows: BTreeMap::new(),
            exits: Vec::new(),
            diagnostics: DiagnosticBag::new(),
            reported_diagnostics: BTreeSet::new(),
            publish: true,
            is_recovered: false,
        }
    }

    pub(super) fn without_publication(
        request: CheckerUnitView<'analysis, C>,
        storage: &'analysis StoragePlan,
        liveness: &'analysis LivenessFacts,
        input: &'analysis StorageFlowInput,
    ) -> Self {
        Self {
            publish: false,
            ..Self::new(request, storage, liveness, input)
        }
    }

    pub(super) fn apply_operation(
        &mut self,
        state: &mut StorageFlowState,
        operation: &AnalysisOperation,
    ) {
        if matches!(operation.kind(), AnalysisOperationKind::Recovery(_)) {
            state.recovered = true;
            self.is_recovered = true;
        }

        self.initialize_operation_storage(state, operation.kind().node());

        for plan in self.input.plans(operation.kind().node()) {
            self.apply_plan(state, *plan);
        }

        self.end_last_use_borrows(state, operation.kind().node());

        if let AnalysisOperationKind::ScopeExit {
            block,
            phase: AnalysisScopeExitPhase::LifecycleResolution,
        } = operation.kind()
        {
            self.record_exit(state, block);
        }
    }

    fn apply_plan(&mut self, state: &mut StorageFlowState, plan: StorageAccessPlan) {
        let purpose = self.effective_purpose(plan);
        let status = self.operation_status(state, plan, purpose);
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

        if let Some(borrow) = borrow {
            self.borrows.insert(plan, borrow);
        }

        if let Some(kind) = diagnostic_kind(status) {
            self.add_diagnostic(kind, plan.access());
        }
    }

    fn operation_status(
        &self,
        state: &StorageFlowState,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
    ) -> StorageOperationStatus {
        let Some(access) = self.storage.access(plan.access()) else {
            return StorageOperationStatus::Recovered;
        };

        if state.recovered || access.is_recovered() {
            return StorageOperationStatus::Recovered;
        }

        let Some(root) = self.storage.root_identity(plan.access()) else {
            return StorageOperationStatus::Recovered;
        };

        let requires_value = matches!(
            purpose,
            StorageAccessPurpose::Read
                | StorageAccessPurpose::Write
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

        if self.has_borrow_conflict(state, plan, purpose) {
            return StorageOperationStatus::ConflictingBorrow;
        }

        if matches!(
            purpose,
            StorageAccessPurpose::Write | StorageAccessPurpose::Assignment
        ) && !self.has_mutation_authority(plan.access())
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
        state
            .initialized
            .extend(self.input.definitions(node).iter().copied());
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

        let own_parent = self
            .input
            .borrow(plan)
            .and_then(|borrow| self.storage.borrow_capability(borrow))
            .and_then(|borrow| borrow.parent());

        state.active_borrows.iter().copied().any(|active| {
            if Some(active) == own_parent {
                return false;
            }

            let Some(capability) = self.storage.borrow_capability(active) else {
                return true;
            };

            if requested == BorrowKind::Shared && capability.kind() == BorrowKind::Shared {
                return false;
            }

            self.storage
                .relationship(capability.access(), plan.access())
                != StorageRelationship::Disjoint
        })
    }

    fn has_mutation_authority(&self, access: StorageAccessId) -> bool {
        let Some(access) = self.storage.access(access) else {
            return false;
        };

        match access.root() {
            StorageAccessRoot::Borrow(capability) => self
                .storage
                .borrow_capability(capability)
                .is_some_and(|capability| capability.kind() == BorrowKind::Mutable),
            StorageAccessRoot::Recovery(_) => false,
            StorageAccessRoot::Storage(storage)
            | StorageAccessRoot::OwnedIndirection { storage, .. } => {
                self.owned_storage_is_mutable(storage)
            }
        }
    }

    fn owned_storage_is_mutable(&self, storage: StorageIdentityId) -> bool {
        match self.storage.identity(storage) {
            Some(StorageIdentity::LocalOwned(AnyBoundNodeId::Pattern(pattern))) => self
                .request
                .view()
                .pattern(pattern)
                .is_some_and(bray_bound_tree::BoundPattern::is_mutable),
            Some(
                StorageIdentity::Result(_)
                | StorageIdentity::Temporary(_)
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
        state.active_borrows.retain(|borrow| {
            !self
                .liveness
                .is_last_use(operation, BoundDependencySubject::BorrowCapability(*borrow))
        });
    }

    fn record_exit(&mut self, state: &StorageFlowState, block: bray_bound_tree::BoundBlockId) {
        if !self.publish {
            return;
        }

        self.exits.push(StorageExitDecision::new(
            block,
            state.initialized.iter().copied(),
            state.active_borrows.iter().copied(),
            state.recovered,
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
        self.storage
            .access_plans()
            .iter()
            .copied()
            .filter_map(|plan| {
                let status = self.statuses.get(&plan).copied()?;

                Some(StorageOperationDecision::new(
                    plan.expression(),
                    self.effective_purpose(plan),
                    plan.access(),
                    self.borrows.get(&plan).copied(),
                    status,
                ))
            })
    }
}

fn storage_is_recovered(storage: &StoragePlan) -> bool {
    storage
        .identities()
        .iter()
        .any(|identity| matches!(identity, StorageIdentity::Error(_)))
        || storage
            .accesses()
            .iter()
            .any(|access| access.is_recovered())
        || storage
            .borrow_capabilities()
            .iter()
            .any(|capability| capability.is_recovered())
}

const fn diagnostic_kind(status: StorageOperationStatus) -> Option<DiagnosticKind> {
    match status {
        StorageOperationStatus::Uninitialized => {
            Some(DiagnosticKind::CheckingUseOfUninitializedStorage)
        }
        StorageOperationStatus::Moved => Some(DiagnosticKind::CheckingUseOfMovedStorage),
        StorageOperationStatus::ConflictingBorrow => {
            Some(DiagnosticKind::CheckingConflictingBorrow)
        }
        StorageOperationStatus::MissingMutationAuthority => {
            Some(DiagnosticKind::CheckingMissingMutationAuthority)
        }
        StorageOperationStatus::NotCopyable => Some(DiagnosticKind::CheckingTypeIsNotCopyable),
        StorageOperationStatus::Valid | StorageOperationStatus::Recovered => None,
    }
}

const fn more_conservative(
    current: StorageOperationStatus,
    incoming: StorageOperationStatus,
) -> StorageOperationStatus {
    if status_rank(incoming) > status_rank(current) {
        incoming
    } else {
        current
    }
}

const fn status_rank(status: StorageOperationStatus) -> u8 {
    match status {
        StorageOperationStatus::Valid => 0,
        StorageOperationStatus::Recovered => 1,
        StorageOperationStatus::Uninitialized => 2,
        StorageOperationStatus::Moved => 3,
        StorageOperationStatus::MissingMutationAuthority => 4,
        StorageOperationStatus::NotCopyable => 5,
        StorageOperationStatus::ConflictingBorrow => 6,
    }
}
