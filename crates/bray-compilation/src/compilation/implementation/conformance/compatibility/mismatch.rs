use bray_symbols::{
    CallableAbi, CallableConstness, CallableExecution, CallableParameterMode, CallablePosition,
    CallableTrust, ReceiverMode, TypeExpressionTemplate,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::compilation::implementation::conformance) enum GenericParameterCategory {
    Type,
    Constant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::compilation::implementation::conformance) enum GenericSurfaceMismatch {
    FulfillmentIsNotGeneric,
    ParameterCount {
        required: usize,
        provided: usize,
    },
    ParameterCategory {
        ordinal: usize,
        required: GenericParameterCategory,
        provided: GenericParameterCategory,
    },
    ConstantParameterType {
        ordinal: usize,
        required: TypeExpressionTemplate,
        provided: TypeExpressionTemplate,
    },
    Constraints(GenericConstraintMismatch),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::compilation::implementation::conformance) enum ConstraintCategory {
    Predicate,
    TraitSatisfaction,
    TypeEquality,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::compilation::implementation::conformance) enum GenericConstraintMismatch {
    Count {
        required: usize,
        provided: usize,
    },
    Ordinal {
        index: usize,
    },
    Category {
        index: usize,
        required: ConstraintCategory,
        provided: ConstraintCategory,
    },
    PredicateDependencies {
        index: usize,
    },
    TraitSatisfaction {
        index: usize,
    },
    TypeEquality {
        index: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::compilation::implementation::conformance) enum CallableContractSurface {
    InvocationPreconditions,
    StaticConstraints,
    CompletionPostconditions,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::compilation::implementation::conformance) enum CallableContractClauseCategory {
    Predicate,
    TraitSatisfaction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::compilation::implementation::conformance) enum CallableBehaviorPhase {
    Invocation,
    DeferredExecution,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::compilation::implementation::conformance) enum CallableBehaviorComponent {
    Effects,
    Capabilities,
    TrustedCapabilities,
    ExecutionRequirements,
    LifecycleObligations,
    Dependencies,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::compilation::implementation::conformance) enum CallableContractMismatch {
    ClauseCount {
        surface: CallableContractSurface,
        required: usize,
        provided: usize,
    },
    ClauseOrdinal {
        surface: CallableContractSurface,
        index: usize,
    },
    ClauseKind {
        surface: CallableContractSurface,
        index: usize,
    },
    ClauseCategory {
        surface: CallableContractSurface,
        index: usize,
        required: CallableContractClauseCategory,
        provided: CallableContractClauseCategory,
    },
    PredicateDependencies {
        surface: CallableContractSurface,
        index: usize,
    },
    TraitSatisfaction {
        surface: CallableContractSurface,
        index: usize,
    },
    Behavior {
        phase: CallableBehaviorPhase,
        component: CallableBehaviorComponent,
    },
    DeferredExecutionPresence,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::compilation::implementation::conformance) enum TraitFulfillmentMismatch {
    MemberCategory,
    Generic(GenericSurfaceMismatch),
    Receiver {
        required: Option<ReceiverMode>,
        provided: Option<ReceiverMode>,
    },
    CallableConstness {
        required: CallableConstness,
        provided: CallableConstness,
    },
    CallableExecution {
        required: CallableExecution,
        provided: CallableExecution,
    },
    CallableTrust {
        required: CallableTrust,
        provided: CallableTrust,
    },
    CallableAbi {
        required: CallableAbi,
        provided: CallableAbi,
    },
    CallableParameterCount {
        required: usize,
        provided: usize,
    },
    CallableParameterName {
        ordinal: usize,
        required: String,
        provided: String,
    },
    CallableParameterPosition {
        ordinal: usize,
        required: CallablePosition,
        provided: CallablePosition,
    },
    CallableParameterMode {
        ordinal: usize,
        required: CallableParameterMode,
        provided: CallableParameterMode,
    },
    CallableParameterType {
        ordinal: usize,
        required: TypeExpressionTemplate,
        provided: TypeExpressionTemplate,
    },
    CallableResultType {
        required: TypeExpressionTemplate,
        provided: TypeExpressionTemplate,
    },
    CallableParameterDefault {
        ordinal: usize,
        required: bool,
        provided: bool,
    },
    CallableContract(CallableContractMismatch),
    ConstantType {
        required: TypeExpressionTemplate,
        provided: TypeExpressionTemplate,
    },
    TypeValueUnavailable,
    PredicateTrust {
        required: bool,
        provided: bool,
    },
    PredicateParameterCount {
        required: usize,
        provided: usize,
    },
    PredicateParameterName {
        ordinal: usize,
        required: String,
        provided: String,
    },
    PredicateParameterType {
        ordinal: usize,
        required: TypeExpressionTemplate,
        provided: TypeExpressionTemplate,
    },
}
