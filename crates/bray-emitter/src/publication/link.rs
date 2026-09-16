use std::path::Path;

use bray_base::Cancellation;
use bray_codegen::ArtifactDigest;
use bray_linker::{
    LinkInputProvenance, LinkInputSource, LinkPlan, LinkedArtifact, LinkedArtifactRequirement,
};

use crate::artifact::content::{ContentValidationError, validate_staged_content};
use crate::{ArtifactId, ArtifactProducer, EmissionPlan, PlannedArtifact};

pub(super) struct PreparedLinkedArtifact<'plan, 'link> {
    planned: &'plan PlannedArtifact,
    path: &'link Path,
    byte_len: u64,
    digest: ArtifactDigest,
}

impl<'plan, 'link> PreparedLinkedArtifact<'plan, 'link> {
    pub(super) const fn planned(&self) -> &'plan PlannedArtifact {
        self.planned
    }

    pub(super) const fn path(&self) -> &'link Path {
        self.path
    }

    pub(super) const fn byte_len(&self) -> u64 {
        self.byte_len
    }

    pub(super) const fn digest(&self) -> &ArtifactDigest {
        &self.digest
    }
}

pub(super) enum LinkedPreparationError {
    Cancelled,
    InvalidContent {
        artifact: ArtifactId,
        error: ContentValidationError,
    },
}

pub(super) fn prepare_linked_artifacts<'plan, 'link>(
    emission: &'plan EmissionPlan,
    link_plan: &'link LinkPlan,
    linked: &'link [LinkedArtifact],
    cancellation: &dyn Cancellation,
) -> Result<Vec<PreparedLinkedArtifact<'plan, 'link>>, LinkedPreparationError> {
    let planned: Vec<_> = emission
        .published_artifacts()
        .filter(|artifact| matches!(artifact.producer(), ArtifactProducer::Linker(_)))
        .collect();

    let mut prepared = Vec::with_capacity(linked.len());

    for (planned, output) in planned.into_iter().zip(link_plan.outputs()) {
        let Some(artifact) = linked
            .binary_search_by_key(&output.destination().id(), LinkedArtifact::destination)
            .ok()
            .map(|index| &linked[index])
        else {
            assert_eq!(
                output.requirement(),
                LinkedArtifactRequirement::Optional,
                "complete linking omitted required output {:?}",
                output.destination().id()
            );

            continue;
        };

        let byte_len = artifact.byte_len().get();

        let digest =
            validate_staged_content(output.destination().path(), byte_len, None, cancellation)
                .map_err(|error| content_error(planned, error))?;

        prepared.push(PreparedLinkedArtifact {
            planned,
            path: output.destination().path(),
            byte_len,
            digest,
        });
    }

    Ok(prepared)
}

fn content_error(
    planned: &PlannedArtifact,
    error: ContentValidationError,
) -> LinkedPreparationError {
    if matches!(error, ContentValidationError::Cancelled) {
        return LinkedPreparationError::Cancelled;
    }

    // The error retains the Arc-backed artifact identity after validation returns.
    LinkedPreparationError::InvalidContent {
        artifact: planned.id().clone(),
        error,
    }
}

pub(super) struct LinkStagingCleanup<'plan>(&'plan LinkPlan);

impl<'plan> LinkStagingCleanup<'plan> {
    pub(super) const fn new(plan: &'plan LinkPlan) -> Self {
        Self(plan)
    }
}

impl Drop for LinkStagingCleanup<'_> {
    fn drop(&mut self) {
        for input in self.0.inputs() {
            if input.provenance() != &LinkInputProvenance::Product {
                continue;
            }

            if let LinkInputSource::File(path) = input.source() {
                let _ = std::fs::remove_file(path);
            }
        }

        for output in self.0.outputs() {
            let _ = std::fs::remove_file(output.destination().path());
        }
    }
}
