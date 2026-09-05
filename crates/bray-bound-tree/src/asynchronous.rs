use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};

use crate::{
    AnyBoundNodeId, BodyBehaviorCall, BoundBlockId, BoundDependencyContractId,
    BoundDependencySubject, BoundExpressionId, BoundUnitId, BoundUnitKind, StorageAccessId,
    StorageIdentityId,
};

/// A language-defined operation on an owned future or task.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AsyncTaskOperationKind {
    /// Transfer an inactive future into an independently scheduled task.
    Start,
    /// Transfer a task into a lazy joining computation.
    Join,
    /// Transfer a task into a lazy cancellation computation.
    Cancel,
}

/// One checked future or task operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AsyncTaskOperation {
    expression: BoundExpressionId,
    kind: AsyncTaskOperationKind,
}

impl AsyncTaskOperation {
    /// Creates one checked operation.
    pub const fn new(expression: BoundExpressionId, kind: AsyncTaskOperationKind) -> Self {
        Self { expression, kind }
    }

    /// Returns the source-correlated operation expression.
    pub const fn expression(self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the semantic operation performed by the expression.
    pub const fn kind(self) -> AsyncTaskOperationKind {
        self.kind
    }
}

/// The language operation that suspends one protected frame.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AsyncSuspensionKind {
    /// Wait for one future expression to complete.
    Await { operand: BoundExpressionId },
    /// Yield execution so another ready task can run.
    Yield,
}

/// One suspension point and the semantic state it retains.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AsyncSuspensionPoint {
    expression: BoundExpressionId,
    kind: AsyncSuspensionKind,
    dependency_contract: Option<BoundDependencyContractId>,
    deferred_calls: Arc<[BodyBehaviorCall]>,
    retained_subjects: Arc<[BoundDependencySubject]>,
    is_recovered: bool,
}

impl AsyncSuspensionPoint {
    /// Creates one normalized suspension point.
    pub fn new(
        expression: BoundExpressionId,
        kind: AsyncSuspensionKind,
        dependency_contract: Option<BoundDependencyContractId>,
        deferred_calls: impl IntoIterator<Item = BodyBehaviorCall>,
        retained_subjects: impl IntoIterator<Item = BoundDependencySubject>,
        is_recovered: bool,
    ) -> Self {
        Self {
            expression,
            kind,
            dependency_contract,
            deferred_calls: sorted_unique_shared_slice(deferred_calls),
            retained_subjects: sorted_unique_shared_slice(retained_subjects),
            is_recovered,
        }
    }

    /// Returns the expression that suspends execution.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the language operation that causes suspension.
    pub const fn kind(&self) -> AsyncSuspensionKind {
        self.kind
    }

    /// Returns the dependency contract carried by the future operand.
    pub const fn dependency_contract(&self) -> Option<BoundDependencyContractId> {
        self.dependency_contract
    }

    /// Returns deferred callable bodies that can execute at this await.
    pub fn deferred_calls(&self) -> &[BodyBehaviorCall] {
        &self.deferred_calls
    }

    /// Returns semantic subjects retained while the current run is suspended.
    pub fn retained_subjects(&self) -> &[BoundDependencySubject] {
        &self.retained_subjects
    }

    /// Returns whether recovery prevented a complete suspension decision.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

/// The ordered cleanup phases selected for one initialized storage identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AsyncCleanupPhases {
    /// Broadcast cancellation without a later lifecycle operation.
    Cancellation,
    /// Resolve lifecycle state without a preceding cancellation broadcast.
    Lifecycle,
    /// Broadcast cancellation before resolving lifecycle state.
    CancellationThenLifecycle,
}

impl AsyncCleanupPhases {
    /// Returns whether cleanup starts with task cancellation broadcast.
    pub const fn includes_cancellation(self) -> bool {
        matches!(self, Self::Cancellation | Self::CancellationThenLifecycle)
    }

    /// Returns whether cleanup resolves lifecycle state.
    pub const fn includes_lifecycle(self) -> bool {
        matches!(self, Self::Lifecycle | Self::CancellationThenLifecycle)
    }
}

