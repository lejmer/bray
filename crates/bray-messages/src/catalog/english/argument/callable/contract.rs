use super::super::selection::format_english_callable_abi;
use super::super::source::{format_english_quoted_text, format_english_type};
use bray_diagnostics::{
    DiagnosticCallableBehaviorComponent, DiagnosticCallableBehaviorPhase,
    DiagnosticCallableConstness, DiagnosticCallableContractClauseCategory,
    DiagnosticCallableContractMismatch, DiagnosticCallableContractSurface,
    DiagnosticCallableExecution, DiagnosticCallableParameterMode, DiagnosticCallablePosition,
    DiagnosticCallableTrust, DiagnosticConstraintCategory, DiagnosticGenericConstraintMismatch,
    DiagnosticGenericParameterCategory, DiagnosticReceiverMode, DiagnosticTraitFulfillmentMismatch,
};

pub(crate) fn format_english_trait_fulfillment_mismatch(
    mismatch: &DiagnosticTraitFulfillmentMismatch,
) -> String {
    let ordinal = |ordinal: u64| ordinal.saturating_add(1);

    match mismatch {
        DiagnosticTraitFulfillmentMismatch::MemberCategory => {
            "the declaration has a different member category".to_owned()
        }
        DiagnosticTraitFulfillmentMismatch::FulfillmentIsNotGeneric => {
            "the trait member is generic but the fulfillment is not".to_owned()
        }
        DiagnosticTraitFulfillmentMismatch::GenericParameterCount { required, provided } => {
            format!(
                "the trait requires {required} generic parameters but the fulfillment declares {provided}"
            )
        }
        DiagnosticTraitFulfillmentMismatch::GenericParameterCategory {
            ordinal: index,
            required,
            provided,
        } => format!(
            "generic parameter {} is {}, but the trait requires {}",
            ordinal(*index),
            format_english_generic_parameter_category(*provided),
            format_english_generic_parameter_category(*required),
        ),
        DiagnosticTraitFulfillmentMismatch::GenericConstantParameterType {
            ordinal: index,
            required,
            provided,
        } => format!(
            "generic constant parameter {} has type {}, but the trait requires {}",
            ordinal(*index),
            format_english_type(provided),
            format_english_type(required),
        ),
        DiagnosticTraitFulfillmentMismatch::GenericConstraints(mismatch) => {
            format_english_generic_constraint_mismatch(mismatch)
        }
        DiagnosticTraitFulfillmentMismatch::Receiver { required, provided } => format!(
            "the receiver is {}, but the trait requires {}",
            format_english_receiver(*provided),
            format_english_receiver(*required),
        ),
        DiagnosticTraitFulfillmentMismatch::CallableConstness { required, provided } => format!(
            "the callable is {}, but the trait requires {}",
            format_english_callable_constness(*provided),
            format_english_callable_constness(*required),
        ),
        DiagnosticTraitFulfillmentMismatch::CallableExecution { required, provided } => format!(
            "the callable is {}, but the trait requires {}",
            format_english_callable_execution(*provided),
            format_english_callable_execution(*required),
        ),
        DiagnosticTraitFulfillmentMismatch::CallableTrust { required, provided } => format!(
            "the callable is {}, but the trait requires {}",
            format_english_callable_trust(*provided),
            format_english_callable_trust(*required),
        ),
        DiagnosticTraitFulfillmentMismatch::CallableAbi { required, provided } => format!(
            "the callable uses the {} ABI, but the trait requires the {} ABI",
            format_english_callable_abi(*provided),
            format_english_callable_abi(*required),
        ),
        DiagnosticTraitFulfillmentMismatch::CallableParameterCount { required, provided } => {
            format!(
                "the trait requires {required} callable parameters but the fulfillment declares {provided}"
            )
        }
        DiagnosticTraitFulfillmentMismatch::CallableParameterName {
            ordinal: index,
            required,
            provided,
        } => format!(
            "callable parameter {} is named {}, but the trait requires {}",
            ordinal(*index),
            format_english_quoted_text(provided),
            format_english_quoted_text(required),
        ),
        DiagnosticTraitFulfillmentMismatch::CallableParameterPosition {
            ordinal: index,
            required,
            provided,
        } => format!(
            "callable parameter {} is {}, but the trait requires {}",
            ordinal(*index),
            format_english_callable_position(*provided),
            format_english_callable_position(*required),
        ),
        DiagnosticTraitFulfillmentMismatch::CallableParameterMode {
            ordinal: index,
            required,
            provided,
        } => format!(
            "callable parameter {} is {}, but the trait requires {}",
            ordinal(*index),
            format_english_callable_parameter_mode(*provided),
            format_english_callable_parameter_mode(*required),
        ),
        DiagnosticTraitFulfillmentMismatch::CallableParameterType {
            ordinal: index,
            required,
            provided,
        } => format!(
            "callable parameter {} has type {}, but the trait requires {}",
            ordinal(*index),
            format_english_type(provided),
            format_english_type(required),
        ),
        DiagnosticTraitFulfillmentMismatch::CallableResultType { required, provided } => format!(
            "the callable returns {}, but the trait requires {}",
            format_english_type(provided),
            format_english_type(required),
        ),
        DiagnosticTraitFulfillmentMismatch::CallableParameterDefault {
            ordinal: index,
            required,
            provided,
        } => format!(
            "callable parameter {} {} a runtime default, but the trait member {}",
            ordinal(*index),
            if *provided {
                "declares"
            } else {
                "does not declare"
            },
            if *required { "does" } else { "does not" },
        ),
        DiagnosticTraitFulfillmentMismatch::CallableContract(mismatch) => {
            format_english_callable_contract_mismatch(mismatch)
        }
        DiagnosticTraitFulfillmentMismatch::ConstantType { required, provided } => format!(
            "the constant has type {}, but the trait requires {}",
            format_english_type(provided),
            format_english_type(required),
        ),
        DiagnosticTraitFulfillmentMismatch::TypeValueUnavailable => {
            "the type fulfillment does not provide a valid type".to_owned()
        }
        DiagnosticTraitFulfillmentMismatch::PredicateTrust { required, provided } => format!(
            "the predicate is {}, but the trait requires {}",
            if *provided { "trusted" } else { "untrusted" },
            if *required { "trusted" } else { "untrusted" },
        ),
        DiagnosticTraitFulfillmentMismatch::PredicateParameterCount { required, provided } => {
            format!(
                "the trait predicate requires {required} parameters but the fulfillment declares {provided}"
            )
        }
        DiagnosticTraitFulfillmentMismatch::PredicateParameterName {
            ordinal: index,
            required,
            provided,
        } => format!(
            "predicate parameter {} is named {}, but the trait requires {}",
            ordinal(*index),
            format_english_quoted_text(provided),
            format_english_quoted_text(required),
        ),
        DiagnosticTraitFulfillmentMismatch::PredicateParameterType {
            ordinal: index,
            required,
            provided,
        } => format!(
            "predicate parameter {} has type {}, but the trait requires {}",
            ordinal(*index),
            format_english_type(provided),
            format_english_type(required),
        ),
    }
}

