use std::io;

use bray_codegen::ArtifactDigest;
use bray_diagnostics::DiagnosticBag;
use bray_linker::{LinkOutcome, LinkPlan, LinkStatus};

use super::super::diagnostic::{PublicationDiagnostics, PublicationError, PublicationErrorKind};
use super::super::generation::publish_managed_generation;
use super::super::link::{LinkStagingCleanup, LinkedPreparationError, prepare_linked_artifacts};
use super::super::publisher::ArtifactPublisher;
use super::super::staging::FilesystemStaging;
use super::failure::{
    ArtifactPublicationFailure, artifact_failure, content_failure, publication_content_error,
};
use super::preparation::{PreparedArtifact, PreparedContent, copy_content, prepare_contributions};

use crate::artifact::content::validate_staged_content;
use crate::{
    ArtifactContribution, ArtifactRequirement, EmissionOutcome, EmissionPlan, EmittedArtifact,
    EmittedArtifactSet, IndirectOutputSink, OutputSink, PlannedArtifact,
    PlannedArtifactDestination, ReplacementPolicy,
};

impl<'host> ArtifactPublisher<'host> {
    /// Publishes the plan-owned package interface and supplied contributions in plan order.
    pub fn publish(
        &self,
        plan: &EmissionPlan,
        contributions: impl IntoIterator<Item = ArtifactContribution>,
    ) -> EmissionOutcome {
        let mut diagnostics = PublicationDiagnostics::new();

        if self.cancellation.is_cancelled() {
            return diagnostics.cancelled(publication_set(plan, []));
        }

        let prepared = match prepare_contributions(plan, contributions, [], &mut diagnostics) {
            Ok(prepared) => prepared,
            Err(error) => return diagnostics.failed(publication_set(plan, []), error),
        };

        if let Err(validation) = self.validate_publication() {
            return EmissionOutcome::failed(
                crate::EmissionFailure::IncompleteProduct,
                publication_set(plan, []),
                validation,
            );
        }

        self.publish_prepared(plan, prepared, diagnostics)
    }

    /// Publishes a complete native link result and other planned contributions.
    pub fn publish_linked(
        &self,
        plan: &EmissionPlan,
        contributions: impl IntoIterator<Item = ArtifactContribution>,
        link_plan: &LinkPlan,
        link_outcome: &LinkOutcome,
    ) -> EmissionOutcome {
        let _staging_cleanup = LinkStagingCleanup::new(link_plan);
        let link_diagnostics = link_outcome.diagnostics();

        if self.cancellation.is_cancelled() {
            let outcome =
                EmissionOutcome::cancelled(publication_set(plan, []), DiagnosticBag::new());

            return merge_link_diagnostics(outcome, link_diagnostics);
        }

        let linked = match link_outcome.status() {
            LinkStatus::Failed(_) => {
                return EmissionOutcome::failed(
                    crate::EmissionFailure::Linking,
                    publication_set(plan, []),
                    link_diagnostics.clone(),
                );
            }
            LinkStatus::Cancelled => {
                let outcome =
                    EmissionOutcome::cancelled(publication_set(plan, []), DiagnosticBag::new());

                return merge_link_diagnostics(outcome, link_diagnostics);
            }
            LinkStatus::Complete(linked) => {
                match prepare_linked_artifacts(plan, link_plan, linked, self.cancellation) {
                    Ok(linked) => linked,
                    Err(LinkedPreparationError::Cancelled) => {
                        let outcome = EmissionOutcome::cancelled(
                            publication_set(plan, []),
                            DiagnosticBag::new(),
                        );

                        return merge_link_diagnostics(outcome, link_diagnostics);
                    }
                    Err(LinkedPreparationError::InvalidContent { artifact, error }) => {
                        let Some(planned) = plan.artifact(&artifact) else {
                            let error = PublicationError::new(
                                artifact,
                                None,
                                PublicationErrorKind::InvalidContribution,
                            );

                            let outcome = PublicationDiagnostics::new()
                                .failed(publication_set(plan, []), error);

                            return merge_link_diagnostics(outcome, link_diagnostics);
                        };

                        let error = publication_content_error(planned, error);

                        let outcome =
                            PublicationDiagnostics::new().failed(publication_set(plan, []), error);

                        return merge_link_diagnostics(outcome, link_diagnostics);
                    }
                }
            }
        };

        let mut diagnostics = PublicationDiagnostics::new();

        let prepared = match prepare_contributions(plan, contributions, linked, &mut diagnostics) {
            Ok(prepared) => prepared,
            Err(error) => {
                let outcome = diagnostics.failed(publication_set(plan, []), error);

                return merge_link_diagnostics(outcome, link_diagnostics);
            }
        };

        if let Err(validation) = self.validate_publication() {
            let outcome = EmissionOutcome::failed(
                crate::EmissionFailure::IncompleteProduct,
                publication_set(plan, []),
                validation,
            );

            return merge_link_diagnostics(outcome, link_diagnostics);
        }

        let outcome = self.publish_prepared(plan, prepared, diagnostics);

        merge_link_diagnostics(outcome, link_diagnostics)
    }

