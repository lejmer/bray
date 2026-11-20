use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};

use crate::{
    AnyBoundNodeId, BorrowCapabilityId, BoundBlockId, BoundExpressionId, BoundUnitId,
    BoundUnitKind, CheckedMemoryOperation, CheckedMemoryOperations, MemoryOperationDecision,
    StorageAccessId, StorageAccessPlan, StorageAccessPurpose, StorageIdentityId, StoragePlan, StorageScope,
};

crate::identity::define_unit_scoped_id!(
    AnalysisOperationId,
    "Identifies one operation in a unit's prepared control-flow graph.",
    pub fn new
);

impl AnalysisOperationId {
    /// Returns the index in the prepared graph's operation arena.
    pub fn to_index(self) -> Option<usize> {
        usize::try_from(self.ordinal()).ok()
    }
}

crate::identity::define_unit_scoped_id!(
    AnalysisEdgeId,
    "Identifies one edge in a unit's prepared control-flow graph.",
    pub fn from_slot
);

impl AnalysisEdgeId {
    /// Returns the index in the prepared graph's edge arena.
    pub fn to_index(self) -> Option<usize> {
        usize::try_from(self.ordinal()).ok()
    }

    /// Constructs a checked edge identity for an arena index.
    pub fn try_from_index(unit: BoundUnitId, index: usize) -> Option<Self> {
        u32::try_from(index).ok().map(|slot| Self::from_slot(unit, slot))
    }
}

/// An evaluated storage route whose exact origin is retained for functional state interpretation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageStateTarget {
    /// A planned access at its exact evaluation site.
    Access(StorageAccessPlan),
    /// A source assignment destination.
    Assignment(BoundExpressionId),
    /// The destination of a checked scalar or aggregate memory write.
    MemoryWrite(BoundExpressionId),
    /// The source of a checked move from initialized raw storage.
    MemoryRead(BoundExpressionId),
    /// The referent of an actual mutable formal input.
    Input {
        /// The exact selected invocation.
        invocation: crate::BoundExecutionSite,
        /// Its receiver-first input ordinal.
        ordinal: bray_symbols::SymbolOrdinal,
    },
}

impl StorageStateTarget {
    /// Returns the operation that established this destination.
    pub fn occurrence(self) -> crate::BoundExecutionSite {
        match self {
            Self::Assignment(expression) | Self::MemoryWrite(expression) | Self::MemoryRead(expression) => expression.into(),
            Self::Access(plan) => plan.occurrence(),
            Self::Input { invocation, .. } => invocation,
        }
    }
}

/// The checker result for one evaluated storage operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageOperationStatus {
    /// Control flow cannot reach the operation.
    Unreachable,
    /// The operation satisfies its storage requirements.
    Valid,
    /// Earlier recovery prevents a complete decision.
    Recovered,
    /// The reached storage is not initialized on every incoming path.
    Uninitialized,
    /// The reached storage or substorage was previously moved.
    Moved,
    /// An active overlapping borrow forbids the operation.
    ConflictingBorrow,
    /// The operation requires mutation authority that is not available.
    MissingMutationAuthority,
    /// The operation requires ownership of storage reached only through a borrow.
    MissingOwnership,
    /// The selected nullable or union substorage is inactive on this control-flow path.
    InactiveProjection,
    /// The operation requires implicit copying for a non-copyable type.
    NotCopyable,
}

/// One source-correlated checked storage operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageOperationDecision {
    plan: StorageAccessPlan,
    purpose: StorageAccessPurpose,
    borrow: Option<BorrowCapabilityId>,
    status: StorageOperationStatus,
}

impl StorageOperationDecision {
    /// Creates one checked operation decision.
    pub const fn new(
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
        borrow: Option<BorrowCapabilityId>,
        status: StorageOperationStatus,
    ) -> Self {
        Self {
            plan,
            purpose,
            borrow,
            status,
        }
    }

    /// Returns the control-flow node where this operation takes effect.
    pub const fn node(self) -> AnyBoundNodeId {
        self.plan.node()
    }

    /// Returns the source expression supplying the accessed value.
    pub const fn expression(self) -> BoundExpressionId {
        self.plan.expression()
    }

    /// Returns the semantic storage operation.
    pub const fn purpose(self) -> StorageAccessPurpose {
        self.purpose
    }

    /// Returns the reached storage access.
    pub const fn access(self) -> StorageAccessId {
        self.plan.access()
    }

    /// Returns the exact checked access, including its invocation occurrence.
    pub const fn plan(self) -> StorageAccessPlan {
        self.plan
    }

    /// Returns the capability established by a borrow operation.
    pub const fn borrow(self) -> Option<BorrowCapabilityId> {
        self.borrow
    }

    /// Returns the checker result.
    pub const fn status(self) -> StorageOperationStatus {
        self.status
    }
}

/// Reachability, storage and borrow state immediately before a suspension.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageSuspensionState {
    reachable: bool,
    occurrence: crate::BoundExecutionSite,
    live: Arc<[StorageIdentityId]>,
    initialized: Arc<[StorageIdentityId]>,
    moved: Arc<[crate::StoragePath]>,
    active_borrows: Arc<[BorrowCapabilityId]>,
    cleanup_retention: Arc<[crate::BoundDependencySubject]>,
    execution_retention: Arc<[crate::BoundDependencySubject]>,
    unavailable_dependencies: Arc<[(crate::BoundDependencySubject, crate::BoundDependencyRequirementKind)]>,
}

impl StorageSuspensionState {
    /// Creates one normalized suspension-state snapshot.
    pub fn new(
        occurrence: impl Into<crate::BoundExecutionSite>,
        live: impl IntoIterator<Item = StorageIdentityId>,
        initialized: impl IntoIterator<Item = StorageIdentityId>,
        moved: impl IntoIterator<Item = crate::StoragePath>,
        active_borrows: impl IntoIterator<Item = BorrowCapabilityId>,
        cleanup_retention: impl IntoIterator<Item = crate::BoundDependencySubject>,
    ) -> Self {
        Self {
            reachable: true,
            occurrence: occurrence.into(),
            live: sorted_unique_shared_slice(live),
            initialized: sorted_unique_shared_slice(initialized),
            moved: sorted_unique_shared_slice(moved),
            active_borrows: sorted_unique_shared_slice(active_borrows),
            cleanup_retention: sorted_unique_shared_slice(cleanup_retention),
            execution_retention: Arc::default(),
            unavailable_dependencies: Arc::default(),
        }
    }

