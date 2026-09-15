use bray_diagnostics::{
    DiagnosticBag, DiagnosticEmissionEvaluationFailure, DiagnosticEmissionFailure,
};
use bray_symbols::ProductIdentity;
use bray_target::TargetIdentity;

use super::common::emission_failure_diagnostics;
use crate::fact::FactQueryError;

pub(super) fn query_failure_diagnostics(
    error: &FactQueryError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> DiagnosticBag {
    if matches!(error, FactQueryError::Cancelled) {
        return DiagnosticBag::new();
    }

    emission_failure_diagnostics(
        DiagnosticEmissionFailure::Evaluation(diagnostic_evaluation_failure(error)),
        product,
        target,
    )
}

pub(crate) fn diagnostic_evaluation_failure(
    error: &FactQueryError,
) -> DiagnosticEmissionEvaluationFailure {
    match error {
        FactQueryError::Cancelled => DiagnosticEmissionEvaluationFailure::Cancelled,
        FactQueryError::Cycle(cycle) => {
            DiagnosticEmissionEvaluationFailure::Cycle(crate::fact::diagnostic_cycle_failure(cycle))
        }
        FactQueryError::SymbolGraph(error) => DiagnosticEmissionEvaluationFailure::SemanticQuery(
            crate::fact::diagnostic_symbol_graph_failure(*error),
        ),
        FactQueryError::CodegenTarget(error) => DiagnosticEmissionEvaluationFailure::Product(
            bray_diagnostics::DiagnosticProductQueryFailure::new(codegen_target_reason(*error), []),
        ),
        FactQueryError::PackageInterface(error) => DiagnosticEmissionEvaluationFailure::Product(
            bray_diagnostics::DiagnosticProductQueryFailure::new(
                interface_validation_reason(error.as_ref()),
                [bray_diagnostics::DiagnosticFailureField::new(
                    "interface_validation_cause",
                    bray_diagnostics::DiagnosticFailureValue::InterfaceValidationFailure(
                        error.diagnostic_failure(),
                    ),
                )],
            ),
        ),
        FactQueryError::ImportedQuery(error) => DiagnosticEmissionEvaluationFailure::SemanticQuery(
            diagnostic_imported_query_failure(error),
        ),
        FactQueryError::Runtime(error) => DiagnosticEmissionEvaluationFailure::Runtime(
            crate::fact::diagnostic_fact_runtime_failure(error),
        ),
        FactQueryError::SemanticValueStoreCreate(_) => {
            DiagnosticEmissionEvaluationFailure::SemanticValueStoreCreate
        }
        FactQueryError::SemanticValueStore(error) => {
            DiagnosticEmissionEvaluationFailure::SemanticValue(
                crate::fact::diagnostic_semantic_value_failure(*error),
            )
        }
        FactQueryError::BindingDependencyUnavailable => {
            DiagnosticEmissionEvaluationFailure::Binding(
                bray_diagnostics::DiagnosticBindingFailure::new(
                    "binding_dependency_unavailable",
                    [],
                ),
            )
        }
        FactQueryError::Binding(error) => DiagnosticEmissionEvaluationFailure::Binding(
            crate::fact::diagnostic_binding_failure(error),
        ),
        FactQueryError::LoweringInput(error) => DiagnosticEmissionEvaluationFailure::LoweringInput(
            super::super::super::lowering_diagnostic::lowering_input_failure(error),
        ),
        FactQueryError::Lowering(error) => DiagnosticEmissionEvaluationFailure::Lowering(
            super::super::super::lowering_diagnostic::lowering_failure(error),
        ),
        FactQueryError::ConstantCallableBodyUnavailable => {
            DiagnosticEmissionEvaluationFailure::ConstantCallableBodyUnavailable
        }
        FactQueryError::ConstantCallableRootUnavailable => {
            DiagnosticEmissionEvaluationFailure::ConstantCallableRootUnavailable
        }
        FactQueryError::AtomicInitializerArgumentUnavailable => {
            DiagnosticEmissionEvaluationFailure::AtomicInitializerArgumentUnavailable
        }
        FactQueryError::AtomicInitializerResultUnavailable => {
            DiagnosticEmissionEvaluationFailure::AtomicInitializerResultUnavailable
        }
        FactQueryError::UninitInitializerResultUnavailable => {
            DiagnosticEmissionEvaluationFailure::UninitInitializerResultUnavailable
        }
        FactQueryError::SemanticUnitContext(error) => {
            DiagnosticEmissionEvaluationFailure::SemanticContext(
                crate::fact::diagnostic_semantic_context_failure(error),
            )
        }
        FactQueryError::SemanticQuery(error) => DiagnosticEmissionEvaluationFailure::SemanticQuery(
            crate::fact::diagnostic_semantic_query_failure(error),
        ),
        FactQueryError::Product(error) => DiagnosticEmissionEvaluationFailure::Product(
            super::product_query::diagnostic_product_query_failure(error),
        ),
        FactQueryError::Foreign(error) => DiagnosticEmissionEvaluationFailure::Foreign(
            super::foreign_query::diagnostic_foreign_query_failure(error),
        ),
        FactQueryError::CheckerInfrastructure(error) => match error {
            bray_checker::CheckerInfrastructureError::AtomicRepresentationTypeUnavailable => {
                DiagnosticEmissionEvaluationFailure::AtomicRepresentationTypeUnavailable
            }
            bray_checker::CheckerInfrastructureError::AtomicRepresentationArgumentsUnavailable => {
                DiagnosticEmissionEvaluationFailure::AtomicRepresentationArgumentsUnavailable
            }
            bray_checker::CheckerInfrastructureError::AtomicInitializerArgumentUnavailable => {
                DiagnosticEmissionEvaluationFailure::AtomicInitializerArgumentUnavailable
            }
            bray_checker::CheckerInfrastructureError::AtomicInitializerResultUnavailable => {
                DiagnosticEmissionEvaluationFailure::AtomicInitializerResultUnavailable
            }
            bray_checker::CheckerInfrastructureError::UninitInitializerResultUnavailable => {
                DiagnosticEmissionEvaluationFailure::UninitInitializerResultUnavailable
            }
            bray_checker::CheckerInfrastructureError::ImportedExecutableTemplateMismatch => {
                DiagnosticEmissionEvaluationFailure::ImportedExecutableTemplateMismatch
            }
            error => DiagnosticEmissionEvaluationFailure::Checker(
                crate::fact::diagnostic_checker_failure(*error),
            ),
        },
    }
}

fn diagnostic_imported_query_failure(
    error: &crate::fact::ImportedQueryFailure,
) -> bray_diagnostics::DiagnosticSemanticQueryFailure {
    use crate::fact::ImportedQueryFailure as Error;

    let (reason, context) = match error {
        Error::InterfaceCapacityExceeded(index) => (
            "imported_query_interface_capacity_exceeded",
            vec![crate::fact::diagnostic_context::natural_field(
                "index", *index,
            )],
        ),
    };

    bray_diagnostics::DiagnosticSemanticQueryFailure::new("imported_query", reason, context)
}

const fn codegen_target_reason(error: bray_codegen::CodegenTargetBuildError) -> &'static str {
    match error {
        bray_codegen::CodegenTargetBuildError::UnsupportedProfile => {
            "codegen_target_unsupported_profile"
        }
        bray_codegen::CodegenTargetBuildError::EmptyTriple => "codegen_target_empty_triple",
        bray_codegen::CodegenTargetBuildError::EmptyCpu => "codegen_target_empty_cpu",
        bray_codegen::CodegenTargetBuildError::EmptyFeature => "codegen_target_empty_feature",
    }
}

