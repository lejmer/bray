use bray_diagnostics::{
    Diagnostic, DiagnosticBag, DiagnosticEmissionCodegenFailure, DiagnosticEmissionFailure,
    DiagnosticFailureField, DiagnosticFailureValue, DiagnosticIoErrorKind,
    DiagnosticPackageInterfaceFailure,
};
use bray_emitter::BackendContributionMergeErrorKind;
use bray_symbols::ProductIdentity;
use bray_target::TargetIdentity;

use super::common::{
    codegen_unit_identity, diagnostic_backend_artifact, emission_failure_diagnostic,
    emission_failure_diagnostics,
};
use super::evaluation::{diagnostic_evaluation_failure, query_failure_diagnostics};
use super::model::ProductEmissionErrorKind;
use crate::compilation::product::{
    codegen_preparation_failure_kind, native_product_preparation_diagnostic,
};
use crate::compilation::{
    CodegenPreparationError, EmissionCodegenError, EmissionCodegenErrorKind,
    PackageInterfaceExportError,
};
use crate::fact::FactQueryError;

pub(super) fn codegen_failure_diagnostics(
    error: &EmissionCodegenError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> DiagnosticBag {
    let failure = match error.kind() {
        EmissionCodegenErrorKind::Request { unit: _, error } => {
            match error.as_ref() {
                CodegenPreparationError::Diagnostics(diagnostics) => return diagnostics.clone(),
                CodegenPreparationError::Query(error) => {
                    return query_failure_diagnostics(error, product, target);
                }
                _ => {}
            }

            let failure = codegen_preparation_failure_kind(error)
                .unwrap_or_else(|| unreachable!("query and diagnostic failures return above"));

            return DiagnosticBag::single(native_product_preparation_diagnostic(
                failure,
                product,
                target.as_str(),
            ));
        }
        EmissionCodegenErrorKind::Generation { unit, .. } => {
            DiagnosticEmissionCodegenFailure::Generation(codegen_unit_identity(unit))
        }
        EmissionCodegenErrorKind::Cancelled(_) => return DiagnosticBag::new(),
        EmissionCodegenErrorKind::Query(error) => {
            return query_failure_diagnostics(error, product, target);
        }
        EmissionCodegenErrorKind::InvalidContributions(error) => {
            return contribution_merge_failure_diagnostics(error.kind(), product, target);
        }
    };

    emission_failure_diagnostics(DiagnosticEmissionFailure::Codegen(failure), product, target)
}

fn contribution_merge_failure_diagnostics(
    error: &BackendContributionMergeErrorKind,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> DiagnosticBag {
    let failure = match error {
        BackendContributionMergeErrorKind::Cancelled => return DiagnosticBag::new(),
        BackendContributionMergeErrorKind::Read { artifact, kind } => {
            DiagnosticEmissionCodegenFailure::MergeReadFailed {
                artifact: diagnostic_backend_artifact(artifact),
                error: DiagnosticIoErrorKind::from(*kind),
            }
        }
        BackendContributionMergeErrorKind::LengthMismatch {
            artifact,
            expected,
            actual,
        } => DiagnosticEmissionCodegenFailure::MergeLengthMismatch {
            artifact: diagnostic_backend_artifact(artifact),
            expected: *expected,
            actual: *actual,
        },
        BackendContributionMergeErrorKind::DigestMismatch {
            artifact,
            expected,
            actual,
        } => DiagnosticEmissionCodegenFailure::MergeDigestMismatch {
            artifact: diagnostic_backend_artifact(artifact),
            expected: expected.diagnostic_digest(),
            actual: actual.diagnostic_digest(),
        },
        BackendContributionMergeErrorKind::InvalidContent(artifact) => {
            DiagnosticEmissionCodegenFailure::MergeInvalidContent(diagnostic_backend_artifact(
                artifact,
            ))
        }
    };

    emission_failure_diagnostics(DiagnosticEmissionFailure::Codegen(failure), product, target)
}

pub(super) fn package_interface_failure_diagnostics(
    kind: &ProductEmissionErrorKind,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Option<DiagnosticBag> {
    let diagnostic = match kind {
        ProductEmissionErrorKind::PackageInterfaceUnavailable => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::Unavailable,
            product,
            target,
        ),
        ProductEmissionErrorKind::PackageInterface(error) => {
            package_interface_export_failure_diagnostic(error, product, target)?
        }
        ProductEmissionErrorKind::PackageImplementationContent(_) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::ImplementationContentTooLarge,
            product,
            target,
        ),
        ProductEmissionErrorKind::PackageInterfaceEncoding(_) => return None,
        _ => return None,
    };

    Some(DiagnosticBag::single(diagnostic))
}

fn package_failure_diagnostic(
    failure: DiagnosticPackageInterfaceFailure,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    emission_failure_diagnostic(
        DiagnosticEmissionFailure::PackageInterface(failure),
        product,
        target,
    )
}

fn package_interface_export_failure_diagnostic(
    error: &PackageInterfaceExportError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Option<Diagnostic> {
    let diagnostic = match error {
        PackageInterfaceExportError::Cancelled
        | PackageInterfaceExportError::Query(FactQueryError::Cancelled)
        | PackageInterfaceExportError::ConstantCallableEvaluation {
            cause: FactQueryError::Cancelled,
            ..
        }
        | PackageInterfaceExportError::ExecutableTemplateEvaluation {
            cause: FactQueryError::Cancelled,
            ..
        }
        | PackageInterfaceExportError::FragmentCoordination(FactQueryError::Cancelled) => {
            return None;
        }
        PackageInterfaceExportError::Query(error) => emission_failure_diagnostic(
            DiagnosticEmissionFailure::Evaluation(diagnostic_evaluation_failure(error)),
            product,
            target,
        ),
        PackageInterfaceExportError::SourceDiagnostic(diagnostic) => {
            // The emitted diagnostic must outlive the cached export failure.
            diagnostic.as_ref().clone()
        }
        PackageInterfaceExportError::InvalidCompilation => package_failure_diagnostic(
            // rust-style: allow(context-erasing-failure-conversion, reason = "the package-interface diagnostic bag retains the exact causes")
            DiagnosticPackageInterfaceFailure::InvalidCompilation,
            product,
            target,
        ),
        PackageInterfaceExportError::InvalidCompilationCause(cause) => package_failure_diagnostic(
            package_interface_invalid_compilation_cause(cause),
            product,
            target,
        ),
        PackageInterfaceExportError::SemanticValueStoreCreate(_) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::SemanticValueStoreCreate,
            product,
            target,
        ),
        PackageInterfaceExportError::SemanticValueStore(error) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::SemanticValue(
                crate::fact::diagnostic_semantic_value_failure(*error),
            ),
            product,
            target,
        ),
        PackageInterfaceExportError::RecoveredPublicSymbol(kind) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::RecoveredPublicSymbol(kind.as_str().to_owned()),
            product,
            target,
        ),
        PackageInterfaceExportError::ConstantCallableEvaluation { declaration, cause } => {
            let cause = diagnostic_evaluation_failure(cause);

            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::ConstantCallableEvaluation {
                    declaration: declaration.clone(),
                    cause,
                },
                product,
                target,
            )
        }
        PackageInterfaceExportError::ExecutableTemplateEvaluation { declaration, cause } => {
            let cause = diagnostic_evaluation_failure(cause);

            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::ExecutableTemplateEvaluation {
                    declaration: declaration.clone(),
                    cause,
                },
                product,
                target,
            )
        }
        PackageInterfaceExportError::FragmentCoordination(error) => {
            let cycle: Box<[String]> = match error {
                FactQueryError::Cycle(cycle) => cycle
                    .facts()
                    .iter()
                    .map(|fact| {
                        crate::profile::ProfileQueryKind::from_key(fact)
                            .name()
                            .to_owned()
                    })
                    .collect(),
                _ => Box::new([]),
            };

            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::DeclarationDiscoveryFailure {
                    cause: diagnostic_evaluation_failure(error),
                    cycle,
                },
                product,
                target,
            )
        }
    };

    Some(diagnostic)
}

