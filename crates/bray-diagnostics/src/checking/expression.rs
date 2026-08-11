/// Source-level expression category retained by checking diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticExpressionCategory {
    /// A source block used as an expression.
    Block,
    /// A literal value.
    Literal,
    /// A resolved or unresolved value-name reference.
    NameReference,
    /// A pattern-local value reference.
    PatternReference,
    /// A unary operator application.
    UnaryOperation,
    /// A binary operator application.
    BinaryOperation,
    /// An assignment.
    Assignment,
    /// A callable invocation.
    Call,
    /// An explicit conversion.
    Conversion,
    /// An anonymous callable.
    AnonymousCallable,
    /// An await expression.
    Await,
    /// A tuple or array aggregate.
    Aggregate,
    /// An element or slice indexing operation.
    Indexing,
    /// Nullable or result propagation.
    Propagation,
    /// Conditional, loop, assertion, or effect control flow.
    ControlFlow,
    /// A fixed-array or lazy-sequence generator.
    Generator,
    /// A value construction expression.
    Construction,
    /// A receiver member access.
    MemberAccess,
    /// A trait-qualified receiver member access.
    TraitMemberAccess,
    /// An expression retained after an earlier semantic error.
    Recovered,
}

impl DiagnosticExpressionCategory {
    /// Returns the stable machine key for the expression category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Block => "block",
            Self::Literal => "literal",
            Self::NameReference => "name_reference",
            Self::PatternReference => "pattern_reference",
            Self::UnaryOperation => "unary_operation",
            Self::BinaryOperation => "binary_operation",
            Self::Assignment => "assignment",
            Self::Call => "call",
            Self::Conversion => "conversion",
            Self::AnonymousCallable => "anonymous_callable",
            Self::Await => "await",
            Self::Aggregate => "aggregate",
            Self::Indexing => "indexing",
            Self::Propagation => "propagation",
            Self::ControlFlow => "control_flow",
            Self::Generator => "generator",
            Self::Construction => "construction",
            Self::MemberAccess => "member_access",
            Self::TraitMemberAccess => "trait_member_access",
            Self::Recovered => "recovered",
        }
    }
}

/// Compile-time operation retained by constant-evaluation diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticConstantOperation {
    /// Unary or binary addition.
    Add,
    /// Unary negation or binary subtraction.
    Subtract,
    /// Multiplication.
    Multiply,
    /// Division.
    Divide,
    /// Remainder.
    Remainder,
    /// Exponentiation.
    Exponentiate,
    /// Logical conjunction.
    LogicalAnd,
    /// Logical disjunction.
    LogicalOr,
    /// Logical negation.
    LogicalNot,
    /// Bitwise conjunction.
    BitwiseAnd,
    /// Bitwise disjunction.
    BitwiseOr,
    /// Bitwise exclusive disjunction.
    BitwiseXor,
    /// Bitwise negation.
    BitwiseNot,
    /// Left shift.
    ShiftLeft,
    /// Right shift.
    ShiftRight,
    /// Equality comparison.
    Equal,
    /// Inequality comparison.
    NotEqual,
    /// Less-than comparison.
    Less,
    /// Less-than-or-equal comparison.
    LessEqual,
    /// Greater-than comparison.
    Greater,
    /// Greater-than-or-equal comparison.
    GreaterEqual,
    /// Matrix multiplication.
    MatrixMultiply,
    /// Explicit scalar or composite conversion.
    Conversion,
}

impl DiagnosticConstantOperation {
    /// Returns the stable machine key for the compile-time operation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Subtract => "subtract",
            Self::Multiply => "multiply",
            Self::Divide => "divide",
            Self::Remainder => "remainder",
            Self::Exponentiate => "exponentiate",
            Self::LogicalAnd => "logical_and",
            Self::LogicalOr => "logical_or",
            Self::LogicalNot => "logical_not",
            Self::BitwiseAnd => "bitwise_and",
            Self::BitwiseOr => "bitwise_or",
            Self::BitwiseXor => "bitwise_xor",
            Self::BitwiseNot => "bitwise_not",
            Self::ShiftLeft => "shift_left",
            Self::ShiftRight => "shift_right",
            Self::Equal => "equal",
            Self::NotEqual => "not_equal",
            Self::Less => "less",
            Self::LessEqual => "less_equal",
            Self::Greater => "greater",
            Self::GreaterEqual => "greater_equal",
            Self::MatrixMultiply => "matrix_multiply",
            Self::Conversion => "conversion",
        }
    }
}

