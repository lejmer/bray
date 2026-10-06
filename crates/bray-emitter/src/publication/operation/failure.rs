use super::super::diagnostic::{PublicationError, PublicationErrorKind};

use crate::artifact::content::ContentValidationError;
use crate::{
    ArtifactId, ArtifactRequirement, EmissionPlan, PlannedArtifact, PlannedArtifactDestination,
};

pub(in crate::publication) enum ArtifactPublicationFailure {
    Cancelled,
    Failed(ArtifactRequirement, PublicationError),
}

pub(in crate::publication) fn content_failure(
    planned: &PlannedArtifact,
    error: ContentValidationError,
) -> ArtifactPublicationFailure {
    if matches!(error, ContentValidationError::Cancelled) {
        return ArtifactPublicationFailure::Cancelled;
    }

    ArtifactPublicationFailure::Failed(
        planned.requirement(),
        publication_content_error(planned, error),
    )
}

pub(super) fn publication_content_error(
    planned: &PlannedArtifact,
    error: ContentValidationError,
) -> PublicationError {
    let kind = match error {
        ContentValidationError::Cancelled => PublicationErrorKind::InvalidContribution,
        ContentValidationError::Read(kind) => PublicationErrorKind::Read(kind),
        ContentValidationError::LengthMismatch { expected, actual } => {
            PublicationErrorKind::LengthMismatch { expected, actual }
        }
        ContentValidationError::DigestMismatch { expected, actual } => {
            PublicationErrorKind::digest_mismatch(expected, actual)
        }
        ContentValidationError::LengthOverflow | ContentValidationError::DigestConstruction => {
            PublicationErrorKind::InvalidContribution
        }
    };

    planned_error(planned, kind)
}

pub(in crate::publication) fn artifact_failure(
    planned: &PlannedArtifact,
    kind: PublicationErrorKind,
) -> ArtifactPublicationFailure {
    ArtifactPublicationFailure::Failed(planned.requirement(), planned_error(planned, kind))
}

pub(super) fn contribution_error(
    artifact: &ArtifactId,
    plan: &EmissionPlan,
    kind: PublicationErrorKind,
) -> PublicationError {
    let sink = plan
        .artifact(artifact)
        .and_then(|planned| match planned.destination() {
            PlannedArtifactDestination::Publish(sink) => Some(sink.clone()),
            PlannedArtifactDestination::Stage => None,
        });

    // Publication errors own the contribution identity after validation returns.
    PublicationError::new(artifact.clone(), sink, kind)
}

pub(in crate::publication) fn planned_error(
    planned: &PlannedArtifact,
    kind: PublicationErrorKind,
) -> PublicationError {
    let sink = match planned.destination() {
        // Publication errors own the destination after the plan borrow ends.
        PlannedArtifactDestination::Publish(sink) => Some(sink.clone()),
        PlannedArtifactDestination::Stage => None,
    };

    // Publication errors own the artifact identity after the plan borrow ends.
    PublicationError::new(planned.id().clone(), sink, kind)
}