    /// Records that correlated control flow cannot reach this suspension.
    pub fn unreachable(occurrence: impl Into<crate::BoundExecutionSite>) -> Self {
        Self {
            reachable: false,
            ..Self::new(occurrence, [], [], [], [], [])
        }
    }

    /// Whether correlated control flow can reach this suspension.
    pub const fn is_reachable(&self) -> bool {
        self.reachable
    }

    /// Returns the exact suspension phase.
    pub const fn occurrence(&self) -> crate::BoundExecutionSite {
        self.occurrence
    }

    /// Returns storage that may be live before suspension.
    pub fn live(&self) -> &[StorageIdentityId] {
        &self.live
    }

    /// Returns storage known to be initialized before suspension.
    pub fn initialized(&self) -> &[StorageIdentityId] {
        &self.initialized
    }

    /// Returns storage accesses moved before suspension.
    pub fn moved(&self) -> &[crate::StoragePath] {
        &self.moved
    }

    /// Returns borrow capabilities active before suspension.
    pub fn active_borrows(&self) -> &[BorrowCapabilityId] {
        &self.active_borrows
    }

    /// Returns dependencies required until pending owners finish cleanup or transfer.
    pub fn cleanup_retention(&self) -> &[crate::BoundDependencySubject] {
        &self.cleanup_retention
    }

    /// Records the execution requirements retained and unavailable across reached paths.
    pub fn with_execution_dependencies(
        mut self,
        retained: impl IntoIterator<Item = crate::BoundDependencySubject>,
        unavailable: impl IntoIterator<Item = (crate::BoundDependencySubject, crate::BoundDependencyRequirementKind)>,
    ) -> Self {
        self.execution_retention = sorted_unique_shared_slice(retained);
        self.unavailable_dependencies = sorted_unique_shared_slice(unavailable);

        self
    }

    pub fn execution_retention(&self) -> &[crate::BoundDependencySubject] {
        &self.execution_retention
    }

    pub fn unavailable_dependencies(&self) -> &[(crate::BoundDependencySubject, crate::BoundDependencyRequirementKind)] {
        &self.unavailable_dependencies
    }
}

/// One lexical scope exit proven reachable by checked control flow.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageExitPoint {
    scope: StorageScope,
    exit: AnyBoundNodeId,
}

impl StorageExitPoint {
    /// Creates one lowering-reachable scope exit.
    pub const fn new(scope: StorageScope, exit: AnyBoundNodeId) -> Self {
        Self { scope, exit }
    }

    /// Returns the lexical scope being exited.
    pub const fn scope(self) -> StorageScope {
        self.scope
    }

    /// Returns the bound node that exits the scope.
    pub const fn exit(self) -> AnyBoundNodeId {
        self.exit
    }
}

/// Storage and borrow state that remains relevant at one lexical scope exit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageExitDecision {
    point: StorageExitPoint,
    scoped_exit: Option<BoundExpressionId>,
    cancellation_broadcast: Arc<[crate::StorageAccessId]>,
    lifecycle: crate::StorageCleanupSchedule,
    cleanup_recoveries: Arc<[crate::AsyncStorageExitDecision]>,
    live: Arc<[StorageIdentityId]>,
    initialized: Arc<[StorageIdentityId]>,
    moved: Arc<[crate::StoragePath]>,
    fully_moved: Arc<[StorageIdentityId]>,
    definitely_absent: Arc<[crate::StoragePath]>,
    active_borrows: Arc<[BorrowCapabilityId]>,
    is_recovered: bool,
}

impl StorageExitDecision {
    /// Creates one normalized scope-exit decision.
    #[expect(
        clippy::too_many_arguments,
        reason = "scope exits retain each independent storage fact without combining their meanings"
    )]
    pub fn new(
        scope: StorageScope,
        exit: AnyBoundNodeId,
        live: impl IntoIterator<Item = StorageIdentityId>,
        initialized: impl IntoIterator<Item = StorageIdentityId>,
        moved: impl IntoIterator<Item = crate::StoragePath>,
        definitely_absent: impl IntoIterator<Item = crate::StoragePath>,
        fully_moved: impl IntoIterator<Item = StorageIdentityId>,
        active_borrows: impl IntoIterator<Item = BorrowCapabilityId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            point: StorageExitPoint::new(scope, exit),
            scoped_exit: None,
            live: sorted_unique_shared_slice(live),
            cancellation_broadcast: Arc::from([]),
            lifecycle: crate::StorageCleanupSchedule::Fixed(Arc::from([])),
            cleanup_recoveries: Arc::from([]),
            initialized: sorted_unique_shared_slice(initialized),
            moved: sorted_unique_shared_slice(moved),
            definitely_absent: sorted_unique_shared_slice(definitely_absent),
            fully_moved: sorted_unique_shared_slice(fully_moved),
            active_borrows: sorted_unique_shared_slice(active_borrows),
            is_recovered,
        }
    }

    /// Returns the identity of this reached scope exit.
    pub const fn point(&self) -> StorageExitPoint { self.point }

    /// Returns the lexical scope being exited.
    pub const fn scope(&self) -> StorageScope { self.point.scope() }

    /// Returns the bound node whose completion or transfer exits the scope.
    pub const fn exit(&self) -> AnyBoundNodeId {
        self.point.exit()
    }

    /// Records the protocol exit that precedes this scope's lifecycle actions.
    pub fn with_scoped_exit(mut self, expression: Option<BoundExpressionId>) -> Self {
        assert!(expression.is_none_or(|expression| self.scope().unit() == expression.unit()),
            "scoped exit belongs to its lexical scope unit");

        self.scoped_exit = expression;

        self
    }

    /// Returns the with expression whose entry owns this exit.
    pub const fn scoped_exit(&self) -> Option<BoundExpressionId> {
        self.scoped_exit
    }

    /// Retains the work and recovery selected by the reached cleanup phases.
    pub fn with_cleanup_selection(
        mut self,
        broadcast: impl IntoIterator<Item = StorageAccessId>,
        lifecycle: crate::StorageCleanupSchedule,
        recoveries: impl IntoIterator<Item = crate::AsyncStorageExitDecision>,
    ) -> Self {
        self.cancellation_broadcast = shared_slice(broadcast);
        self.lifecycle = lifecycle;
        self.cleanup_recoveries = sorted_unique_shared_slice(recoveries);

        self
    }

    /// Returns the ordered accesses from the reached cancellation phase.
    pub fn cancellation_broadcast(&self) -> &[crate::StorageAccessId] {
        &self.cancellation_broadcast
    }

    /// Returns accesses selected on at least one path into lifecycle resolution.
    pub fn lifecycle_resolution(&self) -> &[StorageAccessId] {
        self.lifecycle.accesses()
    }

    /// Returns the checked lifecycle schedule.
    pub fn lifecycle_schedule(&self) -> &crate::StorageCleanupSchedule {
        &self.lifecycle
    }

    /// Iterates scheduled accesses in phase order. An access can occur in both phases.
    pub fn cleanup_accesses(&self) -> impl Iterator<Item = StorageAccessId> + '_ {
        self.cancellation_broadcast.iter().chain(self.lifecycle.accesses()).copied()
    }

    /// Whether this scope runs an exit protocol or has scheduled storage cleanup.
    pub fn has_cleanup(&self) -> bool {
        self.scoped_exit.is_some() || self.cleanup_accesses().next().is_some()
    }

    /// Returns the affected identities and their recovery causes from either phase.
    pub fn cleanup_recoveries(&self) -> &[crate::AsyncStorageExitDecision] {
        &self.cleanup_recoveries
    }

    /// Returns storage live on at least one path into lifecycle resolution.
    pub fn live(&self) -> &[StorageIdentityId] {
        &self.live
    }

    /// Returns storage known to remain initialized.
    pub fn initialized(&self) -> &[StorageIdentityId] {
        &self.initialized
    }

    /// Returns storage accesses whose reached values were moved before this exit.
    pub fn moved(&self) -> &[crate::StoragePath] {
        &self.moved
    }

    /// Returns storage paths with no live value on every incoming path, after moves or absence refinements.
    pub fn definitely_absent(&self) -> &[crate::StoragePath] {
        &self.definitely_absent
    }

    /// Returns storage identities known to be fully moved on every incoming path.
    pub fn fully_moved(&self) -> &[StorageIdentityId] {
        &self.fully_moved
    }

    /// Returns borrows still active at the exit.
    pub fn active_borrows(&self) -> &[BorrowCapabilityId] {
        &self.active_borrows
    }

    /// Returns whether recovery affected the exit state.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

