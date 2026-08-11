use crate::DiagnosticType;

/// Generic parameter category participating in a trait fulfillment mismatch.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticGenericParameterCategory {
    /// A parameter ranging over types.
    Type,
    /// A parameter ranging over compile-time values.
    Constant,
}

/// Receiver ownership mode participating in a trait fulfillment mismatch.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticReceiverMode {
    /// Shared observation without mutation or consumption.
    Shared,
    /// Exclusive mutation without consumption.
    Mutable,
    /// Consumption without mutable local authority.
    Consuming,
    /// Consumption with mutable local authority.
    ConsumingMutable,
}

/// Constant-evaluation eligibility of a callable trait member.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCallableConstness {
    /// Ordinary runtime-only evaluation.
    Runtime,
    /// Compile-time evaluation is supported.
    Constant,
}

/// Execution mode of a callable trait member.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCallableExecution {
    /// Immediate synchronous execution.
    Synchronous,
    /// Suspendable asynchronous execution.
    Asynchronous,
}

/// Trust boundary of a callable trait member.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCallableTrust {
    /// An ordinary safe callable.
    Safe,
    /// A callable with explicit trusted obligations.
    Trusted,
}

/// Caller-visible position policy of a callable parameter.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCallablePosition {
    /// The argument must be supplied by name.
    NamedOnly,
    /// The argument may be supplied by position or name.
    PositionalOrNamed,
}

/// Local mutation authority of an owned callable parameter.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCallableParameterMode {
    /// The parameter binding is immutable.
    Immutable,
    /// The parameter binding has mutable local authority.
    Mutable,
}

/// Callable contract surface that differs from a trait requirement.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCallableContractSurface {
    /// Conditions required before invocation.
    InvocationPreconditions,
    /// Constraints checked during compilation.
    StaticConstraints,
    /// Conditions required after normal completion.
    CompletionPostconditions,
}

/// Semantic category of one generic constraint.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticConstraintCategory {
    /// A constant predicate that must evaluate to true.
    Predicate,
    /// A type that must satisfy an applied trait.
    TraitSatisfaction,
    /// Two type expressions that must resolve to the same type.
    TypeEquality,
}

/// Exact difference between required and provided generic constraints.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticGenericConstraintMismatch {
    /// Constraint counts differ.
    Count {
        /// Number required by the trait member.
        required: u64,
        /// Number declared by the fulfillment.
        provided: u64,
    },
    /// Constraint ordering differs at this zero-based index.
    Ordinal(u64),
    /// Constraint categories differ at this zero-based index.
    Category {
        /// Index of the mismatching constraint.
        index: u64,
        /// Category required by the trait member.
        required: DiagnosticConstraintCategory,
        /// Category declared by the fulfillment.
        provided: DiagnosticConstraintCategory,
    },
    /// Predicate dependencies differ at this zero-based index.
    PredicateDependencies(u64),
    /// Trait subject or application differs at this zero-based index.
    TraitSatisfaction(u64),
    /// One side of a type equality differs at this zero-based index.
    TypeEquality(u64),
}

/// Value category of one callable contract clause.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCallableContractClauseCategory {
    /// A predicate expression.
    Predicate,
    /// A type and applied-trait satisfaction requirement.
    TraitSatisfaction,
}

/// Callable phase whose behavior differs from a trait requirement.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCallableBehaviorPhase {
    /// Behavior while the callable is invoked.
    Invocation,
    /// Behavior retained by deferred asynchronous execution.
    DeferredExecution,
}

/// Exact callable behavior component that differs from a trait requirement.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCallableBehaviorComponent {
    /// Declared effects.
    Effects,
    /// Required capabilities.
    Capabilities,
    /// Required trusted capabilities.
    TrustedCapabilities,
    /// Execution requirements.
    ExecutionRequirements,
    /// Lifecycle obligations.
    LifecycleObligations,
    /// Portable dependencies.
    Dependencies,
}

