use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use bray_diagnostics::DiagnosticKind;
use bray_testing::assert_goal_state_diagnostic_kind;

use super::captured_sinks::{CapturingResolver, commit_buffer};
use super::fixtures::{assert_complete_artifact, contribution, file_bytes, never_cancelled};
use super::plans::{
    filesystem_plan, memory_artifact_plan, memory_plan, package_interface_spec, publication_plan,
    test_generation_store,
};

use crate::{
    ArtifactKind, ArtifactPublisher, EmissionFailure, EmissionPlan, EmissionStatus,
    IndirectOutputSink, OutputSink, OutputSinkId, OutputSinkResolver, OutputSinkTransaction,
    ReplacementPolicy, RequestedArtifactDestination,
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

        assert_eq!(
            bray_testing::diagnostic_at(outcome.diagnostics(), 0).kind(),
            diagnostic
        );

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
        bray_testing::diagnostic_at(rejected.diagnostics(), 0).kind(),
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

    assert_eq!(resolved.path(), published);
}

#[test]
fn publication_creates_a_missing_managed_root_without_link_staging() {
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("fresh").join("output");
    let plan = filesystem_plan(&output, ReplacementPolicy::ReplaceExisting);

    let outcome = ArtifactPublisher::new(&never_cancelled)
        .publish(&plan, [contribution(&plan, b"artifact", None)]);

    assert!(outcome.generation().is_some(), "{outcome:?}");
    assert_eq!(file_bytes(&output.join("application.brayd")), b"artifact");
}

fn stream_plan(stream: OutputSinkId) -> EmissionPlan {
    publication_plan(
        RequestedArtifactDestination::Stream(stream.clone()),
        OutputSink::Stream(stream),
        ReplacementPolicy::RequireAbsent,
    )
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
