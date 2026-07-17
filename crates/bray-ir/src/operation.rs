use bray_runtime_interface::ProtectedAsyncFrameId;
use bray_symbols::{BorrowKind, TypeId};

use crate::{
    MirCall, MirFrameStateId, MirOperand, MirOperationId, MirPlace, MirRuntimeReference,
    MirSourceAnchor, MirStorageId, MirValueId,
};

/// Typed unary operation selected during lowering.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirUnaryOperator {
    /// Numeric negation.
    Negate,
    /// Boolean negation.
    Not,
    /// Bitwise complement.
    BitwiseNot,
}

/// Typed binary operation selected during lowering.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirBinaryOperator {
    /// Numeric addition.
    Add,
    /// Numeric subtraction.
    Subtract,
    /// Numeric multiplication.
    Multiply,
    /// Numeric division.
    Divide,
    /// Numeric remainder.
    Remainder,
    /// Equality comparison.
    Equal,
    /// Inequality comparison.
    NotEqual,
    /// Ordered less-than comparison.
    LessThan,
    /// Ordered less-than-or-equal comparison.
    LessThanOrEqual,
    /// Ordered greater-than comparison.
    GreaterThan,
    /// Ordered greater-than-or-equal comparison.
    GreaterThanOrEqual,
    /// Bitwise conjunction.
    BitwiseAnd,
    /// Bitwise disjunction.
    BitwiseOr,
    /// Bitwise exclusive disjunction.
    BitwiseXor,
    /// Left shift.
    ShiftLeft,
    /// Right shift.
    ShiftRight,
}

/// Terminal state published for one task run.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirTaskTerminalState {
    /// The task completed with a value.
    Completed(MirOperand),
    /// The task observed current-run cancellation.
    Cancelled,
    /// The task panicked with ownership of a panic report.
    Panicked(MirOperand),
}

/// Explicit protected-frame and task operation selected by checked lowering.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirExecutionOperation {
    /// Create an inactive protected frame in destination storage.
    CreateFrame {
        /// Stable frame representation.
        frame: ProtectedAsyncFrameId,
        /// Destination for the inactive frame.
        destination: MirPlace,
    },
    /// Move an inactive frame before its first resume.
    MoveInactiveFrame {
        /// Stable frame representation.
        frame: ProtectedAsyncFrameId,
        /// Source frame storage.
        source: MirPlace,
        /// Destination frame storage.
        destination: MirPlace,
    },
    /// Enter or resume a protected frame state.
    ResumeFrame {
        /// Stable frame representation.
        frame: ProtectedAsyncFrameId,
        /// State being entered.
        state: MirFrameStateId,
        /// Stable frame storage.
        storage: MirStorageId,
        /// Selected private ABI role.
        runtime: MirRuntimeReference,
    },
    /// Compose a child frame directly into its active parent.
    ComposeAwaitedFrame {
        /// Parent protected frame.
        parent: ProtectedAsyncFrameId,
        /// Child protected frame.
        child: ProtectedAsyncFrameId,
        /// Inactive child frame value.
        frame: MirOperand,
    },
    /// Move a directly awaited child's completion into destination storage.
    CommitAwaitedCompletion {
        /// Child protected frame.
        child: ProtectedAsyncFrameId,
        /// Completed child value.
        value: MirOperand,
        /// Destination in the parent frame.
        destination: MirPlace,
    },
    /// Transfer an inactive frame into a newly started task.
    StartTask {
        /// Stable frame representation.
        frame: ProtectedAsyncFrameId,
        /// Inactive frame value.
        value: MirOperand,
        /// Stable task storage.
        task: MirStorageId,
        /// Selected private task-allocation ABI role.
        allocation: MirRuntimeReference,
        /// Selected private task-start ABI role.
        start: MirRuntimeReference,
    },
    /// Request cancellation of an owned task.
    RequestTaskCancellation {
        /// Task control state.
        task: MirStorageId,
        /// Selected private cancellation ABI role.
        runtime: MirRuntimeReference,
    },
    /// Observe whether cancellation was requested for the current run.
    ObserveCurrentRunCancellation {
        /// Selected private cancellation-observation ABI role.
        runtime: MirRuntimeReference,
    },
    /// Register and resolve terminal task observation.
    ResolveTask {
        /// Task control state.
        task: MirStorageId,
        /// Selected private join ABI role.
        runtime: MirRuntimeReference,
    },
    /// Publish exactly one task terminal state.
    PublishTerminalState {
        /// Task control state.
        task: MirStorageId,
        /// Terminal state being published.
        state: MirTaskTerminalState,
        /// Selected private publication ABI role.
        runtime: MirRuntimeReference,
    },
    /// Execute phase-one cancellation broadcast for a protected frame.
    ExecuteCleanupBroadcast {
        /// Stable frame representation.
        frame: ProtectedAsyncFrameId,
        /// Selected private broadcast ABI role.
        runtime: MirRuntimeReference,
    },
    /// Execute phase-two lifecycle resolution for a protected frame.
    ExecuteLifecycleResolution {
        /// Stable frame representation.
        frame: ProtectedAsyncFrameId,
        /// Selected private lifecycle ABI role.
        runtime: MirRuntimeReference,
    },
    /// Transfer ownership of a cleanup incident.
    TransferCleanupIncident {
        /// Incident value being transferred.
        incident: MirOperand,
        /// Selected private transfer ABI role.
        runtime: MirRuntimeReference,
    },
    /// Destroy terminal task control state exactly once.
    DestroyTerminalTask {
        /// Terminal task control state.
        task: MirStorageId,
    },
}