/// Exact difference between required and provided callable contracts.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticCallableContractMismatch {
    /// Clause counts differ on one contract surface.
    ClauseCount {
        /// Contract surface containing the clauses.
        surface: DiagnosticCallableContractSurface,
        /// Number required by the trait member.
        required: u64,
        /// Number declared by the fulfillment.
        provided: u64,
    },
    /// Clause ordering differs at one zero-based index.
    ClauseOrdinal {
        /// Contract surface containing the clause.
        surface: DiagnosticCallableContractSurface,
        /// Index of the mismatching clause.
        index: u64,
    },
    /// Clause directive kinds differ at one zero-based index.
    ClauseKind {
        /// Contract surface containing the clause.
        surface: DiagnosticCallableContractSurface,
        /// Index of the mismatching clause.
        index: u64,
    },
    /// Clause value categories differ at one zero-based index.
    ClauseCategory {
        /// Contract surface containing the clause.
        surface: DiagnosticCallableContractSurface,
        /// Index of the mismatching clause.
        index: u64,
        /// Category required by the trait member.
        required: DiagnosticCallableContractClauseCategory,
        /// Category declared by the fulfillment.
        provided: DiagnosticCallableContractClauseCategory,
    },
    /// Predicate dependencies differ at one zero-based clause index.
    PredicateDependencies {
        /// Contract surface containing the clause.
        surface: DiagnosticCallableContractSurface,
        /// Index of the mismatching clause.
        index: u64,
    },
    /// Trait subject or application differs at one zero-based clause index.
    TraitSatisfaction {
        /// Contract surface containing the clause.
        surface: DiagnosticCallableContractSurface,
        /// Index of the mismatching clause.
        index: u64,
    },
    /// One behavior component differs during one callable phase.
    Behavior {
        /// Callable phase containing the mismatch.
        phase: DiagnosticCallableBehaviorPhase,
        /// Exact behavior component that differs.
        component: DiagnosticCallableBehaviorComponent,
    },
    /// One callable has deferred execution behavior while the other does not.
    DeferredExecutionPresence,
}

