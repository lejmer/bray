use super::super::source::{format_english_artifact_digest, format_english_artifact_kind};

use super::foreign_query::format_english_foreign_query_failure;
use super::linking::format_english_native_link_input_failure;
use super::product_query::format_english_product_query_failure;
use bray_diagnostics::DiagnosticArtifactDigest;

pub(crate) fn format_artifact_failure(
    message: &str,
    artifact: bray_diagnostics::DiagnosticEmissionArtifact,
) -> String {
    format!(
        "{message}: {} artifact #{}",
        format_english_artifact_kind(artifact.kind()),
        artifact.ordinal(),
    )
}

pub(crate) fn format_unit_failure(message: &str, unit: &DiagnosticArtifactDigest) -> String {
    format!(
        "native-code work item {} {message}",
        format_english_artifact_digest(unit),
    )
}

pub(crate) fn format_english_emission_evaluation_failure(
    failure: &bray_diagnostics::DiagnosticEmissionEvaluationFailure,
) -> String {
    use bray_diagnostics::DiagnosticEmissionEvaluationFailure as Failure;

    let message = match failure {
        Failure::Cancelled => "evaluation was cancelled",
        Failure::Cycle(_) => "evaluation encountered a dependency cycle",
        Failure::Runtime(failure) => return format_english_fact_runtime_failure(failure),
        Failure::SemanticValueStoreCreate => {
            "process-local semantic-value store identity capacity was exhausted during evaluation"
        }
        Failure::SemanticValue(failure) => {
            return format_english_semantic_value_failure(*failure);
        }
        Failure::Binding(failure) => return format_english_binding_failure(failure),
        Failure::MirCapacity => "an executable-code identity limit was exceeded during evaluation",
        Failure::SemanticQuery(failure) => {
            return format_english_semantic_query_failure(failure);
        }
        Failure::Product(failure) => return format_english_product_query_failure(failure),
        Failure::Foreign(failure) => return format_english_foreign_query_failure(failure),
    };

    message.to_owned()
}

fn format_english_semantic_query_failure(
    failure: &bray_diagnostics::DiagnosticSemanticQueryFailure,
) -> String {
    let message = match failure.reason() {
        "generic_substitution_ordinal_overflow" => {
            "the generic parameter count exceeds the compiler's supported limit"
        }
        "symbol_capacity_exceeded" => {
            "the declaration count exceeds the compiler's supported limit"
        }
        "imported_query_interface_capacity_exceeded" => {
            "the compiled dependency count exceeds the compiler's supported limit"
        }
        _ => "this construct exceeds the compiler's declaration or expression limits",
    };

    format_failure_detail(message, failure.context())
}

