use std::cmp::Ordering;
use std::io::{self, Read, Write};

use bray_base::Cancellation;
use bray_codegen::{ArtifactContent, ArtifactDigest, ArtifactDigestAlgorithm};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticNote,
    DiagnosticNoteKind, SeverityKind,
};
use bray_linker::{LinkOutcome, LinkPlan, LinkStatus};

use super::diagnostic::{PublicationDiagnostics, PublicationError, PublicationErrorKind};
use super::generation::publish_managed_generation;
use super::link::{
    LinkStagingCleanup, LinkedPreparationError, PreparedLinkedArtifact, prepare_linked_artifacts,
};
use super::staging::FilesystemStaging;
use crate::artifact::content::{
    ContentReader, ContentValidationError, open_content, open_linked_staging, validate_content,
    validate_staged_content,
};
use crate::{
    ArtifactContribution, ArtifactId, ArtifactKind, ArtifactProducer, ArtifactRequirement,
    EmissionOutcome, EmissionPlan, EmittedArtifact, EmittedArtifactSet, IndirectOutputSink,
    OutputSink, OutputSinkResolver, PlannedArtifact, PlannedArtifactDestination, ReplacementPolicy,
};

const COPY_BUFFER_LEN: usize = 64 * 1024;

/// Publishes validated artifact contributions to the immutable plan's external sinks.
#[derive(Clone, Copy)]
pub struct ArtifactPublisher<'host> {
    cancellation: &'host dyn Cancellation,
    resolver: Option<&'host dyn OutputSinkResolver>,
}

impl<'host> ArtifactPublisher<'host> {
    /// Creates a publisher for filesystem-only plans.
    pub const fn new(cancellation: &'host dyn Cancellation) -> Self {
        Self {
            cancellation,
            resolver: None,
        }
    }

    /// Creates a publisher that can resolve memory collectors and writable streams.
    pub const fn with_sink_resolver(
        cancellation: &'host dyn Cancellation,
        resolver: &'host dyn OutputSinkResolver,
    ) -> Self {
        Self {
            cancellation,
            resolver: Some(resolver),
        }
    }

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

        self.publish_prepared(plan, prepared, diagnostics)
    }

    /// Publishes a complete native link result and other planned contributions.
    pub fn publish_linked(
        &self,
        plan: &EmissionPlan,
        contributions: impl IntoIterator<Item = ArtifactContribution>,
        link_plan: &LinkPlan,
        link_outcome: &LinkOutcome,
    ) -> Result<EmissionOutcome, crate::EmissionOutcomeBuildError> {
        let _staging_cleanup = LinkStagingCleanup::new(link_plan);
        let link_diagnostics = link_outcome.diagnostics();

        if self.cancellation.is_cancelled() {
            let outcome =
                EmissionOutcome::cancelled(publication_set(plan, []), DiagnosticBag::new());

            return merge_link_diagnostics(outcome, link_diagnostics);
        }

        let linked = match link_outcome.status() {
            LinkStatus::Failed(_) => {
                return Ok(EmissionOutcome::failed(
                    crate::EmissionFailure::Linking,
                    publication_set(plan, []),
                    link_diagnostics.clone(),
                ));
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
                    Err(LinkedPreparationError::MissingLinkedPlan) => {
                        let outcome = EmissionOutcome::failed(
                            crate::EmissionFailure::Planning,
                            publication_set(plan, []),
                            linked_plan_missing_diagnostics(plan),
                        );

                        return merge_link_diagnostics(outcome, link_diagnostics);
                    }
                    Err(LinkedPreparationError::InvalidRelationship(artifact)) => {
                        let error = PublicationError::new(
                            artifact,
                            None,
                            PublicationErrorKind::InvalidContribution,
                        );

                        let outcome =
                            PublicationDiagnostics::new().failed(publication_set(plan, []), error);

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

                    EmissionOutcome::try_complete(
                        publication.artifacts,
                        Some(publication.generation),
                        diagnostics.into_bag(),
                    )
                    .unwrap_or_else(super::super::EmissionOutcomeBuildError::into_failed_outcome)
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

        EmissionOutcome::try_complete(artifacts, None, diagnostics)
            .unwrap_or_else(super::super::EmissionOutcomeBuildError::into_failed_outcome)
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

pub(super) fn copy_content(
    cancellation: &dyn Cancellation,
    planned: &PlannedArtifact,
    content: &PreparedContent<'_, '_>,
    writer: &mut dyn Write,
) -> Result<(), ArtifactPublicationFailure> {
    if cancellation.is_cancelled() {
        return Err(ArtifactPublicationFailure::Cancelled);
    }

    let mut reader = content
        .open()
        .map_err(|kind| artifact_failure(planned, PublicationErrorKind::Read(kind)))?;

    let mut buffer = [0_u8; COPY_BUFFER_LEN];

    loop {
        if cancellation.is_cancelled() {
            return Err(ArtifactPublicationFailure::Cancelled);
        }

        let read = reader
            .read(&mut buffer)
            .map_err(|error| artifact_failure(planned, PublicationErrorKind::Read(error.kind())))?;

        if read == 0 {
            break;
        }

        if cancellation.is_cancelled() {
            return Err(ArtifactPublicationFailure::Cancelled);
        }

        writer.write_all(&buffer[..read]).map_err(|error| {
            artifact_failure(planned, PublicationErrorKind::Write(error.kind()))
        })?;
    }

    Ok(())
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
) -> Result<EmissionOutcome, crate::EmissionOutcomeBuildError> {
    outcome.try_with_prior_diagnostics(diagnostics)
}

fn linked_plan_missing_diagnostics(plan: &EmissionPlan) -> DiagnosticBag {
    DiagnosticBag::single(
        Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::EmissionLinkedPlanMissing,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::actual_product_identity(
            plan.request().product().to_string(),
        ))
        .with_arg(DiagnosticArg::target_triple(
            plan.request().target().as_str(),
        ))
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::ReportCompilerDefect,
        )),
    )
}

pub(super) struct PreparedArtifact<'plan, 'link> {
    pub(super) planned: &'plan PlannedArtifact,
    pub(super) content: PreparedContent<'plan, 'link>,
}

impl PreparedArtifact<'_, '_> {
    fn is_managed(&self) -> bool {
        matches!(
            self.planned.destination(),
            PlannedArtifactDestination::Publish(OutputSink::ManagedFilesystem { .. })
        )
    }
}

pub(super) enum PreparedContent<'plan, 'link> {
    Contribution(ArtifactContribution),
    Linked(PreparedLinkedArtifact<'plan, 'link>),
}

impl PreparedContent<'_, '_> {
    pub(super) fn id(&self) -> &ArtifactId {
        match self {
            Self::Contribution(contribution) => contribution.id(),
            Self::Linked(linked) => linked.planned().id(),
        }
    }

    pub(super) fn producer(&self) -> &ArtifactProducer {
        match self {
            Self::Contribution(contribution) => contribution.producer(),
            Self::Linked(linked) => linked.planned().producer(),
        }
    }

    pub(super) fn byte_len(&self) -> u64 {
        match self {
            Self::Contribution(contribution) => contribution.content().byte_len(),
            Self::Linked(linked) => linked.byte_len(),
        }
    }

    pub(super) fn digest(&self) -> Option<&ArtifactDigest> {
        match self {
            Self::Contribution(contribution) => contribution.digest(),
            Self::Linked(linked) => Some(linked.digest()),
        }
    }

    pub(super) fn open(&self) -> Result<ContentReader<'_>, io::ErrorKind> {
        match self {
            Self::Contribution(contribution) => open_content(contribution.content()),
            Self::Linked(linked) => open_linked_staging(linked.path()),
        }
    }

    fn validate(
        &self,
        cancellation: &dyn Cancellation,
    ) -> Result<ArtifactDigest, ContentValidationError> {
        match self {
            Self::Contribution(contribution) => {
                validate_content(contribution.content(), contribution.digest(), cancellation)
            }
            Self::Linked(linked) => {
                // Publication records retain the fixed-size digest after staging validation.
                Ok(linked.digest().clone())
            }
        }
    }
}

pub(super) enum ArtifactPublicationFailure {
    Cancelled,
    Failed(ArtifactRequirement, PublicationError),
}

