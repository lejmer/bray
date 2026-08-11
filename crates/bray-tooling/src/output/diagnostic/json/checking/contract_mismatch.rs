use serde::Serialize;

#[derive(Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticGenericConstraintMismatchJson {
    Count {
        required: u64,
        provided: u64,
    },
    Ordinal {
        index: u64,
    },
    Category {
        index: u64,
        required: &'static str,
        provided: &'static str,
    },
    PredicateDependencies {
        index: u64,
    },
    TraitSatisfaction {
        index: u64,
    },
    TypeEquality {
        index: u64,
    },
}

impl DiagnosticGenericConstraintMismatchJson {
    pub(in crate::output::diagnostic::json) fn from_mismatch(
        mismatch: &bray_diagnostics::DiagnosticGenericConstraintMismatch,
    ) -> Self {
        use bray_diagnostics::DiagnosticGenericConstraintMismatch as Mismatch;

        match mismatch {
            Mismatch::Count { required, provided } => Self::Count {
                required: *required,
                provided: *provided,
            },
            Mismatch::Ordinal(index) => Self::Ordinal { index: *index },
            Mismatch::Category {
                index,
                required,
                provided,
            } => Self::Category {
                index: *index,
                required: constraint_category_key(*required),
                provided: constraint_category_key(*provided),
            },
            Mismatch::PredicateDependencies(index) => Self::PredicateDependencies { index: *index },
            Mismatch::TraitSatisfaction(index) => Self::TraitSatisfaction { index: *index },
            Mismatch::TypeEquality(index) => Self::TypeEquality { index: *index },
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "reason", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticCallableContractMismatchJson {
    ClauseCount {
        surface: &'static str,
        required: u64,
        provided: u64,
    },
    ClauseOrdinal {
        surface: &'static str,
        index: u64,
    },
    ClauseKind {
        surface: &'static str,
        index: u64,
    },
    ClauseCategory {
        surface: &'static str,
        index: u64,
        required: &'static str,
        provided: &'static str,
    },
    PredicateDependencies {
        surface: &'static str,
        index: u64,
    },
    TraitSatisfaction {
        surface: &'static str,
        index: u64,
    },
    Behavior {
        phase: &'static str,
        component: &'static str,
    },
    DeferredExecutionPresence,
}

impl DiagnosticCallableContractMismatchJson {
    pub(in crate::output::diagnostic::json) fn from_mismatch(
        mismatch: &bray_diagnostics::DiagnosticCallableContractMismatch,
    ) -> Self {
        use bray_diagnostics::DiagnosticCallableContractMismatch as Mismatch;

        match mismatch {
            Mismatch::ClauseCount {
                surface,
                required,
                provided,
            } => Self::ClauseCount {
                surface: callable_contract_surface_key(*surface),
                required: *required,
                provided: *provided,
            },
            Mismatch::ClauseOrdinal { surface, index } => Self::ClauseOrdinal {
                surface: callable_contract_surface_key(*surface),
                index: *index,
            },
            Mismatch::ClauseKind { surface, index } => Self::ClauseKind {
                surface: callable_contract_surface_key(*surface),
                index: *index,
            },
            Mismatch::ClauseCategory {
                surface,
                index,
                required,
                provided,
            } => Self::ClauseCategory {
                surface: callable_contract_surface_key(*surface),
                index: *index,
                required: callable_clause_category_key(*required),
                provided: callable_clause_category_key(*provided),
            },
            Mismatch::PredicateDependencies { surface, index } => Self::PredicateDependencies {
                surface: callable_contract_surface_key(*surface),
                index: *index,
            },
            Mismatch::TraitSatisfaction { surface, index } => Self::TraitSatisfaction {
                surface: callable_contract_surface_key(*surface),
                index: *index,
            },
            Mismatch::Behavior { phase, component } => Self::Behavior {
                phase: callable_behavior_phase_key(*phase),
                component: callable_behavior_component_key(*component),
            },
            Mismatch::DeferredExecutionPresence => Self::DeferredExecutionPresence,
        }
    }
}

pub(in crate::output::diagnostic::json) const fn generic_parameter_category_key(
    value: bray_diagnostics::DiagnosticGenericParameterCategory,
) -> &'static str {
    match value {
        bray_diagnostics::DiagnosticGenericParameterCategory::Type => "type",
        bray_diagnostics::DiagnosticGenericParameterCategory::Constant => "constant",
    }
}

const fn constraint_category_key(
    value: bray_diagnostics::DiagnosticConstraintCategory,
) -> &'static str {
    match value {
        bray_diagnostics::DiagnosticConstraintCategory::Predicate => "predicate",
        bray_diagnostics::DiagnosticConstraintCategory::TraitSatisfaction => "trait_satisfaction",
        bray_diagnostics::DiagnosticConstraintCategory::TypeEquality => "type_equality",
    }
}

const fn callable_clause_category_key(
    value: bray_diagnostics::DiagnosticCallableContractClauseCategory,
) -> &'static str {
    match value {
        bray_diagnostics::DiagnosticCallableContractClauseCategory::Predicate => "predicate",
        bray_diagnostics::DiagnosticCallableContractClauseCategory::TraitSatisfaction => {
            "trait_satisfaction"
        }
    }
}

