use std::cmp::Ordering;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};

use bray_codegen::ArtifactDigest;
use bray_diagnostics::{DiagnosticBag, DiagnosticId, SeverityKind};

use super::content::{ContentValidationError, open_content, validate_content};
use super::diagnostic::{PublicationError, PublicationErrorKind};
use crate::{
    ArtifactContribution, ArtifactRequirement, EmissionOutcome, EmissionPlan, EmittedArtifact,
    EmittedArtifactSet, IndirectOutputSink, OutputSink, OutputSinkResolver, OutputSinkTransaction,
    PlannedArtifact, PlannedArtifactDestination, ReplacementPolicy,
};

const COPY_BUFFER_LEN: usize = 64 * 1024;

/// Publishes validated artifact contributions to the immutable plan's external sinks.
#[derive(Clone, Copy)]
pub struct ArtifactPublisher<'resolver> {
    resolver: Option<&'resolver dyn OutputSinkResolver>,
}

impl ArtifactPublisher<'_> {
    /// Creates a publisher for filesystem-only plans.
    pub const fn new() -> Self {
        Self { resolver: None }
    }
}

impl<'resolver> ArtifactPublisher<'resolver> {
    /// Creates a publisher that can resolve memory collectors and writable streams.
    pub const fn with_sink_resolver(resolver: &'resolver dyn OutputSinkResolver) -> Self {
        Self {
            resolver: Some(resolver),
        }
    }

    /// Validates and publishes complete contributions in deterministic plan order.
    pub fn publish(
        &self,
        plan: &EmissionPlan,
        contributions: impl IntoIterator<Item = ArtifactContribution>,
    ) -> EmissionOutcome {
        let mut diagnostics = PublicationDiagnostics::new();

        let prepared = match prepare_contributions(plan, contributions, &mut diagnostics) {
            Ok(prepared) => prepared,
            Err(error) => return diagnostics.failed(error),
        };

        let mut emitted = Vec::with_capacity(prepared.len());

        for artifact in prepared {
            match self.publish_artifact(plan.request().replacement(), artifact) {
                Ok(artifact) => emitted.push(artifact),
                Err((ArtifactRequirement::Optional, error)) => {
                    diagnostics.warning(error);
                }
                Err((_, error)) => return diagnostics.failed(error),
            }
        }

        let diagnostics = diagnostics.into_bag();
        let artifacts = EmittedArtifactSet::from_publication(plan, emitted);

        EmissionOutcome::complete(artifacts, diagnostics)
    }

    fn publish_artifact(
        &self,
        replacement: ReplacementPolicy,
        artifact: PreparedArtifact<'_>,
    ) -> Result<EmittedArtifact, (ArtifactRequirement, PublicationError)> {
        let planned = artifact.planned;
        let requirement = planned.requirement();

        let PlannedArtifactDestination::Publish(sink) = planned.destination() else {
            return Err((
                requirement,
                planned_error(planned, PublicationErrorKind::InvalidContribution),
            ));
        };

        let mut reader = open_content(artifact.contribution.content()).map_err(|kind| {
            (
                requirement,
                planned_error(planned, PublicationErrorKind::Read(kind)),
            )
        })?;

        let mut writer = self.open_output(sink, replacement).map_err(|error| {
            (
                requirement,
                planned_error(planned, PublicationErrorKind::Open(error.kind())),
            )
        })?;

        let mut buffer = [0_u8; COPY_BUFFER_LEN];

        loop {
            let read = reader.read(&mut buffer).map_err(|error| {
                (
                    requirement,
                    planned_error(planned, PublicationErrorKind::Read(error.kind())),
                )
            })?;

            if read == 0 {
                break;
            }

            writer.write_all(&buffer[..read]).map_err(|error| {
                (
                    requirement,
                    planned_error(planned, PublicationErrorKind::Write(error.kind())),
                )
            })?;
        }

        writer.flush().map_err(|error| {
            (
                requirement,
                planned_error(planned, PublicationErrorKind::Flush(error.kind())),
            )
        })?;

        writer.commit().map_err(|error| {
            (
                requirement,
                planned_error(planned, PublicationErrorKind::Commit(error.kind())),
            )
        })?;

        // Publication records own stable plan facts independently of the borrowed plan.
        Ok(EmittedArtifact::new(
            planned.id().clone(),
            sink.clone(),
            planned.producer().clone(),
            planned.role(),
            artifact.contribution.content().byte_len(),
            artifact.digest,
        ))
    }

    fn open_output(
        &self,
        sink: &OutputSink,
        replacement: ReplacementPolicy,
    ) -> io::Result<PublicationOutput> {
        match sink {
            OutputSink::Filesystem(path) => {
                let mut options = OpenOptions::new();

                options.write(true);

                match replacement {
                    ReplacementPolicy::RequireAbsent => {
                        options.create_new(true);
                    }
                    ReplacementPolicy::ReplaceExisting => {
                        options.create(true).truncate(true);
                    }
                }

                options.open(path).map(PublicationOutput::Filesystem)
            }
            OutputSink::Memory {
                collector,
                artifact,
            } => self.open_indirect(
                IndirectOutputSink::Memory {
                    collector,
                    artifact,
                },
                replacement,
            ),
            OutputSink::Stream(stream) => {
                self.open_indirect(IndirectOutputSink::Stream(stream), replacement)
            }
        }
    }