fn prepare_contributions<'plan, 'link>(
    plan: &'plan EmissionPlan,
    contributions: impl IntoIterator<Item = ArtifactContribution>,
    linked: impl IntoIterator<Item = PreparedLinkedArtifact<'plan, 'link>>,
    diagnostics: &mut PublicationDiagnostics,
) -> Result<Vec<PreparedArtifact<'plan, 'link>>, PublicationError> {
    let mut contributions: Vec<_> = contributions
        .into_iter()
        .map(PreparedContent::Contribution)
        .collect();

    if let Some(package_interface) = package_interface_contribution(plan)? {
        contributions.push(PreparedContent::Contribution(package_interface));
    }

    contributions.extend(linked.into_iter().map(PreparedContent::Linked));

    contributions.sort_unstable_by(|left, right| left.id().cmp(right.id()));

    if let Some(pair) = contributions
        .windows(2)
        .find(|pair| pair[0].id() == pair[1].id())
    {
        return Err(contribution_error(
            pair[0].id(),
            plan,
            PublicationErrorKind::InvalidContribution,
        ));
    }

    for contribution in &contributions {
        validate_contribution_destination(plan, contribution.id())?;
    }

    let mut contributions = contributions.into_iter().peekable();
    let mut prepared = Vec::new();

    for planned in plan.published_artifacts() {
        let contribution = match contributions.peek() {
            Some(contribution) => match contribution.id().cmp(planned.id()) {
                Ordering::Equal => contributions.next(),
                Ordering::Greater => None,
                Ordering::Less => {
                    return Err(contribution_error(
                        contribution.id(),
                        plan,
                        PublicationErrorKind::InvalidContribution,
                    ));
                }
            },
            None => None,
        };

        let Some(contribution) = contribution else {
            if planned.requirement() == ArtifactRequirement::Required {
                return Err(planned_error(
                    planned,
                    PublicationErrorKind::MissingContribution,
                ));
            }

            continue;
        };

        if contribution.producer() != planned.producer() {
            let error = contribution_error(
                contribution.id(),
                plan,
                PublicationErrorKind::InvalidContribution,
            );

            if planned.requirement() == ArtifactRequirement::Optional {
                diagnostics.warning(error);

                continue;
            }

            return Err(error);
        }

        prepared.push(PreparedArtifact {
            planned,
            content: contribution,
        });
    }

    if let Some(contribution) = contributions.next() {
        return Err(contribution_error(
            contribution.id(),
            plan,
            PublicationErrorKind::InvalidContribution,
        ));
    }

    Ok(prepared)
}

fn package_interface_contribution(
    plan: &EmissionPlan,
) -> Result<Option<ArtifactContribution>, PublicationError> {
    let Some(interface) = plan.package_interface() else {
        return Ok(None);
    };

    let Some(planned) = plan
        .published_artifacts()
        .find(|artifact| artifact.id().kind() == ArtifactKind::PackageInterface)
    else {
        let id = ArtifactId::new(
            plan.request().product().clone(),
            ArtifactKind::PackageInterface,
            0,
        );

        return Err(PublicationError::new(
            id,
            None,
            PublicationErrorKind::InvalidContribution,
        ));
    };

    interface
        .validate_integrity()
        .map_err(|_| planned_error(planned, PublicationErrorKind::InvalidContribution))?;

    let content = ArtifactContent::try_memory(interface.shared_bytes())
        .map_err(|_| planned_error(planned, PublicationErrorKind::InvalidContribution))?;

    if content.byte_len() != interface.byte_len() {
        return Err(planned_error(
            planned,
            PublicationErrorKind::LengthMismatch {
                expected: interface.byte_len(),
                actual: content.byte_len(),
            },
        ));
    }

    let digest = ArtifactDigest::try_new(
        ArtifactDigestAlgorithm::Blake3,
        blake3::hash(interface.bytes()).as_bytes(),
    )
    .ok_or_else(|| planned_error(planned, PublicationErrorKind::InvalidContribution))?;

    Ok(Some(ArtifactContribution::new(
        planned.id().clone(),
        ArtifactProducer::PackageInterface,
        content,
        Some(digest),
    )))
}

fn validate_contribution_destination(
    plan: &EmissionPlan,
    artifact: &ArtifactId,
) -> Result<(), PublicationError> {
    let Some(planned) = plan.artifact(artifact) else {
        return Err(contribution_error(
            artifact,
            plan,
            PublicationErrorKind::InvalidContribution,
        ));
    };

    if !matches!(
        planned.destination(),
        PlannedArtifactDestination::Publish(_)
    ) {
        return Err(contribution_error(
            artifact,
            plan,
            PublicationErrorKind::InvalidContribution,
        ));
    }

    Ok(())
}