pub(in crate::output::diagnostic::json) const fn callable_behavior_phase_key(
    value: bray_diagnostics::DiagnosticCallableBehaviorPhase,
) -> &'static str {
    match value {
        bray_diagnostics::DiagnosticCallableBehaviorPhase::Invocation => "invocation",
        bray_diagnostics::DiagnosticCallableBehaviorPhase::DeferredExecution => {
            "deferred_execution"
        }
    }
}

pub(in crate::output::diagnostic::json) const fn callable_behavior_component_key(
    value: bray_diagnostics::DiagnosticCallableBehaviorComponent,
) -> &'static str {
    match value {
        bray_diagnostics::DiagnosticCallableBehaviorComponent::Effects => "effects",
        bray_diagnostics::DiagnosticCallableBehaviorComponent::Capabilities => "capabilities",
        bray_diagnostics::DiagnosticCallableBehaviorComponent::TrustedCapabilities => {
            "trusted_capabilities"
        }
        bray_diagnostics::DiagnosticCallableBehaviorComponent::ExecutionRequirements => {
            "execution_requirements"
        }
        bray_diagnostics::DiagnosticCallableBehaviorComponent::LifecycleObligations => {
            "lifecycle_obligations"
        }
        bray_diagnostics::DiagnosticCallableBehaviorComponent::Dependencies => "dependencies",
    }
}

pub(in crate::output::diagnostic::json) const fn receiver_mode_key(
    value: bray_diagnostics::DiagnosticReceiverMode,
) -> &'static str {
    match value {
        bray_diagnostics::DiagnosticReceiverMode::Shared => "shared",
        bray_diagnostics::DiagnosticReceiverMode::Mutable => "mutable",
        bray_diagnostics::DiagnosticReceiverMode::Consuming => "consuming",
        bray_diagnostics::DiagnosticReceiverMode::ConsumingMutable => "consuming_mutable",
    }
}

pub(in crate::output::diagnostic::json) const fn callable_constness_key(
    value: bray_diagnostics::DiagnosticCallableConstness,
) -> &'static str {
    match value {
        bray_diagnostics::DiagnosticCallableConstness::Runtime => "runtime",
        bray_diagnostics::DiagnosticCallableConstness::Constant => "constant",
    }
}

pub(in crate::output::diagnostic::json) const fn callable_execution_key(
    value: bray_diagnostics::DiagnosticCallableExecution,
) -> &'static str {
    match value {
        bray_diagnostics::DiagnosticCallableExecution::Synchronous => "synchronous",
        bray_diagnostics::DiagnosticCallableExecution::Asynchronous => "asynchronous",
    }
}

pub(in crate::output::diagnostic::json) const fn callable_trust_key(
    value: bray_diagnostics::DiagnosticCallableTrust,
) -> &'static str {
    match value {
        bray_diagnostics::DiagnosticCallableTrust::Safe => "safe",
        bray_diagnostics::DiagnosticCallableTrust::Trusted => "trusted",
    }
}

pub(in crate::output::diagnostic::json) const fn callable_position_key(
    value: bray_diagnostics::DiagnosticCallablePosition,
) -> &'static str {
    match value {
        bray_diagnostics::DiagnosticCallablePosition::NamedOnly => "named_only",
        bray_diagnostics::DiagnosticCallablePosition::PositionalOrNamed => "positional_or_named",
    }
}

pub(in crate::output::diagnostic::json) const fn callable_parameter_mode_key(
    value: bray_diagnostics::DiagnosticCallableParameterMode,
) -> &'static str {
    match value {
        bray_diagnostics::DiagnosticCallableParameterMode::Immutable => "immutable",
        bray_diagnostics::DiagnosticCallableParameterMode::Mutable => "mutable",
    }
}

pub(in crate::output::diagnostic::json) const fn callable_contract_surface_key(
    value: bray_diagnostics::DiagnosticCallableContractSurface,
) -> &'static str {
    match value {
        bray_diagnostics::DiagnosticCallableContractSurface::InvocationPreconditions => {
            "invocation_preconditions"
        }
        bray_diagnostics::DiagnosticCallableContractSurface::StaticConstraints => {
            "static_constraints"
        }
        bray_diagnostics::DiagnosticCallableContractSurface::CompletionPostconditions => {
            "completion_postconditions"
        }
    }
}
