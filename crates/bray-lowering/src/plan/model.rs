use bray_bound_tree::{
    AnyBoundNodeId, AsyncStorageExitRecoveryCause, BoundBlockId, BoundExpressionId,
    StorageAccessId, StorageIdentityId,
};

/// The semantic plan category that failed lowering-readiness verification.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LoweringPlanKind {
    /// The complete async analysis table.
    Analysis,
    /// One suspension point.
    Suspension,
    /// One future or task operation.
    TaskOperation,
    /// One lexical scope exit.
    ScopeExit,
    /// One initialized storage disposition.
    StorageDisposition,
    /// The ordered task-cancellation phase.
    CancellationPhase,
    /// The ordered lifecycle-resolution phase.
    LifecyclePhase,
}

impl LoweringPlanKind {
    /// Returns the stable machine name for this plan category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Analysis => "analysis",
            Self::Suspension => "suspension",
            Self::TaskOperation => "task_operation",
            Self::ScopeExit => "scope_exit",
            Self::StorageDisposition => "storage_disposition",
            Self::CancellationPhase => "cancellation_phase",
            Self::LifecyclePhase => "lifecycle_phase",
        }
    }
}

/// The exact reason a checked plan is not safe to consume during lowering.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LoweringPlanFailureCause {
    /// Required checked data is absent.
    Missing,
    /// The same semantic identity has more than one decision.
    Duplicate,
    /// The plan contains an identity outside the closed expected set.
    Unexpected,
    /// Earlier recovery prevented a complete decision.
    Recovered,
    /// A storage producer could not establish this exact cleanup requirement.
    StorageRecovery(AsyncStorageExitRecoveryCause),
    /// A decision disagrees with another checked input.
    Contradictory,
    /// Complete decisions appear in the wrong semantic order.
    OutOfOrder,
}

impl LoweringPlanFailureCause {
    /// Returns the stable machine name for this failure cause.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Missing => "missing",
            Self::Duplicate => "duplicate",
            Self::Unexpected => "unexpected",
            Self::Recovered => "recovered",
            Self::StorageRecovery(cause) => match cause {
                AsyncStorageExitRecoveryCause::UnavailableRootAccess => "unavailable_root_access",
                AsyncStorageExitRecoveryCause::UnavailableCleanupShape => {
                    "unavailable_cleanup_shape"
                }
                AsyncStorageExitRecoveryCause::UnavailablePartialCleanup => {
                    "unavailable_partial_cleanup"
                }
                AsyncStorageExitRecoveryCause::UnavailableCleanupOrder => {
                    "unavailable_cleanup_order"
                }
            },
            Self::Contradictory => "contradictory",
            Self::OutOfOrder => "out_of_order",
        }
    }
}

/// One exact checked-plan contract violation at the lowering boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LoweringPlanFailure {
    kind: LoweringPlanKind,
    cause: LoweringPlanFailureCause,
    expression: Option<BoundExpressionId>,
    scope: Option<BoundBlockId>,
    exit: Option<AnyBoundNodeId>,
    storage: Option<StorageIdentityId>,
    access: Option<StorageAccessId>,
}

impl LoweringPlanFailure {
    pub(super) const fn analysis(cause: LoweringPlanFailureCause) -> Self {
        Self::new(
            LoweringPlanKind::Analysis,
            cause,
            None,
            None,
            None,
            None,
            None,
        )
    }

    pub(super) const fn for_expression(
        kind: LoweringPlanKind,
        cause: LoweringPlanFailureCause,
        expression: BoundExpressionId,
    ) -> Self {
        Self::new(kind, cause, Some(expression), None, None, None, None)
    }

    pub(super) const fn scope_exit(
        kind: LoweringPlanKind,
        cause: LoweringPlanFailureCause,
        scope: BoundBlockId,
        exit: AnyBoundNodeId,
    ) -> Self {
        Self::new(kind, cause, None, Some(scope), Some(exit), None, None)
    }

    pub(super) const fn for_storage(
        cause: LoweringPlanFailureCause,
        scope: BoundBlockId,
        exit: AnyBoundNodeId,
        storage: StorageIdentityId,
    ) -> Self {
        Self::new(
            LoweringPlanKind::StorageDisposition,
            cause,
            None,
            Some(scope),
            Some(exit),
            Some(storage),
            None,
        )
    }

    pub(super) const fn storage_requirement(
        cause: LoweringPlanFailureCause,
        storage: StorageIdentityId,
    ) -> Self {
        Self::new(
            LoweringPlanKind::StorageDisposition,
            cause,
            None,
            None,
            None,
            Some(storage),
            None,
        )
    }

    pub(super) const fn for_access(
        kind: LoweringPlanKind,
        cause: LoweringPlanFailureCause,
        scope: BoundBlockId,
        exit: AnyBoundNodeId,
        access: StorageAccessId,
    ) -> Self {
        Self::new(
            kind,
            cause,
            None,
            Some(scope),
            Some(exit),
            None,
            Some(access),
        )
    }

    const fn new(
        kind: LoweringPlanKind,
        cause: LoweringPlanFailureCause,
        expression: Option<BoundExpressionId>,
        scope: Option<BoundBlockId>,
        exit: Option<AnyBoundNodeId>,
        storage: Option<StorageIdentityId>,
        access: Option<StorageAccessId>,
    ) -> Self {
        Self {
            kind,
            cause,
            expression,
            scope,
            exit,
            storage,
            access,
        }
    }

    /// Returns the failed plan category.
    pub const fn kind(self) -> LoweringPlanKind {
        self.kind
    }

    /// Returns the exact contract violation.
    pub const fn cause(self) -> LoweringPlanFailureCause {
        self.cause
    }

    /// Returns the affected expression, when the plan is expression-correlated.
    pub const fn expression(self) -> Option<BoundExpressionId> {
        self.expression
    }

    /// Returns the affected lexical scope, when available.
    pub const fn scope(self) -> Option<BoundBlockId> {
        self.scope
    }

    /// Returns the affected exit occurrence, when available.
    pub const fn exit(self) -> Option<AnyBoundNodeId> {
        self.exit
    }

    /// Returns the affected storage identity, when available.
    pub const fn storage(self) -> Option<StorageIdentityId> {
        self.storage
    }

    /// Returns the affected storage access, when available.
    pub const fn access(self) -> Option<StorageAccessId> {
        self.access
    }
}