/// Exact reason a propagation operator has no compatible enclosing boundary.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticPropagationProblem {
    /// Nullable propagation has no enclosing nullable result boundary.
    NullableBoundaryUnavailable {
        /// Type of the propagated nullable operand.
        operand: crate::DiagnosticType,
        /// Result types of the enclosing yield and callable boundaries considered.
        available_boundaries: Box<[crate::DiagnosticType]>,
    },
    /// Result propagation has no boundary accepting the propagated error type.
    ResultBoundaryUnavailable {
        /// Error type carried by the propagated result.
        source_error: crate::DiagnosticType,
        /// Error types accepted by enclosing result boundaries.
        available_errors: Box<[crate::DiagnosticType]>,
    },
}

impl DiagnosticPropagationProblem {
    /// Returns the stable problem category key.
    pub const fn category(&self) -> &'static str {
        match self {
            Self::NullableBoundaryUnavailable { .. } => "nullable_boundary_unavailable",
            Self::ResultBoundaryUnavailable { .. } => "result_boundary_unavailable",
        }
    }
}

/// Stable representation of a source-level fixed-array length.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticArrayLength {
    /// A closed nonnegative length.
    Exact(u64),
    /// A checked length that still depends on generic or target context.
    Symbolic,
}

/// Set of element counts possible on one continuing generator path.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticYieldCardinality {
    /// A path yields no element.
    Zero,
    /// A path yields multiple elements.
    Multiple,
    /// Paths may yield zero or one element.
    ZeroOrOne,
    /// Paths may yield one or multiple elements.
    OneOrMultiple,
    /// Paths may yield zero or multiple elements.
    ZeroOrMultiple,
    /// Paths may yield zero, one, or multiple elements.
    ZeroOneOrMultiple,
    /// A break can end an iteration before its required yield.
    Break,
    /// Nested control flow prevents a finite cardinality proof.
    Unknown,
}

impl DiagnosticYieldCardinality {
    /// Returns the stable machine key for the possible yield counts.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Zero => "zero",
            Self::Multiple => "multiple",
            Self::ZeroOrOne => "zero_or_one",
            Self::OneOrMultiple => "one_or_multiple",
            Self::ZeroOrMultiple => "zero_or_multiple",
            Self::ZeroOneOrMultiple => "zero_one_or_multiple",
            Self::Break => "break",
            Self::Unknown => "unknown",
        }
    }
}

/// Exact reason a fixed-array generator's result length cannot be proven.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticArrayGeneratorCardinalityProblem {
    /// The selected iteration protocol does not expose an exact source count.
    SourceCountUnavailable {
        /// Checked iteration-source type.
        source: crate::DiagnosticType,
        /// Checked yielded element type.
        element: crate::DiagnosticType,
        /// Required result-array length.
        required: DiagnosticArrayLength,
    },
    /// The exact iteration count differs from the result-array length.
    SourceLengthMismatch {
        /// Checked iteration-source type.
        source: crate::DiagnosticType,
        /// Checked yielded element type.
        element: crate::DiagnosticType,
        /// Exact selected iteration count.
        source_length: DiagnosticArrayLength,
        /// Required result-array length.
        required: DiagnosticArrayLength,
    },
    /// Generator control flow does not yield exactly once per source element.
    YieldCountNotExact {
        /// Checked yielded element type.
        element: crate::DiagnosticType,
        /// Selected iteration count when the protocol makes it statically known.
        source_length: DiagnosticArrayLength,
        /// Required result-array length.
        required: DiagnosticArrayLength,
        /// Counts possible on continuing paths.
        actual: DiagnosticYieldCardinality,
    },
}

impl DiagnosticArrayGeneratorCardinalityProblem {
    /// Returns the stable problem category key.
    pub const fn category(&self) -> &'static str {
        match self {
            Self::SourceCountUnavailable { .. } => "source_count_unavailable",
            Self::SourceLengthMismatch { .. } => "source_length_mismatch",
            Self::YieldCountNotExact { .. } => "yield_count_not_exact",
        }
    }
}