/// One explicit MIR operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirOperationKind {
    /// Assign an operand into storage.
    Store {
        /// Destination storage place.
        destination: MirPlace,
        /// Value being stored.
        value: MirOperand,
    },
    /// Borrow a storage place.
    Borrow {
        /// Checked borrow category.
        kind: BorrowKind,
        /// Borrowed place.
        place: MirPlace,
    },
    /// Apply a typed unary operator.
    Unary {
        /// Selected operator.
        operator: MirUnaryOperator,
        /// Input value.
        operand: MirOperand,
    },
    /// Apply a typed binary operator.
    Binary {
        /// Selected operator.
        operator: MirBinaryOperator,
        /// Left input.
        left: MirOperand,
        /// Right input.
        right: MirOperand,
    },
    /// Apply a checked semantic conversion.
    Convert {
        /// Input value.
        operand: MirOperand,
        /// Converted type.
        target: TypeId,
    },
    /// Invoke an exact callable target.
    Call(MirCall),
    /// Run checked finalization for a storage place.
    Finalize(MirPlace),
    /// Destroy a storage place after its value is no longer live.
    Destroy(MirPlace),
    /// Perform a protected-frame or task operation.
    Execution(MirExecutionOperation),
}

/// One committed operation and its optional result value.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct MirOperationCommit {
    operation: MirOperationId,
    result: Option<MirValueId>,
}

impl MirOperationCommit {
    pub(crate) const fn new(operation: MirOperationId, result: Option<MirValueId>) -> Self {
        Self { operation, result }
    }

    /// Returns the committed operation ID.
    pub const fn operation(self) -> MirOperationId {
        self.operation
    }

    /// Returns the value produced by the operation, when any.
    pub const fn result(self) -> Option<MirValueId> {
        self.result
    }
}

/// One operation stored in a MIR unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirOperation {
    source: MirSourceAnchor,
    kind: MirOperationKind,
    result: Option<MirValueId>,
}

impl MirOperation {
    pub(crate) const fn new(
        source: MirSourceAnchor,
        kind: MirOperationKind,
        result: Option<MirValueId>,
    ) -> Self {
        Self {
            source,
            kind,
            result,
        }
    }

    /// Returns the operation's source provenance.
    pub const fn source(&self) -> &MirSourceAnchor {
        &self.source
    }

    /// Returns the explicit operation payload.
    pub const fn kind(&self) -> &MirOperationKind {
        &self.kind
    }

    /// Returns the value produced by the operation, when any.
    pub const fn result(&self) -> Option<MirValueId> {
        self.result
    }
}
