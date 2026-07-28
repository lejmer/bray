use std::sync::Arc;

use bray_base::shared_slice;
use bray_bound_tree::{
    ConstructionDefaultProvider, ConstructionInputId, ConstructionTarget, PatternProjection,
    SelectedConversion,
};
use bray_runtime_interface::ProtectedAsyncFrameId;
use bray_symbols::{BorrowKind, ConstantTermId};

use crate::{
    MirCall, MirCleanupPhase, MirFrameStateId, MirOperand, MirOperationId, MirPlace,
    MirRuntimeReference, MirSourceAnchor, MirStorageId, MirValueId,
};

/// The checked semantic role of one store operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirStoreKind {
    /// Initialize storage that does not currently contain a live value.
    Initialize,
    /// Assign to storage using the checked replacement semantics.
    Assign,
}

/// The checked source of one panic report.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirPanicCause {
    /// An explicit `panic` message.
    Message(MirOperand),
    /// A failed assertion with an optional evaluated message.
    Assertion(Option<MirOperand>),
}

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

/// The normalized representation built by one aggregate operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirAggregateKind {
    /// A tuple value.
    Tuple,
    /// A fixed array with one operand per element.
    Array,
    /// A fixed array produced by repeating one value a checked number of times.
    RepeatedArray,
}

/// The normalized result accumulated by one generator expression.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirGeneratorKind {
    /// A fixed array whose checked count determines its final extent.
    Array,
    /// A lazy generator value.
    General,
}

/// One explicit generator accumulation step.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirGeneratorOperation {
    /// Initialize generator result storage.
    Begin {
        /// Result representation being accumulated.
        kind: MirGeneratorKind,
        /// Destination retaining the in-progress result.
        destination: MirPlace,
        /// Exact element count when checking proved one.
        exact_count: Option<ConstantTermId>,
    },
    /// Append one yielded element.
    Push {
        /// In-progress result storage.
        destination: MirPlace,
        /// Element yielded by the generator body.
        value: MirOperand,
    },
    /// Finish accumulation and produce the generator expression value.
    Finish {
        /// Completed result storage.
        destination: MirPlace,
    },
}

/// One aggregate construction with operands in evaluation order.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirAggregate {
    kind: MirAggregateKind,
    operands: Arc<[MirOperand]>,
}

impl MirAggregate {
    /// Creates one normalized aggregate construction.
    pub fn new(kind: MirAggregateKind, operands: impl IntoIterator<Item = MirOperand>) -> Self {
        Self {
            kind,
            operands: shared_slice(operands),
        }
    }

    /// Returns the aggregate representation.
    pub const fn kind(&self) -> MirAggregateKind {
        self.kind
    }

    /// Returns aggregate operands in evaluation order.
    pub fn operands(&self) -> &[MirOperand] {
        &self.operands
    }
}

/// One supplied or defaulted input of a normalized construction operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirConstructionInput {
    /// A source value mapped to its exact declaration input.
    Explicit {
        /// The initialized field or parameter.
        input: ConstructionInputId,
        /// The input's declaration-order ordinal.
        ordinal: u32,
        /// The evaluated source value.
        value: MirOperand,
    },
    /// An omitted input supplied by its declaration-owned runtime default.
    Default {
        /// The initialized field or parameter.
        input: ConstructionInputId,
        /// The input's declaration-order ordinal.
        ordinal: u32,
        /// The exact default provider.
        provider: ConstructionDefaultProvider,
    },
}

impl MirConstructionInput {
    /// Returns the initialized field or parameter.
    pub const fn input(&self) -> ConstructionInputId {
        match self {
            Self::Explicit { input, .. } | Self::Default { input, .. } => *input,
        }
    }

    /// Returns the input's declaration-order ordinal.
    pub const fn ordinal(&self) -> u32 {
        match self {
            Self::Explicit { ordinal, .. } | Self::Default { ordinal, .. } => *ordinal,
        }
    }
}

/// One normalized struct, union-variant, or type-form construction.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirConstruction {
    target: ConstructionTarget,
    inputs: Arc<[MirConstructionInput]>,
}

impl MirConstruction {
    /// Creates one construction from its checked target and ordered inputs.
    pub fn new(
        target: ConstructionTarget,
        inputs: impl IntoIterator<Item = MirConstructionInput>,
    ) -> Self {
        Self {
            target,
            inputs: shared_slice(inputs),
        }
    }

    /// Returns the exact construction target.
    pub const fn target(&self) -> ConstructionTarget {
        self.target
    }

    /// Returns explicit inputs in source order followed by defaults in declaration order.
    pub fn inputs(&self) -> &[MirConstructionInput] {
        &self.inputs
    }
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
pub enum MirAsyncOperation {
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
        /// Checked initialization or assignment behavior.
        kind: MirStoreKind,
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
    /// Construct a tuple or fixed-array value.
    Aggregate(MirAggregate),
    /// Construct a declared or compiler-known value.
    Construct(MirConstruction),
    /// Apply a checked semantic conversion.
    Convert {
        /// Input value.
        operand: MirOperand,
        /// Exact checked conversion plan.
        conversion: SelectedConversion,
    },
    /// Project one nested subject using an exact checked pattern step.
    PatternProjection {
        /// Parent pattern subject.
        subject: MirOperand,
        /// Exact checked structural projection.
        projection: PatternProjection,
        /// Checked ownership operation used to obtain the projected value.
        operation: bray_bound_tree::PatternOperation,
    },
    /// Perform one generator accumulation step.
    Generator(MirGeneratorOperation),
    /// Invoke an exact callable target.
    Call(MirCall),
    /// Create an owned panic report from one checked failure cause.
    PanicReport(MirPanicCause),
    /// Run checked finalization for a storage place.
    Finalize(MirPlace),
    /// Destroy a storage place after its value is no longer live.
    Destroy(MirPlace),
    /// Perform one checked cleanup phase for a storage place.
    Cleanup {
        /// Exact cleanup phase selected by checking.
        phase: MirCleanupPhase,
        /// Storage whose checked cleanup obligation is executed.
        place: MirPlace,
    },
    /// Perform a protected-frame or task operation.
    Async(MirAsyncOperation),
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
