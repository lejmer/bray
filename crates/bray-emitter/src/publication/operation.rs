use std::cmp::Ordering;
use std::io::{self, Read, Write};

use bray_base::Cancellation;
use bray_codegen::{ArtifactContent, ArtifactDigest, ArtifactDigestAlgorithm};

use super::content::{
    ContentValidationError, open_content, validate_content, validate_staged_content,
};
use super::diagnostic::{PublicationDiagnostics, PublicationError, PublicationErrorKind};
use super::staging::FilesystemStaging;
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

        let prepared = match prepare_contributions(plan, contributions, &mut diagnostics) {
            Ok(prepared) => prepared,
            Err(error) => return diagnostics.failed(publication_set(plan, []), error),
        };

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

        EmissionOutcome::complete(artifacts, diagnostics)
    }

    fn publish_artifact(
        &self,
        replacement: ReplacementPolicy,
        artifact: PreparedArtifact<'_>,
    ) -> Result<EmittedArtifact, ArtifactPublicationFailure> {
        let planned = artifact.planned;

        let PlannedArtifactDestination::Publish(sink) = planned.destination() else {
            return Err(artifact_failure(
                planned,
                PublicationErrorKind::InvalidContribution,
            ));
        };

        let digest = match sink {
            OutputSink::Filesystem(path) => self.publish_filesystem(
                planned,
                artifact.contribution.content(),
                artifact.contribution.digest(),
                path,
                replacement,
            )?,
            OutputSink::Memory {
                collector,
                artifact: artifact_id,
            } => self.publish_indirect(
                planned,
                artifact.contribution.content(),
                artifact.contribution.digest(),
                IndirectOutputSink::Memory {
                    collector,
                    artifact: artifact_id,
                },
                replacement,
            )?,
            OutputSink::Stream(stream) => self.publish_indirect(
                planned,
                artifact.contribution.content(),
                artifact.contribution.digest(),
                IndirectOutputSink::Stream(stream),
                replacement,
            )?,
        };

        // Publication records own stable plan facts independently of the borrowed plan.
        Ok(EmittedArtifact::new(
            planned.id().clone(),
            sink.clone(),
            planned.producer().clone(),
            planned.role(),
            artifact.contribution.content().byte_len(),
            digest,
        ))
    }

    fn publish_filesystem(
        &self,
        planned: &PlannedArtifact,
        content: &ArtifactContent,
        expected_digest: Option<&ArtifactDigest>,
        destination: &std::path::Path,
        replacement: ReplacementPolicy,
    ) -> Result<ArtifactDigest, ArtifactPublicationFailure> {
        let mut staging = FilesystemStaging::create(destination, planned.id().kind(), replacement)
            .map_err(|error| artifact_failure(planned, PublicationErrorKind::Open(error.kind())))?;

        self.copy_content(planned, content, &mut staging)?;

        if self.cancellation.is_cancelled() {
            return Err(ArtifactPublicationFailure::Cancelled);
        }

        let staging = staging.finish().map_err(|error| {
            artifact_failure(planned, PublicationErrorKind::Flush(error.kind()))
        })?;

        let digest = validate_staged_content(
            staging.path(),
            content.byte_len(),
            expected_digest,
            self.cancellation,
        )
        .map_err(|error| content_failure(planned, error))?;

        if self.cancellation.is_cancelled() {
            return Err(ArtifactPublicationFailure::Cancelled);
        }

        staging.promote(destination, replacement).map_err(|error| {
            artifact_failure(planned, PublicationErrorKind::Commit(error.kind()))
        })?;

        Ok(digest)
    }

    fn publish_indirect(
        &self,
        planned: &PlannedArtifact,
        content: &ArtifactContent,
        expected_digest: Option<&ArtifactDigest>,
        sink: IndirectOutputSink<'_>,
        replacement: ReplacementPolicy,
    ) -> Result<ArtifactDigest, ArtifactPublicationFailure> {
        let digest = validate_content(content, expected_digest, self.cancellation)
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

        self.copy_content(planned, content, transaction.as_mut())?;

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

    fn copy_content(
        &self,
        planned: &PlannedArtifact,
        content: &ArtifactContent,
        writer: &mut dyn Write,
    ) -> Result<(), ArtifactPublicationFailure> {
        let mut reader = open_content(content)
            .map_err(|kind| artifact_failure(planned, PublicationErrorKind::Read(kind)))?;

        let mut buffer = [0_u8; COPY_BUFFER_LEN];

        loop {
            if self.cancellation.is_cancelled() {
                return Err(ArtifactPublicationFailure::Cancelled);
            }

            let read = reader.read(&mut buffer).map_err(|error| {
                artifact_failure(planned, PublicationErrorKind::Read(error.kind()))
            })?;

            if read == 0 {
                break;
            }

            writer.write_all(&buffer[..read]).map_err(|error| {
                artifact_failure(planned, PublicationErrorKind::Write(error.kind()))
            })?;
        }

        Ok(())
    }
}

