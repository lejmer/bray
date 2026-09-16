use bray_codegen::{AssemblySyntaxKind, DebugInformationMode, DebugInformationOutputMode};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticAssemblySyntaxKind, DiagnosticBag,
    DiagnosticDebugInformationMode, DiagnosticDebugOutputMode, DiagnosticEmissionFailure,
    DiagnosticEmissionPlanningFailure, DiagnosticId, DiagnosticKind, SeverityKind,
};
use bray_emitter::EmissionPlanningError;
use bray_symbols::ProductIdentity;
use bray_target::TargetIdentity;

use super::common::emission_failure_diagnostic;
use crate::compilation::diagnostics::diagnostic_product_kind;

fn planning_failure(failure: DiagnosticEmissionPlanningFailure) -> DiagnosticEmissionFailure {
    DiagnosticEmissionFailure::Planning(failure)
}

pub(super) fn planning_failure_diagnostics(
    error: &EmissionPlanningError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> DiagnosticBag {
    let diagnostic = match error {
        EmissionPlanningError::TargetMismatch {
            requested,
            selected,
        } => {
            return DiagnosticBag::single(
                Diagnostic::new(
                    DiagnosticId::new(0),
                    DiagnosticKind::EmissionTargetMismatch,
                    SeverityKind::Error,
                )
                .with_arg(DiagnosticArg::actual_product_identity(product.to_string()))
                .with_arg(DiagnosticArg::actual_target_triple(requested.as_str()))
                .with_arg(DiagnosticArg::expected_target_triple(selected.as_str())),
            );
        }
        EmissionPlanningError::ProductArtifactMismatch {
            product: kind,
            artifact,
        } => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::ProductArtifactMismatch {
                product: diagnostic_product_kind(*kind),
                artifact: artifact.diagnostic_kind(),
            }),
            product,
            target,
        ),
        EmissionPlanningError::MissingPackageInterfaceArtifact => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::MissingPackageInterfaceArtifact),
            product,
            target,
        ),
        EmissionPlanningError::UnexpectedPackageInterfaceArtifact => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::UnexpectedPackageInterfaceArtifact),
            product,
            target,
        ),
        EmissionPlanningError::PackageInterfaceProductMismatch { expected, actual } => {
            emission_failure_diagnostic(
                planning_failure(
                    DiagnosticEmissionPlanningFailure::PackageInterfaceProductMismatch {
                        expected: expected.to_string(),
                        actual: format!(
                            "{}/{}",
                            actual.package().as_str(),
                            actual.product().as_str(),
                        ),
                    },
                ),
                product,
                target,
            )
        }
        EmissionPlanningError::MissingLinkedProduct => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::MissingLinkedProduct),
            product,
            target,
        ),
        EmissionPlanningError::MultipleLinkedProducts => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::MultipleLinkedProducts),
            product,
            target,
        ),
        EmissionPlanningError::LinkedCompanionRequirementMismatch => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::LinkedCompanionRequirementMismatch),
            product,
            target,
        ),
        EmissionPlanningError::MissingBackend => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::MissingBackend),
            product,
            target,
        ),
        EmissionPlanningError::MissingCodegenUnits => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::MissingCodegenUnits),
            product,
            target,
        ),
        EmissionPlanningError::UnsupportedBackendTarget(unsupported) => {
            emission_failure_diagnostic(
                planning_failure(DiagnosticEmissionPlanningFailure::UnsupportedBackendTarget(
                    unsupported.as_str().to_owned(),
                )),
                product,
                target,
            )
        }
        EmissionPlanningError::UnsupportedBackendArtifact(artifact) => emission_failure_diagnostic(
            planning_failure(
                DiagnosticEmissionPlanningFailure::UnsupportedBackendArtifact(
                    artifact.diagnostic_kind(),
                ),
            ),
            product,
            target,
        ),
        EmissionPlanningError::UnsupportedDebugInformation(mode) => emission_failure_diagnostic(
            planning_failure(
                DiagnosticEmissionPlanningFailure::UnsupportedDebugInformation(
                    diagnostic_debug_information_mode(*mode),
                ),
            ),
            product,
            target,
        ),
        EmissionPlanningError::UnsupportedDebugOutput(mode) => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::UnsupportedDebugOutput(
                diagnostic_debug_output_mode(*mode),
            )),
            product,
            target,
        ),
        EmissionPlanningError::UnsupportedAssemblySyntax(syntax) => emission_failure_diagnostic(
            planning_failure(
                DiagnosticEmissionPlanningFailure::UnsupportedAssemblySyntax(
                    diagnostic_assembly_syntax(*syntax),
                ),
            ),
            product,
            target,
        ),
        EmissionPlanningError::InvalidDebugOutput {
            information,
            output,
        } => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::InvalidDebugOutput {
                information: diagnostic_debug_information_mode(*information),
                output: diagnostic_debug_output_mode(*output),
            }),
            product,
            target,
        ),
        EmissionPlanningError::MissingRequiredDebugCompanion => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::MissingRequiredDebugCompanion),
            product,
            target,
        ),
        EmissionPlanningError::UnexpectedDebugCompanion => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::UnexpectedDebugCompanion),
            product,
            target,
        ),
        EmissionPlanningError::MissingLinkableArtifact => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::MissingLinkableArtifact),
            product,
            target,
        ),
        EmissionPlanningError::MissingSerializationArtifact(artifact) => {
            emission_failure_diagnostic(
                planning_failure(
                    DiagnosticEmissionPlanningFailure::MissingSerializationArtifact(
                        artifact.diagnostic_kind(),
                    ),
                ),
                product,
                target,
            )
        }
        EmissionPlanningError::MissingOutputName(artifact) => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::MissingOutputName(
                artifact.diagnostic_kind(),
            )),
            product,
            target,
        ),
        EmissionPlanningError::InvalidProductName => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::InvalidProductName),
            product,
            target,
        ),
        EmissionPlanningError::InvalidGeneratedFileName(artifact) => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::InvalidGeneratedFileName(
                artifact.diagnostic_kind(),
            )),
            product,
            target,
        ),
        EmissionPlanningError::MultipleArtifactsForSingleSink => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::MultipleArtifactsForSingleSink),
            product,
            target,
        ),
        EmissionPlanningError::MissingExplicitFileName => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::MissingExplicitFileName),
            product,
            target,
        ),
        EmissionPlanningError::InvalidExplicitFileName => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::InvalidExplicitFileName),
            product,
            target,
        ),
        EmissionPlanningError::ExplicitOutputSuffixMismatch(artifact) => {
            emission_failure_diagnostic(
                planning_failure(
                    DiagnosticEmissionPlanningFailure::ExplicitOutputSuffixMismatch(
                        artifact.diagnostic_kind(),
                    ),
                ),
                product,
                target,
            )
        }
        EmissionPlanningError::ManagedProductDestinationRequired => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::ManagedProductDestinationRequired),
            product,
            target,
        ),
        EmissionPlanningError::ArtifactOrdinalOverflow(artifact) => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::ArtifactOrdinalOverflow(
                artifact.diagnostic_kind(),
            )),
            product,
            target,
        ),
        EmissionPlanningError::OutputCollision(sink) => emission_failure_diagnostic(
            planning_failure(DiagnosticEmissionPlanningFailure::OutputCollision(
                sink.diagnostic_sink(),
            )),
            product,
            target,
        ),
    };

    DiagnosticBag::single(diagnostic)
}

