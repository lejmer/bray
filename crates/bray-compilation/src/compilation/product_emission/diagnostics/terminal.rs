use bray_diagnostics::{
    Diagnostic, DiagnosticBag, DiagnosticEmissionCodegenFailure, DiagnosticEmissionFailure,
    DiagnosticFailureField,
    DiagnosticFailureValue, DiagnosticIoErrorKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticNote, DiagnosticNoteKind, DiagnosticPackageInterfaceFailure,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind,
};
use bray_emitter::BackendContributionMergeErrorKind;
use bray_package_interface::{
    InterfaceSemanticCommitError, PackageImplementationArtifactBuildError,
    PackageInterfaceExportBuildError, PackageInterfaceExportSurfaceError,
    PackageInterfaceSurfaceBuildError,
};
use bray_symbols::{ProductIdentity, diagnostic_external_symbol_identity};
use bray_target::TargetIdentity;

use super::common::{
    codegen_unit_identity, diagnostic_backend_artifact, emission_failure_diagnostic,
    emission_failure_diagnostics,
};
use super::evaluation::{diagnostic_evaluation_failure, query_failure_diagnostics};
use super::model::ProductEmissionErrorKind;
use crate::compilation::diagnostics::diagnostic_interface_symbol_reference;
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
        EmissionCodegenErrorKind::DuplicateUnit(unit) => {
            DiagnosticEmissionCodegenFailure::DuplicateUnit(codegen_unit_identity(unit))
        }
        EmissionCodegenErrorKind::DuplicateMappings(unit) => {
            DiagnosticEmissionCodegenFailure::DuplicateMappings(codegen_unit_identity(unit))
        }
        EmissionCodegenErrorKind::MissingUnit(unit) => {
            DiagnosticEmissionCodegenFailure::MissingUnit(codegen_unit_identity(unit))
        }
        EmissionCodegenErrorKind::MissingMappings(unit) => {
            DiagnosticEmissionCodegenFailure::MissingMappings(codegen_unit_identity(unit))
        }
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
        ProductEmissionErrorKind::PackageImplementation(error) => {
            package_implementation_failure_diagnostic(error, product, target)?
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
        PackageInterfaceExportError::ExecutionEvidence(diagnostic) => {
            // The emitted diagnostic must outlive the cached export failure.
            diagnostic.as_ref().clone()
        }
        PackageInterfaceExportError::InvalidCompilation => package_failure_diagnostic(
            // rust-style: allow(context-erasing-failure-conversion, reason = "the package-interface diagnostic bag retains the exact causes")
            DiagnosticPackageInterfaceFailure::InvalidCompilation,
            product,
            target,
        ),
        PackageInterfaceExportError::InvalidCompilationCause(cause) => {
            package_compiler_defect_diagnostic(
                package_interface_invalid_compilation_cause(cause),
                product,
                target,
            )
        }
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
        PackageInterfaceExportError::IncompletePublicDeclarationSemantics(kind) => {
            package_compiler_defect_diagnostic(
                DiagnosticPackageInterfaceFailure::IncompletePublicDeclaration(
                    kind.as_str().to_owned(),
                ),
                product,
                target,
            )
        }
        PackageInterfaceExportError::IncompleteSemanticFragment {
            declaration,
            table,
            reference,
        } => {
            // The emitted diagnostic remains valid after the product error is released.
            package_compiler_defect_diagnostic(
                DiagnosticPackageInterfaceFailure::LostDeclarationReference {
                    declaration: declaration.clone(),
                    table: table.as_str().to_owned(),
                    reference: *reference,
                },
                product,
                target,
            )
        }
        PackageInterfaceExportError::MissingSemanticFragmentSymbol(identity) => {
            package_compiler_defect_diagnostic(
                DiagnosticPackageInterfaceFailure::MissingDeclarationData(
                    diagnostic_external_symbol_identity(identity),
                ),
                product,
                target,
            )
        }
        PackageInterfaceExportError::ConflictingSemanticFragment {
            first,
            second,
            first_span,
            second_span,
            identity,
        } => {
            // The emitted diagnostic remains valid after the product error is released.
            let diagnostic = package_compiler_defect_diagnostic(
                DiagnosticPackageInterfaceFailure::DuplicateDeclarationIdentity {
                    first: first.clone(),
                    second: second.clone(),
                    identity: diagnostic_external_symbol_identity(identity),
                },
                product,
                target,
            );

            with_duplicate_declaration_locations(diagnostic, *first_span, *second_span)
        }
        PackageInterfaceExportError::CyclicSemanticFragment {
            declaration,
            table,
            reference,
        } => {
            // The emitted diagnostic remains valid after the product error is released.
            package_compiler_defect_diagnostic(
                DiagnosticPackageInterfaceFailure::RecursiveDeclarationData {
                    declaration: declaration.clone(),
                    table: table.as_str().to_owned(),
                    reference: *reference,
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

            package_compiler_defect_diagnostic(
                DiagnosticPackageInterfaceFailure::DeclarationDiscoveryFailure {
                    cause: diagnostic_evaluation_failure(error),
                    cycle,
                },
                product,
                target,
            )
        }
        PackageInterfaceExportError::FragmentCommit(error) => {
            package_interface_fragment_failure_diagnostic(error, product, target)
        }
        PackageInterfaceExportError::Surface(error) => {
            package_interface_surface_failure_diagnostic(error, product, target)
        }
        PackageInterfaceExportError::Bundle(error) => {
            package_interface_bundle_failure_diagnostic(error, product, target)?
        }
    };

    Some(diagnostic)
}

fn package_interface_invalid_compilation_cause(
    cause: &crate::compilation::PackageInterfaceInvalidCompilationCause,
) -> DiagnosticPackageInterfaceFailure {
    use crate::compilation::PackageInterfaceInvalidCompilationCause as Cause;

    let (reason, context): (&'static str, Box<[DiagnosticFailureField]>) = match cause {
        Cause::CodegenTarget(cause) => (
            match cause {
                bray_codegen::CodegenTargetBuildError::UnsupportedProfile => {
                    "invalid_compilation_codegen_target_unsupported_profile"
                }
                bray_codegen::CodegenTargetBuildError::EmptyTriple => {
                    "invalid_compilation_codegen_target_empty_triple"
                }
                bray_codegen::CodegenTargetBuildError::EmptyCpu => {
                    "invalid_compilation_codegen_target_empty_cpu"
                }
                bray_codegen::CodegenTargetBuildError::EmptyFeature => {
                    "invalid_compilation_codegen_target_empty_feature"
                }
            },
            Vec::new().into_boxed_slice(),
        ),
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
        Cause::CheckedTemplate { reason } => (
            "invalid_compilation_checked_template",
            vec![DiagnosticFailureField::new(
                "cause",
                DiagnosticFailureValue::Text((*reason).to_owned()),
            )]
            .into_boxed_slice(),
        ),
        Cause::ExecutableTemplate { reason } => (
            "invalid_compilation_executable_template",
            vec![DiagnosticFailureField::new(
                "cause",
                DiagnosticFailureValue::Text((*reason).to_owned()),
            )]
            .into_boxed_slice(),
        ),
        Cause::ExportContract(contract) => (
            "invalid_compilation_export_contract",
            vec![DiagnosticFailureField::new(
                "contract",
                DiagnosticFailureValue::Text(contract.name().to_owned()),
            )]
            .into_boxed_slice(),
        ),
    };

    DiagnosticPackageInterfaceFailure::InvalidCompilationCause { reason, context }
}

fn package_compiler_defect_diagnostic(
    failure: DiagnosticPackageInterfaceFailure,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    package_failure_diagnostic(failure, product, target).with_note(DiagnosticNote::new(
            DiagnosticNoteKind::ReportCompilerDefect,
        ))
}

fn with_duplicate_declaration_locations(
    mut diagnostic: Diagnostic,
    first: Option<bray_source::SourceSpan>,
    second: Option<bray_source::SourceSpan>,
) -> Diagnostic {
    let primary = second.or(first);

    if let Some(primary) = primary {
        diagnostic = diagnostic
            .with_primary_span(primary)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::DuplicateDeclaration,
                primary,
            ));
    }

    if let Some(first) = first
        && Some(first) != primary
    {
        diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
            DiagnosticRelatedLocationKind::FirstDeclaration,
            first,
        ));
    }

    diagnostic
}