    fn open_indirect(
        &self,
        sink: IndirectOutputSink<'_>,
        replacement: ReplacementPolicy,
    ) -> io::Result<PublicationOutput> {
        let Some(resolver) = self.resolver else {
            return Err(io::Error::from(io::ErrorKind::NotFound));
        };

        resolver
            .open(sink, replacement)
            .map(PublicationOutput::Indirect)
    }
}

impl Default for ArtifactPublisher<'_> {
    fn default() -> Self {
        Self::new()
    }
}

enum PublicationOutput {
    Filesystem(File),
    Indirect(Box<dyn OutputSinkTransaction>),
}

impl PublicationOutput {
    fn commit(self) -> io::Result<()> {
        match self {
            Self::Filesystem(_) => Ok(()),
            Self::Indirect(transaction) => transaction.commit(),
        }
    }
}

impl Write for PublicationOutput {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::Filesystem(file) => file.write(buffer),
            Self::Indirect(transaction) => transaction.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Filesystem(file) => file.flush(),
            Self::Indirect(transaction) => transaction.flush(),
        }
    }
}

struct PreparedArtifact<'plan> {
    planned: &'plan PlannedArtifact,
    contribution: ArtifactContribution,
    digest: ArtifactDigest,
}

fn prepare_contributions<'plan>(
    plan: &'plan EmissionPlan,
    contributions: impl IntoIterator<Item = ArtifactContribution>,
    diagnostics: &mut PublicationDiagnostics,
) -> Result<Vec<PreparedArtifact<'plan>>, PublicationError> {
    let mut contributions: Vec<_> = contributions.into_iter().collect();

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

        match validate_content(contribution.content(), contribution.digest()) {
            Ok(digest) => prepared.push(PreparedArtifact {
                planned,
                contribution,
                digest,
            }),
            Err(error) if planned.requirement() == ArtifactRequirement::Optional => {
                diagnostics.warning(content_error(planned, error));
            }
            Err(error) => return Err(content_error(planned, error)),
        }
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