const fn diagnostic_debug_information_mode(
    kind: DebugInformationMode,
) -> DiagnosticDebugInformationMode {
    match kind {
        DebugInformationMode::None => DiagnosticDebugInformationMode::None,
        DebugInformationMode::LineTables => DiagnosticDebugInformationMode::LineTables,
        DebugInformationMode::Full => DiagnosticDebugInformationMode::Full,
    }
}

const fn diagnostic_debug_output_mode(
    kind: DebugInformationOutputMode,
) -> DiagnosticDebugOutputMode {
    match kind {
        DebugInformationOutputMode::Omit => DiagnosticDebugOutputMode::Omit,
        DebugInformationOutputMode::Embedded => DiagnosticDebugOutputMode::Embedded,
        DebugInformationOutputMode::Separate => DiagnosticDebugOutputMode::Separate,
    }
}

const fn diagnostic_assembly_syntax(kind: AssemblySyntaxKind) -> DiagnosticAssemblySyntaxKind {
    match kind {
        AssemblySyntaxKind::TargetDefault => DiagnosticAssemblySyntaxKind::TargetDefault,
        AssemblySyntaxKind::Intel => DiagnosticAssemblySyntaxKind::Intel,
        AssemblySyntaxKind::Att => DiagnosticAssemblySyntaxKind::Att,
    }
}