/// A contract violation while constructing durable storage-flow analysis.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum StorageFlowBuildError {
    /// A decision references an identity owned by another unit.
    ForeignUnit,
    /// The same direct-await expression has more than one state snapshot.
    DuplicateSuspension,
    /// The same expression has more than one memory-flow decision.
    DuplicateMemoryOperation,
    /// The same assignment has more than one old-storage decision.
    DuplicateReplacement,
}

/// Immutable storage, ownership, and borrow decisions for one bound unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StorageFlow {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    operations: Arc<[StorageOperationDecision]>,
    operation_indices: Arc<[usize]>,
    suspensions: Arc<[StorageSuspensionState]>,
    deferred_calls: std::collections::BTreeMap<crate::BoundExecutionSite, std::collections::BTreeSet<crate::BodyBehaviorCall>>,
    reachable_blocks: Arc<[BoundBlockId]>,
    conditions: std::collections::BTreeMap<BoundExpressionId, bool>,
    proven_preconditions: Arc<[crate::BoundExecutionSite]>,
    state_targets: std::collections::BTreeMap<(StorageStateTarget, AnalysisOperationId), super::capture::OperationOutcomes<Option<crate::StorageTarget>>>,
    pub(super) capture_retirements: std::collections::BTreeMap<(crate::StorageMutation, crate::StorageValueBoundary), Arc<[(crate::StorageTarget, crate::StorageValueRetirement)]>>,
    pub(super) operation_retirements: std::collections::BTreeMap<(AnalysisOperationId, crate::StorageValueBoundary), super::capture::OperationOutcomes<Vec<(crate::StorageTarget, crate::StorageValueRetirement)>>>,
    input_observation_scopes:
        std::collections::BTreeMap<crate::BoundExecutionSite, crate::StorageMutationScope>,
    pub(super) input_captures: std::collections::BTreeMap<(AnalysisOperationId, Option<crate::StorageDeferredProducer>), Arc<[crate::StorageValueCapture]>>,
    pub(super) attempt_values: std::collections::BTreeMap<(AnalysisOperationId, Option<crate::StorageDeferredProducer>), Arc<[crate::StorageValueTransition]>>,
    pub(super) operation_invalidations: std::collections::BTreeMap<AnalysisOperationId, super::capture::OperationOutcomes<crate::StorageInvalidation>>,
    pub(super) normal_completions: std::collections::BTreeMap<AnalysisOperationId, super::capture::OperationOutcomes<crate::StorageCompletion>>,
    pub(super) normal_completion_sites: std::collections::BTreeMap<crate::BoundExecutionSite, Arc<[AnalysisOperationId]>>,
    dependencies: bray_symbols::DependencyContractTemplateData,
    report_collections: Arc<[Option<BoundExpressionId>]>,
    reporting_completions: Arc<[crate::BoundExecutionSite]>,
    receiver_cleanup: Arc<[bray_symbols::TypeId]>,
    pub(super) outcome_mutations:
        std::collections::BTreeMap<crate::StorageMutation, crate::StorageInvalidation>,
    completions: std::collections::BTreeMap<crate::StorageMutation, crate::StorageCompletion>,
    excluded_edges: Arc<[AnalysisEdgeId]>,
    failing_cleanup: Arc<[(StorageExitPoint, bray_symbols::DependencyFailureKind)]>,
    exits: Arc<[StorageExitDecision]>,
    pub(super) cleanup_entries: std::collections::BTreeMap<(AnalysisOperationId, Option<crate::StorageCleanupAction>), crate::StorageCleanupEntry>,
    pub(super) cleanup_remainders: std::collections::BTreeMap<crate::StorageMutation, std::collections::BTreeSet<crate::StorageCleanupRemainder>>,
    memory_operations: Arc<[MemoryOperationDecision]>,
    checked_memory_operations: Option<CheckedMemoryOperations>,
    replacements: Arc<[crate::StorageReplacementDecision]>,
    is_recovered: bool,
}