/// Exact semantic difference between a trait requirement and its supplied fulfillment.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticTraitFulfillmentMismatch {
    /// Requirement and fulfillment use different member categories.
    MemberCategory,
    /// The requirement is generic but the fulfillment is not.
    FulfillmentIsNotGeneric,
    /// Generic parameter counts differ.
    GenericParameterCount {
        /// Number required by the trait member.
        required: u64,
        /// Number declared by the fulfillment.
        provided: u64,
    },
    /// One generic parameter has the wrong category.
    GenericParameterCategory {
        /// Zero-based generic parameter ordinal.
        ordinal: u64,
        /// Category required by the trait member.
        required: DiagnosticGenericParameterCategory,
        /// Category declared by the fulfillment.
        provided: DiagnosticGenericParameterCategory,
    },
    /// One generic constant parameter has the wrong declared type.
    GenericConstantParameterType {
        /// Zero-based generic parameter ordinal.
        ordinal: u64,
        /// Type required by the trait member.
        required: DiagnosticType,
        /// Type declared by the fulfillment.
        provided: DiagnosticType,
    },
    /// Generic constraints differ after trait arguments are substituted.
    GenericConstraints(DiagnosticGenericConstraintMismatch),
    /// Receiver presence or ownership mode differs.
    Receiver {
        /// Receiver mode required by the trait member, or absence of a receiver.
        required: Option<DiagnosticReceiverMode>,
        /// Receiver mode declared by the fulfillment, or absence of a receiver.
        provided: Option<DiagnosticReceiverMode>,
    },
    /// Callable constant-evaluation eligibility differs.
    CallableConstness {
        /// Eligibility required by the trait member.
        required: DiagnosticCallableConstness,
        /// Eligibility declared by the fulfillment.
        provided: DiagnosticCallableConstness,
    },
    /// Callable execution mode differs.
    CallableExecution {
        /// Execution mode required by the trait member.
        required: DiagnosticCallableExecution,
        /// Execution mode declared by the fulfillment.
        provided: DiagnosticCallableExecution,
    },
    /// Callable trust boundary differs.
    CallableTrust {
        /// Trust boundary required by the trait member.
        required: DiagnosticCallableTrust,
        /// Trust boundary declared by the fulfillment.
        provided: DiagnosticCallableTrust,
    },
    /// Callable ABI differs.
    CallableAbi {
        /// ABI required by the trait member.
        required: crate::DiagnosticCallableAbi,
        /// ABI declared by the fulfillment.
        provided: crate::DiagnosticCallableAbi,
    },
    /// Callable parameter counts differ.
    CallableParameterCount {
        /// Number required by the trait member.
        required: u64,
        /// Number declared by the fulfillment.
        provided: u64,
    },
    /// One callable parameter has the wrong name.
    CallableParameterName {
        /// Zero-based callable parameter ordinal.
        ordinal: u64,
        /// Name required by the trait member.
        required: String,
        /// Name declared by the fulfillment.
        provided: String,
    },
    /// One callable parameter has the wrong position policy.
    CallableParameterPosition {
        /// Zero-based callable parameter ordinal.
        ordinal: u64,
        /// Position policy required by the trait member.
        required: DiagnosticCallablePosition,
        /// Position policy declared by the fulfillment.
        provided: DiagnosticCallablePosition,
    },
    /// One callable parameter has the wrong mutation mode.
    CallableParameterMode {
        /// Zero-based callable parameter ordinal.
        ordinal: u64,
        /// Mutation mode required by the trait member.
        required: DiagnosticCallableParameterMode,
        /// Mutation mode declared by the fulfillment.
        provided: DiagnosticCallableParameterMode,
    },
    /// One callable parameter has the wrong type.
    CallableParameterType {
        /// Zero-based callable parameter ordinal.
        ordinal: u64,
        /// Type required by the trait member.
        required: DiagnosticType,
        /// Type declared by the fulfillment.
        provided: DiagnosticType,
    },
    /// Callable result type differs.
    CallableResultType {
        /// Result type required by the trait member.
        required: DiagnosticType,
        /// Result type declared by the fulfillment.
        provided: DiagnosticType,
    },
    /// One callable parameter differs in runtime-default availability.
    CallableParameterDefault {
        /// Zero-based callable parameter ordinal.
        ordinal: u64,
        /// Whether the trait member declares a default.
        required: bool,
        /// Whether the fulfillment declares a default.
        provided: bool,
    },
    /// One callable contract leaf differs.
    CallableContract(DiagnosticCallableContractMismatch),
    /// A constant fulfillment has the wrong declared type.
    ConstantType {
        /// Type required by the trait member.
        required: DiagnosticType,
        /// Type declared by the fulfillment.
        provided: DiagnosticType,
    },
    /// A type fulfillment could not provide a resolved type value.
    TypeValueUnavailable,
    /// Predicate trust differs.
    PredicateTrust {
        /// Whether the trait predicate is trusted.
        required: bool,
        /// Whether the fulfillment predicate is trusted.
        provided: bool,
    },
    /// Predicate parameter counts differ.
    PredicateParameterCount {
        /// Number required by the trait predicate.
        required: u64,
        /// Number declared by the fulfillment.
        provided: u64,
    },
    /// One predicate parameter has the wrong name.
    PredicateParameterName {
        /// Zero-based predicate parameter ordinal.
        ordinal: u64,
        /// Name required by the trait predicate.
        required: String,
        /// Name declared by the fulfillment.
        provided: String,
    },
    /// One predicate parameter has the wrong type.
    PredicateParameterType {
        /// Zero-based predicate parameter ordinal.
        ordinal: u64,
        /// Type required by the trait predicate.
        required: DiagnosticType,
        /// Type declared by the fulfillment.
        provided: DiagnosticType,
    },
}