const fn format_english_generic_parameter_category(
    category: DiagnosticGenericParameterCategory,
) -> &'static str {
    match category {
        DiagnosticGenericParameterCategory::Type => "a type parameter",
        DiagnosticGenericParameterCategory::Constant => "a constant parameter",
    }
}

pub(crate) fn format_english_receiver(mode: Option<DiagnosticReceiverMode>) -> &'static str {
    match mode {
        None => "absent",
        Some(DiagnosticReceiverMode::Shared) => "shared",
        Some(DiagnosticReceiverMode::Mutable) => "mutable",
        Some(DiagnosticReceiverMode::Consuming) => "consuming",
        Some(DiagnosticReceiverMode::ConsumingMutable) => "consuming and mutable",
    }
}

const fn format_english_callable_constness(value: DiagnosticCallableConstness) -> &'static str {
    match value {
        DiagnosticCallableConstness::Runtime => "runtime-only",
        DiagnosticCallableConstness::Constant => "available during compile-time evaluation",
    }
}

pub(crate) const fn format_english_callable_execution(
    value: DiagnosticCallableExecution,
) -> &'static str {
    match value {
        DiagnosticCallableExecution::Synchronous => "synchronous",
        DiagnosticCallableExecution::Asynchronous => "asynchronous",
    }
}

const fn format_english_callable_trust(value: DiagnosticCallableTrust) -> &'static str {
    match value {
        DiagnosticCallableTrust::Safe => "safe",
        DiagnosticCallableTrust::Trusted => "trusted",
    }
}

const fn format_english_callable_position(value: DiagnosticCallablePosition) -> &'static str {
    match value {
        DiagnosticCallablePosition::NamedOnly => "name-only",
        DiagnosticCallablePosition::PositionalOrNamed => "positional or named",
    }
}

const fn format_english_callable_parameter_mode(
    value: DiagnosticCallableParameterMode,
) -> &'static str {
    match value {
        DiagnosticCallableParameterMode::Immutable => "immutable",
        DiagnosticCallableParameterMode::Mutable => "mutable",
    }
}