impl StorageFlow {
    /// Validates and creates one durable storage-flow result.
    pub fn try_new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        operations: impl IntoIterator<Item = StorageOperationDecision>,
        suspensions: impl IntoIterator<Item = StorageSuspensionState>,
        reachable_blocks: impl IntoIterator<Item = BoundBlockId>,
        exits: impl IntoIterator<Item = StorageExitDecision>,
        is_recovered: bool,
    ) -> Result<Self, StorageFlowBuildError> {
        let operations = operations.into_iter().collect::<Vec<_>>();
        let mut operation_indices = (0..operations.len()).collect::<Vec<_>>();

        operation_indices.sort_unstable_by_key(|index| operations[*index].plan());

        assert!(operation_indices.windows(2).all(|pair| operations[pair[0]].plan() != operations[pair[1]].plan()),
            "checked storage publishes one decision per exact access plan");

        let mut suspensions = suspensions.into_iter().collect::<Vec<_>>();
        let reachable_blocks = sorted_unique_shared_slice(reachable_blocks);
        let mut exits = exits.into_iter().collect::<Vec<_>>();

        exits.sort_unstable_by_key(StorageExitDecision::point);

        assert!(exits.windows(2).all(|pair| pair[0].point() != pair[1].point()),
            "storage flow publishes one decision per reached scope exit");

        if operations.iter().any(|operation| {
            operation.node().unit() != unit
                || operation.expression().unit() != unit
                || operation.access().unit() != unit
                || operation
                    .borrow()
                    .is_some_and(|borrow| borrow.unit() != unit)
        }) || suspensions.iter().any(|suspension| {
            suspension.occurrence().node().unit() != unit
                || suspension
                    .live()
                    .iter()
                    .any(|storage| storage.unit() != unit)
                || suspension
                    .initialized()
                    .iter()
                    .any(|storage| storage.unit() != unit)
                || suspension
                    .moved()
                    .iter()
                    .any(|access| !access.is_valid_for(unit))
                || suspension
                    .cleanup_retention()
                    .iter()
                    .chain(suspension.execution_retention())
                    .any(|subject| !subject.is_valid_for(unit))
                || suspension
                    .active_borrows()
                    .iter()
                    .any(|borrow| borrow.unit() != unit)
                || suspension.unavailable_dependencies().iter().any(|(subject, _)| !subject.is_valid_for(unit))
        }) || reachable_blocks.iter().any(|block| block.unit() != unit)
            || exits.iter().any(|exit| {
                exit.scope().unit() != unit
                    || exit.exit().unit() != unit
                    || exit.cancellation_broadcast().iter().any(|access| access.unit() != unit)
                    || exit.lifecycle_resolution().iter().any(|access| access.unit() != unit)
                    || exit.cleanup_recoveries().iter().any(|recovery| recovery.identity().unit() != unit)
                    || exit.live().iter().any(|storage| storage.unit() != unit)
                    || exit
                        .initialized()
                        .iter()
                        .any(|storage| storage.unit() != unit)
                    || exit
                        .moved()
                        .iter()
                        .chain(exit.definitely_absent())
                        .any(|access| !access.is_valid_for(unit))
                    || exit
                        .fully_moved()
                        .iter()
                        .any(|storage| storage.unit() != unit)
                    || exit
                        .active_borrows()
                        .iter()
                        .any(|borrow| borrow.unit() != unit)
            })
        {
            return Err(StorageFlowBuildError::ForeignUnit);
        }

        suspensions.sort_unstable_by_key(StorageSuspensionState::occurrence);

        if suspensions
            .windows(2)
            .any(|pair| pair[0].occurrence() == pair[1].occurrence())
        {
            return Err(StorageFlowBuildError::DuplicateSuspension);
        }

        Ok(Self {
            unit,
            kind,
            operations: shared_slice(operations),
            operation_indices: shared_slice(operation_indices),
            suspensions: shared_slice(suspensions),
            deferred_calls: Default::default(),
            reachable_blocks,
            conditions: Default::default(),
            proven_preconditions: Arc::from([]),
            excluded_edges: Arc::from([]),
            failing_cleanup: Arc::from([]),
            exits: shared_slice(exits),
            cleanup_entries: Default::default(),
            cleanup_remainders: Default::default(),
            memory_operations: Arc::from([]),
            checked_memory_operations: None,
            state_targets: std::collections::BTreeMap::new(),
            capture_retirements: std::collections::BTreeMap::new(),
            operation_retirements: Default::default(),
            input_observation_scopes: Default::default(),
            operation_invalidations: Default::default(),
            normal_completions: Default::default(),
            normal_completion_sites: Default::default(),
            input_captures: Default::default(),
            attempt_values: Default::default(),
            dependencies: bray_symbols::DependencyContractTemplateData::new([]),
            report_collections: Arc::from([]),
            reporting_completions: Arc::from([]),
            receiver_cleanup: Arc::from([]),
            outcome_mutations: Default::default(),
            completions: Default::default(),
            replacements: Arc::from([]),
            is_recovered,
        })
    }

    /// Publishes named calls from actually admitted frame execution.
    pub fn with_deferred_calls(
        mut self,
        calls: std::collections::BTreeMap<crate::BoundExecutionSite, std::collections::BTreeSet<crate::BodyBehaviorCall>>,
    ) -> Self {
        assert!(calls.keys().all(|site| site.node().unit() == self.unit), "deferred calls belong to their executing unit");
        self.deferred_calls = calls;

        self
    }

    /// Returns the named deferred call contributions of one reached operation.
    pub fn deferred_calls_at(&self, site: crate::BoundExecutionSite) -> impl Iterator<Item = &crate::BodyBehaviorCall> {
        self.deferred_calls.get(&site).into_iter().flatten()
    }

    /// Returns the named call contributions of all reached frame executions.
    pub fn deferred_calls(&self) -> impl Iterator<Item = &crate::BodyBehaviorCall> {
        self.deferred_calls.values().flatten()
    }

    /// Publishes the edge exclusions used to solve storage flow and its remaining cleanup routes.
    pub fn with_exit_reachability(
        mut self,
        excluded_edges: impl IntoIterator<Item = AnalysisEdgeId>,
        failing_cleanup: impl IntoIterator<Item = (StorageExitPoint, bray_symbols::DependencyFailureKind)>,
    ) -> Self {
        self.excluded_edges = sorted_unique_shared_slice(excluded_edges);
        self.failing_cleanup = sorted_unique_shared_slice(failing_cleanup);

        self
    }

    /// Returns edges omitted from the checked control flow.
    pub fn excluded_edges(&self) -> &[AnalysisEdgeId] {
        &self.excluded_edges
    }

    /// Returns whether ordinary cleanup can leave this scope abnormally.
    pub fn cleanup_may_fail(&self, point: StorageExitPoint, kind: bray_symbols::DependencyFailureKind) -> bool {
        self.failing_cleanup.binary_search(&(point, kind)).is_ok()
    }

    /// Publishes resolved observations for task results, invocation failure and ordered cleanup.
    pub fn with_outcome_mutations(
        mut self,
        mutations: std::collections::BTreeMap<crate::StorageMutation, crate::StorageInvalidation>,
    ) -> Self {
        self.outcome_mutations = mutations;

        self
    }

    /// Returns the observations resolved for one selected transition.
    pub fn outcome_mutation(
        &self,
        mutation: crate::StorageMutation,
    ) -> Option<&crate::StorageInvalidation> {
        self.outcome_mutations.get(&mutation)
    }

    /// Returns whether a checked failure of this kind is reachable at the invocation.
    pub fn invocation_can_fail(&self, occurrence: crate::BoundExecutionSite, kind: bray_symbols::DependencyFailureKind) -> bool {
        let first = crate::StorageMutation::InvocationFailure { occurrence, kind, edge: AnalysisEdgeId::from_slot(self.unit, 0), row: None };
        let last = crate::StorageMutation::InvocationFailure { occurrence, kind, edge: AnalysisEdgeId::from_slot(self.unit, u32::MAX), row: Some(usize::MAX) };

        self.outcome_mutations.range(first..=last).next().is_some()
    }

    /// Returns the reached rows of one checked failure edge.
    pub fn invocation_failure_mutations(&self, occurrence: crate::BoundExecutionSite,
        kind: bray_symbols::DependencyFailureKind, edge: AnalysisEdgeId,
    ) -> impl Iterator<Item = (Option<usize>, &crate::StorageInvalidation)> {
        use crate::StorageMutation;

        let first = StorageMutation::InvocationFailure { occurrence, kind, edge, row: None };
        let last = StorageMutation::InvocationFailure { occurrence, kind, edge, row: Some(usize::MAX) };

        self.outcome_mutations.range(first..=last).map(|(mutation, invalidation)| {
            let StorageMutation::InvocationFailure { row, .. } = mutation else {
                unreachable!("the invocation failure range contains only failure transitions");
            };

            (*row, invalidation)
        })
    }

    /// Publishes results of cleanup actions, scoped transfers, and failure edges.
    pub fn with_completions(
        mut self,
        mut observations: std::collections::BTreeMap<crate::StorageMutation, crate::StorageCompletion>,
    ) -> Self {
        assert!(observations.keys().all(|mutation| !matches!(mutation, crate::StorageMutation::Operation(_))));

        observations.retain(|mutation, completion| {
            !matches!(mutation, crate::StorageMutation::InvocationFailure { .. }) || !completion.publications().is_empty()
        });

        self.completions = observations;

        self
    }

    /// Returns the results published by one reached outcome outside normal operation completion.
    pub fn completion(
        &self,
        mutation: crate::StorageMutation,
    ) -> Option<&crate::StorageCompletion> {
        self.completions.get(&mutation)
    }

    /// Publishes represented owners that may remain in a consuming receiver's caller-owned tail.
    pub fn with_receiver_cleanup(
        mut self,
        types: impl IntoIterator<Item = bray_symbols::TypeId>,
    ) -> Self {
        self.receiver_cleanup = sorted_unique_shared_slice(types);

        self
    }

    /// Returns the checked residual receiver owners, including every reachable cleanup outcome.
    pub fn receiver_cleanup(&self) -> &[bray_symbols::TypeId] {
        &self.receiver_cleanup
    }

    /// Publishes source and catch collections that can retain owned cleanup reports.
    pub fn with_report_collections(
        mut self,
        collections: impl IntoIterator<Item = Option<BoundExpressionId>>,
    ) -> Self {
        self.report_collections = sorted_unique_shared_slice(collections);

        self
    }

    /// Whether this source or catch boundary needs a pending owned report collection.
    pub fn has_report_collection(&self, boundary: Option<BoundExpressionId>) -> bool {
        self.report_collections.binary_search(&boundary).is_ok()
    }

    /// Publishes invocation completions that transfer a normal owned report.
    pub fn with_reporting_completions(
        mut self,
        occurrences: impl IntoIterator<Item = crate::BoundExecutionSite>,
    ) -> Self {
        self.reporting_completions = sorted_unique_shared_slice(occurrences);

        self
    }

    /// Whether this checked completion transfers a normal owned report.
    pub fn completion_reports(&self, occurrence: crate::BoundExecutionSite) -> bool {
        self.reporting_completions
            .binary_search(&occurrence)
            .is_ok()
    }

    /// Publishes carried dependencies, exact reference relations and possible mutations from this flow.
    pub fn with_dependencies(
        mut self,
        dependencies: bray_symbols::DependencyContractTemplateData,
    ) -> Self {
        self.dependencies = if self.is_recovered {
            // Recovery cannot establish exact identities or absence of effects.
            // Required lifetimes remain obligations even when another analysis recovered.
            bray_symbols::DependencyContractTemplateData::new(
                dependencies.requirements().iter().cloned(),
            )
        } else {
            dependencies
        };

        self
    }

    /// Returns the body boundary facts established by the composite flow.
    pub const fn dependencies(&self) -> &bray_symbols::DependencyContractTemplateData {
        &self.dependencies
    }

    /// Adds source-ordered replacement decisions, rejecting foreign and duplicate identities.
    pub fn with_replacements(
        mut self,
        replacements: impl IntoIterator<Item = crate::StorageReplacementDecision>,
    ) -> Result<Self, StorageFlowBuildError> {
        let mut replacements = replacements.into_iter().collect::<Vec<_>>();

        if replacements.iter().any(|decision| {
            decision.expression().unit() != self.unit
                || decision.access().unit() != self.unit
                || decision
                    .moved()
                    .iter()
                    .chain(decision.definitely_absent())
                    .any(|access| !access.is_valid_for(self.unit))
                || decision.parts().is_some_and(|parts| {
                    parts.iter().any(|part| !part.is_valid_for(self.unit))
                })
        }) {
            return Err(StorageFlowBuildError::ForeignUnit);
        }

        replacements.sort_unstable_by_key(crate::StorageReplacementDecision::expression);

        if replacements
            .windows(2)
            .any(|pair| pair[0].expression() == pair[1].expression())
        {
            return Err(StorageFlowBuildError::DuplicateReplacement);
        }

        self.replacements = shared_slice(replacements);

        Ok(self)
    }

    /// Returns old-storage decisions after replacement operands have been evaluated.
    pub fn replacements(&self) -> &[crate::StorageReplacementDecision] {
        &self.replacements
    }

    /// Returns the checked old state and cleanup for one evaluated assignment.
    pub fn replacement(
        &self,
        expression: BoundExpressionId,
    ) -> Option<&crate::StorageReplacementDecision> {
        self.replacements
            .binary_search_by_key(&expression, crate::StorageReplacementDecision::expression)
            .ok()
            .map(|index| &self.replacements[index])
    }

    /// Adds validated memory-flow decisions without mutating the published analysis.
    pub fn with_memory_operations(
        mut self,
        checked: &CheckedMemoryOperations,
        decisions: impl IntoIterator<Item = MemoryOperationDecision>,
    ) -> Result<Self, StorageFlowBuildError> {
        if checked.unit() != self.unit || checked.kind() != self.kind {
            return Err(StorageFlowBuildError::ForeignUnit);
        }

        let mut operations = decisions.into_iter().collect::<Vec<_>>();

        if operations
            .iter()
            .any(|operation| operation.expression().unit() != self.unit)
        {
            return Err(StorageFlowBuildError::ForeignUnit);
        }

        operations.sort_unstable_by_key(|operation| operation.expression());

        if operations
            .windows(2)
            .any(|pair| pair[0].expression() == pair[1].expression())
        {
            return Err(StorageFlowBuildError::DuplicateMemoryOperation);
        }

        self.memory_operations = shared_slice(operations);

        // Storage flow shares the immutable selected operations with their producer.
        self.checked_memory_operations = (!checked.operations().is_empty()).then(|| checked.clone());

        Ok(self)
    }

    /// Returns the bound unit described by this analysis.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the semantic category of the bound unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns checked operations in deterministic evaluation order.
    pub fn operations(&self) -> &[StorageOperationDecision] {
        &self.operations
    }

    /// Returns the checked decision for one exact planned operation.
    pub fn operation(&self, plan: StorageAccessPlan) -> Option<StorageOperationDecision> {
        self.operation_indices.binary_search_by_key(&plan, |index| self.operations[*index].plan())
            .ok().map(|index| self.operations[self.operation_indices[index]])
    }

    /// Returns direct-await storage states in expression identity order.
    pub fn suspensions(&self) -> &[StorageSuspensionState] {
        &self.suspensions
    }

    /// Publishes boolean results established at their branch sites by correlated flow.
    pub fn with_conditions(
        mut self,
        conditions: std::collections::BTreeMap<BoundExpressionId, bool>,
    ) -> Result<Self, StorageFlowBuildError> {
        if conditions
            .keys()
            .any(|expression| expression.unit() != self.unit)
        {
            return Err(StorageFlowBuildError::ForeignUnit);
        }

        self.conditions = conditions;

        Ok(self)
    }

    /// Publishes call boundaries whose ordinary requirements hold before invocation.
    pub fn with_proven_preconditions(
        mut self,
        invocations: impl IntoIterator<Item = crate::BoundExecutionSite>,
    ) -> Self {
        self.proven_preconditions = sorted_unique_shared_slice(invocations);

        assert!(
            self.proven_preconditions
                .iter()
                .all(|site| site.node().unit() == self.unit),
            "proven preconditions belong to their checked unit"
        );

        self
    }

    /// Whether every ordinary requirement is established before this invocation.
    pub fn preconditions_proven(&self, invocation: crate::BoundExecutionSite) -> bool {
        self.proven_preconditions.binary_search(&invocation).is_ok()
    }

    /// Publishes exact-operation origins, including reached operations without a definite target.
    pub fn with_state_targets(
        mut self,
        targets: std::collections::BTreeMap<((StorageStateTarget, AnalysisOperationId), crate::StorageOutcomeRows), Option<crate::StorageTarget>>,
    ) -> Result<Self, StorageFlowBuildError> {
        if targets.iter().any(|(((destination, operation), _), access)| {
            destination.occurrence().node().unit() != self.unit
                || operation.unit() != self.unit
                || access.as_ref().is_some_and(|access| !access.is_valid_for(self.unit))
        }) {
            return Err(StorageFlowBuildError::ForeignUnit);
        }

        self.state_targets = super::capture::OperationOutcomes::group(targets, |target, incoming| {
            if target != incoming {
                *target = None;
            }
        });

        Ok(self)
    }

    /// Returns the actual target agreed by the selected outcomes of this graph operation.
    pub fn operation_target(
        &self, operation: AnalysisOperationId, target: StorageStateTarget, selection: Option<&crate::StorageOutcomeRows>,
    ) -> Option<&crate::StorageTarget> {
        self.state_targets.get(&(target, operation))?.common_for(selection)?.as_ref()
    }

    fn common_target(&self, target: StorageStateTarget) -> Option<&crate::StorageTarget> {
        let first = (target, AnalysisOperationId::new(self.unit, 0));
        let last = (target, AnalysisOperationId::new(self.unit, u32::MAX));
        let mut reached = self.state_targets.range(first..=last).map(|(_, value)| value.combined().as_ref());
        let target = reached.next().flatten()?;

        reached.all(|value| value == Some(target)).then_some(target)
    }

    /// Publishes mutable-input confinement for the selected callable's Observation premise.
    pub fn with_input_observation_scopes(
        mut self,
        scopes: std::collections::BTreeMap<
            crate::BoundExecutionSite,
            crate::StorageMutationScope,
        >,
    ) -> Self {
        assert!(
            scopes.keys().all(|site| site.node().unit() == self.unit),
            "input observation scopes belong to their checked unit"
        );

        self.input_observation_scopes = scopes;

        self
    }

    /// Returns direct-input confinement under a retained callable Observation dependency.
    pub fn input_observation_scope(
        &self,
        site: crate::BoundExecutionSite,
    ) -> Option<crate::StorageMutationScope> {
        self.input_observation_scopes.get(&site).copied()
    }

    /// Returns the resolved origin established for an assignment, including borrowed referents.
    pub fn assignment_target(
        &self,
        expression: BoundExpressionId,
    ) -> Option<&crate::StorageTarget> {
        self.common_target(StorageStateTarget::Assignment(expression))
    }

    /// Returns an access's actual storage origin agreed across all reachable evaluations.
    pub fn access_target(&self, plan: StorageAccessPlan) -> Option<&crate::StorageTarget> {
        self.common_target(StorageStateTarget::Access(plan))
    }

    /// Returns a mutable input's referent agreed by all reachable evaluations of the invocation.
    pub fn invocation_input_target(
        &self,
        invocation: crate::BoundExecutionSite,
        ordinal: bray_symbols::SymbolOrdinal,
    ) -> Option<&crate::StorageTarget> {
        self.common_target(StorageStateTarget::Input {
            invocation,
            ordinal,
        })
    }

    /// Returns an invocation-owned inline component whose assignment cannot affect its caller.
    /// References, allocations, slices and protected payloads retain their separate authority.
    pub fn confined_assignment(
        &self,
        storage: &StoragePlan,
        expression: BoundExpressionId,
    ) -> Option<&crate::StoragePath> {
        let path = self.assignment_target(expression)?.storage_path()?;

        (storage.identity(path.root)?.is_invocation_owned(self.kind) && path.is_inline())
            .then_some(path)
    }

    /// Returns the established decision for a selected raw-memory operation.
    pub fn memory_operation_status(
        &self,
        expression: BoundExpressionId,
    ) -> Option<crate::MemoryOperationStatus> {
        self.memory_operations
            .binary_search_by_key(&expression, |operation| operation.expression())
            .ok()
            .map(|index| self.memory_operations[index].status())
    }

    /// Returns a boolean result established on every reachable evaluation of a condition.
    pub fn condition_value(&self, expression: BoundExpressionId) -> Option<bool> {
        self.conditions.get(&expression).copied()
    }

    /// Whether the correlated flow can enter a lexical block.
    pub fn is_block_reachable(&self, block: BoundBlockId) -> bool {
        self.reachable_blocks.binary_search(&block).is_ok()
    }

    /// Returns storage state immediately before one direct-await expression.
    pub fn suspension(
        &self,
        occurrence: impl Into<crate::BoundExecutionSite>,
    ) -> Option<&StorageSuspensionState> {
        self.suspensions
            .binary_search_by_key(&occurrence.into(), StorageSuspensionState::occurrence)
            .ok()
            .map(|index| &self.suspensions[index])
    }

    /// Returns reached lexical scope-exit decisions in point order.
    pub fn exits(&self) -> &[StorageExitDecision] {
        &self.exits
    }

    /// Returns the checked decision if this lexical scope exit was reached.
    pub fn exit(&self, point: StorageExitPoint) -> Option<&StorageExitDecision> {
        self.exits.binary_search_by_key(&point, StorageExitDecision::point)
            .ok().map(|index| &self.exits[index])
    }

    /// Returns memory-operation decisions in bound-expression order.
    pub fn memory_operations(&self) -> &[MemoryOperationDecision] {
        &self.memory_operations
    }

    /// Returns the checked memory operation for one bound expression.
    pub fn checked_memory_operation(
        &self,
        expression: BoundExpressionId,
    ) -> Option<&CheckedMemoryOperation> {
        self.checked_memory_operations.as_ref()?.operation(expression)
    }

    /// Returns whether recovery prevented complete storage decisions.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