    fn publish_prepared(
        &self,
        plan: &EmissionPlan,
        prepared: Vec<PreparedArtifact<'_, '_>>,
        mut diagnostics: PublicationDiagnostics,
    ) -> EmissionOutcome {
        if prepared.iter().any(PreparedArtifact::is_managed) {
            return match publish_managed_generation(
                plan,
                prepared,
                plan.request().replacement(),
                self.cancellation,
            ) {
                Ok(publication) => {
                    if let Some(warning) = publication.warning {
                        diagnostics.warning(warning);
                    }

                    EmissionOutcome::complete(
                        publication.artifacts,
                        Some(publication.generation),
                        diagnostics.into_bag(),
                    )
                }
                Err(ArtifactPublicationFailure::Cancelled) => {
                    diagnostics.cancelled(publication_set(plan, []))
                }
                Err(ArtifactPublicationFailure::Failed(_, error)) => {
                    diagnostics.failed(publication_set(plan, []), error)
                }
            };
        }

        let mut emitted = Vec::with_capacity(prepared.len());

        for artifact in prepared {
            if self.cancellation.is_cancelled() {
                return diagnostics.cancelled(publication_set(plan, emitted));
            }

            match self.publish_artifact(plan.request().replacement(), artifact) {
                Ok(artifact) => emitted.push(artifact),
                Err(ArtifactPublicationFailure::Cancelled) => {
                    return diagnostics.cancelled(publication_set(plan, emitted));
                }
                Err(ArtifactPublicationFailure::Failed(ArtifactRequirement::Optional, error)) => {
                    diagnostics.warning(error);
                }
                Err(ArtifactPublicationFailure::Failed(_, error)) => {
                    return diagnostics.failed(publication_set(plan, emitted), error);
                }
            }
        }

        let diagnostics = diagnostics.into_bag();
        let artifacts = publication_set(plan, emitted);

        EmissionOutcome::complete(artifacts, None, diagnostics)
    }

    fn publish_artifact(
        &self,
        replacement: ReplacementPolicy,
        artifact: PreparedArtifact<'_, '_>,
    ) -> Result<EmittedArtifact, ArtifactPublicationFailure> {
        let planned = artifact.planned;

        let PlannedArtifactDestination::Publish(sink) = planned.destination() else {
            return Err(artifact_failure(
                planned,
                PublicationErrorKind::InvalidContribution,
            ));
        };

        let digest = match sink {
            OutputSink::ManagedFilesystem { .. } => {
                return Err(artifact_failure(
                    planned,
                    PublicationErrorKind::InvalidContribution,
                ));
            }
            OutputSink::Filesystem(path) => {
                self.publish_filesystem(planned, &artifact.content, path, replacement)?
            }
            OutputSink::Memory {
                collector,
                artifact: artifact_id,
            } => self.publish_indirect(
                planned,
                &artifact.content,
                IndirectOutputSink::Memory {
                    collector,
                    artifact: artifact_id,
                },
                replacement,
            )?,
            OutputSink::Stream(stream) => self.publish_indirect(
                planned,
                &artifact.content,
                IndirectOutputSink::Stream(stream),
                replacement,
            )?,
        };

        // Publication records own stable plan records independently of the borrowed plan.
        Ok(EmittedArtifact::new(
            planned.id().clone(),
            sink.clone(),
            planned.producer().clone(),
            planned.role(),
            artifact.content.byte_len(),
            digest,
        ))
    }