fn package_interface_fragment_failure_diagnostic(
    error: &InterfaceSemanticCommitError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    let failure = match error {
        InterfaceSemanticCommitError::MissingReference { table, reference } => {
            DiagnosticPackageInterfaceFailure::MissingPackageReference {
                table: table.as_str().to_owned(),
                reference: *reference,
            }
        }
        InterfaceSemanticCommitError::UnexpectedPackageRecord(table) => {
            DiagnosticPackageInterfaceFailure::MisassignedPackageRecord(table.as_str().to_owned())
        }
        InterfaceSemanticCommitError::ConflictingRecord { owner, kind } => {
            DiagnosticPackageInterfaceFailure::ConflictingDeclarationRecord {
                kind: kind.as_str().to_owned(),
                owner: diagnostic_interface_symbol_reference(owner),
            }
        }
        InterfaceSemanticCommitError::CyclicReference(table) => {
            DiagnosticPackageInterfaceFailure::RecursiveDeclarationReference(
                table.as_str().to_owned(),
            )
        }
        InterfaceSemanticCommitError::IdentityOverflow(table) => {
            DiagnosticPackageInterfaceFailure::SemanticTableOverflow {
                table: table.as_str().to_owned(),
                maximum: u32::MAX,
            }
        }
    };

    package_compiler_defect_diagnostic(failure, product, target)
}

