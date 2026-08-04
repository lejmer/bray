use std::sync::Arc;

use bray_base::shared_slice;
use bray_bound_tree::{
    BoundCallResult, BoundUnitKey, CheckedMemoryOperationKind, ConstructionDefaultProvider,
    ConstructionInputId, ConstructionTarget, PatternProjection, SelectedConversion,
};
use bray_runtime_interface::{ProtectedAsyncFrameId, RootExecution};
use bray_symbols::{BorrowKind, ConstantTermId, TypeId};

use crate::{
    MirCall, MirCleanupPhase, MirFrameReference, MirFrameStateId, MirOperand, MirOperationId,
    MirPlace, MirRuntimeReference, MirSourceAnchor, MirStorageId, MirValueId,
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
    /// An explicit test failure with its evaluated message.
    ExplicitTestFailure(MirOperand),
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
    /// The present state of a nullable value.
    NullablePresent,
}

/// The normalized result accumulated by one generator expression.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirGeneratorKind {
    /// A fixed array whose checked count determines its final extent.
    Array,
    /// A lazy generator value.
    General,
}

/// Compiler-provided UTF-8 text behavior selected for one call.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirTextOperationKind {
    /// Count Unicode scalar values.
    ScalarCount,
    /// Test whether text is empty.
    IsEmpty,
    /// Compare text values for equality.
    Equals,
    /// Select one Unicode scalar by scalar index.
    ScalarAt,
    /// Copy one half-open scalar range into owned text.
    ScalarSlice,
    /// Borrow the underlying valid UTF-8 bytes.
    Utf8,
    /// Validate and copy borrowed UTF-8 bytes into owned text.
    FromUtf8,
    /// Return a character's Unicode scalar value.
    CharacterScalarValue,
    /// Construct a character from a valid Unicode scalar value.
    CharacterFromScalarValue,
    /// Return a character's UTF-8 encoded length.
    CharacterUtf8Length,
    /// Return one byte from a character's UTF-8 encoding.
    CharacterUtf8Byte,
    /// Test whether a character is alphabetic.
    CharacterIsAlphabetic,
    /// Test whether a character is numeric.
    CharacterIsNumeric,
    /// Test whether a character is whitespace.
    CharacterIsWhitespace,
    /// Release one owned text storage reference.
    Release,
}

/// One explicit UTF-8 text operation with evaluated operands.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirTextOperation {
    kind: MirTextOperationKind,
    operands: Arc<[MirOperand]>,
    operand_types: Arc<[TypeId]>,
    result_type: Option<TypeId>,
}

impl MirTextOperation {
    /// Creates one text operation in evaluation order.
    pub fn new(
        kind: MirTextOperationKind,
        operands: impl IntoIterator<Item = MirOperand>,
        operand_types: impl IntoIterator<Item = TypeId>,
        result_type: Option<TypeId>,
    ) -> Self {
        Self {
            kind,
            operands: shared_slice(operands),
            operand_types: shared_slice(operand_types),
            result_type,
        }
    }

    /// Returns the selected text behavior.
    pub const fn kind(&self) -> MirTextOperationKind {
        self.kind
    }

    /// Returns evaluated operands in declaration order.
    pub fn operands(&self) -> &[MirOperand] {
        &self.operands
    }

    /// Returns selected operand types in declaration order.
    pub fn operand_types(&self) -> &[TypeId] {
        &self.operand_types
    }

    /// Returns the exact operation result type.
    pub const fn result_type(&self) -> Option<TypeId> {
        self.result_type
    }
}

/// One explicit compiler-provided memory operation with evaluated operands.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirMemoryOperation {
    kind: CheckedMemoryOperationKind,
    operands: Arc<[MirOperand]>,
    operand_types: Arc<[TypeId]>,
    result_type: Option<TypeId>,
}

impl MirMemoryOperation {
    /// Creates one memory operation in evaluation order.
    pub fn new(
        kind: CheckedMemoryOperationKind,
        operands: impl IntoIterator<Item = MirOperand>,
        operand_types: impl IntoIterator<Item = TypeId>,
        result_type: Option<TypeId>,
    ) -> Self {
        Self {
            kind,
            operands: shared_slice(operands),
            operand_types: shared_slice(operand_types),
            result_type,
        }
    }

    /// Returns the checked memory behavior.
    pub const fn kind(&self) -> CheckedMemoryOperationKind {
        self.kind
    }

    /// Returns evaluated operands in declaration order.
    pub fn operands(&self) -> &[MirOperand] {
        &self.operands
    }

    /// Returns the selected parameter types in declaration order.
    pub fn operand_types(&self) -> &[TypeId] {
        &self.operand_types
    }