fn content_error(planned: &PlannedArtifact, error: ContentValidationError) -> PublicationError {
    let kind = match error {
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

struct PublicationDiagnostics {
    pending: Vec<PendingDiagnostic>,
}

impl PublicationDiagnostics {
    const fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    fn warning(&mut self, error: PublicationError) {
        self.pending.push(PendingDiagnostic {
            error,
            severity: SeverityKind::Warning,
        });
    }

    fn failed(mut self, error: PublicationError) -> EmissionOutcome {
        // Failed outcomes retain the Arc-backed artifact identity after diagnostics consume error.
        let artifact = error.artifact().clone();
        let failure = error.failure().with_artifact(artifact);

        self.pending.push(PendingDiagnostic {
            error,
            severity: SeverityKind::Error,
        });

        EmissionOutcome::failed(failure, self.into_bag())
    }

    fn into_bag(mut self) -> DiagnosticBag {
        self.pending
            .sort_by(|left, right| left.error.artifact().cmp(right.error.artifact()));

        let mut bag = DiagnosticBag::with_capacity(self.pending.len());
        let mut next_id = 0_u32;

        for pending in self.pending {
            let id = DiagnosticId::new(next_id);
            let (_, diagnostic) = pending.error.into_diagnostic(id, pending.severity);

            bag.add(diagnostic);

            if let Some(id) = next_id.checked_add(1) {
                next_id = id;
            }
        }

        bag
    }
}

struct PendingDiagnostic {
    error: PublicationError,
    severity: SeverityKind,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};

    use bray_codegen::{ArtifactContent, ArtifactDigest, ArtifactDigestAlgorithm};
    use bray_diagnostics::{DiagnosticArgName, DiagnosticArgValue, DiagnosticKind, SeverityKind};
    use bray_testing::TemporaryFile;

    use super::ArtifactPublisher;
    use crate::test_support::{product_identity, target_identity};
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
        let contribution = contribution(&plan, b"interface bytes", None);

        let outcome =
            ArtifactPublisher::with_sink_resolver(&resolver).publish(&plan, [contribution]);

        assert_complete_artifact(&outcome, b"interface bytes");
        assert_eq!(resolver.bytes("test.memory"), b"interface bytes");
    }

    #[test]
    fn stream_publication_writes_complete_bytes() {
        let Some(stream) = OutputSinkId::try_new("test.stream") else {
            panic!("test stream identity must be valid");
        };

        let plan = stream_plan(stream.clone());
        let resolver = CapturingResolver::new([stream]);
        let contribution = contribution(&plan, b"stream bytes", None);

        let outcome =
            ArtifactPublisher::with_sink_resolver(&resolver).publish(&plan, [contribution]);

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

        let outcome =
            ArtifactPublisher::with_sink_resolver(&resolver).publish(&plan, [contribution]);

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

        let outcome = ArtifactPublisher::new().publish(
            &require_absent,
            [contribution(&require_absent, contribution_bytes, None)],
        );

        assert!(matches!(
            outcome.status(),
            EmissionStatus::Failed(EmissionFailure::Publication(_))
        ));

        assert_eq!(
            outcome.diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::EmissionArtifactOpenFailed
        );

        assert_eq!(file_bytes(output.path()), b"existing");

        let replace = filesystem_plan(output.path(), ReplacementPolicy::ReplaceExisting);

        let outcome = ArtifactPublisher::new()
            .publish(&replace, [contribution(&replace, contribution_bytes, None)]);

        assert_complete_artifact(&outcome, contribution_bytes);
        assert_eq!(file_bytes(output.path()), contribution_bytes);
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
        let outcome =
            ArtifactPublisher::with_sink_resolver(&resolver).publish(&plan, [contribution]);

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

        let interface = contribution_for(
            &plan,
            ArtifactKind::PackageInterface,
            b"interface",
            None,
            None,
        );

        let metadata = contribution_for(
            &plan,
            ArtifactKind::DependencyMetadata,
            b"metadata",
            None,
            Some(ArtifactProducer::DependencyMetadata(
                DependencyMetadataProducerId::new(1),
            )),
        );

        let outcome =
            ArtifactPublisher::with_sink_resolver(&resolver).publish(&plan, [metadata, interface]);

        let Some(artifacts) = outcome.artifacts() else {
            panic!("an invalid optional contribution must not fail the product");
        };

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

        let interface = contribution_for(
            &plan,
            ArtifactKind::PackageInterface,
            b"interface",
            None,
            None,
        );

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

        let outcome = ArtifactPublisher::with_sink_resolver(&resolver)
            .publish(&plan, [executable, metadata, interface]);

        assert!(outcome.artifacts().is_some());

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
        let outcome = ArtifactPublisher::new().publish(&plan, []);

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
        memory_artifact_plan(collector, [package_interface_spec()])
    }

    fn stream_plan(stream: OutputSinkId) -> EmissionPlan {
        publication_plan(
            RequestedArtifactDestination::Stream(stream.clone()),
            OutputSink::Stream(stream),
            ReplacementPolicy::RequireAbsent,
        )
    }

    fn filesystem_plan(path: &std::path::Path, replacement: ReplacementPolicy) -> EmissionPlan {
        publication_plan(
            RequestedArtifactDestination::FilesystemFile(path.to_owned()),
            OutputSink::Filesystem(path.to_owned()),
            replacement,
        )
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
                ArtifactKind::PackageInterface,
                ArtifactRequirement::Required,
            )],
            replacement,
        ) else {
            panic!("test publication request must be valid");
        };

        let artifact = PlannedArtifact::new(
            ArtifactId::new(request.product().clone(), ArtifactKind::PackageInterface, 0),
            ArtifactRequirement::Required,
            ArtifactRole::Product,
            ArtifactProducer::PackageInterface,
            PlannedArtifactDestination::Publish(sink),
        );

        let Ok(plan) = EmissionPlan::try_new(request, None, [artifact], []) else {
            panic!("test publication plan must be valid");
        };

        plan
    }

    fn memory_artifact_plan(
        collector: OutputSinkId,
        artifacts: impl IntoIterator<Item = TestArtifactSpec>,
    ) -> EmissionPlan {
        let artifacts: Vec<_> = artifacts.into_iter().collect();

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

        let Ok(plan) = EmissionPlan::try_new(request, None, planned, []) else {
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
        contribution_for(plan, ArtifactKind::PackageInterface, bytes, digest, None)
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
        let Some(artifacts) = outcome.artifacts() else {
            panic!("publication must complete");
        };

        let Ok(expected_len) = u64::try_from(expected.len()) else {
            panic!("test artifact length must fit the publication contract");
        };

        assert!(outcome.diagnostics().is_empty());

        assert_eq!(artifacts.artifacts().len(), 1);
        assert_eq!(artifacts.artifacts()[0].byte_len(), expected_len);

        assert_eq!(
            artifacts.artifacts()[0].digest().bytes(),
            blake3::hash(expected).as_bytes()
        );
    }

    fn file_bytes(path: &std::path::Path) -> Vec<u8> {
        let Ok(bytes) = std::fs::read(path) else {
            panic!("published test artifact must be readable");
        };

        bytes
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
            let mut destination = self
                .destination
                .lock()
                .map_err(|_| io::Error::from(io::ErrorKind::Other))?;

            *destination = self.buffer;

            Ok(())
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
            let mut destination = self
                .destination
                .lock()
                .map_err(|_| io::Error::from(io::ErrorKind::Other))?;

            *destination = self.buffer;

            Ok(())
        }
    }
}