pub(super) fn content_failure(
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

fn publication_content_error(
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

pub(super) fn artifact_failure(
    planned: &PlannedArtifact,
    kind: PublicationErrorKind,
) -> ArtifactPublicationFailure {
    ArtifactPublicationFailure::Failed(planned.requirement(), planned_error(planned, kind))
}

fn contribution_error(
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

pub(super) fn planned_error(
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::{self, Write};
    use std::num::NonZeroU64;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier, Mutex};

    use bray_codegen::{ArtifactContent, ArtifactDigest, ArtifactDigestAlgorithm};
    use bray_diagnostics::DiagnosticBag;
    use bray_diagnostics::{DiagnosticArgName, DiagnosticArgValue, DiagnosticKind, SeverityKind};
    use bray_linker::{
        DebugLinkPolicy, LinkFailure, LinkInput, LinkInputId, LinkInputKind, LinkInputMode,
        LinkInputProvenance, LinkInputSource, LinkModel, LinkOutcome, LinkPlan, LinkPlanBuilder,
        LinkPolicy, LinkTarget, LinkedArtifact, LinkedArtifactKind, LinkedArtifactRequirement,
        LinkedProductKind, LinkerDriverIdentity, LinkerDriverKind, PlannedLinkedArtifact,
        SectionGarbageCollectionPolicy, StagingDestination, StagingDestinationId, StagingPathKey,
    };
    use bray_target::{CodeModel, ObjectFormat, RelocationModel, TargetArchitecture};
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::ArtifactPublisher;
    use crate::test_support::{interface_artifact, product_identity, target_identity};
    use crate::{
        ArtifactContribution, ArtifactId, ArtifactKind, ArtifactProducer, ArtifactRequirement,
        ArtifactRole, DependencyMetadataProducerId, EmissionFailure, EmissionOutcome, EmissionPlan,
        EmissionRequest, EmissionStatus, IndirectOutputSink, LinkerProducerId, OutputSink,
        OutputSinkId, OutputSinkResolver, OutputSinkTransaction, PlannedArtifact,
        PlannedArtifactDestination, ProductKind, ReplacementPolicy, RequestedArtifact,
        RequestedArtifactDestination,
    };

    #[test]
    fn memory_publication_writes_complete_bytes_and_records_digest() {
        let Some(collector) = OutputSinkId::try_new("test.memory") else {
            panic!("test collector identity must be valid");
        };

        let plan = memory_plan(collector.clone());
        let resolver = CapturingResolver::new([collector]);
        let contribution = contribution(&plan, b"artifact bytes", None);

        let outcome = ArtifactPublisher::with_sink_resolver(&never_cancelled, &resolver)
            .publish(&plan, [contribution]);

        assert_complete_artifact(&outcome, b"artifact bytes");
        assert_eq!(resolver.bytes("test.memory"), b"artifact bytes");
    }

    #[test]
    fn package_interface_publication_uses_the_completed_plan_artifact() {
        let Some(collector) = OutputSinkId::try_new("test.package-interface") else {
            panic!("test collector identity must be valid");
        };

        let plan = memory_artifact_plan(collector.clone(), [package_interface_spec()]);
        let resolver = CapturingResolver::new([collector]);

        let interface = plan
            .package_interface()
            .unwrap_or_else(|| panic!("test plan must retain its package interface"));

        let outcome =
            ArtifactPublisher::with_sink_resolver(&never_cancelled, &resolver).publish(&plan, []);

        assert_complete_artifact(&outcome, interface.bytes());

        assert_eq!(resolver.bytes("test.package-interface"), interface.bytes());
    }

    #[test]
    fn stream_publication_writes_complete_bytes() {
        let Some(stream) = OutputSinkId::try_new("test.stream") else {
            panic!("test stream identity must be valid");
        };

        let plan = stream_plan(stream.clone());
        let resolver = CapturingResolver::new([stream]);
        let contribution = contribution(&plan, b"stream bytes", None);

        let outcome = ArtifactPublisher::with_sink_resolver(&never_cancelled, &resolver)
            .publish(&plan, [contribution]);

        assert_complete_artifact(&outcome, b"stream bytes");
        assert_eq!(resolver.bytes("test.stream"), b"stream bytes");
    }

    #[test]
    fn failed_indirect_writes_flushes_and_commits_discard_buffered_bytes() {
        let Some(stream) = OutputSinkId::try_new("test.partial") else {
            panic!("test stream identity must be valid");
        };

        let plan = stream_plan(stream);
        let contribution = contribution(&plan, b"partial bytes must stay hidden", None);

        for (failure, diagnostic) in [
            (
                IndirectFailure::Write,
                DiagnosticKind::EmissionArtifactWriteFailed,
            ),
            (
                IndirectFailure::Flush,
                DiagnosticKind::EmissionArtifactFlushFailed,
            ),
            (
                IndirectFailure::Commit,
                DiagnosticKind::EmissionArtifactCommitFailed,
            ),
        ] {
            let resolver = FailingResolver::new(failure);

            let outcome = ArtifactPublisher::with_sink_resolver(&never_cancelled, &resolver)
                .publish(&plan, [contribution.clone()]);

            assert!(matches!(
                outcome.status(),
                EmissionStatus::Failed(EmissionFailure::Publication(_))
            ));

            assert_eq!(outcome.diagnostics().diagnostics()[0].kind(), diagnostic);

            match failure {
                IndirectFailure::Write => assert_goal_state_diagnostic_kind(
                    outcome.diagnostics(),
                    DiagnosticKind::EmissionArtifactWriteFailed,
                ),
                IndirectFailure::Flush => assert_goal_state_diagnostic_kind(
                    outcome.diagnostics(),
                    DiagnosticKind::EmissionArtifactFlushFailed,
                ),
                IndirectFailure::Commit => assert_goal_state_diagnostic_kind(
                    outcome.diagnostics(),
                    DiagnosticKind::EmissionArtifactCommitFailed,
                ),
            }

            assert_eq!(resolver.bytes(), b"");
        }
    }

    #[test]
    fn filesystem_publication_obeys_replacement_policy() {
        let Ok(output) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let require_absent = filesystem_plan(output.path(), ReplacementPolicy::RequireAbsent);

        let first = ArtifactPublisher::new(&never_cancelled).publish(
            &require_absent,
            [contribution(&require_absent, b"first", None)],
        );

        assert_complete_artifact(&first, b"first");

        let reference = test_generation_store(output.path()).join("published-generation.json");
        let first_reference = file_bytes(&reference);

        let rejected = ArtifactPublisher::new(&never_cancelled).publish(
            &require_absent,
            [contribution(&require_absent, b"second", None)],
        );

        assert!(matches!(
            rejected.status(),
            EmissionStatus::Failed(EmissionFailure::Publication(_))
        ));

        assert_eq!(
            rejected.diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::EmissionArtifactCommitFailed
        );

        assert_goal_state_diagnostic_kind(
            rejected.diagnostics(),
            DiagnosticKind::EmissionArtifactCommitFailed,
        );

        assert_eq!(file_bytes(&reference), first_reference);

        assert_eq!(
            file_bytes(&output.path().join("application.brayd")),
            b"first"
        );

        let replace = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

        let replaced = ArtifactPublisher::new(&never_cancelled)
            .publish(&replace, [contribution(&replace, b"second", None)]);

        assert_complete_artifact(&replaced, b"second");
        assert_ne!(file_bytes(&reference), first_reference);

        let generation = replaced
            .generation()
            .unwrap_or_else(|| panic!("managed publication must expose a generation"));

        let artifact = &replaced.artifacts().artifacts()[0];

        let path = generation
            .artifact_path(artifact.id())
            .unwrap_or_else(|| panic!("managed artifact path must resolve"));

        let published = generation
            .published_artifact_path(artifact.id())
            .unwrap_or_else(|| panic!("stable artifact path must resolve"));

        let private = path
            .parent()
            .unwrap_or_else(|| panic!("managed artifact must have a private directory"));

        let locator = private
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| panic!("generation locator must be portable"));

        assert_eq!(private.parent(), Some(generation.store()));
        assert!(bray_base::decode_lowercase_hex::<8>(locator).is_some());

        assert_eq!(file_bytes(&path), b"second");
        assert_eq!(file_bytes(&published), b"second");

        let resolved = crate::resolve_published_artifact(
            output.path(),
            replace.request().product(),
            ArtifactKind::DependencyMetadata,
            0,
        )
        .unwrap_or_else(|error| panic!("published manifest must resolve: {error:?}"));

        assert_eq!(resolved, published);
    }

    #[test]
    fn identical_managed_generations_are_reused_by_content_identity() {
        let Ok(output) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

        let first = ArtifactPublisher::new(&never_cancelled)
            .publish(&plan, [contribution(&plan, b"stable", None)]);

        let second = ArtifactPublisher::new(&never_cancelled)
            .publish(&plan, [contribution(&plan, b"stable", None)]);

        assert_complete_artifact(&first, b"stable");
        assert_complete_artifact(&second, b"stable");

        let first_generation = first
            .generation()
            .unwrap_or_else(|| panic!("first managed generation must exist"));

        let second_generation = second
            .generation()
            .unwrap_or_else(|| panic!("second managed generation must exist"));

        assert_eq!(first_generation.identity(), second_generation.identity());

        let artifact = first.artifacts().artifacts()[0].id();

        assert_eq!(
            first_generation.artifact_path(artifact),
            second_generation.artifact_path(artifact)
        );
    }

    #[test]
    fn managed_generation_retention_keeps_only_current_and_preceding_products() {
        let Ok(output) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

        let first = ArtifactPublisher::new(&never_cancelled)
            .publish(&plan, [contribution(&plan, b"first", None)]);

        assert_complete_artifact(&first, b"first");

        let first_generation = first
            .generation()
            .unwrap_or_else(|| panic!("first managed generation must exist"));

        let first_path = first_generation
            .artifact_path(first.artifacts().artifacts()[0].id())
            .unwrap_or_else(|| panic!("first private artifact path must resolve"));

        let second = ArtifactPublisher::new(&never_cancelled)
            .publish(&plan, [contribution(&plan, b"second", None)]);

        assert_complete_artifact(&second, b"second");
        assert!(first_path.exists());

        let second_path = second
            .generation()
            .and_then(|generation| {
                generation.artifact_path(second.artifacts().artifacts()[0].id())
            })
            .unwrap_or_else(|| panic!("second private artifact path must resolve"));

        let third = ArtifactPublisher::new(&never_cancelled)
            .publish(&plan, [contribution(&plan, b"third", None)]);

        assert_complete_artifact(&third, b"third");
        assert!(!first_path.exists());
        assert!(second_path.exists());

        assert_eq!(
            file_bytes(&output.path().join("application.brayd")),
            b"third"
        );
    }

    #[test]
    fn replacement_removes_public_artifacts_absent_from_the_new_generation() {
        let Ok(output) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let interface = output.path().join("application.brayint");
        let metadata = output.path().join("application.brayd");

        let first = filesystem_artifact_plan([
            (package_interface_spec(), interface.clone()),
            (required_dependency_metadata_spec(), metadata.clone()),
        ]);

        let first_outcome = ArtifactPublisher::new(&never_cancelled).publish(
            &first,
            [contribution_for(
                &first,
                ArtifactKind::DependencyMetadata,
                b"metadata",
                None,
                None,
            )],
        );

        assert!(matches!(first_outcome.status(), EmissionStatus::Complete));
        assert!(metadata.is_file());

        let replacement = filesystem_artifact_plan([(package_interface_spec(), interface)]);

        let replacement_outcome =
            ArtifactPublisher::new(&never_cancelled).publish(&replacement, []);

        assert!(matches!(
            replacement_outcome.status(),
            EmissionStatus::Complete
        ));

        assert!(!metadata.exists());
    }

    #[test]
    fn concurrent_product_publishers_leave_one_complete_visible_generation() {
        let Ok(output) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let plan = Arc::new(filesystem_plan(
            output.path(),
            ReplacementPolicy::ReplaceExisting,
        ));

        let barrier = Arc::new(Barrier::new(3));

        let outcomes = std::thread::scope(|scope| {
            let publish = |bytes: &'static [u8]| {
                let plan = Arc::clone(&plan);
                let barrier = Arc::clone(&barrier);

                scope.spawn(move || {
                    let contribution = contribution(&plan, bytes, None);
                    barrier.wait();

                    ArtifactPublisher::new(&never_cancelled).publish(&plan, [contribution])
                })
            };

            let first = publish(b"first concurrent product");
            let second = publish(b"second concurrent product");
            barrier.wait();

            [
                first
                    .join()
                    .unwrap_or_else(|_| panic!("first publisher must finish")),
                second
                    .join()
                    .unwrap_or_else(|_| panic!("second publisher must finish")),
            ]
        });

        assert!(
            outcomes
                .iter()
                .all(|outcome| matches!(outcome.status(), EmissionStatus::Complete))
        );

        let visible = file_bytes(&output.path().join("application.brayd"));

        assert!(matches!(
            visible.as_slice(),
            b"first concurrent product" | b"second concurrent product"
        ));

        let resolved = crate::resolve_published_artifact(
            output.path(),
            plan.request().product(),
            ArtifactKind::DependencyMetadata,
            0,
        )
        .unwrap_or_else(|error| panic!("visible generation must resolve: {error:?}"));

        assert_eq!(file_bytes(&resolved), visible);
    }

    #[test]
    fn corrupted_existing_generation_is_rejected_as_a_collision() {
        let Ok(output) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

        let first = ArtifactPublisher::new(&never_cancelled)
            .publish(&plan, [contribution(&plan, b"stable", None)]);

        assert_complete_artifact(&first, b"stable");

        let generation = first
            .generation()
            .unwrap_or_else(|| panic!("managed generation must exist"));

        let artifact = &first.artifacts().artifacts()[0];

        let path = generation
            .artifact_path(artifact.id())
            .unwrap_or_else(|| panic!("managed artifact path must resolve"));

        std::fs::write(path, b"broken")
            .unwrap_or_else(|error| panic!("test generation must be corrupted: {error}"));

        let outcome = ArtifactPublisher::new(&never_cancelled)
            .publish(&plan, [contribution(&plan, b"stable", None)]);

        assert!(matches!(
            outcome.status(),
            EmissionStatus::Failed(EmissionFailure::Publication(_))
        ));

        assert_eq!(
            outcome.diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::EmissionGenerationCollision
        );

        assert_goal_state_diagnostic_kind(
            outcome.diagnostics(),
            DiagnosticKind::EmissionGenerationCollision,
        );

        assert!(outcome.artifacts().artifacts().is_empty());
        assert!(outcome.generation().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn existing_generation_reuse_requires_manifest_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let Ok(output) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

        let first = ArtifactPublisher::new(&never_cancelled)
            .publish(&plan, [contribution(&plan, b"stable", None)]);

        assert_complete_artifact(&first, b"stable");

        let generation = first
            .generation()
            .unwrap_or_else(|| panic!("managed generation must exist"));

        let artifact = &first.artifacts().artifacts()[0];

        let path = generation
            .artifact_path(artifact.id())
            .unwrap_or_else(|| panic!("managed artifact path must resolve"));

        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
            .unwrap_or_else(|error| panic!("test permissions must be changed: {error}"));

        let outcome = ArtifactPublisher::new(&never_cancelled)
            .publish(&plan, [contribution(&plan, b"stable", None)]);

        assert_eq!(
            outcome.diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::EmissionGenerationCollision
        );

        assert!(matches!(
            outcome.status(),
            EmissionStatus::Failed(EmissionFailure::Publication(_))
        ));
    }

    #[test]
    fn cancellation_observed_after_reference_commit_cannot_retract_success() {
        let Ok(output) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let reference = test_generation_store(output.path()).join("published-generation.json");
        let cancellation = || reference.exists();
        let plan = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

        let outcome = ArtifactPublisher::new(&cancellation)
            .publish(&plan, [contribution(&plan, b"complete", None)]);

        assert_complete_artifact(&outcome, b"complete");
        assert!(reference.is_file());
    }

    #[test]
    fn cancellation_preserves_the_published_generation() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let plan = filesystem_plan(directory.path(), ReplacementPolicy::ReplaceExisting);

        let existing = ArtifactPublisher::new(&never_cancelled)
            .publish(&plan, [contribution(&plan, b"existing", None)]);

        assert_complete_artifact(&existing, b"existing");

        let reference = test_generation_store(directory.path()).join("published-generation.json");
        let existing_reference = file_bytes(&reference);

        let existing_artifact = existing
            .generation()
            .and_then(|generation| {
                generation.published_artifact_path(existing.artifacts().artifacts()[0].id())
            })
            .unwrap_or_else(|| panic!("stable existing artifact path must resolve"));

        let contribution = contribution(&plan, b"replacement", None);
        let outcome = ArtifactPublisher::new(&always_cancelled).publish(&plan, [contribution]);

        assert!(matches!(outcome.status(), EmissionStatus::Cancelled));
        assert!(outcome.artifacts().artifacts().is_empty());
        assert!(outcome.diagnostics().is_empty());
        assert_eq!(file_bytes(&reference), existing_reference);
        assert_eq!(file_bytes(&existing_artifact), b"existing");
    }

    #[test]
    fn failed_generation_exposes_no_partial_artifacts_or_reference() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let interface_path = directory.path().join("application.brayi");
        let metadata_path = directory.path().join("application.brayd");

        let plan = filesystem_artifact_plan([
            (package_interface_spec(), interface_path.clone()),
            (required_dependency_metadata_spec(), metadata_path.clone()),
        ]);

        let wrong_digest = ArtifactDigest::try_new(ArtifactDigestAlgorithm::Blake3, [0_u8; 32])
            .unwrap_or_else(|| panic!("test digest must be valid"));

        let metadata = contribution_for(
            &plan,
            ArtifactKind::DependencyMetadata,
            b"metadata",
            Some(wrong_digest),
            None,
        );

        let outcome = ArtifactPublisher::new(&never_cancelled).publish(&plan, [metadata]);

        assert!(matches!(
            outcome.status(),
            EmissionStatus::Failed(EmissionFailure::InvalidContribution(_))
        ));

        assert!(outcome.artifacts().artifacts().is_empty());
        assert!(outcome.generation().is_none());

        assert!(
            !directory
                .path()
                .join(".bray/application/published-generation.json")
                .exists()
        );
    }

    #[test]
    fn cancellation_discards_private_generation_without_partial_records() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let interface_path = directory.path().join("application.brayi");
        let metadata_path = directory.path().join("application.brayd");

        let plan = filesystem_artifact_plan([
            (package_interface_spec(), interface_path.clone()),
            (required_dependency_metadata_spec(), metadata_path.clone()),
        ]);

        let metadata = contribution_for(
            &plan,
            ArtifactKind::DependencyMetadata,
            b"metadata",
            None,
            None,
        );

        let observations = AtomicUsize::new(0);
        let cancellation = || observations.fetch_add(1, Ordering::AcqRel) >= 4;
        let outcome = ArtifactPublisher::new(&cancellation).publish(&plan, [metadata]);

        assert!(matches!(outcome.status(), EmissionStatus::Cancelled));
        assert!(outcome.artifacts().artifacts().is_empty());
        assert!(outcome.generation().is_none());

        assert!(
            !directory
                .path()
                .join(".bray/application/published-generation.json")
                .exists()
        );
    }

    #[test]
    fn digest_mismatches_fail_before_any_bytes_are_published() {
        let Some(collector) = OutputSinkId::try_new("test.digest") else {
            panic!("test collector identity must be valid");
        };

        let plan = memory_plan(collector.clone());
        let resolver = CapturingResolver::new([collector]);

        let Some(wrong_digest) =
            ArtifactDigest::try_new(ArtifactDigestAlgorithm::Blake3, [0_u8; 32])
        else {
            panic!("test digest must be valid");
        };

        let contribution = contribution(&plan, b"content", Some(wrong_digest));

        let outcome = ArtifactPublisher::with_sink_resolver(&never_cancelled, &resolver)
            .publish(&plan, [contribution]);

        assert!(matches!(
            outcome.status(),
            EmissionStatus::Failed(EmissionFailure::InvalidContribution(_))
        ));

        assert_eq!(
            outcome.diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::EmissionArtifactDigestMismatch
        );

        assert_goal_state_diagnostic_kind(
            outcome.diagnostics(),
            DiagnosticKind::EmissionArtifactDigestMismatch,
        );

        let diagnostic = &outcome.diagnostics().diagnostics()[0];

        let Some(expected) = diagnostic
            .args()
            .iter()
            .find(|arg| arg.name() == DiagnosticArgName::ExpectedArtifactDigest)
        else {
            panic!("digest mismatch must retain the declared digest");
        };

        let DiagnosticArgValue::ArtifactDigest(expected) = expected.value() else {
            panic!("expected digest argument must remain typed");
        };

        let Some(actual) = diagnostic
            .args()
            .iter()
            .find(|arg| arg.name() == DiagnosticArgName::ActualArtifactDigest)
        else {
            panic!("digest mismatch must retain the measured digest");
        };

        let DiagnosticArgValue::ArtifactDigest(actual) = actual.value() else {
            panic!("actual digest argument must remain typed");
        };

        assert_eq!(expected.bytes(), &[0_u8; 32]);
        assert_eq!(actual.bytes(), blake3::hash(b"content").as_bytes());

        assert_eq!(resolver.bytes("test.digest"), b"");
    }

    #[test]
    fn optional_structural_failures_warn_and_omit_the_artifact() {
        let Some(collector) = OutputSinkId::try_new("test.optional") else {
            panic!("test collector identity must be valid");
        };

        let plan = memory_artifact_plan(
            collector.clone(),
            [package_interface_spec(), dependency_metadata_spec()],
        );

        let resolver = CapturingResolver::new([collector]);

        let metadata = contribution_for(
            &plan,
            ArtifactKind::DependencyMetadata,
            b"metadata",
            None,
            Some(ArtifactProducer::DependencyMetadata(
                DependencyMetadataProducerId::new(1),
            )),
        );

        let outcome = ArtifactPublisher::with_sink_resolver(&never_cancelled, &resolver)
            .publish(&plan, [metadata]);

        let artifacts = outcome.artifacts();

        assert!(
            matches!(outcome.status(), EmissionStatus::Complete),
            "unexpected emission outcome: {outcome:#?}"
        );

        assert_eq!(artifacts.artifacts().len(), 1);
        assert_eq!(outcome.diagnostics().warnings().count(), 1);

        assert_eq!(
            outcome.diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::EmissionInvalidContribution
        );

        assert_goal_state_diagnostic_kind(
            outcome.diagnostics(),
            DiagnosticKind::EmissionInvalidContribution,
        );
    }

    #[test]
    fn mixed_publication_warnings_follow_canonical_artifact_order() {
        let Some(collector) = OutputSinkId::try_new("test.order") else {
            panic!("test collector identity must be valid");
        };

        let plan = memory_artifact_plan(
            collector.clone(),
            [
                package_interface_spec(),
                dependency_metadata_spec(),
                executable_spec(),
            ],
        );

        let resolver =
            CapturingResolver::failing_open([collector], ArtifactKind::DependencyMetadata);

        let metadata = contribution_for(
            &plan,
            ArtifactKind::DependencyMetadata,
            b"metadata",
            None,
            None,
        );

        let Some(wrong_digest) =
            ArtifactDigest::try_new(ArtifactDigestAlgorithm::Blake3, [0_u8; 32])
        else {
            panic!("test digest must be valid");
        };

        let executable = contribution_for(
            &plan,
            ArtifactKind::Executable,
            b"executable",
            Some(wrong_digest),
            None,
        );

        let outcome = ArtifactPublisher::with_sink_resolver(&never_cancelled, &resolver)
            .publish(&plan, [executable, metadata]);

        assert!(matches!(outcome.status(), EmissionStatus::Complete));
        assert_eq!(outcome.artifacts().artifacts().len(), 1);

        assert_eq!(
            outcome
                .diagnostics()
                .iter()
                .map(|diagnostic| diagnostic.kind())
                .collect::<Vec<_>>(),
            vec![
                DiagnosticKind::EmissionArtifactOpenFailed,
                DiagnosticKind::EmissionArtifactDigestMismatch,
            ]
        );

        assert_goal_state_diagnostic_kind(
            outcome.diagnostics(),
            DiagnosticKind::EmissionArtifactOpenFailed,
        );

        assert_goal_state_diagnostic_kind(
            outcome.diagnostics(),
            DiagnosticKind::EmissionArtifactDigestMismatch,
        );
    }

    #[test]
    fn missing_required_contributions_emit_structured_errors() {
        let Some(collector) = OutputSinkId::try_new("test.missing") else {
            panic!("test collector identity must be valid");
        };

        let plan = memory_plan(collector);
        let outcome = ArtifactPublisher::new(&never_cancelled).publish(&plan, []);

        assert!(matches!(
            outcome.status(),
            EmissionStatus::Failed(EmissionFailure::MissingContribution(_))
        ));

        assert_eq!(outcome.diagnostics().len(), 1);

        assert_eq!(
            outcome.diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::EmissionMissingContribution
        );

        assert_goal_state_diagnostic_kind(
            outcome.diagnostics(),
            DiagnosticKind::EmissionMissingContribution,
        );

        assert_eq!(
            outcome.diagnostics().diagnostics()[0].severity(),
            SeverityKind::Error
        );
    }

    #[test]
    fn linked_products_and_companions_publish_from_validated_staging() {
        let cases = [
            linked_case(
                LinkedProductKind::Executable,
                ArtifactKind::Executable,
                LinkedArtifactKind::Executable,
                None,
            ),
            linked_case(
                LinkedProductKind::SharedLibrary,
                ArtifactKind::SharedLibrary,
                LinkedArtifactKind::SharedLibrary,
                Some(LinkedArtifactKind::ImportLibrary),
            ),
            linked_case(
                LinkedProductKind::Executable,
                ArtifactKind::Executable,
                LinkedArtifactKind::Executable,
                Some(LinkedArtifactKind::DebugCompanion),
            ),
            linked_case(
                LinkedProductKind::StaticLibrary,
                ArtifactKind::StaticLibrary,
                LinkedArtifactKind::StaticLibrary,
                Some(LinkedArtifactKind::PlatformCompanion),
            ),
        ];

        for case in cases {
            let Ok(directory) = tempfile::tempdir() else {
                panic!("test output directory must be created");
            };

            let fixture = linked_publication_fixture(directory.path(), case);

            let outcome =
                valid_linked_outcome(ArtifactPublisher::new(&never_cancelled).publish_linked(
                    &fixture.emission,
                    [],
                    &fixture.link,
                    &fixture.outcome,
                ));

            assert!(
                matches!(outcome.status(), EmissionStatus::Complete),
                "unexpected linked emission outcome: {outcome:#?}"
            );

            assert_eq!(
                outcome.artifacts().artifacts().len(),
                fixture.final_artifacts.len()
            );

            let generation = outcome
                .generation()
                .unwrap_or_else(|| panic!("managed publication must expose its generation"));

            for (record, artifact) in outcome
                .artifacts()
                .artifacts()
                .iter()
                .zip(&fixture.final_artifacts)
            {
                let path = generation
                    .artifact_path(record.id())
                    .unwrap_or_else(|| panic!("published artifact path must resolve"));

                assert_eq!(file_bytes(&path), artifact.bytes);
                assert!(!artifact.staging_path.exists());
            }

            assert!(!fixture.input_path.exists());
        }
    }

    #[test]
    fn linked_publication_reports_an_emission_plan_without_linked_artifacts() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let fixture = linked_publication_fixture(
            directory.path(),
            linked_case(
                LinkedProductKind::Executable,
                ArtifactKind::Executable,
                LinkedArtifactKind::Executable,
                None,
            ),
        );

        let Some(collector) = OutputSinkId::try_new("test.missing-linked-plan") else {
            panic!("test collector identity must be valid");
        };

        let outcome =
            valid_linked_outcome(ArtifactPublisher::new(&never_cancelled).publish_linked(
                &memory_plan(collector),
                [],
                &fixture.link,
                &fixture.outcome,
            ));

        assert!(matches!(
            outcome.status(),
            EmissionStatus::Failed(EmissionFailure::Planning)
        ));

        assert_eq!(outcome.diagnostics().len(), 1);

        assert_goal_state_diagnostic_kind(
            outcome.diagnostics(),
            DiagnosticKind::EmissionLinkedPlanMissing,
        );
    }

    #[test]
    fn linked_output_validation_preserves_every_existing_destination() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let case = linked_case(
            LinkedProductKind::SharedLibrary,
            ArtifactKind::SharedLibrary,
            LinkedArtifactKind::SharedLibrary,
            Some(LinkedArtifactKind::ImportLibrary),
        );

        let fixture = linked_publication_fixture(directory.path(), case);
        let missing = &fixture.final_artifacts[1];

        std::fs::remove_file(&missing.staging_path)
            .unwrap_or_else(|error| panic!("test companion staging must be removed: {error}"));

        for artifact in &fixture.final_artifacts {
            std::fs::write(&artifact.final_path, b"existing")
                .unwrap_or_else(|error| panic!("test destination must be written: {error}"));
        }

        let outcome =
            valid_linked_outcome(ArtifactPublisher::new(&never_cancelled).publish_linked(
                &fixture.emission,
                [],
                &fixture.link,
                &fixture.outcome,
            ));

        assert!(matches!(
            outcome.status(),
            EmissionStatus::Failed(EmissionFailure::InvalidContribution(_))
        ));

        assert_eq!(
            outcome.diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::EmissionArtifactReadFailed
        );

        assert_goal_state_diagnostic_kind(
            outcome.diagnostics(),
            DiagnosticKind::EmissionArtifactReadFailed,
        );

        for artifact in &fixture.final_artifacts {
            assert_eq!(file_bytes(&artifact.final_path), b"existing");
            assert!(!artifact.staging_path.exists());
        }

        assert!(!fixture.input_path.exists());
    }

    #[test]
    fn invalid_linked_output_length_preserves_the_existing_destination() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let case = linked_case(
            LinkedProductKind::Executable,
            ArtifactKind::Executable,
            LinkedArtifactKind::Executable,
            None,
        );

        let mut fixture = linked_publication_fixture(directory.path(), case);
        let artifact = &fixture.final_artifacts[0];

        std::fs::write(&artifact.final_path, b"existing")
            .unwrap_or_else(|error| panic!("test destination must be written: {error}"));

        let linked = fixture.link.outputs().iter().map(|output| {
            LinkedArtifact::new(output.kind(), output.destination().id(), NonZeroU64::MIN)
        });

        fixture.outcome = LinkOutcome::try_complete(&fixture.link, linked, DiagnosticBag::new())
            .unwrap_or_else(|error| panic!("test link outcome must be valid: {error:?}"));

        let outcome =
            valid_linked_outcome(ArtifactPublisher::new(&never_cancelled).publish_linked(
                &fixture.emission,
                [],
                &fixture.link,
                &fixture.outcome,
            ));

        assert!(matches!(
            outcome.status(),
            EmissionStatus::Failed(EmissionFailure::InvalidContribution(_))
        ));

        assert_eq!(
            outcome.diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::EmissionArtifactLengthMismatch
        );

        assert_goal_state_diagnostic_kind(
            outcome.diagnostics(),
            DiagnosticKind::EmissionArtifactLengthMismatch,
        );

        assert_eq!(file_bytes(&artifact.final_path), b"existing");
        assert!(!artifact.staging_path.exists());
        assert!(!fixture.input_path.exists());
    }

    #[test]
    fn link_failure_and_cancellation_preserve_existing_destinations() {
        for failed in [true, false] {
            let Ok(directory) = tempfile::tempdir() else {
                panic!("test output directory must be created");
            };

            let case = linked_case(
                LinkedProductKind::Executable,
                ArtifactKind::Executable,
                LinkedArtifactKind::Executable,
                None,
            );

            let mut fixture = linked_publication_fixture(directory.path(), case);
            let artifact = &fixture.final_artifacts[0];

            std::fs::write(&artifact.final_path, b"existing")
                .unwrap_or_else(|error| panic!("test destination must be written: {error}"));

            fixture.outcome = if failed {
                LinkOutcome::failed(&fixture.link, LinkFailure::Invocation, DiagnosticBag::new())
            } else {
                LinkOutcome::cancelled(DiagnosticBag::new())
            };

            let outcome =
                valid_linked_outcome(ArtifactPublisher::new(&never_cancelled).publish_linked(
                    &fixture.emission,
                    [],
                    &fixture.link,
                    &fixture.outcome,
                ));

            assert!(!matches!(outcome.status(), EmissionStatus::Complete));
            assert_eq!(file_bytes(&artifact.final_path), b"existing");
            assert!(!artifact.staging_path.exists());
            assert!(!fixture.input_path.exists());
        }
    }

    #[test]
    fn cancellation_before_linked_validation_cleans_private_staging() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let case = linked_case(
            LinkedProductKind::Executable,
            ArtifactKind::Executable,
            LinkedArtifactKind::Executable,
            None,
        );

        let fixture = linked_publication_fixture(directory.path(), case);
        let artifact = &fixture.final_artifacts[0];

        std::fs::write(&artifact.final_path, b"existing")
            .unwrap_or_else(|error| panic!("test destination must be written: {error}"));

        let outcome =
            valid_linked_outcome(ArtifactPublisher::new(&always_cancelled).publish_linked(
                &fixture.emission,
                [],
                &fixture.link,
                &fixture.outcome,
            ));

        assert!(matches!(outcome.status(), EmissionStatus::Cancelled));
        assert_eq!(file_bytes(&artifact.final_path), b"existing");
        assert!(!artifact.staging_path.exists());
        assert!(!fixture.input_path.exists());
    }

    #[derive(Clone, Copy)]
    struct LinkedPublicationCase {
        product: LinkedProductKind,
        artifact: ArtifactKind,
        linked: LinkedArtifactKind,
        companion: Option<LinkedArtifactKind>,
    }

    struct LinkedPublicationFixture {
        emission: EmissionPlan,
        link: LinkPlan,
        outcome: LinkOutcome,
        input_path: PathBuf,
        final_artifacts: Vec<LinkedFinalArtifact>,
    }

    struct LinkedFinalArtifact {
        final_path: PathBuf,
        staging_path: PathBuf,
        bytes: &'static [u8],
    }

    const fn linked_case(
        product: LinkedProductKind,
        artifact: ArtifactKind,
        linked: LinkedArtifactKind,
        companion: Option<LinkedArtifactKind>,
    ) -> LinkedPublicationCase {
        LinkedPublicationCase {
            product,
            artifact,
            linked,
            companion,
        }
    }

    fn linked_publication_fixture(
        directory: &Path,
        case: LinkedPublicationCase,
    ) -> LinkedPublicationFixture {
        let product_kind = match case.product {
            LinkedProductKind::Executable => ProductKind::Executable,
            LinkedProductKind::SharedLibrary | LinkedProductKind::StaticLibrary => {
                ProductKind::Library
            }
        };

        let executable_host = (product_kind == ProductKind::Executable)
            .then(crate::test_support::executable_host_contract);

        let mut requested = vec![RequestedArtifact::new(
            case.artifact,
            ArtifactRequirement::Required,
        )];

        if case.companion.is_some() {
            requested.push(RequestedArtifact::new(
                ArtifactKind::LinkedCompanion,
                ArtifactRequirement::Required,
            ));
        }

        let Ok(request) = EmissionRequest::try_new(
            product_identity(),
            product_kind,
            executable_host,
            target_identity(),
            RequestedArtifactDestination::FilesystemDirectory(directory.to_owned().into()),
            requested,
            ReplacementPolicy::ReplaceExisting,
        ) else {
            panic!("test linked emission request must be valid");
        };

        let primary_final = directory.join("primary.final");
        let primary_staging = directory.join("primary.stage");

        let mut planned = vec![linked_planned_artifact(
            &request,
            case.artifact,
            ArtifactRole::Product,
            primary_final.clone(),
        )];

        let mut final_artifacts = vec![LinkedFinalArtifact {
            final_path: primary_final,
            staging_path: primary_staging.clone(),
            bytes: b"primary linked bytes",
        }];

        if case.companion.is_some() {
            let companion_final = directory.join("companion.final");
            let companion_staging = directory.join("companion.stage");

            planned.push(linked_planned_artifact(
                &request,
                ArtifactKind::LinkedCompanion,
                ArtifactRole::Companion,
                companion_final.clone(),
            ));

            final_artifacts.push(LinkedFinalArtifact {
                final_path: companion_final,
                staging_path: companion_staging,
                bytes: b"companion linked bytes",
            });
        }

        let Ok(emission) = EmissionPlan::try_new(request, None, None, planned, [], None) else {
            panic!("test linked emission plan must be valid");
        };

        let input_path = directory.join("input.o");

        std::fs::write(&input_path, b"object")
            .unwrap_or_else(|error| panic!("test link input must be written: {error}"));

        let mut builder = LinkPlanBuilder::new(
            product_identity(),
            case.product,
            linked_target(case.product),
            linked_driver_identity(),
            match case.product {
                LinkedProductKind::Executable | LinkedProductKind::SharedLibrary => {
                    bray_linker::LinkStartupMode::PlatformCompilerDriver
                }
                LinkedProductKind::StaticLibrary => bray_linker::LinkStartupMode::NotApplicable,
            },
            LinkPolicy::new(
                bray_linker::DeadStripPolicy::Preserve,
                SectionGarbageCollectionPolicy::Preserve,
                if case.companion == Some(LinkedArtifactKind::DebugCompanion) {
                    DebugLinkPolicy::Companion
                } else {
                    DebugLinkPolicy::None
                },
                None,
            ),
        );

        builder.push_input(linked_input(&input_path));

        builder.push_output(linked_output(0, case.linked, &primary_staging));

        if let Some(companion) = case.companion {
            builder.push_output(linked_output(
                1,
                companion,
                &final_artifacts[1].staging_path,
            ));
        }

        if case.product == LinkedProductKind::Executable {
            builder.set_executable_host(crate::test_support::executable_host_contract());
        }

        let link = builder
            .finish()
            .unwrap_or_else(|error| panic!("test link plan must be valid: {error:?}"));

        for artifact in &final_artifacts {
            std::fs::write(&artifact.staging_path, artifact.bytes)
                .unwrap_or_else(|error| panic!("test linked staging must be written: {error}"));
        }

        let artifacts = link
            .outputs()
            .iter()
            .zip(&final_artifacts)
            .map(|(output, artifact)| {
                let Some(byte_len) = NonZeroU64::new(
                    u64::try_from(artifact.bytes.len())
                        .unwrap_or_else(|_| panic!("test linked length must be representable")),
                ) else {
                    panic!("test linked staging must be nonempty");
                };

                LinkedArtifact::new(output.kind(), output.destination().id(), byte_len)
            });

        let outcome = LinkOutcome::try_complete(&link, artifacts, DiagnosticBag::new())
            .unwrap_or_else(|error| panic!("test link outcome must be valid: {error:?}"));

        LinkedPublicationFixture {
            emission,
            link,
            outcome,
            input_path,
            final_artifacts,
        }
    }

    fn linked_planned_artifact(
        request: &EmissionRequest,
        kind: ArtifactKind,
        role: ArtifactRole,
        path: PathBuf,
    ) -> PlannedArtifact {
        let root = path
            .parent()
            .unwrap_or_else(|| panic!("test linked path must have a parent"));

        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_else(|| panic!("test linked path must have a portable name"));

        PlannedArtifact::new(
            ArtifactId::new(request.product().clone(), kind, 0),
            ArtifactRequirement::Required,
            role,
            ArtifactProducer::Linker(LinkerProducerId::new(0)),
            PlannedArtifactDestination::Publish(OutputSink::ManagedFilesystem {
                root: root.to_owned(),
                artifact: crate::ManagedArtifactPath::try_new(name)
                    .unwrap_or_else(|| panic!("test managed path must be valid")),
                published: path,
            }),
        )
    }

    fn linked_input(path: &Path) -> LinkInput {
        LinkInput::try_new(
            LinkInputId::new(0),
            LinkInputKind::RelocatableObject,
            LinkInputSource::file(path),
            LinkInputProvenance::Product,
            LinkInputMode::Ordinary,
        )
        .unwrap_or_else(|error| panic!("test link input must be valid: {error:?}"))
    }

    fn linked_output(ordinal: u32, kind: LinkedArtifactKind, path: &Path) -> PlannedLinkedArtifact {
        let Some(path_key) = StagingPathKey::try_new(path.to_string_lossy().into_owned()) else {
            panic!("test staging path identity must be valid");
        };

        let destination =
            StagingDestination::try_new(StagingDestinationId::new(ordinal), path, path_key)
                .unwrap_or_else(|error| {
                    panic!("test staging destination must be valid: {error:?}")
                });

        PlannedLinkedArtifact::new(kind, LinkedArtifactRequirement::Required, destination)
    }

    fn linked_target(product: LinkedProductKind) -> LinkTarget {
        LinkTarget::try_new(
            target_identity(),
            "x86_64-unknown-linux-gnu",
            TargetArchitecture::X86_64,
            ObjectFormat::Elf,
            RelocationModel::PositionIndependent,
            CodeModel::Small,
            if product == LinkedProductKind::StaticLibrary {
                LinkModel::Static
            } else {
                LinkModel::Dynamic
            },
        )
        .unwrap_or_else(|error| panic!("test link target must be valid: {error:?}"))
    }

    fn linked_driver_identity() -> LinkerDriverIdentity {
        LinkerDriverIdentity::try_new(LinkerDriverKind::EmbeddedLld, "lld", "1", "20")
            .unwrap_or_else(|| panic!("test linker identity must be valid"))
    }

    fn memory_plan(collector: OutputSinkId) -> EmissionPlan {
        memory_artifact_plan(collector, [required_dependency_metadata_spec()])
    }

    fn stream_plan(stream: OutputSinkId) -> EmissionPlan {
        publication_plan(
            RequestedArtifactDestination::Stream(stream.clone()),
            OutputSink::Stream(stream),
            ReplacementPolicy::RequireAbsent,
        )
    }

    fn filesystem_plan(path: &Path, replacement: ReplacementPolicy) -> EmissionPlan {
        publication_plan(
            RequestedArtifactDestination::FilesystemDirectory(path.to_owned().into()),
            OutputSink::ManagedFilesystem {
                root: path.to_owned(),
                artifact: crate::ManagedArtifactPath::try_new("application.brayd")
                    .unwrap_or_else(|| panic!("test managed path must be valid")),
                published: path.join("application.brayd"),
            },
            replacement,
        )
    }

    fn filesystem_artifact_plan(
        artifacts: impl IntoIterator<Item = (TestArtifactSpec, PathBuf)>,
    ) -> EmissionPlan {
        let artifacts: Vec<_> = artifacts.into_iter().collect();

        let package_interface = artifacts
            .iter()
            .any(|(artifact, _)| artifact.kind == ArtifactKind::PackageInterface)
            .then(interface_artifact);

        let requested = artifacts
            .iter()
            .map(|(artifact, _)| RequestedArtifact::new(artifact.kind, artifact.requirement));

        let root = artifacts
            .first()
            .and_then(|(_, path)| path.parent())
            .unwrap_or_else(|| panic!("test artifact paths must share a parent"))
            .to_owned();

        let Ok(request) = EmissionRequest::try_new(
            product_identity(),
            ProductKind::Library,
            None,
            target_identity(),
            RequestedArtifactDestination::FilesystemDirectory(root.clone().into()),
            requested,
            ReplacementPolicy::ReplaceExisting,
        ) else {
            panic!("test filesystem publication request must be valid");
        };

        let product = request.product().clone();

        let planned = artifacts.into_iter().map(|(artifact, path)| {
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_else(|| panic!("test artifact path must have a portable name"));

            PlannedArtifact::new(
                ArtifactId::new(product.clone(), artifact.kind, 0),
                artifact.requirement,
                artifact.role,
                artifact.producer,
                PlannedArtifactDestination::Publish(OutputSink::ManagedFilesystem {
                    root: root.clone(),
                    artifact: crate::ManagedArtifactPath::try_new(name)
                        .unwrap_or_else(|| panic!("test managed path must be valid")),
                    published: path,
                }),
            )
        });

        let Ok(plan) = EmissionPlan::try_new(request, None, None, planned, [], package_interface)
        else {
            panic!("test filesystem publication plan must be valid");
        };

        plan
    }

    fn publication_plan(
        destination: RequestedArtifactDestination,
        sink: OutputSink,
        replacement: ReplacementPolicy,
    ) -> EmissionPlan {
        let Ok(request) = EmissionRequest::try_new(
            product_identity(),
            ProductKind::Library,
            None,
            target_identity(),
            destination,
            [RequestedArtifact::new(
                ArtifactKind::DependencyMetadata,
                ArtifactRequirement::Required,
            )],
            replacement,
        ) else {
            panic!("test publication request must be valid");
        };

        let artifact = PlannedArtifact::new(
            ArtifactId::new(
                request.product().clone(),
                ArtifactKind::DependencyMetadata,
                0,
            ),
            ArtifactRequirement::Required,
            ArtifactRole::Companion,
            ArtifactProducer::DependencyMetadata(DependencyMetadataProducerId::new(0)),
            PlannedArtifactDestination::Publish(sink),
        );

        let Ok(plan) = EmissionPlan::try_new(request, None, None, [artifact], [], None) else {
            panic!("test publication plan must be valid");
        };

        plan
    }

    fn memory_artifact_plan(
        collector: OutputSinkId,
        artifacts: impl IntoIterator<Item = TestArtifactSpec>,
    ) -> EmissionPlan {
        let artifacts: Vec<_> = artifacts.into_iter().collect();

        let package_interface = artifacts
            .iter()
            .any(|artifact| artifact.kind == ArtifactKind::PackageInterface)
            .then(interface_artifact);

        let requested = artifacts
            .iter()
            .map(|artifact| RequestedArtifact::new(artifact.kind, artifact.requirement));

        let Ok(request) = EmissionRequest::try_new(
            product_identity(),
            ProductKind::Library,
            None,
            target_identity(),
            RequestedArtifactDestination::Memory(collector.clone()),
            requested,
            ReplacementPolicy::RequireAbsent,
        ) else {
            panic!("test memory publication request must be valid");
        };

        let product = request.product().clone();

        let planned = artifacts.into_iter().map(|artifact| {
            let id = ArtifactId::new(product.clone(), artifact.kind, 0);

            PlannedArtifact::new(
                id.clone(),
                artifact.requirement,
                artifact.role,
                artifact.producer,
                PlannedArtifactDestination::Publish(OutputSink::Memory {
                    collector: collector.clone(),
                    artifact: id,
                }),
            )
        });

        let Ok(plan) = EmissionPlan::try_new(request, None, None, planned, [], package_interface)
        else {
            panic!("test memory publication plan must be valid");
        };

        plan
    }

    fn package_interface_spec() -> TestArtifactSpec {
        TestArtifactSpec {
            kind: ArtifactKind::PackageInterface,
            requirement: ArtifactRequirement::Required,
            role: ArtifactRole::Product,
            producer: ArtifactProducer::PackageInterface,
        }
    }

    fn dependency_metadata_spec() -> TestArtifactSpec {
        TestArtifactSpec {
            kind: ArtifactKind::DependencyMetadata,
            requirement: ArtifactRequirement::Optional,
            role: ArtifactRole::Companion,
            producer: ArtifactProducer::DependencyMetadata(DependencyMetadataProducerId::new(0)),
        }
    }

    fn required_dependency_metadata_spec() -> TestArtifactSpec {
        TestArtifactSpec {
            kind: ArtifactKind::DependencyMetadata,
            requirement: ArtifactRequirement::Required,
            role: ArtifactRole::Companion,
            producer: ArtifactProducer::DependencyMetadata(DependencyMetadataProducerId::new(0)),
        }
    }

    fn executable_spec() -> TestArtifactSpec {
        TestArtifactSpec {
            kind: ArtifactKind::Executable,
            requirement: ArtifactRequirement::Optional,
            role: ArtifactRole::Product,
            producer: ArtifactProducer::Linker(LinkerProducerId::new(0)),
        }
    }

    struct TestArtifactSpec {
        kind: ArtifactKind,
        requirement: ArtifactRequirement,
        role: ArtifactRole,
        producer: ArtifactProducer,
    }

    fn contribution(
        plan: &EmissionPlan,
        bytes: &[u8],
        digest: Option<ArtifactDigest>,
    ) -> ArtifactContribution {
        contribution_for(plan, ArtifactKind::DependencyMetadata, bytes, digest, None)
    }

    fn contribution_for(
        plan: &EmissionPlan,
        kind: ArtifactKind,
        bytes: &[u8],
        digest: Option<ArtifactDigest>,
        producer: Option<ArtifactProducer>,
    ) -> ArtifactContribution {
        let Some(planned) = plan
            .published_artifacts()
            .find(|artifact| artifact.id().kind() == kind)
        else {
            panic!("test plan must publish the requested artifact kind");
        };

        let Ok(content) = ArtifactContent::try_memory(bytes.to_vec()) else {
            panic!("test artifact content must be valid");
        };

        let producer = producer.unwrap_or_else(|| planned.producer().clone());

        ArtifactContribution::new(planned.id().clone(), producer, content, digest)
    }

    fn assert_complete_artifact(outcome: &EmissionOutcome, expected: &[u8]) {
        let artifacts = outcome.artifacts();

        let Ok(expected_len) = u64::try_from(expected.len()) else {
            panic!("test artifact length must fit the publication contract");
        };

        assert!(
            matches!(outcome.status(), EmissionStatus::Complete),
            "unexpected emission outcome: {outcome:#?}"
        );

        assert!(outcome.diagnostics().is_empty());

        assert_eq!(artifacts.artifacts().len(), 1);
        assert_eq!(artifacts.artifacts()[0].byte_len(), expected_len);

        assert_eq!(
            artifacts.artifacts()[0].digest().bytes(),
            blake3::hash(expected).as_bytes()
        );
    }

    fn file_bytes(path: &Path) -> Vec<u8> {
        let Ok(bytes) = std::fs::read(path) else {
            panic!("published test artifact must be readable");
        };

        bytes
    }

    fn test_generation_store(root: &Path) -> PathBuf {
        root.join(".bray/application")
    }

    struct CapturingResolver {
        sinks: BTreeMap<String, Arc<Mutex<Vec<u8>>>>,
        fail_open: Option<ArtifactKind>,
    }

    impl CapturingResolver {
        fn new(sinks: impl IntoIterator<Item = OutputSinkId>) -> Self {
            Self::with_failure(sinks, None)
        }

        fn failing_open(sinks: impl IntoIterator<Item = OutputSinkId>, kind: ArtifactKind) -> Self {
            Self::with_failure(sinks, Some(kind))
        }

        fn with_failure(
            sinks: impl IntoIterator<Item = OutputSinkId>,
            fail_open: Option<ArtifactKind>,
        ) -> Self {
            let sinks = sinks
                .into_iter()
                .map(|sink| (sink.as_str().to_owned(), Arc::new(Mutex::new(Vec::new()))))
                .collect();

            Self { sinks, fail_open }
        }

        fn bytes(&self, sink: &str) -> Vec<u8> {
            let Some(bytes) = self.sinks.get(sink) else {
                panic!("test sink must exist");
            };

            let Ok(bytes) = bytes.lock() else {
                panic!("test sink lock must be available");
            };

            bytes.clone()
        }
    }

    impl OutputSinkResolver for CapturingResolver {
        fn open(
            &self,
            sink: IndirectOutputSink<'_>,
            _replacement: ReplacementPolicy,
        ) -> io::Result<Box<dyn OutputSinkTransaction>> {
            let (identity, artifact_kind) = match sink {
                IndirectOutputSink::Memory {
                    collector,
                    artifact,
                } => (collector, Some(artifact.kind())),
                IndirectOutputSink::Stream(stream) => (stream, None),
            };

            if artifact_kind.is_some_and(|kind| self.fail_open == Some(kind)) {
                return Err(io::Error::from(io::ErrorKind::BrokenPipe));
            }

            let Some(bytes) = self.sinks.get(identity.as_str()) else {
                return Err(io::Error::from(io::ErrorKind::NotFound));
            };

            Ok(Box::new(CapturingWriter {
                destination: Arc::clone(bytes),
                buffer: Vec::new(),
            }))
        }
    }

    struct CapturingWriter {
        destination: Arc<Mutex<Vec<u8>>>,
        buffer: Vec<u8>,
    }

    impl Write for CapturingWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.buffer.extend_from_slice(buffer);

            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl OutputSinkTransaction for CapturingWriter {
        fn commit(self: Box<Self>) -> io::Result<()> {
            let Self {
                destination,
                buffer,
            } = *self;

            commit_buffer(destination, buffer)
        }
    }

    struct FailingResolver {
        destination: Arc<Mutex<Vec<u8>>>,
        failure: IndirectFailure,
    }

    impl FailingResolver {
        fn new(failure: IndirectFailure) -> Self {
            Self {
                destination: Arc::new(Mutex::new(Vec::new())),
                failure,
            }
        }

        fn bytes(&self) -> Vec<u8> {
            let Ok(bytes) = self.destination.lock() else {
                panic!("test sink lock must be available");
            };

            bytes.clone()
        }
    }

    impl OutputSinkResolver for FailingResolver {
        fn open(
            &self,
            _sink: IndirectOutputSink<'_>,
            _replacement: ReplacementPolicy,
        ) -> io::Result<Box<dyn OutputSinkTransaction>> {
            Ok(Box::new(FailingTransaction {
                destination: Arc::clone(&self.destination),
                buffer: Vec::new(),
                accepted_write: false,
                failure: self.failure,
            }))
        }
    }

    struct FailingTransaction {
        destination: Arc<Mutex<Vec<u8>>>,
        buffer: Vec<u8>,
        accepted_write: bool,
        failure: IndirectFailure,
    }

    impl Write for FailingTransaction {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if self.failure == IndirectFailure::Write && self.accepted_write {
                return Err(io::Error::from(io::ErrorKind::BrokenPipe));
            }

            let accepted = if self.failure == IndirectFailure::Write {
                buffer.len().min(3)
            } else {
                buffer.len()
            };

            self.buffer.extend_from_slice(&buffer[..accepted]);

            self.accepted_write = true;

            Ok(accepted)
        }

        fn flush(&mut self) -> io::Result<()> {
            if self.failure == IndirectFailure::Flush {
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            } else {
                Ok(())
            }
        }
    }

    impl OutputSinkTransaction for FailingTransaction {
        fn commit(self: Box<Self>) -> io::Result<()> {
            let Self {
                destination,
                buffer,
                failure,
                ..
            } = *self;

            if failure == IndirectFailure::Commit {
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            } else {
                commit_buffer(destination, buffer)
            }
        }
    }

    #[derive(Clone, Copy, Eq, PartialEq)]
    enum IndirectFailure {
        Write,
        Flush,
        Commit,
    }

    fn commit_buffer(destination: Arc<Mutex<Vec<u8>>>, buffer: Vec<u8>) -> io::Result<()> {
        let mut destination = destination
            .lock()
            .map_err(|_| io::Error::from(io::ErrorKind::Other))?;

        *destination = buffer;

        Ok(())
    }

    fn valid_linked_outcome(
        result: Result<EmissionOutcome, crate::EmissionOutcomeBuildError>,
    ) -> EmissionOutcome {
        result.unwrap_or_else(|error| panic!("test linked publication must validate: {error:?}"))
    }

    fn never_cancelled() -> bool {
        false
    }

    fn always_cancelled() -> bool {
        true
    }
}