#[cfg(test)]
mod tests {
    use super::{
        StorageExitDecision, StorageExitPoint, StorageFlow, StorageFlowBuildError,
        StorageOperationDecision, StorageOperationStatus, StorageSuspensionState,
    };
    use crate::{
        BoundBlockId, BoundExpressionId, BoundUnitId, BoundUnitKind, StorageAccessId,
        StorageAccessPlan, StorageAccessPurpose, StorageIdentityId,
    };

    #[test]
    fn storage_flow_retains_operations_and_normalized_exit_state() {
        let unit = BoundUnitId::new(3);
        let expression = BoundExpressionId::from_slot(unit, 0);
        let access = StorageAccessId::from_slot(unit, 0);
        let storage = StorageIdentityId::from_slot(unit, 0);
        let moved = StorageIdentityId::from_slot(unit, 1);
        let scope = BoundBlockId::from_slot(unit, 0);

        let operation = StorageOperationDecision::new(
            StorageAccessPlan::new(expression.into(), expression, StorageAccessPurpose::Read, access),
            StorageAccessPurpose::Read,
            None,
            StorageOperationStatus::Valid,
        );

        let exit = StorageExitDecision::new(
            scope.into(),
            scope.into(),
            [storage, storage],
            [storage, storage],
            [
                crate::StoragePath::root(moved),
                crate::StoragePath::root(moved),
            ],
            [],
            [moved, moved],
            [],
            false,
        );

        let suspension = StorageSuspensionState::new(
            expression,
            [storage],
            [storage],
            [crate::StoragePath::root(storage)],
            [],
            [],
        );

        let storage_flow = StorageFlow::try_new(
            unit,
            BoundUnitKind::CallableBody,
            [operation],
            [suspension],
            [scope],
            [exit],
            false,
        )
        .unwrap_or_else(|error| panic!("unit-local storage-flow analysis must build: {error:?}"));

        assert_eq!(storage_flow.operations(), &[operation]);

        assert_eq!(
            storage_flow
                .suspension(expression)
                .map(StorageSuspensionState::initialized),
            Some(&[storage][..])
        );

        let exit = storage_flow.exit(StorageExitPoint::new(scope.into(), scope.into()))
            .expect("reached exit retains its storage decision");

        assert_eq!(exit.initialized(), &[storage]);

        assert_eq!(
            exit.moved(),
            &[crate::StoragePath::root(moved)]
        );

        assert_eq!(exit.fully_moved(), &[moved]);
        assert!(!storage_flow.is_recovered());
    }

