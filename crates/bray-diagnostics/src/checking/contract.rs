use crate::{DiagnosticConstantOperation, DiagnosticTypeArgument};

/// The boundary that owns a condition which could not be proved.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticContractObligationKind {
    /// A selected call's entry requirement.
    CallRequirement,
    /// An implicitly selected lifecycle entry requirement.
    CleanupRequirement,
    /// A promise at normal completion.
    Postcondition,
    /// A compiler-only execution-context requirement.
    ExecutionRequirement,
}

/// A failed condition with its declaration meaning and actual supplied values.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticContractObligation {
    /// The boundary responsible for this proof.
    pub kind: DiagnosticContractObligationKind,
    /// The original declaration condition.
    pub required: DiagnosticCondition,
    /// The condition after binding the actual arguments.
    pub actual: DiagnosticCondition,
}

/// Source-level condition data, independent of compiler symbols and proof internals.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCondition {
    /// A source name or exact source expression.
    Source(String),
    /// A scalar literal in source notation.
    Literal(String),
    /// A Boolean value.
    Boolean(bool),
    /// The normal completion value.
    Result,
    /// A positional input whose source spelling is unavailable.
    Argument(u32),
    /// A predicate or deterministic function application.
    Application {
        /// Qualified declaration path.
        path: Vec<String>,
        /// Selected generic arguments.
        types: Vec<DiagnosticTypeArgument>,
        /// Supplied argument values.
        arguments: Vec<DiagnosticCondition>,
    },
    /// A built-in operation on other conditions.
    Operation {
        /// The selected operation.
        operation: DiagnosticConstantOperation,
        /// Its ordered operands.
        operands: Vec<DiagnosticCondition>,
    },
    /// One component of a supplied value.
    Projection {
        /// The containing value.
        value: Box<DiagnosticCondition>,
        /// The selected component.
        projection: DiagnosticConditionProjection,
    },
    /// A present nullable value.
    Present(Box<DiagnosticCondition>),
    /// A value for which the proof has no known meaning.
    Unknown,
    /// The absent nullable value.
    Absent,
    /// A string scalar without source escaping applied.
    String(String),
    /// A character scalar without source escaping applied.
    Character(char),
    /// An input retained by a deferred producer.
    CapturedInput {
        /// The future or task retaining the input.
        owner: Box<DiagnosticCondition>,
        /// Receiver-first input ordinal.
        ordinal: u32,
    },
    /// A test of an active union variant.
    VariantTest {
        /// The supplied union value.
        value: Box<DiagnosticCondition>,
        /// The selected variant name.
        variant: String,
    },
    /// A pointer to statically owned storage.
    StaticAddress(Vec<String>),
    /// A subtree omitted by the diagnostic display work bound.
    Elided,
}

/// A component selected by a diagnostic condition.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticConditionProjection {
    /// A declared field.
    Field(String),
    /// A tuple field selected by ordinal.
    TupleElement(u32),
    /// An array element selected by ordinal.
    ArrayElement(u32),
    /// A dynamic array element.
    Index(Box<DiagnosticCondition>),
    /// A half-open array slice.
    Slice {
        /// Optional inclusive lower bound.
        lower: Option<Box<DiagnosticCondition>>,
        /// Optional exclusive upper bound.
        upper: Option<Box<DiagnosticCondition>>,
    },
    /// The present nullable payload.
    NullableValue,
}

/// The specific boundary that prevented contract certification.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticExecutionProofFailure {
    /// A checked operation does not establish the requested promise.
    Unproven,
    /// A required callable has source-checking errors.
    DependencyErrors,
    /// A dependency has no checked evidence for the requested contract.
    MissingEvidence,
    /// The provided callable contract does not imply a required promise.
    ContractMismatch,
    /// Completion would assume its own unverified promise.
    Circular,
    /// The traversal stopped at its configured proof depth.
    DepthLimit {
        /// Observed dependency depth.
        depth: u64,
        /// Maximum permitted dependency depth.
        limit: u64,
    },
}