/// The exact outcome assigned to one live storage identity at a scope exit.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AsyncStorageExitDisposition {
    /// The identity remains owned by an enclosing lexical scope.
    Retained,
    /// The identity transfers through the enclosing result or destructor boundary.
    Transferred,
    /// The identity was moved in full before the exit.
    Moved,
    /// The identity requires no cancellation or lifecycle operation.
    NoCleanup,
    /// The identity requires ordered cleanup through its root access.
    Cleanup {
        /// The root access used by both cleanup phases.
        access: StorageAccessId,
        /// The ordered phases required by the checked type representation.
        phases: AsyncCleanupPhases,
    },
    /// Earlier recovery prevented a complete disposition.
    Recovered(AsyncStorageExitRecoveryCause),
}

/// The exact missing analysis that prevented one storage disposition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AsyncStorageExitRecoveryCause {
    /// Storage planning did not publish the identity's root access.
    UnavailableRootAccess,
    /// Recovery prevented the checker from deciding the cleanup shape.
    UnavailableCleanupShape,
    /// Partial represented storage has no complete codegen-ready cleanup plan.
    UnavailablePartialCleanup,
    /// Cleanup dependencies do not admit a complete lifecycle order.
    UnavailableCleanupOrder,
}

/// Type-driven cleanup work required when an owned initialized identity leaves its scope.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AsyncStorageCleanupRequirement {
    /// The stored type requires no cleanup phase.
    None,
    /// The stored type requires these ordered cleanup phases.
    Cleanup(AsyncCleanupPhases),
    /// Recovery prevented a complete cleanup requirement.
    Recovered(AsyncStorageExitRecoveryCause),
}

/// Independently checked ownership, transfer, and cleanup facts for one live identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AsyncStorageRequirement {
    identity: StorageIdentityId,
    owner: Option<BoundBlockId>,
    transfers: bool,
    cleanup: AsyncStorageCleanupRequirement,
}

impl AsyncStorageRequirement {
    /// Creates one checked storage requirement.
    pub const fn new(
        identity: StorageIdentityId,
        owner: Option<BoundBlockId>,
        transfers: bool,
        cleanup: AsyncStorageCleanupRequirement,
    ) -> Self {
        Self {
            identity,
            owner,
            transfers,
            cleanup,
        }
    }

    /// Returns the storage identity described by this requirement.
    pub const fn identity(self) -> StorageIdentityId {
        self.identity
    }

    /// Returns the lexical scope that owns the identity.
    pub const fn owner(self) -> Option<BoundBlockId> {
        self.owner
    }

    /// Returns whether this identity transfers through its unit boundary.
    pub const fn transfers(self) -> bool {
        self.transfers
    }

    /// Returns the type-driven cleanup requirement.
    pub const fn cleanup(self) -> AsyncStorageCleanupRequirement {
        self.cleanup
    }
}

/// One live storage identity and its scope-exit disposition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AsyncStorageExitDecision {
    identity: StorageIdentityId,
    disposition: AsyncStorageExitDisposition,
}

impl AsyncStorageExitDecision {
    /// Creates one source-correlated storage disposition.
    pub const fn new(
        identity: StorageIdentityId,
        disposition: AsyncStorageExitDisposition,
    ) -> Self {
        Self {
            identity,
            disposition,
        }
    }

    /// Returns the live storage identity being disposed.
    pub const fn identity(self) -> StorageIdentityId {
        self.identity
    }

    /// Returns the exact checked disposition.
    pub const fn disposition(self) -> AsyncStorageExitDisposition {
        self.disposition
    }
}

/// The ordered semantic work required when one lexical task scope exits.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct AsyncScopeExitPlan {
    scope: BoundBlockId,
    exit: AnyBoundNodeId,
    storage: Arc<[AsyncStorageExitDecision]>,
    cancellation_broadcast: Arc<[StorageAccessId]>,
    lifecycle_resolution: Arc<[StorageAccessId]>,
    moved: Arc<[StorageAccessId]>,
    is_recovered: bool,
}

