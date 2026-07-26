use std::io;

use bray_codegen::{ArtifactDigest, ArtifactDigestAlgorithm};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm,
    DiagnosticArtifactKind, DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind,
    DiagnosticOutputSink, SeverityKind,
};

use crate::{
    ArtifactId, ArtifactKind, EmissionFailure, EmissionOutcome, EmittedArtifactSet, OutputSink,
};

pub(super) struct PublicationDiagnostics {
    pending: Vec<PendingDiagnostic>,
}

impl PublicationDiagnostics {
    pub(super) const fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    pub(super) fn warning(&mut self, error: PublicationError) {
        self.pending.push(PendingDiagnostic {
            error,
            severity: SeverityKind::Warning,
        });
    }

    pub(super) fn failed(
        mut self,
        artifacts: EmittedArtifactSet,
        error: PublicationError,
    ) -> EmissionOutcome {
        // Failed outcomes retain the Arc-backed artifact identity after diagnostics consume error.
        let artifact = error.artifact().clone();
        let failure = error.failure().with_artifact(artifact);

        self.pending.push(PendingDiagnostic {
            error,
            severity: SeverityKind::Error,
        });

        EmissionOutcome::failed(failure, artifacts, self.into_bag())
    }

    pub(super) fn cancelled(self, artifacts: EmittedArtifactSet) -> EmissionOutcome {
        EmissionOutcome::cancelled(artifacts, self.into_bag())
    }