pub(super) const fn interface_validation_reason(
    error: &bray_package_interface::InterfaceValidationError,
) -> &'static str {
    use bray_package_interface::InterfaceValidationError as Error;

    match error {
        Error::InvalidMagic { .. } => "interface_invalid_magic",
        Error::UnsupportedFormatRevision { .. } => "interface_unsupported_format_revision",
        Error::UnsupportedLanguageRevision { .. } => "interface_unsupported_language_revision",
        Error::UnsupportedByteOrder { .. } => "interface_unsupported_byte_order",
        Error::UnsupportedRequiredFlags { .. } => "interface_unsupported_required_flags",
        Error::Truncated { .. } => "interface_truncated",
        Error::TrailingBytes { .. } => "interface_trailing_bytes",
        Error::Malformed { .. } => "interface_malformed",
        Error::InvalidUtf8 { .. } => "interface_invalid_utf8",
        Error::Compression { .. } => "interface_compression",
        Error::DigestUnavailable { .. } => "interface_digest_unavailable",
        Error::AllocationUnavailable { .. } => "interface_allocation_unavailable",
        Error::SurfaceBuild { .. } => "interface_surface_build",
        Error::ArtifactHashMismatch { .. } => "interface_artifact_hash_mismatch",
        Error::ContentHashMismatch { .. } => "interface_content_hash_mismatch",
        Error::SectionChecksumMismatch { .. } => "interface_section_checksum_mismatch",
        Error::UnknownSectionChecksumMismatch { .. } => {
            "interface_unknown_section_checksum_mismatch"
        }
        Error::SectionContentHashMismatch { .. } => "interface_section_content_hash_mismatch",
        Error::PayloadChecksumMismatch { .. } => "interface_payload_checksum_mismatch",
        Error::PayloadContentHashMismatch { .. } => "interface_payload_content_hash_mismatch",
        Error::SpecializationKeyMismatch { .. } => "interface_specialization_key_mismatch",
        Error::ImplementationConfigurationMismatch { .. } => {
            "interface_implementation_configuration_mismatch"
        }
        Error::ImplementationInterfaceIdentityMismatch { .. } => {
            "interface_implementation_identity_mismatch"
        }
        Error::ImplementationDependencyMismatch { .. } => {
            "interface_implementation_dependency_mismatch"
        }
        Error::ResourceLimitExceeded { .. } => "interface_resource_limit_exceeded",
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticEmissionEvaluationFailure, DiagnosticFailureValue,
        DiagnosticInterfaceValidationFailure,
    };

    use super::diagnostic_evaluation_failure;
    use crate::fact::FactQueryError;

    #[test]
    fn package_interface_conversion_preserves_the_typed_validation_failure() {
        let failure = diagnostic_evaluation_failure(&FactQueryError::PackageInterface(Box::new(
            bray_package_interface::InterfaceValidationError::InvalidMagic {
                actual: *b"not-bray",
            },
        )));

        let DiagnosticEmissionEvaluationFailure::Product(failure) = failure else {
            panic!("package-interface validation must remain a product query failure");
        };

        assert_eq!(failure.as_str(), "interface_invalid_magic");
        assert_eq!(failure.context()[0].name(), "interface_validation_cause");

        assert_eq!(
            failure.context()[0].value(),
            &DiagnosticFailureValue::InterfaceValidationFailure(
                DiagnosticInterfaceValidationFailure::InvalidMagic {
                    actual: *b"not-bray",
                }
            )
        );
    }
}