impl AsyncScopeExitPlan {
    /// Creates a two-phase scope-exit plan.
    pub fn new(
        scope: BoundBlockId,
        exit: AnyBoundNodeId,
        storage: impl IntoIterator<Item = AsyncStorageExitDecision>,
        cancellation_broadcast: impl IntoIterator<Item = StorageAccessId>,
        lifecycle_resolution: impl IntoIterator<Item = StorageAccessId>,
        moved: impl IntoIterator<Item = StorageAccessId>,
        is_recovered: bool,
    ) -> Self {
        Self {
            scope,
            exit,
            storage: shared_slice(storage),
            cancellation_broadcast: shared_slice(cancellation_broadcast),
            lifecycle_resolution: shared_slice(lifecycle_resolution),
            moved: sorted_unique_shared_slice(moved),
            is_recovered,
        }
    }

    /// Returns the lexical scope being exited.
    pub const fn scope(&self) -> BoundBlockId {
        self.scope
    }

    /// Returns the bound node whose completion or transfer exits the scope.
    pub const fn exit(&self) -> AnyBoundNodeId {
        self.exit
    }

    /// Returns one disposition for every live identity at this exit.
    pub fn storage(&self) -> &[AsyncStorageExitDecision] {
        &self.storage
    }

    /// Returns owned storage visited during cancellation broadcast.
    pub fn cancellation_broadcast(&self) -> &[StorageAccessId] {
        &self.cancellation_broadcast
    }

    /// Returns owned storage resolved after cancellation broadcast completes.
    pub fn lifecycle_resolution(&self) -> &[StorageAccessId] {
        &self.lifecycle_resolution
    }

    /// Returns paths already moved at this scope exit.
    pub fn moved(&self) -> &[StorageAccessId] {
        &self.moved
    }

    /// Returns whether recovery prevented a complete cleanup plan.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

/// A malformed async analysis table.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum AsyncAnalysisBuildError {
    /// One analysis references semantic state owned by another bound unit.
    ForeignUnit,
}

/// Durable async frame, suspension, task, and cleanup analysis for one bound unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CheckedAsync {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    frame_dependencies: Arc<[BoundDependencySubject]>,
    suspensions: Arc<[AsyncSuspensionPoint]>,
    task_operations: Arc<[AsyncTaskOperation]>,
    storage_requirements: Arc<[AsyncStorageRequirement]>,
    scope_exits: Arc<[AsyncScopeExitPlan]>,
    is_recovered: bool,
}