pub(crate) fn format_english_native_product_failure(
    kind: &bray_diagnostics::DiagnosticNativeProductFailureKind,
) -> String {
    use bray_diagnostics::DiagnosticNativeProductFailureKind as Kind;

    let message = match kind {
        Kind::CodegenBackendNotSelected => "no native-code generator is selected",
        Kind::MissingRuntime => "the asynchronous product has no selected runtime",
        Kind::InvalidNativeLinkInput(failure) => {
            return format_english_native_link_input_failure(failure);
        }
        Kind::EvaluationCancelled => "product construction was cancelled",
        Kind::EvaluationCycle(_) => "evaluation encountered a dependency cycle",
        Kind::EvaluationRuntime(failure) => return format_english_fact_runtime_failure(failure),
        Kind::EvaluationSemanticValueStoreCreate => {
            "process-local semantic-value store identity capacity was exhausted during product construction"
        }
        Kind::EvaluationSemanticValue(failure) => {
            return format_english_semantic_value_failure(*failure);
        }
        Kind::EvaluationBinding(failure) => {
            return format_english_binding_failure(failure);
        }
        Kind::EvaluationSemanticQuery(failure) => {
            return format_english_semantic_query_failure(failure);
        }
        Kind::EvaluationProduct(failure) => return format_english_product_query_failure(failure),
        Kind::EvaluationForeign(failure) => return format_english_foreign_query_failure(failure),
        Kind::CodegenTargetUnsupportedProfile => {
            "the selected target profile cannot generate native code"
        }
        Kind::NativeResolution(detail) => match detail.reason() {
            "native_variant_conflict" => {
                "the selected artifacts disagree about the native implementation of the same library"
            }
            "native_static_conflict" => {
                "the selected libraries disagree about the cleanup requirements of the same static declaration"
            }
            "native_static_ambiguous_order" => {
                "distinct static instances in the selected libraries have the same cleanup ordering key"
            }
            "native_static_missing" => {
                "a selected library requires a static instance whose cleanup provider is absent"
            }
            "native_static_lifecycle_cycle" => {
                "static cleanup dependencies in the selected libraries form a cycle"
            }
            "native_symbol_unresolved" => {
                "the selected libraries do not define a required native symbol"
            }
            _ => "the selected libraries define the same required native symbol more than once",
        },
        Kind::RuntimeSelectionIncompatible(_) => {
            "the selected runtime is incompatible with the product"
        }
        Kind::RuntimeSelectionMissingRoleOwner(_) => {
            "the selected runtime has no owner for a required ABI role"
        }
        Kind::RuntimeSelectionMissingCapabilityOwner(_) => {
            "the selected runtime has no owner for a required capability"
        }
        Kind::CodegenBackendUnsupportedTarget => {
            "native code generation is unavailable for the selected target"
        }
        Kind::CodegenBackendUnsupportedTargetDetail(_) => {
            "native code generation is unavailable for the selected target"
        }
        Kind::CodegenBackendUnsupportedArtifact(_) => {
            "native code generation cannot produce a requested artifact"
        }
        Kind::CodegenBackendInvalidConfiguration
        | Kind::CodegenBackendInvalidConfigurationDetail(_) => {
            "native-code configuration is internally inconsistent"
        }
        Kind::CodegenBackendResourceExhausted => {
            "native code generation exceeded an available resource budget"
        }
        Kind::CodegenBackendResourceLimit(_) => {
            "native code generation exceeded an available resource budget"
        }
        Kind::CodegenBackendLibraryFailure(_) => {
            "backend-library processing failed for valid native-code input"
        }
        Kind::CodegenBackendToolFailure(_) => {
            "native code generation received an unsuccessful support-program result"
        }
        Kind::CodegenBackendArtifactConstruction(_) => {
            "a requested native artifact could not be constructed"
        }
        Kind::CodegenBackendArtifactRead(_) => "native artifact storage could not be read",
        Kind::CodegenBackendUnavailable => "no native-code generator is available",
        Kind::CodegenMirCapacityExceeded => {
            "generated executable code exceeded the compiler implementation capacity"
        }
        Kind::CodegenInvalidAbiMapping(_) => "a callable ABI mapping is invalid",
        Kind::CodegenUnsupportedType(_) => {
            "the native-code generator cannot represent a required type"
        }
        Kind::CodegenExactIntegerCapacity {
            minimum_bits,
            maximum_bits,
        } => {
            return format!(
                "contract arithmetic needs at least {minimum_bits} bits, exceeding this compiler's {maximum_bits}-bit limit"
            );
        }
        Kind::CodegenLayoutOverflow(_) => "a required type layout exceeds the selected target",
    };

    message.to_owned()
}

fn format_english_fact_runtime_failure(
    failure: &bray_diagnostics::DiagnosticFactRuntimeFailure,
) -> String {
    let message = match failure.reason() {
        "worker_pool_creation" => "the host could not start the requested compiler workers",
        _ => "this compilation exceeds a compiler resource limit",
    };

    format_failure_detail(message, failure.context())
}