fn package_interface_surface_failure_diagnostic(
    error: &PackageInterfaceExportSurfaceError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    match error {
        PackageInterfaceExportSurfaceError::DuplicateSymbol(key) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::DuplicateSymbol(key.kind().as_str().to_owned()),
            product,
            target,
        ),
        PackageInterfaceExportSurfaceError::MissingSymbol(key) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::MissingSymbol(key.kind().as_str().to_owned()),
            product,
            target,
        ),
        PackageInterfaceExportSurfaceError::SymbolCountOverflow => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::SymbolCountOverflow,
            product,
            target,
        ),
        PackageInterfaceExportSurfaceError::Surface(error) => {
            package_interface_structural_failure_diagnostic(error, product, target)
        }
    }
}

fn package_interface_bundle_failure_diagnostic(
    error: &PackageInterfaceExportBuildError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Option<Diagnostic> {
    let failure = match error {
        PackageInterfaceExportBuildError::MissingSemantics(key) => {
            return Some(package_compiler_defect_diagnostic(
                DiagnosticPackageInterfaceFailure::MissingDeclarationData(
                    diagnostic_external_symbol_identity(key),
                ),
                product,
                target,
            ));
        }
        PackageInterfaceExportBuildError::Validation(_) => return None,
        PackageInterfaceExportBuildError::DuplicateConstantCallableBody(owner) => {
            DiagnosticPackageInterfaceFailure::DuplicateConstantCallableBody(owner.raw())
        }
        PackageInterfaceExportBuildError::DuplicateExecutableTemplate(owner) => {
            DiagnosticPackageInterfaceFailure::DuplicateExecutableTemplate(owner.raw())
        }
        PackageInterfaceExportBuildError::InvalidExecutableTemplateFamily(owner) => {
            DiagnosticPackageInterfaceFailure::InvalidExecutableTemplateFamily(owner.raw())
        }
        PackageInterfaceExportBuildError::DuplicateNativeBoundary(owner) => {
            DiagnosticPackageInterfaceFailure::DuplicateNativeBoundary(owner.raw())
        }
    };

    Some(package_failure_diagnostic(failure, product, target))
}

fn package_implementation_failure_diagnostic(
    error: &PackageImplementationArtifactBuildError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Option<Diagnostic> {
    let failure = match error {
        PackageImplementationArtifactBuildError::DuplicateCallableBody(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationDuplicateCallableBody(owner.raw())
        }
        PackageImplementationArtifactBuildError::DuplicateExecutableTemplate(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationDuplicateExecutableTemplate(
                owner.raw(),
            )
        }
        PackageImplementationArtifactBuildError::InvalidExecutableTemplateFamily(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationInvalidExecutableTemplateFamily(
                owner.raw(),
            )
        }
        PackageImplementationArtifactBuildError::DuplicateNativeBoundary(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationDuplicateNativeBoundary(owner.raw())
        }
        PackageImplementationArtifactBuildError::DuplicateSpecialization => {
            DiagnosticPackageInterfaceFailure::ImplementationDuplicateSpecialization
        }
        PackageImplementationArtifactBuildError::SpecializationIdentityMismatch => {
            DiagnosticPackageInterfaceFailure::ImplementationSpecializationIdentityMismatch
        }
        PackageImplementationArtifactBuildError::InvalidExecutableOwner(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationInvalidExecutableOwner(owner.raw())
        }
        PackageImplementationArtifactBuildError::InvalidNativeBoundaryOwner(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationInvalidNativeBoundaryOwner(owner.raw())
        }
        PackageImplementationArtifactBuildError::InvalidCallableOwner(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationInvalidCallableOwner(owner.raw())
        }
        PackageImplementationArtifactBuildError::InvalidBody(_)
        | PackageImplementationArtifactBuildError::InvalidArtifact(_) => return None,
    };

    Some(package_failure_diagnostic(failure, product, target))
}

fn package_interface_structural_failure_diagnostic(
    error: &PackageInterfaceSurfaceBuildError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    package_failure_diagnostic(
        DiagnosticPackageInterfaceFailure::SymbolGraph(
            bray_package_interface::diagnostic_surface_problem(error),
        ),
        product,
        target,
    )
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArgValue, DiagnosticEmissionEvaluationFailure, DiagnosticEmissionFailure,
        DiagnosticInterfaceSymbolIdentity, DiagnosticInterfaceSymbolKind, DiagnosticLabelKind,
        DiagnosticNoteKind, DiagnosticPackageInterfaceFailure, DiagnosticRelatedLocationKind,
    };
    use bray_messages::DiagnosticRenderer;
    use bray_package_interface::{InterfaceSemanticCommitError, InterfaceSemanticTableKind};
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};
    use bray_symbols::{ExternalSymbolKey, PackageIdentity, ProductIdentity};
    use bray_target::TargetIdentity;

    use super::super::evaluation::diagnostic_evaluation_failure;
    use super::{
        package_interface_export_failure_diagnostic, package_interface_fragment_failure_diagnostic,
    };
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
            FactRuntimeFailure::WorkerTerminated {
                worker: Some(1),
                item: None,
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

        let capacity_failure = || {
            FactQueryError::MirCapacity(bray_ir::MirCapacityError::IdentityCapacityExceeded)
        };

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

            assert!(!diagnostic
                .labels()
                .iter()
                .any(|label| label.kind() == DiagnosticLabelKind::CompilerDefectSource));

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

    #[test]
    fn compiler_owned_interface_failures_include_reporting_guidance() {
        let (product, target) = identities();

        let diagnostic = package_interface_fragment_failure_diagnostic(
            &InterfaceSemanticCommitError::MissingReference {
                table: InterfaceSemanticTableKind::Type,
                reference: 7,
            },
            &product,
            &target,
        );

        assert!(
            diagnostic
                .notes()
                .iter()
                .any(|note| { note.kind() == DiagnosticNoteKind::ReportCompilerDefect })
        );
    }

    #[test]
    fn duplicate_interface_identities_point_to_both_source_declarations() {
        let (product, target) = identities();

        let first_span = SourceSpan::new(
            SourceId::new(1),
            TextRange::new(TextSize::new(10), TextSize::new(20)),
        );

        let second_span = SourceSpan::new(
            SourceId::new(1),
            TextRange::new(TextSize::new(30), TextSize::new(40)),
        );

        let owner = DiagnosticInterfaceSymbolIdentity::Package("example.package".to_owned());

        let declaration = |ordinal| DiagnosticInterfaceSymbolIdentity::SourceDeclaration {
            owner: Box::new(owner.clone()),
            kind: DiagnosticInterfaceSymbolKind::Function,
            declaration: ordinal,
        };

        let error = PackageInterfaceExportError::ConflictingSemanticFragment {
            first: declaration(1),
            second: declaration(2),
            first_span: Some(first_span),
            second_span: Some(second_span),
            identity: ExternalSymbolKey::package(
                PackageIdentity::try_new("example.shared")
                    .unwrap_or_else(|| panic!("test package identity must be valid")),
            ),
        };

        let diagnostic = package_interface_export_failure_diagnostic(&error, &product, &target)
            .unwrap_or_else(|| panic!("duplicate identities must produce a diagnostic"));

        assert_eq!(diagnostic.primary_span(), Some(second_span));

        assert!(diagnostic.labels().iter().any(|label| {
            label.kind() == DiagnosticLabelKind::DuplicateDeclaration && label.span() == second_span
        }));

        assert!(diagnostic.related_locations().iter().any(|location| {
            location.kind() == DiagnosticRelatedLocationKind::FirstDeclaration
                && location.span() == first_span
        }));

        assert_eq!(
            DiagnosticRenderer::english().render(&diagnostic).message(),
            "cannot emit product 'example.package/library' for target 'test-target': exported declarations 'example.package::function declaration 1' and 'example.package::function declaration 2' both resolve to stable identity 'example.shared'"
        );
    }
}