impl CheckedAsync {
    /// Validates and creates one immutable async analysis table.
    pub fn try_new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        frame_dependencies: impl IntoIterator<Item = BoundDependencySubject>,
        suspensions: impl IntoIterator<Item = AsyncSuspensionPoint>,
        task_operations: impl IntoIterator<Item = AsyncTaskOperation>,
        storage_requirements: impl IntoIterator<Item = AsyncStorageRequirement>,
        scope_exits: impl IntoIterator<Item = AsyncScopeExitPlan>,
        is_recovered: bool,
    ) -> Result<Self, AsyncAnalysisBuildError> {
        let frame_dependencies = sorted_unique_shared_slice(frame_dependencies);
        let suspensions = shared_slice(suspensions);
        let task_operations = shared_slice(task_operations);
        let storage_requirements = shared_slice(storage_requirements);
        let scope_exits = shared_slice(scope_exits);

        if frame_dependencies
            .iter()
            .any(|subject| !subject.is_valid_for(unit))
            || suspensions.iter().any(|suspension| {
                suspension.expression().unit() != unit
                    || match suspension.kind() {
                        AsyncSuspensionKind::Await { operand } => operand.unit() != unit,
                        AsyncSuspensionKind::Yield => false,
                    }
                    || suspension
                        .dependency_contract()
                        .is_some_and(|contract| contract.unit() != unit)
                    || suspension
                        .retained_subjects()
                        .iter()
                        .any(|subject| !subject.is_valid_for(unit))
            })
            || task_operations
                .iter()
                .any(|operation| operation.expression().unit() != unit)
            || storage_requirements.iter().any(|requirement| {
                requirement.identity().unit() != unit
                    || requirement
                        .owner()
                        .is_some_and(|owner| owner.unit() != unit)
            })
            || scope_exits.iter().any(|exit| {
                exit.scope().unit() != unit
                    || exit.exit().unit() != unit
                    || exit.storage().iter().any(|decision| {
                        decision.identity().unit() != unit
                            || matches!(
                                decision.disposition(),
                                AsyncStorageExitDisposition::Cleanup { access, .. }
                                    if access.unit() != unit
                            )
                    })
                    || exit
                        .cancellation_broadcast()
                        .iter()
                        .chain(exit.lifecycle_resolution())
                        .chain(exit.moved())
                        .any(|access| access.unit() != unit)
            })
        {
            return Err(AsyncAnalysisBuildError::ForeignUnit);
        }

        Ok(Self {
            unit,
            kind,
            frame_dependencies,
            suspensions,
            task_operations,
            storage_requirements,
            scope_exits,
            is_recovered,
        })
    }

    /// Returns the checked bound unit.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the checked unit category.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns every semantic subject retained by the async frame.
    pub fn frame_dependencies(&self) -> &[BoundDependencySubject] {
        &self.frame_dependencies
    }

    /// Returns suspension points in evaluation order.
    pub fn suspensions(&self) -> &[AsyncSuspensionPoint] {
        &self.suspensions
    }

    /// Returns task operations in evaluation order.
    pub fn task_operations(&self) -> &[AsyncTaskOperation] {
        &self.task_operations
    }

    /// Returns independently checked storage requirements in identity order.
    pub fn storage_requirements(&self) -> &[AsyncStorageRequirement] {
        &self.storage_requirements
    }

    /// Returns two-phase cleanup plans in control-flow order.
    pub fn scope_exits(&self) -> &[AsyncScopeExitPlan] {
        &self.scope_exits
    }

    /// Returns whether recovery prevented complete async checking.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AsyncCleanupPhases, AsyncScopeExitPlan, AsyncStorageExitDecision,
        AsyncStorageExitDisposition, AsyncSuspensionKind, AsyncSuspensionPoint, AsyncTaskOperation,
        AsyncTaskOperationKind, CheckedAsync,
    };
    use crate::{
        BoundBlockId, BoundDependencySubject, BoundExpressionId, BoundUnitId, BoundUnitKind,
        StorageAccessId,
    };

    #[test]
    fn async_analysis_normalizes_frame_dependencies_and_preserves_operation_order() {
        let unit = BoundUnitId::new(7);
        let await_expression = BoundExpressionId::from_slot(unit, 2);
        let operand = BoundExpressionId::from_slot(unit, 1);
        let storage = StorageAccessId::from_slot(unit, 3);
        let dependency = BoundDependencySubject::StorageAccess(storage);

        let suspension = AsyncSuspensionPoint::new(
            await_expression,
            AsyncSuspensionKind::Await { operand },
            None,
            [],
            [dependency],
            false,
        );

        let operation = AsyncTaskOperation::new(operand, AsyncTaskOperationKind::Start);

        let cleanup = AsyncScopeExitPlan::new(
            BoundBlockId::from_slot(unit, 4),
            BoundBlockId::from_slot(unit, 4).into(),
            [AsyncStorageExitDecision::new(
                crate::StorageIdentityId::from_slot(unit, 5),
                AsyncStorageExitDisposition::Cleanup {
                    access: storage,
                    phases: AsyncCleanupPhases::CancellationThenLifecycle,
                },
            )],
            [storage],
            [storage],
            [storage],
            false,
        );

        let analysis = CheckedAsync::try_new(
            unit,
            BoundUnitKind::CallableBody,
            [dependency, dependency],
            [suspension],
            [operation],
            [],
            [cleanup],
            false,
        )
        .unwrap_or_else(|error| panic!("unit-local analysis must build: {error:?}"));

        assert_eq!(analysis.frame_dependencies(), &[dependency]);
        assert_eq!(analysis.task_operations(), &[operation]);
        assert_eq!(analysis.suspensions().len(), 1);
        assert_eq!(analysis.scope_exits().len(), 1);
        assert_eq!(analysis.scope_exits()[0].moved(), &[storage]);
        assert_eq!(analysis.scope_exits()[0].storage().len(), 1);
    }

    #[test]
    fn async_analysis_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CheckedAsync>();
    }
}
