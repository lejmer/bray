use crate::{
    MirAggregate, MirAnonymousCallableReference, MirAsyncOperation, MirCall, MirCallableReference,
    MirCleanupPhase, MirConstruction, MirGeneratorOperation, MirHostOperation, MirMemoryOperation,
    MirNullableQuery, MirOperand, MirPlace, MirTextOperation,
};
use bray_bound_tree::{PatternProjection, SelectedConversion};
use bray_symbols::BorrowKind;

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
    /// Runtime admission rejected an independent task before publication.
    TaskAdmission,
    /// Storage for an inactive protected frame could not be allocated.
    FrameAllocation,
    /// Uniform cleanup storage could not be secured before local owner construction.
    CleanupAdmission,
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

/// One explicit numeric conversion selected by a standard-library operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirNumericConversionKind {
    /// Discards source precision or high-order integer bits as required by the target type.
    Truncate,
}

/// One explicit MIR operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirOperationKind {
    /// Create a capture-free anonymous callable value for an independently lowered unit.
    AnonymousCallable(MirAnonymousCallableReference),
    /// Materialize the address of one declared callable instance.
    DeclaredCallable(MirCallableReference),
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
    /// Construct a tuple, fixed-array, or range value.
    Aggregate(MirAggregate),
    /// Construct a declared or compiler-known value.
    Construct(MirConstruction),
    /// Admit the local nominal owner's cleanup allowance, returning ScalarBool success.
    /// Child allowances and moves do not admit again.
    AdmitCleanup(bray_symbols::TypeId),
    /// Discharge the local nominal owner's allowance after whole-owner destruction.
    /// Child allowances and moves are handled by their existing owners.
    DischargeCleanup(bray_symbols::TypeId),
    /// Apply a checked semantic conversion.
    Convert {
        /// Input value.
        operand: MirOperand,
        /// Exact checked conversion plan.
        conversion: SelectedConversion,
    },
    /// Apply an explicit numeric conversion policy.
    NumericConversion {
        /// Selected conversion policy.
        kind: MirNumericConversionKind,
        /// Input value.
        operand: MirOperand,
    },
    /// Test one nullable value's state without exposing its payload.
    NullableQuery(MirNullableQuery),
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
    /// Retains guarded implicit receiver cleanup until its destructor invocation is specialized.
    DestructorRemainder {
        /// Ordinary destruction or lifecycle resolution required for this represented part.
        role: crate::MirGeneratedLifecycleRole,
        /// Initialized represented part reached through the checked receiver decomposition.
        place: MirPlace,
    },
    /// Resolves ownership of an abandoned cleanup error without graceful finalization.
    Abandon {
        /// Exact ownership step selected by lowering.
        action: crate::MirAbandonmentAction,
        /// Initialized storage retained or consumed by the selected step.
        place: MirPlace,
    },
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

impl MirOperationKind {
    /// Returns the role and receiver of a direct semantic lifecycle operation.
    /// An unresolved destructor remainder keeps its separate specialization contract.
    pub const fn lifecycle_action(&self) -> Option<(crate::MirGeneratedLifecycleRole, &MirPlace)> {
        match self {
            Self::Finalize(place) => Some((crate::MirGeneratedLifecycleRole::Finalize, place)),
            Self::Destroy(place) => Some((crate::MirGeneratedLifecycleRole::Destroy, place)),
            Self::Abandon { action, place } => {
                Some((crate::MirGeneratedLifecycleRole::Abandon(*action), place))
            }
            Self::Cleanup { phase, place } => {
                Some((crate::MirGeneratedLifecycleRole::Cleanup(*phase), place))
            }
            _ => None,
        }
    }
}
