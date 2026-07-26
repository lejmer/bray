use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BorrowCapabilityId, BoundDependencySubject, CheckedRefinementFacts,
    LivenessFacts, PatternPredicate, RefinementFact, RefinementFactKind, StorageAccessId,
    StorageAccessPlan, StorageAccessPurpose, StorageAccessRoot, StorageExitDecision,
    StorageFlowFacts, StorageIdentity, StorageIdentityId, StorageOperationDecision,
    StorageOperationStatus, StoragePlan, StorageProjection, StorageRelationship,
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
use super::decision::{diagnostic_kind, more_conservative};
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

    let domain = StorageFlowDomain::new(
        &graph,
        &reachability,
        storage,
        liveness,
        refinements,
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

    let mut collector = StorageFlowCollector::new(request, storage, liveness, refinements, &input);

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
    refinements: &'analysis CheckedRefinementFacts,
    input: &'analysis StorageFlowInput,
    statuses: BTreeMap<StorageAccessPlan, StorageOperationStatus>,
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
        refinements: &'analysis CheckedRefinementFacts,
        input: &'analysis StorageFlowInput,
    ) -> Self {
        Self {
            request,
            storage,
            liveness,
            refinements,
            input,
            statuses: BTreeMap::new(),
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
        refinements: &'analysis CheckedRefinementFacts,
        input: &'analysis StorageFlowInput,
    ) -> Self {
        Self {
            publish: false,
            ..Self::new(request, storage, liveness, refinements, input)
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

        let refinements = self.refinements.facts_before(operation.kind().node());

        for plan in self.input.plans(operation.kind().node()) {
            self.apply_plan(state, *plan, refinements);
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
            state.moved.iter().copied(),
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

fn projection_is_available(
    storage: &StoragePlan,
    access: StorageAccessId,
    projection: StorageProjection,
    refinements: &[RefinementFact],
) -> bool {
    let related = refinements.iter().filter(|fact| {
        fact.dependencies().iter().any(|dependency| {
            storage.relationship(*dependency, access) != StorageRelationship::Disjoint
        })
    });

    match projection {
        StorageProjection::NullableValue => related.into_iter().any(|fact| {
            matches!(
                fact.kind(),
                RefinementFactKind::NullablePresence {
                    is_present: true,
                    ..
                } | RefinementFactKind::Pattern {
                    predicate: PatternPredicate::NullablePresent,
                    ..
                }
            )
        }),
        StorageProjection::ActiveUnionPayloadField { variant, .. } => {
            related.into_iter().any(|fact| {
                matches!(
                    fact.kind(),
                    RefinementFactKind::Pattern {
                        predicate: PatternPredicate::ActiveUnionVariant(active),
                        ..
                    } if active == variant
                )
            })
        }
        StorageProjection::ProductField(_)
        | StorageProjection::TupleElement(_)
        | StorageProjection::ElementFromStart(_)
        | StorageProjection::ElementFromEnd(_)
        | StorageProjection::Element(_)
        | StorageProjection::SliceRange { .. }
        | StorageProjection::OwnedTarget => true,
    }
}
