use std::num::NonZeroU64;

use bray_diagnostics::DiagnosticBag;

use crate::{
    LinkFailure, LinkInputSource, LinkOutcome, LinkOutcomeBuildError, LinkPlan, LinkedArtifact,
    LinkedArtifactRequirement, LinkedArtifactSetBuildError,
};
use crate::outcome::failed_outcome;

pub(crate) fn validate_file_inputs(plan: &LinkPlan) -> Result<(), LinkFailure> {
    for input in plan.inputs() {
        let LinkInputSource::File(path) = input.source() else {
            continue;
        };

        let Ok(metadata) = std::fs::metadata(path) else {
            return Err(LinkFailure::MissingInput(input.id()));
        };

        if !metadata.is_file() {
            return Err(LinkFailure::MissingInput(input.id()));
        }
    }

    Ok(())
}

pub(crate) fn complete_linked_outputs(plan: &LinkPlan) -> LinkOutcome {
    let mut artifacts = Vec::new();

    for output in plan.outputs() {
        let destination = output.destination();

        let metadata = match std::fs::metadata(destination.path()) {
            Ok(metadata) => metadata,
            Err(_) if output.requirement() == LinkedArtifactRequirement::Optional => {
                continue;
            }
            Err(_) => {
                return failed_outcome(LinkFailure::MissingOutput(
                    destination.id(),
                ));
            }
        };

        let Some(byte_len) = NonZeroU64::new(metadata.len()) else {
            return failed_outcome(LinkFailure::InvalidOutput(
                destination.id(),
            ));
        };

        if !metadata.is_file() {
            return failed_outcome(LinkFailure::InvalidOutput(
                destination.id(),
            ));
        }

        artifacts.push(LinkedArtifact::new(
            output.kind(),
            destination.id(),
            byte_len,
        ));
    }

    match LinkOutcome::try_complete(plan, artifacts, DiagnosticBag::new()) {
        Ok(outcome) => outcome,
        Err(error) => failed_outcome(link_outcome_failure(error)),
    }
}

fn link_outcome_failure(error: LinkOutcomeBuildError) -> LinkFailure {
    match error {
        LinkOutcomeBuildError::ErrorDiagnostics(_) => LinkFailure::Invocation,
        LinkOutcomeBuildError::InvalidArtifacts(LinkedArtifactSetBuildError::MissingRequired(
            destination,
        )) => LinkFailure::MissingOutput(destination),
        LinkOutcomeBuildError::InvalidArtifacts(
            LinkedArtifactSetBuildError::DuplicateArtifact(destination)
            | LinkedArtifactSetBuildError::UnplannedArtifact(destination)
            | LinkedArtifactSetBuildError::KindMismatch(destination),
        ) => LinkFailure::InvalidOutput(destination),
    }
}