    pub(super) fn into_bag(mut self) -> DiagnosticBag {
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

#[derive(Debug)]
pub(super) struct PublicationError {
    artifact: ArtifactId,
    sink: Option<Box<OutputSink>>,
    kind: PublicationErrorKind,
}

impl PublicationError {
    pub(super) fn new(
        artifact: ArtifactId,
        sink: Option<OutputSink>,
        kind: PublicationErrorKind,
    ) -> Self {
        Self {
            artifact,
            sink: sink.map(Box::new),
            kind,
        }
    }

    pub(super) fn into_diagnostic(
        self,
        id: DiagnosticId,
        severity: SeverityKind,
    ) -> (ArtifactId, Diagnostic) {
        let diagnostic_kind = self.kind.diagnostic_kind();
        let io_error_kind = self.kind.io_error_kind();
        let artifact_kind = self.artifact.kind();
        let artifact_ordinal = self.artifact.ordinal();

        let diagnostic = Diagnostic::new(id, diagnostic_kind, severity)
            .with_arg(DiagnosticArg::artifact_kind(diagnostic_artifact_kind(
                artifact_kind,
            )))
            .with_arg(DiagnosticArg::artifact_ordinal(artifact_ordinal));

        let mut diagnostic = self.kind.with_detail_args(diagnostic);

        if let Some(sink) = self.sink {
            diagnostic = diagnostic.with_arg(DiagnosticArg::output_sink(diagnostic_sink(*sink)));
        }

        if let Some(kind) = io_error_kind {
            diagnostic = diagnostic.with_arg(DiagnosticArg::io_error_kind(kind));
        }

        (self.artifact, diagnostic)
    }

    pub(super) const fn failure(&self) -> PublicationFailureKind {
        self.kind.failure()
    }

    pub(super) const fn artifact(&self) -> &ArtifactId {
        &self.artifact
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum PublicationErrorKind {
    MissingContribution,
    InvalidContribution,
    Read(io::ErrorKind),
    LengthMismatch { expected: u64, actual: u64 },
    DigestMismatch(Box<ArtifactDigestMismatch>),
    Open(io::ErrorKind),
    Write(io::ErrorKind),
    Flush(io::ErrorKind),
    Commit(io::ErrorKind),
}

impl PublicationErrorKind {
    pub(super) fn digest_mismatch(expected: ArtifactDigest, actual: ArtifactDigest) -> Self {
        Self::DigestMismatch(Box::new(ArtifactDigestMismatch { expected, actual }))
    }

    const fn diagnostic_kind(&self) -> DiagnosticKind {
        match self {
            Self::MissingContribution => DiagnosticKind::EmissionMissingContribution,
            Self::InvalidContribution => DiagnosticKind::EmissionInvalidContribution,
            Self::Read(_) => DiagnosticKind::EmissionArtifactReadFailed,
            Self::LengthMismatch { .. } => DiagnosticKind::EmissionArtifactLengthMismatch,
            Self::DigestMismatch(_) => DiagnosticKind::EmissionArtifactDigestMismatch,
            Self::Open(_) => DiagnosticKind::EmissionArtifactOpenFailed,
            Self::Write(_) => DiagnosticKind::EmissionArtifactWriteFailed,
            Self::Flush(_) => DiagnosticKind::EmissionArtifactFlushFailed,
            Self::Commit(_) => DiagnosticKind::EmissionArtifactCommitFailed,
        }
    }

    fn io_error_kind(&self) -> Option<DiagnosticIoErrorKind> {
        match self {
            Self::Read(kind)
            | Self::Open(kind)
            | Self::Write(kind)
            | Self::Flush(kind)
            | Self::Commit(kind) => Some(DiagnosticIoErrorKind::from(*kind)),
            Self::MissingContribution
            | Self::InvalidContribution
            | Self::LengthMismatch { .. }
            | Self::DigestMismatch(_) => None,
        }
    }

    const fn failure(&self) -> PublicationFailureKind {
        match self {
            Self::MissingContribution => PublicationFailureKind::MissingContribution,
            Self::InvalidContribution
            | Self::Read(_)
            | Self::LengthMismatch { .. }
            | Self::DigestMismatch(_) => PublicationFailureKind::InvalidContribution,
            Self::Open(_) | Self::Write(_) | Self::Flush(_) | Self::Commit(_) => {
                PublicationFailureKind::Publication
            }
        }
    }

    fn with_detail_args(self, diagnostic: Diagnostic) -> Diagnostic {
        match self {
            Self::LengthMismatch { expected, actual } => diagnostic
                .with_arg(DiagnosticArg::expected_byte_count(expected))
                .with_arg(DiagnosticArg::actual_byte_count(actual)),
            Self::DigestMismatch(facts) => diagnostic
                .with_arg(DiagnosticArg::expected_artifact_digest(diagnostic_digest(
                    facts.expected,
                )))
                .with_arg(DiagnosticArg::actual_artifact_digest(diagnostic_digest(
                    facts.actual,
                ))),
            Self::MissingContribution
            | Self::InvalidContribution
            | Self::Read(_)
            | Self::Open(_)
            | Self::Write(_)
            | Self::Flush(_)
            | Self::Commit(_) => diagnostic,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ArtifactDigestMismatch {
    expected: ArtifactDigest,
    actual: ArtifactDigest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PublicationFailureKind {
    MissingContribution,
    InvalidContribution,
    Publication,
}

impl PublicationFailureKind {
    pub(super) const fn with_artifact(self, artifact: ArtifactId) -> EmissionFailure {
        match self {
            Self::MissingContribution => EmissionFailure::MissingContribution(artifact),
            Self::InvalidContribution => EmissionFailure::InvalidContribution(artifact),
            Self::Publication => EmissionFailure::Publication(artifact),
        }
    }
}

const fn diagnostic_artifact_kind(kind: ArtifactKind) -> DiagnosticArtifactKind {
    match kind {
        ArtifactKind::Assembly => DiagnosticArtifactKind::Assembly,
        ArtifactKind::BackendIr => DiagnosticArtifactKind::BackendIr,
        ArtifactKind::BackendBitcode => DiagnosticArtifactKind::BackendBitcode,
        ArtifactKind::RelocatableObject => DiagnosticArtifactKind::RelocatableObject,
        ArtifactKind::ExecutableModule => DiagnosticArtifactKind::ExecutableModule,
        ArtifactKind::DebugCompanion => DiagnosticArtifactKind::DebugCompanion,
        ArtifactKind::PackageInterface => DiagnosticArtifactKind::PackageInterface,
        ArtifactKind::DependencyMetadata => DiagnosticArtifactKind::DependencyMetadata,
        ArtifactKind::Executable => DiagnosticArtifactKind::Executable,
        ArtifactKind::StaticLibrary => DiagnosticArtifactKind::StaticLibrary,
        ArtifactKind::SharedLibrary => DiagnosticArtifactKind::SharedLibrary,
        ArtifactKind::LinkedCompanion => DiagnosticArtifactKind::LinkedCompanion,
    }
}

fn diagnostic_sink(sink: OutputSink) -> DiagnosticOutputSink {
    match sink {
        OutputSink::Filesystem(path) => DiagnosticOutputSink::Filesystem(path),
        OutputSink::Memory { collector, .. } => {
            DiagnosticOutputSink::Memory(collector.as_str().to_owned())
        }
        OutputSink::Stream(stream) => DiagnosticOutputSink::Stream(stream.as_str().to_owned()),
    }
}

fn diagnostic_digest(digest: ArtifactDigest) -> DiagnosticArtifactDigest {
    let algorithm = match digest.algorithm() {
        ArtifactDigestAlgorithm::Blake3 => DiagnosticArtifactDigestAlgorithm::Blake3,
        ArtifactDigestAlgorithm::Sha256 => DiagnosticArtifactDigestAlgorithm::Sha256,
    };

    DiagnosticArtifactDigest::new(algorithm, digest.into_bytes())
}
