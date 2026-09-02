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
        FactQueryError::InfrastructureFailure => {
            DiagnosticEmissionEvaluationFailure::Infrastructure
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
        Error::MissingDependencyInput(interface) => (
            "imported_query_missing_dependency_input",
            vec![crate::fact::diagnostic_context::count_field(
                "interface",
                u64::from(interface.raw()),
            )],
        ),
        Error::MissingLoadedInterface(interface) => (
            "imported_query_missing_loaded_interface",
            vec![crate::fact::diagnostic_context::count_field(
                "interface",
                u64::from(interface.raw()),
            )],
        ),
        Error::MissingLoadedImplementation(interface) => (
            "imported_query_missing_loaded_implementation",
            vec![crate::fact::diagnostic_context::count_field(
                "interface",
                u64::from(interface.raw()),
            )],
        ),
        Error::MissingLoadedSurface(interface) => (
            "imported_query_missing_loaded_surface",
            vec![crate::fact::diagnostic_context::count_field(
                "interface",
                u64::from(interface.raw()),
            )],
        ),
        Error::MissingLoadedSemanticGraph(interface) => (
            "imported_query_missing_loaded_semantic_graph",
            vec![crate::fact::diagnostic_context::count_field(
                "interface",
                u64::from(interface.raw()),
            )],
        ),
        Error::MissingSemanticGraph(key)
        | Error::MissingInterfaceSurface(key)
        | Error::MissingInterfaceSymbol(key)
        | Error::MissingImportedSymbol(key) => {
            let reason = match error {
                Error::MissingSemanticGraph(_) => "imported_query_missing_semantic_graph",
                Error::MissingInterfaceSurface(_) => "imported_query_missing_interface_surface",
                Error::MissingInterfaceSymbol(_) => "imported_query_missing_interface_symbol",
                Error::MissingImportedSymbol(_) => "imported_query_missing_imported_symbol",
                _ => unreachable!(),
            };

            (reason, imported_record_context(*key))
        }
        Error::MissingLoadedInterfaceViews(interface) => (
            "imported_query_missing_loaded_interface_views",
            vec![crate::fact::diagnostic_context::count_field(
                "interface",
                u64::from(interface.raw()),
            )],
        ),
        Error::MissingCurrentInterface(interface) => (
            "imported_query_missing_current_interface",
            vec![crate::fact::diagnostic_context::count_field(
                "interface",
                u64::from(interface.raw()),
            )],
        ),
        Error::MissingResolvedSymbol(address) => (
            "imported_query_missing_resolved_symbol",
            vec![
                crate::fact::diagnostic_context::count_field(
                    "interface",
                    u64::from(address.interface().raw()),
                ),
                crate::fact::diagnostic_context::count_field(
                    "symbol",
                    u64::from(address.symbol().raw()),
                ),
            ],
        ),
        Error::ExecutableTemplateMismatch(failure) => {
            let mut fields = vec![
                crate::fact::diagnostic_context::count_field(
                    "interface",
                    u64::from(failure.interface().raw()),
                ),
                crate::fact::diagnostic_context::count_field(
                    "symbol",
                    u64::from(failure.symbol().raw()),
                ),
                crate::fact::diagnostic_context::count_field(
                    "template",
                    u64::from(failure.template().raw()),
                ),
                crate::fact::diagnostic_context::count_field(
                    "expected_unit",
                    u64::from(failure.expected_unit().raw()),
                ),
                crate::fact::diagnostic_context::count_field(
                    "actual_unit",
                    u64::from(failure.actual_unit().raw()),
                ),
                crate::fact::diagnostic_context::identity_field(
                    "expected_key",
                    failure.expected_key(),
                ),
                crate::fact::diagnostic_context::identity_field("actual_key", failure.actual_key()),
            ];

            crate::fact::push_mir_target_contract(
                &mut fields,
                crate::fact::TargetContractSide::Expected,
                failure.expected_target(),
            );

            crate::fact::push_mir_target_contract(
                &mut fields,
                crate::fact::TargetContractSide::Actual,
                failure.actual_target(),
            );

            ("imported_query_executable_template_mismatch", fields)
        }
        Error::InterfaceCapacityExceeded(index) => (
            "imported_query_interface_capacity_exceeded",
            vec![crate::fact::diagnostic_context::natural_field(
                "index", *index,
            )],
        ),
    };

    bray_diagnostics::DiagnosticSemanticQueryFailure::new("imported_query", reason, context)
}

