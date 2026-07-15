use std::cmp::Ordering;
use std::fs::OpenOptions;
use std::io::{self, Read, Write};

use bray_codegen::ArtifactDigest;
use bray_diagnostics::{DiagnosticBag, DiagnosticId, SeverityKind};

use super::content::{ContentValidationError, open_content, validate_content};
use super::diagnostic::{PublicationError, PublicationErrorKind};
use crate::{
    ArtifactContribution, ArtifactRequirement, EmissionFailure, EmissionOutcome, EmissionPlan,
    EmittedArtifact, EmittedArtifactSet, IndirectOutputSink, OutputSink, OutputSinkResolver,
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

        match EmittedArtifactSet::try_new(plan, emitted) {
            Ok(artifacts) => match EmissionOutcome::try_complete(artifacts, diagnostics) {
                Ok(outcome) => outcome,
                Err(diagnostics) => {
                    EmissionOutcome::failed(EmissionFailure::IncompleteProduct, diagnostics)
                }
            },
            Err(_) => EmissionOutcome::failed(EmissionFailure::IncompleteProduct, diagnostics),
        }
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
    ) -> io::Result<Box<dyn Write + Send>> {
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

                options
                    .open(path)
                    .map(|file| Box::new(file) as Box<dyn Write + Send>)
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
    ) -> io::Result<Box<dyn Write + Send>> {
        let Some(resolver) = self.resolver else {
            return Err(io::Error::from(io::ErrorKind::NotFound));
        };

        resolver.open(sink, replacement)
    }
}

impl Default for ArtifactPublisher<'_> {
    fn default() -> Self {
        Self::new()
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
        validate_contribution(plan, contribution)?;
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

fn validate_contribution(
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
    ) || contribution.producer() != planned.producer()
    {
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
        ContentValidationError::DigestMismatch => PublicationErrorKind::DigestMismatch,
        ContentValidationError::LengthMismatch | ContentValidationError::DigestConstruction => {
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
    next_id: u32,
    bag: DiagnosticBag,
}

impl PublicationDiagnostics {
    const fn new() -> Self {
        Self {
            next_id: 0,
            bag: DiagnosticBag::new(),
        }
    }

    fn warning(&mut self, error: PublicationError) {
        let (_, diagnostic) = error.into_diagnostic(self.next_id(), SeverityKind::Warning);

        self.bag.add(diagnostic);
    }

    fn failed(mut self, error: PublicationError) -> EmissionOutcome {
        let failure = error.failure();
        let (artifact, diagnostic) = error.into_diagnostic(self.next_id(), SeverityKind::Error);

        self.bag.add(diagnostic);

        EmissionOutcome::failed(failure.with_artifact(artifact), self.bag)
    }

    fn next_id(&mut self) -> DiagnosticId {
        let id = DiagnosticId::new(self.next_id);

        if let Some(next) = self.next_id.checked_add(1) {
            self.next_id = next;
        }

        id
    }

    fn into_bag(self) -> DiagnosticBag {
        self.bag
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::io::{self, Write};
    use std::sync::{Arc, Mutex};

    use bray_codegen::{ArtifactContent, ArtifactDigest, ArtifactDigestAlgorithm};
    use bray_diagnostics::{DiagnosticKind, SeverityKind};
    use bray_testing::TemporaryFile;

    use super::ArtifactPublisher;
    use crate::test_support::{product_identity, target_identity};
    use crate::{
        ArtifactContribution, ArtifactId, ArtifactKind, ArtifactProducer, ArtifactRequirement,
        ArtifactRole, EmissionFailure, EmissionOutcome, EmissionPlan, EmissionRequest,
        EmissionStatus, IndirectOutputSink, OutputSink, OutputSinkId, OutputSinkResolver,
        PlannedArtifact, PlannedArtifactDestination, ProductKind, ReplacementPolicy,
        RequestedArtifact, RequestedArtifactDestination,
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

        assert_eq!(resolver.bytes("test.digest"), b"");
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
        let id = ArtifactId::new(product_identity(), ArtifactKind::PackageInterface, 0);

        let sink = OutputSink::Memory {
            collector: collector.clone(),
            artifact: id,
        };

        publication_plan(
            RequestedArtifactDestination::Memory(collector),
            sink,
            ReplacementPolicy::RequireAbsent,
        )
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

    fn contribution(
        plan: &EmissionPlan,
        bytes: &[u8],
        digest: Option<ArtifactDigest>,
    ) -> ArtifactContribution {
        let Some(planned) = plan.published_artifacts().next() else {
            panic!("test plan must publish one artifact");
        };

        let Ok(content) = ArtifactContent::try_memory(bytes.to_vec()) else {
            panic!("test artifact content must be valid");
        };

        ArtifactContribution::new(
            planned.id().clone(),
            planned.producer().clone(),
            content,
            digest,
        )
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
    }

    impl CapturingResolver {
        fn new(sinks: impl IntoIterator<Item = OutputSinkId>) -> Self {
            let sinks = sinks
                .into_iter()
                .map(|sink| (sink.as_str().to_owned(), Arc::new(Mutex::new(Vec::new()))))
                .collect();

            Self { sinks }
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
        ) -> io::Result<Box<dyn Write + Send>> {
            let identity = match sink {
                IndirectOutputSink::Memory { collector, .. } => collector,
                IndirectOutputSink::Stream(stream) => stream,
            };

            let Some(bytes) = self.sinks.get(identity.as_str()) else {
                return Err(io::Error::from(io::ErrorKind::NotFound));
            };

            Ok(Box::new(CapturingWriter {
                bytes: Arc::clone(bytes),
            }))
        }
    }

    struct CapturingWriter {
        bytes: Arc<Mutex<Vec<u8>>>,
    }

    impl Write for CapturingWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            let mut bytes = self
                .bytes
                .lock()
                .map_err(|_| io::Error::from(io::ErrorKind::Other))?;

            bytes.extend_from_slice(buffer);

            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