fn publication_set(
    plan: &EmissionPlan,
    artifacts: impl IntoIterator<Item = EmittedArtifact>,
) -> EmittedArtifactSet {
    EmittedArtifactSet::from_publication(plan, artifacts)
}

struct PreparedArtifact<'plan> {
    planned: &'plan PlannedArtifact,
    contribution: ArtifactContribution,
}

enum ArtifactPublicationFailure {
    Cancelled,
    Failed(ArtifactRequirement, PublicationError),
}

fn prepare_contributions<'plan>(
    plan: &'plan EmissionPlan,
    contributions: impl IntoIterator<Item = ArtifactContribution>,
    diagnostics: &mut PublicationDiagnostics,
) -> Result<Vec<PreparedArtifact<'plan>>, PublicationError> {
    let mut contributions: Vec<_> = contributions.into_iter().collect();

    if let Some(package_interface) = package_interface_contribution(plan)? {
        contributions.push(package_interface);
    }

    contributions.sort_unstable_by(|left, right| left.id().cmp(right.id()));

    if let Some(pair) = contributions
        .windows(2)
        .find(|pair| pair[0].id() == pair[1].id())
    {
        return Err(contribution_error(
            &pair[0],
            plan,
            PublicationErrorKind::InvalidContribution,
        ));
    }

    for contribution in &contributions {
        validate_contribution_destination(plan, contribution)?;
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
                        contribution,
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
                &contribution,
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
            contribution,
        });
    }

    if let Some(contribution) = contributions.next() {
        return Err(contribution_error(
            &contribution,
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
    contribution: &ArtifactContribution,
) -> Result<(), PublicationError> {
    let Some(planned) = plan.artifact(contribution.id()) else {
        return Err(contribution_error(
            contribution,
            plan,
            PublicationErrorKind::InvalidContribution,
        ));
    };

    if !matches!(
        planned.destination(),
        PlannedArtifactDestination::Publish(_)
    ) {
        return Err(contribution_error(
            contribution,
            plan,
            PublicationErrorKind::InvalidContribution,
        ));
    }

    Ok(())
}

fn content_failure(
    planned: &PlannedArtifact,
    error: ContentValidationError,
) -> ArtifactPublicationFailure {
    let kind = match error {
        ContentValidationError::Cancelled => return ArtifactPublicationFailure::Cancelled,
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

    artifact_failure(planned, kind)
}

fn artifact_failure(
    planned: &PlannedArtifact,
    kind: PublicationErrorKind,
) -> ArtifactPublicationFailure {
    ArtifactPublicationFailure::Failed(planned.requirement(), planned_error(planned, kind))
}

fn contribution_error(
    contribution: &ArtifactContribution,
    plan: &EmissionPlan,
    kind: PublicationErrorKind,
) -> PublicationError {
    let sink = plan
        .artifact(contribution.id())
        .and_then(|planned| match planned.destination() {
            PlannedArtifactDestination::Publish(sink) => Some(sink.clone()),
            PlannedArtifactDestination::Stage => None,
        });

    // Publication errors own the contribution identity after validation returns.
    PublicationError::new(contribution.id().clone(), sink, kind)
}

fn planned_error(planned: &PlannedArtifact, kind: PublicationErrorKind) -> PublicationError {
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
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use bray_codegen::{ArtifactContent, ArtifactDigest, ArtifactDigestAlgorithm};
    use bray_diagnostics::{DiagnosticArgName, DiagnosticArgValue, DiagnosticKind, SeverityKind};
    use bray_testing::TemporaryFile;

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
    fn failed_indirect_writes_discard_buffered_bytes() {
        let Some(stream) = OutputSinkId::try_new("test.partial") else {
            panic!("test stream identity must be valid");
        };

        let plan = stream_plan(stream);
        let resolver = PartiallyFailingResolver::new();
        let contribution = contribution(&plan, b"partial bytes must stay hidden", None);

        let outcome = ArtifactPublisher::with_sink_resolver(&never_cancelled, &resolver)
            .publish(&plan, [contribution]);

        assert!(matches!(
            outcome.status(),
            EmissionStatus::Failed(EmissionFailure::Publication(_))
        ));

        assert_eq!(
            outcome.diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::EmissionArtifactWriteFailed
        );

        assert_eq!(resolver.bytes(), b"");
    }

    #[test]
    fn filesystem_publication_obeys_replacement_policy() {
        let output = TemporaryFile::write("application.brayi", b"existing");
        let contribution_bytes = b"replacement";

        let require_absent = filesystem_plan(output.path(), ReplacementPolicy::RequireAbsent);

        let outcome = ArtifactPublisher::new(&never_cancelled).publish(
            &require_absent,
            [contribution(&require_absent, contribution_bytes, None)],
        );

        assert!(matches!(
            outcome.status(),
            EmissionStatus::Failed(EmissionFailure::Publication(_))
        ));

        assert_eq!(
            outcome.diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::EmissionArtifactCommitFailed
        );

        assert_eq!(file_bytes(output.path()), b"existing");

        let replace = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

        let outcome = ArtifactPublisher::new(&never_cancelled)
            .publish(&replace, [contribution(&replace, contribution_bytes, None)]);

        assert_complete_artifact(&outcome, contribution_bytes);
        assert_eq!(file_bytes(output.path()), contribution_bytes);
    }

    #[test]
    fn cancellation_during_staged_validation_preserves_the_destination() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let destination = directory.path().join("application.brayi");

        if std::fs::write(&destination, b"existing").is_err() {
            panic!("test destination must be written");
        }

        let plan = filesystem_plan(&destination, ReplacementPolicy::ReplaceExisting);
        let contribution = contribution(&plan, b"replacement", None);
        let observations = AtomicUsize::new(0);
        let cancellation = || observations.fetch_add(1, Ordering::AcqRel) >= 5;

        let outcome = ArtifactPublisher::new(&cancellation).publish(&plan, [contribution]);

        assert!(matches!(outcome.status(), EmissionStatus::Cancelled));
        assert!(outcome.artifacts().artifacts().is_empty());
        assert!(outcome.diagnostics().is_empty());
        assert_eq!(file_bytes(&destination), b"existing");
        assert_eq!(directory_entry_count(directory.path()), 1);
    }

    #[test]
    fn failure_does_not_claim_or_attempt_a_cross_path_transaction() {
        let Ok(directory) = tempfile::tempdir() else {
            panic!("test output directory must be created");
        };

        let interface_path = directory.path().join("application.brayi");
        let metadata_path = directory.path().join("application.brayd");

        if std::fs::write(&interface_path, b"old interface").is_err() {
            panic!("test interface destination must be written");
        }

        if std::fs::create_dir(&metadata_path).is_err() {
            panic!("test metadata destination directory must be created");
        }

        let plan = filesystem_artifact_plan([
            (package_interface_spec(), interface_path.clone()),
            (required_dependency_metadata_spec(), metadata_path.clone()),
        ]);

        let interface_bytes = plan
            .package_interface()
            .map(bray_package_interface::InterfaceArtifact::bytes)
            .unwrap_or_else(|| panic!("test plan must retain its package interface"));

        let metadata = contribution_for(
            &plan,
            ArtifactKind::DependencyMetadata,
            b"metadata",
            None,
            None,
        );

        let outcome = ArtifactPublisher::new(&never_cancelled).publish(&plan, [metadata]);

        assert!(matches!(
            outcome.status(),
            EmissionStatus::Failed(EmissionFailure::Publication(_))
        ));

        assert_eq!(outcome.artifacts().artifacts().len(), 1);
        assert_eq!(
            outcome.artifacts().artifacts()[0].sink(),
            &OutputSink::Filesystem(interface_path.clone())
        );

        assert_eq!(file_bytes(&interface_path), interface_bytes);
        assert!(metadata_path.is_dir());
        assert_eq!(directory_entry_count(directory.path()), 2);
    }

    #[test]
    fn cancellation_retains_records_for_already_published_artifacts() {
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

        let cancellation = || interface_path.exists();
        let outcome = ArtifactPublisher::new(&cancellation).publish(&plan, [metadata]);

        assert!(matches!(outcome.status(), EmissionStatus::Cancelled));
        assert_eq!(outcome.artifacts().artifacts().len(), 1);
        assert!(interface_path.is_file());
        assert!(!metadata_path.exists());
        assert_eq!(directory_entry_count(directory.path()), 1);
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

        assert!(matches!(outcome.status(), EmissionStatus::Complete));
        assert_eq!(artifacts.artifacts().len(), 1);
        assert_eq!(outcome.diagnostics().warnings().count(), 1);

        assert_eq!(
            outcome.diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::EmissionInvalidContribution
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

        assert_eq!(
            outcome.diagnostics().diagnostics()[0].severity(),
            SeverityKind::Error
        );
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
            RequestedArtifactDestination::FilesystemFile(path.to_owned()),
            OutputSink::Filesystem(path.to_owned()),
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

        let Ok(request) = EmissionRequest::try_new(
            product_identity(),
            ProductKind::Library,
            target_identity(),
            RequestedArtifactDestination::FilesystemDirectory("unused".into()),
            requested,
            ReplacementPolicy::ReplaceExisting,
        ) else {
            panic!("test filesystem publication request must be valid");
        };

        let product = request.product().clone();

        let planned = artifacts.into_iter().map(|(artifact, path)| {
            PlannedArtifact::new(
                ArtifactId::new(product.clone(), artifact.kind, 0),
                artifact.requirement,
                artifact.role,
                artifact.producer,
                PlannedArtifactDestination::Publish(OutputSink::Filesystem(path)),
            )
        });

        let Ok(plan) = EmissionPlan::try_new(request, None, planned, [], package_interface) else {
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

        let Ok(plan) = EmissionPlan::try_new(request, None, [artifact], [], None) else {
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

        let Ok(plan) = EmissionPlan::try_new(request, None, planned, [], package_interface) else {
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

        assert!(matches!(outcome.status(), EmissionStatus::Complete));
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

    fn directory_entry_count(path: &Path) -> usize {
        let Ok(entries) = std::fs::read_dir(path) else {
            panic!("test output directory must be readable");
        };

        entries.count()
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

    #[derive(Default)]
    struct PartiallyFailingResolver {
        destination: Arc<Mutex<Vec<u8>>>,
    }

    impl PartiallyFailingResolver {
        fn new() -> Self {
            Self::default()
        }

        fn bytes(&self) -> Vec<u8> {
            let Ok(bytes) = self.destination.lock() else {
                panic!("test sink lock must be available");
            };

            bytes.clone()
        }
    }

    impl OutputSinkResolver for PartiallyFailingResolver {
        fn open(
            &self,
            _sink: IndirectOutputSink<'_>,
            _replacement: ReplacementPolicy,
        ) -> io::Result<Box<dyn OutputSinkTransaction>> {
            Ok(Box::new(PartiallyFailingTransaction {
                destination: Arc::clone(&self.destination),
                buffer: Vec::new(),
                accepted_write: false,
            }))
        }
    }

    struct PartiallyFailingTransaction {
        destination: Arc<Mutex<Vec<u8>>>,
        buffer: Vec<u8>,
        accepted_write: bool,
    }

    impl Write for PartiallyFailingTransaction {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if self.accepted_write {
                return Err(io::Error::from(io::ErrorKind::BrokenPipe));
            }

            let accepted = buffer.len().min(3);

            self.buffer.extend_from_slice(&buffer[..accepted]);
            self.accepted_write = true;

            Ok(accepted)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl OutputSinkTransaction for PartiallyFailingTransaction {
        fn commit(self: Box<Self>) -> io::Result<()> {
            let Self {
                destination,
                buffer,
                ..
            } = *self;

            commit_buffer(destination, buffer)
        }
    }

    fn commit_buffer(destination: Arc<Mutex<Vec<u8>>>, buffer: Vec<u8>) -> io::Result<()> {
        let mut destination = destination
            .lock()
            .map_err(|_| io::Error::from(io::ErrorKind::Other))?;

        *destination = buffer;

        Ok(())
    }

    fn never_cancelled() -> bool {
        false
    }
}
