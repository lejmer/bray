use std::io;

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArtifactKind, DiagnosticId, DiagnosticIoErrorKind,
    DiagnosticKind, DiagnosticOutputSink, SeverityKind,
};

use crate::{ArtifactId, ArtifactKind, EmissionFailure, OutputSink};

#[derive(Debug)]
pub(super) struct PublicationError {
    artifact: ArtifactId,
    sink: Option<OutputSink>,
    kind: PublicationErrorKind,
}

impl PublicationError {
    pub(super) const fn new(
        artifact: ArtifactId,
        sink: Option<OutputSink>,
        kind: PublicationErrorKind,
    ) -> Self {
        Self {
            artifact,
            sink,
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

        let mut diagnostic = Diagnostic::new(id, diagnostic_kind, severity)
            .with_arg(DiagnosticArg::artifact_kind(diagnostic_artifact_kind(
                artifact_kind,
            )))
            .with_arg(DiagnosticArg::artifact_ordinal(artifact_ordinal));

        if let Some(sink) = self.sink {
            diagnostic = diagnostic.with_arg(DiagnosticArg::output_sink(diagnostic_sink(sink)));
        }

        if let Some(kind) = io_error_kind {
            diagnostic = diagnostic.with_arg(DiagnosticArg::io_error_kind(kind));
        }

        (self.artifact, diagnostic)
    }

    pub(super) const fn failure(&self) -> PublicationFailureKind {
        self.kind.failure()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PublicationErrorKind {
    MissingContribution,
    InvalidContribution,
    Read(io::ErrorKind),
    DigestMismatch,
    Open(io::ErrorKind),
    Write(io::ErrorKind),
    Flush(io::ErrorKind),
}

impl PublicationErrorKind {
    const fn diagnostic_kind(self) -> DiagnosticKind {
        match self {
            Self::MissingContribution => DiagnosticKind::EmissionMissingContribution,
            Self::InvalidContribution => DiagnosticKind::EmissionInvalidContribution,
            Self::Read(_) => DiagnosticKind::EmissionArtifactReadFailed,
            Self::DigestMismatch => DiagnosticKind::EmissionArtifactDigestMismatch,
            Self::Open(_) => DiagnosticKind::EmissionArtifactOpenFailed,
            Self::Write(_) => DiagnosticKind::EmissionArtifactWriteFailed,
            Self::Flush(_) => DiagnosticKind::EmissionArtifactFlushFailed,
        }
    }

    fn io_error_kind(self) -> Option<DiagnosticIoErrorKind> {
        match self {
            Self::Read(kind) | Self::Open(kind) | Self::Write(kind) | Self::Flush(kind) => {
                Some(DiagnosticIoErrorKind::from(kind))
            }
            Self::MissingContribution | Self::InvalidContribution | Self::DigestMismatch => None,
        }
    }

    const fn failure(self) -> PublicationFailureKind {
        match self {
            Self::MissingContribution => PublicationFailureKind::MissingContribution,
            Self::InvalidContribution | Self::Read(_) | Self::DigestMismatch => {
                PublicationFailureKind::InvalidContribution
            }
            Self::Open(_) | Self::Write(_) | Self::Flush(_) => PublicationFailureKind::Publication,
        }
    }
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
