use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticEmissionArtifactOperation,
    DiagnosticEmissionFailure, DiagnosticEmissionLinkPlanFailure, DiagnosticEmissionStagingFailure,
    DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind, DiagnosticLinkInputKind,
    DiagnosticLinkedArtifactKind, DiagnosticLinkedProductKind, SeverityKind,
};
use bray_emitter::{LinkPlanConstructionError, LinkStagingError};
use bray_linker::{
    LinkInputBuildError, LinkInputKind, LinkPlanBuildError, LinkedArtifactKind, LinkedProductKind,
    StagingDestinationBuildError,
};
use bray_symbols::ProductIdentity;
use bray_target::TargetIdentity;

use super::common::{
    diagnostic_artifact, emission_artifact_diagnostic, emission_failure_diagnostics,
};

fn link_plan_failure(failure: DiagnosticEmissionLinkPlanFailure) -> DiagnosticEmissionFailure {
    DiagnosticEmissionFailure::LinkPlan(failure)
}

pub(super) fn link_plan_failure_diagnostics(
    error: &LinkPlanConstructionError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> DiagnosticBag {
    let failure = match error {
        LinkPlanConstructionError::MissingLinkedProduct => {
            DiagnosticEmissionLinkPlanFailure::MissingLinkedProduct
        }
        LinkPlanConstructionError::TargetMismatch { planned, selected } => {
            return DiagnosticBag::single(
                Diagnostic::new(
                    DiagnosticId::new(0),
                    DiagnosticKind::EmissionTargetMismatch,
                    SeverityKind::Error,
                )
                .with_arg(DiagnosticArg::actual_product_identity(product.to_string()))
                .with_arg(DiagnosticArg::expected_target_triple(planned.as_str()))
                .with_arg(DiagnosticArg::actual_target_triple(selected.as_str())),
            );
        }
        LinkPlanConstructionError::DuplicateStagedArtifact(artifact) => {
            DiagnosticEmissionLinkPlanFailure::DuplicateStagedArtifact(diagnostic_artifact(
                artifact,
            ))
        }
        LinkPlanConstructionError::MissingStagedArtifact(artifact) => {
            DiagnosticEmissionLinkPlanFailure::MissingStagedArtifact(diagnostic_artifact(artifact))
        }
        LinkPlanConstructionError::UnexpectedStagedArtifact(artifact) => {
            DiagnosticEmissionLinkPlanFailure::UnexpectedStagedArtifact(diagnostic_artifact(
                artifact,
            ))
        }
        LinkPlanConstructionError::UnsupportedStagedArtifact(artifact) => {
            DiagnosticEmissionLinkPlanFailure::UnsupportedStagedArtifact(diagnostic_artifact(
                artifact,
            ))
        }
        LinkPlanConstructionError::DuplicateOutputStaging(artifact) => {
            DiagnosticEmissionLinkPlanFailure::DuplicateOutputStaging(diagnostic_artifact(artifact))
        }
        LinkPlanConstructionError::MissingOutputStaging(artifact) => {
            DiagnosticEmissionLinkPlanFailure::MissingOutputStaging(diagnostic_artifact(artifact))
        }
        LinkPlanConstructionError::UnexpectedOutputStaging(artifact) => {
            DiagnosticEmissionLinkPlanFailure::UnexpectedOutputStaging(diagnostic_artifact(
                artifact,
            ))
        }
        LinkPlanConstructionError::OutputKindMismatch { artifact, kind } => {
            DiagnosticEmissionLinkPlanFailure::OutputKindMismatch {
                artifact: diagnostic_artifact(artifact),
                actual: diagnostic_linked_artifact_kind(*kind),
            }
        }
        LinkPlanConstructionError::InvalidStartupInputKind(kind) => {
            DiagnosticEmissionLinkPlanFailure::InvalidStartupInputKind(diagnostic_link_input_kind(
                *kind,
            ))
        }
        LinkPlanConstructionError::InvalidNativeInputKind(kind) => {
            DiagnosticEmissionLinkPlanFailure::InvalidNativeInputKind(diagnostic_link_input_kind(
                *kind,
            ))
        }
        LinkPlanConstructionError::InvalidTerminationInputKind(kind) => {
            DiagnosticEmissionLinkPlanFailure::InvalidTerminationInputKind(
                diagnostic_link_input_kind(*kind),
            )
        }
        LinkPlanConstructionError::InputOrdinalOverflow => {
            DiagnosticEmissionLinkPlanFailure::InputOrdinalOverflow
        }
        LinkPlanConstructionError::OutputOrdinalOverflow => {
            DiagnosticEmissionLinkPlanFailure::OutputOrdinalOverflow
        }
        LinkPlanConstructionError::InvalidInput(error) => link_input_failure_kind(*error),
        LinkPlanConstructionError::InvalidOutputStaging(
            StagingDestinationBuildError::EmptyPath,
        ) => DiagnosticEmissionLinkPlanFailure::OutputEmptyPath,
        LinkPlanConstructionError::InvalidLinkPlan(error) => {
            return link_plan_contract_failure_diagnostics(error, product, target);
        }
        LinkPlanConstructionError::Linker(failure) => {
            return bray_linker::link_failure_diagnostics(failure);
        }
    };

    emission_failure_diagnostics(link_plan_failure(failure), product, target)
}

fn link_plan_contract_failure_diagnostics(
    error: &LinkPlanBuildError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> DiagnosticBag {
    emission_failure_diagnostics(
        link_plan_failure(link_plan_contract_failure(error)),
        product,
        target,
    )
}

const fn link_input_failure_kind(error: LinkInputBuildError) -> DiagnosticEmissionLinkPlanFailure {
    match error {
        LinkInputBuildError::EmptyFilePath => DiagnosticEmissionLinkPlanFailure::InputEmptyPath,
        LinkInputBuildError::SourceKindMismatch => {
            DiagnosticEmissionLinkPlanFailure::InputSourceKindMismatch
        }
        LinkInputBuildError::WholeArchiveRequiresArchive => {
            DiagnosticEmissionLinkPlanFailure::WholeArchiveRequiresArchive
        }
    }
}

fn link_plan_contract_failure(error: &LinkPlanBuildError) -> DiagnosticEmissionLinkPlanFailure {
    match error {
        LinkPlanBuildError::MissingInputs => DiagnosticEmissionLinkPlanFailure::MissingInputs,
        LinkPlanBuildError::DuplicateInput(input) => {
            DiagnosticEmissionLinkPlanFailure::DuplicateInput(input.ordinal())
        }
        LinkPlanBuildError::DuplicateSearchPath => {
            DiagnosticEmissionLinkPlanFailure::DuplicateSearchPath
        }
        LinkPlanBuildError::MissingRuntimeComponent => {
            DiagnosticEmissionLinkPlanFailure::MissingRuntimeComponent
        }
        LinkPlanBuildError::UnexpectedRuntimeComponent => {
            DiagnosticEmissionLinkPlanFailure::UnexpectedRuntimeComponent
        }
        LinkPlanBuildError::RuntimeArtifactMismatch => {
            DiagnosticEmissionLinkPlanFailure::RuntimeArtifactMismatch
        }
        LinkPlanBuildError::MissingPrimaryOutput => {
            DiagnosticEmissionLinkPlanFailure::MissingPrimaryOutput
        }
        LinkPlanBuildError::MultiplePrimaryOutputs => {
            DiagnosticEmissionLinkPlanFailure::MultiplePrimaryOutputs
        }
        LinkPlanBuildError::OptionalPrimaryOutput => {
            DiagnosticEmissionLinkPlanFailure::OptionalPrimaryOutput
        }
        LinkPlanBuildError::DuplicateOutput(output) => {
            DiagnosticEmissionLinkPlanFailure::DuplicateOutput(output.ordinal())
        }
        LinkPlanBuildError::OutputPathCollision { first, second } => {
            DiagnosticEmissionLinkPlanFailure::OutputPathCollision {
                first: first.ordinal(),
                second: second.ordinal(),
            }
        }
        LinkPlanBuildError::MissingStartupMode => {
            DiagnosticEmissionLinkPlanFailure::MissingStartupMode
        }
        LinkPlanBuildError::UnexpectedStartupMode => {
            DiagnosticEmissionLinkPlanFailure::UnexpectedStartupMode
        }
        LinkPlanBuildError::MissingStartupInput => {
            DiagnosticEmissionLinkPlanFailure::MissingStartupInput
        }
        LinkPlanBuildError::UnexpectedStartupInput(input) => {
            DiagnosticEmissionLinkPlanFailure::UnexpectedStartupInput(input.ordinal())
        }
        LinkPlanBuildError::MissingDebugCompanion => {
            DiagnosticEmissionLinkPlanFailure::MissingDebugCompanion
        }
        LinkPlanBuildError::UnexpectedDebugCompanion => {
            DiagnosticEmissionLinkPlanFailure::UnexpectedDebugCompanion
        }
        LinkPlanBuildError::IncompatibleOutputKind { product, artifact } => {
            DiagnosticEmissionLinkPlanFailure::IncompatibleOutputKind {
                product: diagnostic_linked_product_kind(*product),
                artifact: diagnostic_linked_artifact_kind(*artifact),
            }
        }
    }
}

const fn diagnostic_link_input_kind(kind: LinkInputKind) -> DiagnosticLinkInputKind {
    match kind {
        LinkInputKind::RelocatableObject => DiagnosticLinkInputKind::RelocatableObject,
        LinkInputKind::Bitcode => DiagnosticLinkInputKind::Bitcode,
        LinkInputKind::Archive => DiagnosticLinkInputKind::Archive,
        LinkInputKind::StartupObject => DiagnosticLinkInputKind::StartupObject,
        LinkInputKind::TerminationObject => DiagnosticLinkInputKind::TerminationObject,
        LinkInputKind::RuntimeComponent => DiagnosticLinkInputKind::RuntimeComponent,
        LinkInputKind::NativeLibrary => DiagnosticLinkInputKind::NativeLibrary,
        LinkInputKind::Framework => DiagnosticLinkInputKind::Framework,
    }
}

const fn diagnostic_linked_artifact_kind(kind: LinkedArtifactKind) -> DiagnosticLinkedArtifactKind {
    match kind {
        LinkedArtifactKind::Executable => DiagnosticLinkedArtifactKind::Executable,
        LinkedArtifactKind::SharedLibrary => DiagnosticLinkedArtifactKind::SharedLibrary,
        LinkedArtifactKind::StaticLibrary => DiagnosticLinkedArtifactKind::StaticLibrary,
        LinkedArtifactKind::ImportLibrary => DiagnosticLinkedArtifactKind::ImportLibrary,
        LinkedArtifactKind::DebugCompanion => DiagnosticLinkedArtifactKind::DebugCompanion,
        LinkedArtifactKind::PlatformCompanion => DiagnosticLinkedArtifactKind::PlatformCompanion,
    }
}

const fn diagnostic_linked_product_kind(kind: LinkedProductKind) -> DiagnosticLinkedProductKind {
    match kind {
        LinkedProductKind::Executable => DiagnosticLinkedProductKind::Executable,
        LinkedProductKind::SharedLibrary => DiagnosticLinkedProductKind::SharedLibrary,
        LinkedProductKind::StaticLibrary => DiagnosticLinkedProductKind::StaticLibrary,
    }
}

pub(super) fn staging_failure_diagnostics(
    error: &LinkStagingError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> DiagnosticBag {
    let failure = match error {
        LinkStagingError::Storage(error) => {
            // The emission error retains its owned context after diagnostic rendering.
            return DiagnosticBag::single(
                error
                    .as_ref()
                    .clone()
                    .into_diagnostic(DiagnosticId::new(0), SeverityKind::Error),
            );
        }
        LinkStagingError::DuplicateContribution(artifact) => {
            DiagnosticEmissionStagingFailure::DuplicateContribution(diagnostic_artifact(artifact))
        }
        LinkStagingError::MissingContribution(artifact) => {
            DiagnosticEmissionStagingFailure::MissingContribution(diagnostic_artifact(artifact))
        }
        LinkStagingError::InvalidContribution(artifact) => {
            DiagnosticEmissionStagingFailure::InvalidContribution(diagnostic_artifact(artifact))
        }
        LinkStagingError::UnexpectedContribution(artifact) => {
            DiagnosticEmissionStagingFailure::UnexpectedContribution(diagnostic_artifact(artifact))
        }
        LinkStagingError::UnsupportedOutput(artifact) => {
            DiagnosticEmissionStagingFailure::UnsupportedOutput(diagnostic_artifact(artifact))
        }
        LinkStagingError::InvalidStagingPath(artifact) => {
            DiagnosticEmissionStagingFailure::InvalidPath(diagnostic_artifact(artifact))
        }
        LinkStagingError::MissingArtifacts => DiagnosticEmissionStagingFailure::Incomplete,
        LinkStagingError::Create { artifact, kind } => {
            return staging_io_failure_diagnostics(
                artifact,
                *kind,
                DiagnosticEmissionArtifactOperation::CreateStagingStorage,
                product,
                target,
            );
        }
        LinkStagingError::Read { artifact, kind } => {
            return staging_io_failure_diagnostics(
                artifact,
                *kind,
                DiagnosticEmissionArtifactOperation::ReadContribution,
                product,
                target,
            );
        }
        LinkStagingError::Write { artifact, kind } => {
            return staging_io_failure_diagnostics(
                artifact,
                *kind,
                DiagnosticEmissionArtifactOperation::WriteStagingStorage,
                product,
                target,
            );
        }
        LinkStagingError::Flush { artifact, kind } => {
            return staging_io_failure_diagnostics(
                artifact,
                *kind,
                DiagnosticEmissionArtifactOperation::FlushStagingStorage,
                product,
                target,
            );
        }
        LinkStagingError::Cancelled => return DiagnosticBag::new(),
    };

    emission_failure_diagnostics(DiagnosticEmissionFailure::Staging(failure), product, target)
}

fn staging_io_failure_diagnostics(
    artifact: &bray_emitter::ArtifactId,
    kind: std::io::ErrorKind,
    operation: DiagnosticEmissionArtifactOperation,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> DiagnosticBag {
    DiagnosticBag::single(
        emission_artifact_diagnostic(
            DiagnosticKind::EmissionArtifactIoFailed,
            artifact,
            product,
            target,
        )
        .with_arg(DiagnosticArg::emission_artifact_operation(operation))
        .with_arg(DiagnosticArg::io_error_kind(DiagnosticIoErrorKind::from(
            kind,
        ))),
    )
}