pub(crate) fn format_english_runtime_artifact_problem(
    problem: &bray_diagnostics::DiagnosticRuntimeArtifactProblem,
) -> String {
    use bray_diagnostics::DiagnosticRuntimeArtifactProblem as Problem;

    match problem {
        Problem::MetadataSizeLimitExceeded => "metadata exceeds the size limit".to_owned(),
        Problem::MalformedMetadata => "malformed metadata".to_owned(),
        Problem::UnsupportedFormat => "unsupported metadata format".to_owned(),
        Problem::InvalidRuntimeIdentity => "invalid runtime identity".to_owned(),
        Problem::InvalidArtifactIdentity => "invalid artifact identity".to_owned(),
        Problem::InvalidTarget => "invalid target identity".to_owned(),
        Problem::InvalidPanicAbi => "invalid panic ABI identity".to_owned(),
        Problem::UnknownCapability => "unknown runtime capability".to_owned(),
        Problem::UnknownRole => "unknown runtime role".to_owned(),
        Problem::UnknownPlatformService => "unknown native operation".to_owned(),
        Problem::InvalidRoleSymbol => "invalid runtime role symbol".to_owned(),
        Problem::UnknownRoleImplementation => "unknown runtime role implementation".to_owned(),
        Problem::UnknownComponentPurpose => "unknown runtime component purpose".to_owned(),
        Problem::InvalidComponentIdentity => "invalid runtime component identity".to_owned(),
        Problem::InvalidNativeIndexDigest => "invalid runtime native-index digest".to_owned(),
        Problem::DuplicateContractRole(role) => format!("duplicate runtime contract role `{role}`"),
        Problem::CompilerOwnedRole(role) => {
            format!("compiler-owned role `{role}` is published by the runtime")
        }
        Problem::MissingCooperativeExecution => {
            "runtime contract omits cooperative execution".to_owned()
        }
        Problem::InvalidNativeIndexFileName => "invalid runtime native-index file name".to_owned(),
        Problem::InvalidNativeIndexes => {
            "runtime native indexes do not cover each product category once".to_owned()
        }
        Problem::InvalidNativeArtifact(cause) => {
            super::standard_library::format_english_native_artifact_problem(*cause).to_owned()
        }
        Problem::DuplicateComponent(component) => {
            format!("duplicate runtime component `{component}`")
        }
        Problem::UnknownComponentRole(role) => {
            format!("component claims unknown runtime role `{role}`")
        }
        Problem::UnknownComponentCapability(capability) => {
            format!("component claims unknown runtime capability `{capability}`")
        }
        Problem::TestRoleInProductComponent(component) => {
            format!("product component `{component}` claims the test-entry role")
        }
        Problem::MissingRoleOwner { purpose, role } => format!(
            "{} runtime surface has no owner for role `{role}`",
            format_english_runtime_artifact_purpose(*purpose),
        ),
        Problem::DuplicateRoleOwner { purpose, role } => format!(
            "{} runtime surface has multiple owners for role `{role}`",
            format_english_runtime_artifact_purpose(*purpose),
        ),
        Problem::MissingCapabilityOwner {
            purpose,
            capability,
        } => format!(
            "{} runtime surface has no owner for capability `{capability}`",
            format_english_runtime_artifact_purpose(*purpose),
        ),
        Problem::DuplicateCapabilityOwner {
            purpose,
            capability,
        } => format!(
            "{} runtime surface has multiple owners for capability `{capability}`",
            format_english_runtime_artifact_purpose(*purpose),
        ),
        Problem::DuplicatePlatformServiceOwner { purpose } => format!(
            "the same native operation is provided more than once for {} programs",
            format_english_runtime_artifact_purpose(*purpose),
        ),
        Problem::InvalidNativeTarget => "runtime target has no native representation".to_owned(),
        Problem::IncompatibleIndexTarget => {
            "runtime native index targets another platform".to_owned()
        }
    }
}

const fn format_english_runtime_artifact_purpose(
    purpose: bray_diagnostics::DiagnosticRuntimeArtifactPurpose,
) -> &'static str {
    match purpose {
        bray_diagnostics::DiagnosticRuntimeArtifactPurpose::Product => "product",
        bray_diagnostics::DiagnosticRuntimeArtifactPurpose::TestRunner => "test-runner",
    }
}