    #[test]
    fn guarded_await_distinguishes_producers_that_share_a_row_number() {
        use crate::{AnalysisOperationId, StorageAccessId, StorageOutcomeRows, StorageDeferredProducer, StorageInvalidation, StoragePath};

        let unit = BoundUnitId::new(4);
        let operation = AnalysisOperationId::new(unit, 0);
        let producer = |slot| StorageDeferredProducer::from(BoundExpressionId::from_slot(unit, slot));
        let first = producer(0);
        let second = producer(1);
        let path = |slot| StoragePath::root(StorageIdentityId::from_slot(unit, slot));
        let selection = |producer, rows| StorageOutcomeRows { producer, rows: Some(rows), task: None };

        let effects = |slots: &[u32]| StorageInvalidation::Observed {
            transition: crate::StorageObservationTransition::Contents,
            scope: crate::StorageMutationScope::Invocation,
            accesses: slots.iter().map(|&slot| (StorageAccessId::from_slot(unit, slot), vec![path(slot)])).collect(),
            snapshots: Default::default(),
            referents: Default::default(),
            stable_descriptors: Default::default(),
            stable_values: Default::default(),
            stable_reference_slots: Default::default(),
            normal_snapshots: Default::default(),
            stable_pointees: Default::default(),
        };

        let flow = StorageFlow::try_new(unit, BoundUnitKind::CallableBody, [], [], [], [], false)
            .expect("the fixture owns both producers and their await")
            .with_operation_invalidations([
                ((operation, selection(Some(first.clone()), [0].into())), effects(&[0])),
                ((operation, selection(Some(second.clone()), [0].into())), effects(&[1])),
                ((operation, selection(None, [0].into())), effects(&[2])),
            ].into());

        for (source, slot) in [(&first, 0), (&second, 1)] {
            let reached = flow.selected_operation_invalidation(operation, Some(&selection(Some(source.clone()), [0].into())));

            assert_eq!(reached.as_deref(), Some(&effects(&[slot, 2])));
        }

        let reached = flow.selected_operation_invalidation(operation, Some(&selection(Some(first), [0, 1].into())));

        assert_eq!(reached.as_deref(), Some(&effects(&[0, 1, 2])));
    }