fn package_interface_invalid_compilation_cause(
    cause: &crate::compilation::PackageInterfaceInvalidCompilationCause,
) -> DiagnosticPackageInterfaceFailure {
    use crate::compilation::PackageInterfaceInvalidCompilationCause as Cause;

    let (reason, context): (&'static str, Box<[DiagnosticFailureField]>) = match cause {
        Cause::Capacity { field, actual } => (
            "invalid_compilation_capacity",
            vec![
                DiagnosticFailureField::new(
                    "field",
                    DiagnosticFailureValue::Text((*field).to_owned()),
                ),
                DiagnosticFailureField::new("actual", DiagnosticFailureValue::Text(actual.clone())),
            ]
            .into_boxed_slice(),
        ),
        Cause::RuntimeRequirements(cause) => (
            match cause {
                bray_runtime_interface::RuntimeRequirementsMergeError::RuntimeIdentityMismatch => {
                    "invalid_compilation_runtime_identity_mismatch"
                }
                bray_runtime_interface::RuntimeRequirementsMergeError::RuntimeAbiMismatch => {
                    "invalid_compilation_runtime_abi_mismatch"
                }
                bray_runtime_interface::RuntimeRequirementsMergeError::FrameAbiMismatch(_) => {
                    "invalid_compilation_runtime_frame_abi_mismatch"
                }
                bray_runtime_interface::RuntimeRequirementsMergeError::TargetMismatch => {
                    "invalid_compilation_runtime_target_mismatch"
                }
                bray_runtime_interface::RuntimeRequirementsMergeError::PanicAbiMismatch => {
                    "invalid_compilation_runtime_panic_abi_mismatch"
                }
            },
            match cause {
                bray_runtime_interface::RuntimeRequirementsMergeError::FrameAbiMismatch(
                    operation,
                ) => vec![DiagnosticFailureField::new(
                    "operation",
                    DiagnosticFailureValue::Text(format!("{operation:?}")),
                )]
                .into_boxed_slice(),
                _ => Vec::new().into_boxed_slice(),
            },
        ),
        Cause::ExportRequestMismatch => (
            "invalid_compilation_export_contract",
            vec![DiagnosticFailureField::new(
                "contract",
                DiagnosticFailureValue::Text("request_mismatch".to_owned()),
            )]
            .into_boxed_slice(),
        ),
    };

    DiagnosticPackageInterfaceFailure::InvalidCompilationCause { reason, context }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArgValue, DiagnosticEmissionEvaluationFailure, DiagnosticEmissionFailure,
        DiagnosticInterfaceSymbolIdentity, DiagnosticPackageInterfaceFailure,
    };

    use bray_symbols::{PackageIdentity, ProductIdentity};
    use bray_target::TargetIdentity;

    use super::super::evaluation::diagnostic_evaluation_failure;
    use super::package_interface_export_failure_diagnostic;
    use crate::compilation::PackageInterfaceExportError;
    use crate::fact::{FactQueryError, FactRuntimeFailure};

    fn identities() -> (ProductIdentity, TargetIdentity) {
        let package = PackageIdentity::try_new("example.package")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = ProductIdentity::try_new(package, "library")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let target = TargetIdentity::try_new("test-target")
            .unwrap_or_else(|| panic!("test target identity must be valid"));

        (product, target)
    }

    #[test]
    fn cancellation_does_not_produce_a_user_diagnostic() {
        let (product, target) = identities();

        let declaration = DiagnosticInterfaceSymbolIdentity::Package("example.package".to_owned());

        let errors = [
            PackageInterfaceExportError::Cancelled,
            PackageInterfaceExportError::Query(FactQueryError::Cancelled),
            PackageInterfaceExportError::ConstantCallableEvaluation {
                declaration: declaration.clone(),
                cause: FactQueryError::Cancelled,
            },
            PackageInterfaceExportError::ExecutableTemplateEvaluation {
                declaration,
                cause: FactQueryError::Cancelled,
            },
            PackageInterfaceExportError::FragmentCoordination(FactQueryError::Cancelled),
        ];

        for error in errors {
            assert!(
                package_interface_export_failure_diagnostic(&error, &product, &target).is_none()
            );
        }
    }

    #[test]
    fn package_interface_query_coordination_uses_the_evaluation_diagnostic_boundary() {
        let (product, target) = identities();

        let error = PackageInterfaceExportError::Query(FactQueryError::from(
            FactRuntimeFailure::UnitSourceCapacityExhausted {
                source_count: usize::MAX,
            },
        ));

        assert!(package_interface_export_failure_diagnostic(&error, &product, &target).is_some());
    }

    #[test]
    fn terminal_evaluation_failures_retain_mir_capacity() {
        assert_eq!(
            diagnostic_evaluation_failure(&FactQueryError::MirCapacity(
                bray_ir::MirCapacityError::IdentityCapacityExceeded,
            )),
            DiagnosticEmissionEvaluationFailure::MirCapacity
        );
    }

    #[test]
    fn package_evaluation_mir_capacity_failures_do_not_invent_source_context() {
        let (product, target) = identities();

        let capacity_failure =
            || FactQueryError::MirCapacity(bray_ir::MirCapacityError::IdentityCapacityExceeded);

        let errors = [
            PackageInterfaceExportError::ConstantCallableEvaluation {
                declaration: DiagnosticInterfaceSymbolIdentity::Package(
                    "example.package".to_owned(),
                ),
                cause: capacity_failure(),
            },
            PackageInterfaceExportError::ExecutableTemplateEvaluation {
                declaration: DiagnosticInterfaceSymbolIdentity::Package(
                    "example.package".to_owned(),
                ),
                cause: capacity_failure(),
            },
        ];

        let expected_cause = DiagnosticEmissionEvaluationFailure::MirCapacity;

        for error in errors {
            let diagnostic = package_interface_export_failure_diagnostic(&error, &product, &target)
                .unwrap_or_else(|| panic!("package evaluation failure must diagnose"));

            assert_eq!(diagnostic.primary_span(), None);

            assert!(diagnostic.labels().is_empty());

            assert!(diagnostic.args().iter().any(|arg| {
                let DiagnosticArgValue::EmissionFailure(
                    DiagnosticEmissionFailure::PackageInterface(failure),
                ) = arg.value()
                else {
                    return false;
                };

                match failure {
                    DiagnosticPackageInterfaceFailure::ConstantCallableEvaluation {
                        cause, ..
                    }
                    | DiagnosticPackageInterfaceFailure::ExecutableTemplateEvaluation {
                        cause,
                        ..
                    } => cause == &expected_cause,
                    _ => false,
                }
            }));
        }
    }
}