    /// Returns the selected result type when the operation produces a value.
    pub const fn result_type(&self) -> Option<TypeId> {
        self.result_type
    }
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
        /// Checked yielded-element type whose layout governs accumulation.
        element: TypeId,
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
    /// Broadcast task cleanup through initialized accumulated elements.
    CleanupBroadcast {
        /// Generator value storage retaining the accumulation.
        destination: MirPlace,
        /// Checked yielded-element type whose cleanup helper is invoked.
        element: TypeId,
        /// Runtime operation that visits initialized elements without releasing storage.
        runtime: MirRuntimeReference,
    },
    /// Destroy initialized accumulated elements and release their storage.
    Destroy {
        /// Generator value storage retaining the accumulation.
        destination: MirPlace,
        /// Checked yielded-element type whose lifecycle helpers are invoked.
        element: TypeId,
        /// Runtime operation that visits elements in reverse and releases storage.
        runtime: MirRuntimeReference,
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

/// Checked work captured by one newly constructed inactive future.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirFrameInitializer {
    /// Invoke an async callable when the frame is first driven.
    Callable(MirCall),
    /// Observe a task terminal result, optionally requesting cancellation first.
    TaskObservation {
        /// Owned task whose terminal result is observed.
        task: MirOperand,
        /// Checked lazy future produced by the task operation.
        result: bray_bound_tree::BoundFutureConstruction,
        /// Whether driving the frame first requests task cancellation.
        request_cancellation: bool,
    },
}

impl MirFrameInitializer {
    /// Returns the source-visible future type produced by this initializer.
    pub const fn future_type(&self) -> Option<TypeId> {
        match self {
            Self::Callable(call) => match call.result() {
                BoundCallResult::LazyFuture(result) => Some(result.future_type()),
                BoundCallResult::Immediate(_) => None,
            },
            Self::TaskObservation { result, .. } => Some(result.future_type()),
        }
    }
}

/// Explicit protected-frame and task operation selected by checked lowering.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirAsyncOperation {
    /// Create an inactive protected frame value.
    CreateFrame {
        /// Static or existential frame representation.
        frame: MirFrameReference,
        /// Deferred work captured by the frame.
        initializer: MirFrameInitializer,
    },
    /// Move an inactive frame before its first resume.
    MoveInactiveFrame {
        /// Static or existential frame representation.
        frame: MirFrameReference,
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
        /// Static or existential child frame representation.
        child: MirFrameReference,
        /// Inactive child frame value.
        frame: MirOperand,
    },
    /// Move a directly awaited child's completion into the operation result.
    CommitAwaitedCompletion {
        /// Static or existential child frame representation.
        child: MirFrameReference,
    },
    /// Transfer an inactive frame into a newly started task.
    StartTask {
        /// Static or existential frame representation.
        frame: MirFrameReference,
        /// Inactive frame value.
        value: MirOperand,
        /// Selected private task-allocation ABI role.
        allocation: MirRuntimeReference,
        /// Selected private task-start ABI role.
        start: MirRuntimeReference,
    },
    /// Request cancellation of an owned task.
    RequestTaskCancellation {
        /// Owned task control state.
        task: MirOperand,
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
        /// Owned task control state.
        task: MirOperand,
        /// Selected private join ABI role.
        runtime: MirRuntimeReference,
    },
    /// Publish exactly one task terminal state.
    PublishTerminalState {
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
        task: MirOperand,
    },
}

/// Explicit compiler-generated product-host operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirHostOperation {
    /// Establish and execute the selected source root.
    ExecuteRoot {
        /// Exact source unit selected as the product root.
        root: BoundUnitKey,
        /// Synchronous or protected-frame root execution.
        execution: RootExecution,
        /// Selected private root-execution ABI role.
        runtime: MirRuntimeReference,
    },
    /// Observe the root terminal record.
    ObserveRootTerminal {
        /// Selected private terminal-observation ABI role.
        runtime: MirRuntimeReference,
    },
    /// Map and release the observed terminal root payload.
    ResolveRootTerminal {
        /// Recoverable error type whose lifecycle the host resolves after reporting.
        error: Option<TypeId>,
        /// Selected private completion-release ABI role.
        completion: MirRuntimeReference,
        /// Selected private panic-reporting ABI role.
        panic: MirRuntimeReference,
        /// Selected private recoverable-entry-failure reporting ABI role.
        entry_failure: MirRuntimeReference,
    },
    /// Report and destroy cleanup incidents transferred to the host.
    ReportCleanupIncidents {
        /// Selected private cleanup-reporting ABI role.
        runtime: MirRuntimeReference,
    },
    /// Shut product execution infrastructure down in checked order.
    StructuredShutdown {
        /// Selected private shutdown ABI role.
        runtime: MirRuntimeReference,
    },
}

/// One explicit MIR operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirOperationKind {
    /// Create a capture-free anonymous callable value for an independently lowered unit.
    AnonymousCallable(BoundUnitKey),
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
    /// Perform one checked compiler-provided memory operation.
    Memory(MirMemoryOperation),
    /// Perform one compiler-provided UTF-8 text operation.
    Text(MirTextOperation),
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
    /// Perform a compiler-generated executable-host operation.
    Host(MirHostOperation),
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