    #[test]
    fn unreachable_suspensions_remain_distinct_from_missing_results() {
        let unit = BoundUnitId::new(4);
        let unreachable = BoundExpressionId::from_slot(unit, 0);
        let missing = BoundExpressionId::from_slot(unit, 1);

        let flow = StorageFlow::try_new(
            unit,
            BoundUnitKind::CallableBody,
            [],
            [StorageSuspensionState::unreachable(unreachable)],
            [],
            [],
            false,
        )
        .expect("fixture storage flow must be valid");

        assert!(
            !flow
                .suspension(unreachable)
                .expect("fixture flow table must retain the unreachable suspension")
                .is_reachable()
        );

        assert!(flow.suspension(missing).is_none());
        assert!(!flow.is_recovered());
    }

    #[test]
    fn recovered_flow_retains_requirements_without_establishing_authority() {
        use bray_symbols::{
            DependencyBorrowRelation, DependencyContractTemplateData, DependencyReferenceKind,
            DependencyRequirement, DependencyRequirementKind, DependencySubject,
            DependencySubjectRoot,
        };

        let receiver = DependencySubject::root(DependencySubjectRoot::Receiver);

        let requirement = DependencyRequirement::direct(
            receiver.clone(),
            DependencyRequirementKind::StorageAlive,
        );

        let relation = DependencyBorrowRelation::new(
            Some(receiver),
            DependencySubject::root(DependencySubjectRoot::Result),
            DependencyReferenceKind::Borrow,
        );

        let flow = StorageFlow::try_new(
            BoundUnitId::new(9),
            BoundUnitKind::CallableBody,
            [],
            [],
            [],
            [],
            true,
        )
        .expect("empty recovered flow is valid")
        .with_dependencies(
            DependencyContractTemplateData::new([requirement.clone()])
                .with_borrows([relation])
                .with_effects([]),
        );

        assert_eq!(flow.dependencies().requirements(), &[requirement]);
        assert!(flow.dependencies().borrows().is_empty());

        assert_eq!(
            flow.dependencies().effects(),
            &bray_symbols::DependencyStorageEffect::opaque_call()
        );
    }