fn format_english_binding_failure(failure: &bray_diagnostics::DiagnosticBindingFailure) -> String {
    if let Some(cause) = failure.semantic_value_failure() {
        return format_english_semantic_value_failure(cause);
    }

    format_failure_detail(
        "this declaration exceeds the compiler's expression or local binding limits",
        failure.context(),
    )
}

pub(super) fn format_english_semantic_value_failure(
    failure: bray_diagnostics::DiagnosticSemanticValueFailure,
) -> String {
    String::from(format_english_semantic_value_failure_detail(failure))
}

pub(crate) const fn format_english_semantic_value_failure_detail(
    failure: bray_diagnostics::DiagnosticSemanticValueFailure,
) -> &'static str {
    use bray_diagnostics::DiagnosticSemanticValueFailure as Failure;

    match failure {
        Failure::CapacityExhausted { .. } => {
            "semantic-value capacity was exhausted while retaining data required by this product"
        }
    }
}

pub(in crate::catalog::english::argument) fn format_failure_detail(
    message: &str,
    fields: &[bray_diagnostics::DiagnosticFailureField],
) -> String {
    let mut output = message.to_owned();

    for field in fields {
        let label = match field.name() {
            "host_error_message" | "report" => "host report",
            "workers" => "requested workers",
            "index" | "value" | "actual" | "capacity_actual" => "requested count",
            "maximum" => "supported limit",
            "source_byte_count" => "source bytes",
            "requested_product" => "requested product",
            "compilation_package" => "loaded package",
            "compilation_product_kind" => "loaded product kind",
            "backend_name" => "backend",
            "backend_revision" => "backend revision",
            "backend_toolchain_revision" => "toolchain revision",
            "runtime_role" => "runtime role",
            "platform_role" => "platform role",
            "first_role" => "first role",
            "duplicate_role" => "duplicate role",
            _ => continue,
        };

        let value = match field.value() {
            bray_diagnostics::DiagnosticFailureValue::Text(value)
            | bray_diagnostics::DiagnosticFailureValue::Natural(value) => value.as_str(),
            _ => continue,
        };

        output.push_str(". ");
        output.push_str(label);
        output.push_str(": ");
        output.push_str(value);
    }

    output
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticFailureField, DiagnosticFailureValue};

    #[test]
    fn worker_creation_reports_the_host_cause_and_requested_count() {
        let message = super::format_english_fact_runtime_failure(
            &bray_diagnostics::DiagnosticFactRuntimeFailure::new(
                "worker_pool_creation",
                [
                    DiagnosticFailureField::new(
                        "workers",
                        DiagnosticFailureValue::Natural("20".to_owned()),
                    ),
                    DiagnosticFailureField::new(
                        "host_error_message",
                        DiagnosticFailureValue::Text("permission denied".to_owned()),
                    ),
                ],
            ),
        );

        assert!(message.contains("20"));
        assert!(message.contains("permission denied"));
        assert!(!message.contains("internal compiler error"));
    }

    #[test]
    fn capacity_rendering_retains_the_count_without_internal_identities() {
        let message = super::format_english_semantic_query_failure(
            &bray_diagnostics::DiagnosticSemanticQueryFailure::new(
                "semantic_query_capacity",
                "symbol_capacity_exceeded",
                [
                    DiagnosticFailureField::new(
                        "value",
                        DiagnosticFailureValue::Natural("4294967296".to_owned()),
                    ),
                    DiagnosticFailureField::new(
                        "symbol",
                        DiagnosticFailureValue::Identity([7; 32]),
                    ),
                ],
            ),
        );

        assert!(message.contains("declaration count"));
        assert!(message.contains("4294967296"));
        assert!(!message.contains("symbol_capacity_exceeded"));
        assert!(!message.contains("07070707"));
        assert!(!message.contains("internal compiler error"));
    }
}