    fn publish_filesystem(
        &self,
        planned: &PlannedArtifact,
        content: &PreparedContent<'_, '_>,
        destination: &std::path::Path,
        replacement: ReplacementPolicy,
    ) -> Result<ArtifactDigest, ArtifactPublicationFailure> {
        if self.cancellation.is_cancelled() {
            return Err(ArtifactPublicationFailure::Cancelled);
        }

        let mut staging = FilesystemStaging::create(destination, planned.id().kind(), replacement)
            .map_err(|error| artifact_failure(planned, PublicationErrorKind::Open(error.kind())))?;

        copy_content(self.cancellation, planned, content, &mut staging)?;

        if self.cancellation.is_cancelled() {
            return Err(ArtifactPublicationFailure::Cancelled);
        }

        let staging = staging.finish().map_err(|error| {
            artifact_failure(planned, PublicationErrorKind::Flush(error.kind()))
        })?;

        let digest = validate_staged_content(
            staging.path(),
            content.byte_len(),
            content.digest(),
            self.cancellation,
        )
        .map_err(|error| content_failure(planned, error))?;

        if self.cancellation.is_cancelled() {
            return Err(ArtifactPublicationFailure::Cancelled);
        }

        staging.promote(destination).map_err(|error| {
            artifact_failure(planned, PublicationErrorKind::Commit(error.kind()))
        })?;

        Ok(digest)
    }

    fn publish_indirect(
        &self,
        planned: &PlannedArtifact,
        content: &PreparedContent<'_, '_>,
        sink: IndirectOutputSink<'_>,
        replacement: ReplacementPolicy,
    ) -> Result<ArtifactDigest, ArtifactPublicationFailure> {
        let digest = content
            .validate(self.cancellation)
            .map_err(|error| content_failure(planned, error))?;

        if self.cancellation.is_cancelled() {
            return Err(ArtifactPublicationFailure::Cancelled);
        }

        let Some(resolver) = self.resolver else {
            return Err(artifact_failure(
                planned,
                PublicationErrorKind::Open(io::ErrorKind::NotFound),
            ));
        };

        let mut transaction = resolver
            .open(sink, replacement)
            .map_err(|error| artifact_failure(planned, PublicationErrorKind::Open(error.kind())))?;

        copy_content(self.cancellation, planned, content, transaction.as_mut())?;

        if self.cancellation.is_cancelled() {
            return Err(ArtifactPublicationFailure::Cancelled);
        }

        transaction.flush().map_err(|error| {
            artifact_failure(planned, PublicationErrorKind::Flush(error.kind()))
        })?;

        if self.cancellation.is_cancelled() {
            return Err(ArtifactPublicationFailure::Cancelled);
        }

        transaction.commit().map_err(|error| {
            artifact_failure(planned, PublicationErrorKind::Commit(error.kind()))
        })?;

        Ok(digest)
    }
}

fn publication_set(
    plan: &EmissionPlan,
    artifacts: impl IntoIterator<Item = EmittedArtifact>,
) -> EmittedArtifactSet {
    EmittedArtifactSet::from_publication(plan, artifacts)
}

fn merge_link_diagnostics(
    outcome: EmissionOutcome,
    diagnostics: &DiagnosticBag,
) -> EmissionOutcome {
    outcome.with_prior_diagnostics(diagnostics)
}