fn imported_record_context(
    key: crate::fact::ImportedSemanticRecordKey,
) -> Vec<bray_diagnostics::DiagnosticFailureField> {
    vec![
        crate::fact::diagnostic_context::count_field("interface", u64::from(key.interface().raw())),
        crate::fact::diagnostic_context::count_field("owner", u64::from(key.owner().raw())),
        crate::fact::diagnostic_context::text_field("record_kind", key.kind().as_str()),
    ]
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
    use bray_package_interface::InterfaceSemanticRecordKind;
    use bray_symbols::{
        ImportedInterfaceId, InterfaceSymbolId, PackageIdentity, ProductIdentity,
    };

    use super::{diagnostic_evaluation_failure, diagnostic_imported_query_failure};
    use crate::fact::{
        FactQueryError, ImportedExecutableTemplateMismatch, ImportedQueryFailure,
        ImportedSemanticRecordKey,
    };

    #[test]
    fn imported_query_conversion_preserves_interface_owner_and_record_kind() {
        let key = ImportedSemanticRecordKey::new(
            ImportedInterfaceId::new(7),
            InterfaceSymbolId::new(11),
            InterfaceSemanticRecordKind::CallableSignature,
        );

        let failure =
            diagnostic_imported_query_failure(&ImportedQueryFailure::MissingInterfaceSymbol(key));

        let names: Vec<_> = failure.context().iter().map(|field| field.name()).collect();

        assert_eq!(failure.category(), "imported_query");
        assert_eq!(failure.reason(), "imported_query_missing_interface_symbol");
        assert_eq!(names, ["interface", "owner", "record_kind"]);
    }

    #[test]
    fn imported_query_conversion_preserves_missing_loaded_interface_identity() {
        let failure = diagnostic_imported_query_failure(
            &ImportedQueryFailure::MissingLoadedImplementation(ImportedInterfaceId::new(23)),
        );

        assert_eq!(
            failure.reason(),
            "imported_query_missing_loaded_implementation"
        );

        assert_eq!(
            failure.context()[0].value(),
            &DiagnosticFailureValue::Count(23)
        );
    }

    #[test]
    fn imported_template_mismatch_conversion_preserves_the_real_payload() {
        let package = PackageIdentity::try_new("example")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let expected_product = ProductIdentity::try_new(package.clone(), "expected")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let actual_product = ProductIdentity::try_new(package, "actual")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let expected_target = bray_ir::MirTargetContract::new(
            bray_target::test_support::test_target_profile(),
            bray_runtime_interface::RuntimeAbiVersion::new(3, 7),
        );

        let actual_target = bray_ir::MirTargetContract::new(
            bray_target::NativeTarget::X86_64WindowsMsvc.profile(),
            bray_runtime_interface::RuntimeAbiVersion::new(4, 1),
        );

        let error = ImportedQueryFailure::ExecutableTemplateMismatch(Box::new(
            ImportedExecutableTemplateMismatch::new(
                ImportedInterfaceId::new(7),
                InterfaceSymbolId::new(11),
                bray_ir::MirExecutableTemplateId::ROOT,
                bray_ir::MirUnitId::new(17),
                bray_ir::MirUnitId::new(29),
                bray_ir::MirUnitKey::ExecutableHost(expected_product),
                bray_ir::MirUnitKey::ExecutableHost(actual_product),
                expected_target,
                actual_target,
            ),
        ));

        let failure = diagnostic_imported_query_failure(&error);
        let fields = failure.context();
        let names: Vec<_> = fields.iter().map(|field| field.name()).collect();

        assert_eq!(failure.category(), "imported_query");

        assert_eq!(
            failure.reason(),
            "imported_query_executable_template_mismatch"
        );

        assert_eq!(
            &names[..7],
            [
                "interface",
                "symbol",
                "template",
                "expected_unit",
                "actual_unit",
                "expected_key",
                "actual_key",
            ]
        );

        assert_eq!(fields[0].value(), &DiagnosticFailureValue::Count(7));
        assert_eq!(fields[1].value(), &DiagnosticFailureValue::Count(11));
        assert_eq!(fields[2].value(), &DiagnosticFailureValue::Count(0));
        assert_eq!(fields[3].value(), &DiagnosticFailureValue::Count(17));
        assert_eq!(fields[4].value(), &DiagnosticFailureValue::Count(29));
        assert_ne!(fields[5].value(), fields[6].value());

        for expected in [
            "expected_target_identity",
            "expected_runtime_abi_major",
            "expected_runtime_abi_minor",
            "actual_target_identity",
            "actual_runtime_abi_major",
            "actual_runtime_abi_minor",
        ] {
            assert!(names.contains(&expected), "missing target field {expected}");
        }
    }

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
