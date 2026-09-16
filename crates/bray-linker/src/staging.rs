use std::num::NonZeroU64;

use bray_diagnostics::DiagnosticBag;

use crate::outcome::failed_outcome;
use crate::{
    LinkFailure, LinkInputSource, LinkOutcome, LinkPlan, LinkedArtifact, LinkedArtifactRequirement,
};

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
                return failed_outcome(plan, LinkFailure::MissingOutput(destination.id()));
            }
        };

        let Some(byte_len) = NonZeroU64::new(metadata.len()) else {
            return failed_outcome(plan, LinkFailure::InvalidOutput(destination.id()));
        };

        if !metadata.is_file() {
            return failed_outcome(plan, LinkFailure::InvalidOutput(destination.id()));
        }

        artifacts.push(LinkedArtifact::new(
            output.kind(),
            destination.id(),
            byte_len,
        ));
    }

    LinkOutcome::complete(artifacts, DiagnosticBag::new())
}