    #[test]
    fn storage_flow_rejects_foreign_ids() {
        let unit = BoundUnitId::new(4);
        let foreign = BoundUnitId::new(5);

        let decision = StorageOperationDecision::new(
            StorageAccessPlan::new(
                BoundExpressionId::from_slot(unit, 0).into(),
                BoundExpressionId::from_slot(unit, 0),
                StorageAccessPurpose::Read,
                StorageAccessId::from_slot(foreign, 0),
            ),
            StorageAccessPurpose::Read,
            None,
            StorageOperationStatus::Recovered,
        );

        assert_eq!(
            StorageFlow::try_new(
                unit,
                BoundUnitKind::CallableBody,
                [decision],
                [],
                [],
                [],
                true,
            ),
            Err(StorageFlowBuildError::ForeignUnit)
        );
    }

    #[test]
    fn derived_paths_reject_foreign_projection_occurrences() {
        let unit = BoundUnitId::new(6);
        let expression = BoundExpressionId::from_slot(unit, 0);

        let target = crate::StoragePath {
            root: StorageIdentityId::from_slot(unit, 0),
            projections: [crate::StorageProjection::Element(
                BoundExpressionId::from_slot(BoundUnitId::new(7), 0),
            )]
            .into(),
        };

        let flow =
            StorageFlow::try_new(unit, BoundUnitKind::CallableBody, [], [], [], [], false)
                .expect("fixture storage flow must be valid");

        assert_eq!(
            flow.with_state_targets(
                [(
                    ((super::StorageStateTarget::Assignment(expression), super::AnalysisOperationId::new(unit, 0)), Default::default()),
                    Some(crate::StorageTarget::storage(target)),
                )]
                .into()
            ),
            Err(StorageFlowBuildError::ForeignUnit)
        );
    }

    #[test]
    fn assignment_publication_retains_borrowed_origins_and_checks_their_selectors() {
        let unit = BoundUnitId::new(6);
        let expression = BoundExpressionId::from_slot(unit, 0);

        for selector_unit in [unit, BoundUnitId::new(7)] {
            let target = crate::StorageTarget {
                previous: false,
                origin: crate::StorageOrigin::Entry {
                    reference: crate::StoragePath::root(StorageIdentityId::from_slot(unit, 0)),
                    layer: 1,
                    projections: [crate::StorageProjection::Element(
                        BoundExpressionId::from_slot(selector_unit, 0),
                    )]
                    .into(),
                kind: bray_symbols::DependencyReferenceKind::Address,
},
            };

            let flow =
                StorageFlow::try_new(unit, BoundUnitKind::CallableBody, [], [], [], [], false)
                    .expect("fixture storage flow must be valid");

            let access = super::StorageStateTarget::Assignment(expression);
            let selected = super::AnalysisOperationId::new(unit, 0);
            let copied = super::AnalysisOperationId::new(unit, 1);

            let rows = |row| crate::StorageOutcomeRows {
                rows: Some([row].into()),
                ..Default::default()
            };

            let published = flow.with_state_targets(
                [
                    (((access, selected), rows(0)), Some(target.clone())),
                    (((access, selected), rows(1)), None),
                    (((access, copied), Default::default()), None),
                ]
                .into(),
            );

            if selector_unit == unit {
                let published = published.expect("fixture assignment targets belong to the unit");

                assert_eq!(
                    published.operation_target(selected, access, Some(&rows(0))),
                    Some(&target)
                );

                assert!(published.operation_target(selected, access, Some(&rows(1))).is_none());
                assert!(published.operation_target(selected, access, None).is_none());
                assert!(published.operation_target(copied, access, None).is_none());
                assert!(published.assignment_target(expression).is_none(), "one exact operation cannot establish the whole source site's target");
            } else {
                assert_eq!(published, Err(StorageFlowBuildError::ForeignUnit));
            }
        }
    }

    #[test]
    fn storage_flow_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<StorageFlow>();
    }
}