fn format_english_generic_constraint_mismatch(
    mismatch: &DiagnosticGenericConstraintMismatch,
) -> String {
    let ordinal = |index: u64| index.saturating_add(1);

    match mismatch {
        DiagnosticGenericConstraintMismatch::Count { required, provided } => format!(
            "the trait requires {required} generic constraints but the fulfillment declares {provided}"
        ),
        DiagnosticGenericConstraintMismatch::Ordinal(index) => format!(
            "generic constraint {} appears in a different declaration position",
            ordinal(*index),
        ),
        DiagnosticGenericConstraintMismatch::Category {
            index,
            required,
            provided,
        } => format!(
            "generic constraint {} is {}, but the trait requires {}",
            ordinal(*index),
            format_english_constraint_category(*provided),
            format_english_constraint_category(*required),
        ),
        DiagnosticGenericConstraintMismatch::PredicateDependencies(index) => format!(
            "generic predicate constraint {} depends on different values",
            ordinal(*index),
        ),
        DiagnosticGenericConstraintMismatch::TraitSatisfaction(index) => format!(
            "generic trait constraint {} has a different subject or trait application",
            ordinal(*index),
        ),
        DiagnosticGenericConstraintMismatch::TypeEquality(index) => format!(
            "generic type-equality constraint {} compares different types",
            ordinal(*index),
        ),
    }
}

fn format_english_callable_contract_mismatch(
    mismatch: &DiagnosticCallableContractMismatch,
) -> String {
    let ordinal = |index: u64| index.saturating_add(1);

    match mismatch {
        DiagnosticCallableContractMismatch::ClauseCount {
            surface,
            required,
            provided,
        } => format!(
            "the trait requires {required} {} but the fulfillment declares {provided}",
            format_english_callable_contract_surface(*surface),
        ),
        DiagnosticCallableContractMismatch::ClauseOrdinal { surface, index } => format!(
            "{} clause {} appears in a different declaration position",
            format_english_callable_contract_surface(*surface),
            ordinal(*index),
        ),
        DiagnosticCallableContractMismatch::ClauseKind { surface, index } => format!(
            "{} clause {} uses a different directive kind",
            format_english_callable_contract_surface(*surface),
            ordinal(*index),
        ),
        DiagnosticCallableContractMismatch::ClauseCategory {
            surface,
            index,
            required,
            provided,
        } => format!(
            "{} clause {} is {}, but the trait requires {}",
            format_english_callable_contract_surface(*surface),
            ordinal(*index),
            format_english_callable_clause_category(*provided),
            format_english_callable_clause_category(*required),
        ),
        DiagnosticCallableContractMismatch::PredicateDependencies { surface, index } => format!(
            "{} predicate clause {} depends on different values",
            format_english_callable_contract_surface(*surface),
            ordinal(*index),
        ),
        DiagnosticCallableContractMismatch::TraitSatisfaction { surface, index } => format!(
            "{} trait clause {} has a different subject or trait application",
            format_english_callable_contract_surface(*surface),
            ordinal(*index),
        ),
        DiagnosticCallableContractMismatch::Behavior { phase, component } => format!(
            "the {} {} differ from the trait member",
            format_english_callable_behavior_phase(*phase),
            format_english_callable_behavior_component(*component),
        ),
        DiagnosticCallableContractMismatch::DeferredExecutionPresence => {
            "the callable and trait member disagree about deferred execution behavior".to_owned()
        }
    }
}

const fn format_english_constraint_category(value: DiagnosticConstraintCategory) -> &'static str {
    match value {
        DiagnosticConstraintCategory::Predicate => "a predicate",
        DiagnosticConstraintCategory::TraitSatisfaction => "a trait requirement",
        DiagnosticConstraintCategory::TypeEquality => "a type equality",
    }
}

const fn format_english_callable_clause_category(
    value: DiagnosticCallableContractClauseCategory,
) -> &'static str {
    match value {
        DiagnosticCallableContractClauseCategory::Predicate => "a predicate",
        DiagnosticCallableContractClauseCategory::TraitSatisfaction => "a trait requirement",
    }
}

const fn format_english_callable_behavior_phase(
    value: DiagnosticCallableBehaviorPhase,
) -> &'static str {
    match value {
        DiagnosticCallableBehaviorPhase::Invocation => "invocation",
        DiagnosticCallableBehaviorPhase::DeferredExecution => "deferred execution",
    }
}

const fn format_english_callable_behavior_component(
    value: DiagnosticCallableBehaviorComponent,
) -> &'static str {
    match value {
        DiagnosticCallableBehaviorComponent::Effects => "effects",
        DiagnosticCallableBehaviorComponent::Capabilities => "capabilities",
        DiagnosticCallableBehaviorComponent::TrustedCapabilities => "trusted capabilities",
        DiagnosticCallableBehaviorComponent::ExecutionRequirements => "execution requirements",
        DiagnosticCallableBehaviorComponent::LifecycleObligations => "lifecycle obligations",
        DiagnosticCallableBehaviorComponent::Dependencies => "dependencies",
    }
}

const fn format_english_callable_contract_surface(
    value: DiagnosticCallableContractSurface,
) -> &'static str {
    match value {
        DiagnosticCallableContractSurface::InvocationPreconditions => "invocation preconditions",
        DiagnosticCallableContractSurface::StaticConstraints => "compile-time constraints",
        DiagnosticCallableContractSurface::CompletionPostconditions => "completion postconditions",
    }
}
