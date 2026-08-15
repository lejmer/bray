use std::io;

use bray_codegen::ArtifactDigest;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticIoErrorKind, DiagnosticKind,
    SeverityKind,
};

use crate::{ArtifactId, EmissionFailure, EmissionOutcome, EmittedArtifactSet, OutputSink};

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
            .with_arg(DiagnosticArg::artifact_kind(
                artifact_kind.diagnostic_kind(),
            ))
            .with_arg(DiagnosticArg::artifact_ordinal(artifact_ordinal));

        let mut diagnostic = self.kind.with_detail_args(diagnostic);

        if let Some(sink) = self.sink {
            diagnostic = diagnostic.with_arg(DiagnosticArg::output_sink(sink.diagnostic_sink()));
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
    ManagedPublicationUnsupported,
    GenerationCollision,
    InvalidGenerationManifest,
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
            Self::ManagedPublicationUnsupported => {
                DiagnosticKind::EmissionManagedPublicationUnsupported
            }
            Self::GenerationCollision => DiagnosticKind::EmissionGenerationCollision,
            Self::InvalidGenerationManifest => DiagnosticKind::EmissionGenerationManifestInvalid,
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
            | Self::DigestMismatch(_)
            | Self::ManagedPublicationUnsupported
            | Self::GenerationCollision
            | Self::InvalidGenerationManifest => None,
        }
    }

    const fn failure(&self) -> PublicationFailureKind {
        match self {
            Self::MissingContribution => PublicationFailureKind::MissingContribution,
            Self::InvalidContribution
            | Self::Read(_)
            | Self::LengthMismatch { .. }
            | Self::DigestMismatch(_) => PublicationFailureKind::InvalidContribution,
            Self::Open(_)
            | Self::Write(_)
            | Self::Flush(_)
            | Self::Commit(_)
            | Self::ManagedPublicationUnsupported
            | Self::GenerationCollision
            | Self::InvalidGenerationManifest => PublicationFailureKind::Publication,
        }
    }

    fn with_detail_args(self, diagnostic: Diagnostic) -> Diagnostic {
        match self {
            Self::LengthMismatch { expected, actual } => diagnostic
                .with_arg(DiagnosticArg::expected_byte_count(expected))
                .with_arg(DiagnosticArg::actual_byte_count(actual)),
            Self::DigestMismatch(mismatches) => diagnostic
                .with_arg(DiagnosticArg::expected_artifact_digest(
                    mismatches.expected.diagnostic_digest(),
                ))
                .with_arg(DiagnosticArg::actual_artifact_digest(
                    mismatches.actual.diagnostic_digest(),
                )),
            Self::MissingContribution
            | Self::InvalidContribution
            | Self::Read(_)
            | Self::Open(_)
            | Self::Write(_)
            | Self::Flush(_)
            | Self::Commit(_)
            | Self::ManagedPublicationUnsupported
            | Self::GenerationCollision
            | Self::InvalidGenerationManifest => diagnostic,
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

#[cfg(test)]
mod tests {
    use std::io;

    use bray_diagnostics::{DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};
    use bray_testing::{assert_goal_state_diagnostic, assert_goal_state_diagnostic_kind};

    use super::{PublicationError, PublicationErrorKind};
    use crate::test_support::product_identity;
    use crate::{ArtifactId, ArtifactKind};

    #[test]
    fn publication_failures_map_to_exact_structured_diagnostics() {
        let cases = [
            (
                PublicationErrorKind::Read(io::ErrorKind::NotFound),
                DiagnosticKind::EmissionArtifactReadFailed,
            ),
            (
                PublicationErrorKind::LengthMismatch {
                    expected: 1,
                    actual: 2,
                },
                DiagnosticKind::EmissionArtifactLengthMismatch,
            ),
            (
                PublicationErrorKind::Flush(io::ErrorKind::BrokenPipe),
                DiagnosticKind::EmissionArtifactFlushFailed,
            ),
        ];

        for (error, expected) in cases {
            assert_eq!(error.diagnostic_kind(), expected);
        }

        let artifact = ArtifactId::new(product_identity(), ArtifactKind::Executable, 0);

        let (_, read) = PublicationError::new(
            artifact.clone(),
            None,
            PublicationErrorKind::Read(io::ErrorKind::NotFound),
        )
        .into_diagnostic(DiagnosticId::new(0), SeverityKind::Error);

        assert_goal_state_diagnostic_kind(
            &DiagnosticBag::single(read),
            DiagnosticKind::EmissionArtifactReadFailed,
        );

        let (_, length) = PublicationError::new(
            artifact.clone(),
            None,
            PublicationErrorKind::LengthMismatch {
                expected: 1,
                actual: 2,
            },
        )
        .into_diagnostic(DiagnosticId::new(0), SeverityKind::Error);

        assert_goal_state_diagnostic_kind(
            &DiagnosticBag::single(length),
            DiagnosticKind::EmissionArtifactLengthMismatch,
        );

        let (_, flush) = PublicationError::new(
            artifact,
            None,
            PublicationErrorKind::Flush(io::ErrorKind::BrokenPipe),
        )
        .into_diagnostic(DiagnosticId::new(0), SeverityKind::Error);

        assert_goal_state_diagnostic_kind(
            &DiagnosticBag::single(flush),
            DiagnosticKind::EmissionArtifactFlushFailed,
        );
    }

    #[test]
    fn managed_publication_failures_publish_goal_state_diagnostics() {
        let cases = [
            (
                PublicationErrorKind::ManagedPublicationUnsupported,
                DiagnosticKind::EmissionManagedPublicationUnsupported,
            ),
            (
                PublicationErrorKind::InvalidGenerationManifest,
                DiagnosticKind::EmissionGenerationManifestInvalid,
            ),
        ];

        for (index, (kind, expected)) in cases.into_iter().enumerate() {
            let error = PublicationError::new(
                ArtifactId::new(product_identity(), ArtifactKind::Executable, 0),
                None,
                kind,
            );

            let (_, diagnostic) =
                error.into_diagnostic(DiagnosticId::from_index(index), SeverityKind::Error);

            assert_eq!(diagnostic.kind(), expected);
            assert_goal_state_diagnostic(&diagnostic);

            match expected {
                DiagnosticKind::EmissionManagedPublicationUnsupported => {
                    assert_goal_state_diagnostic_kind(
                        &DiagnosticBag::single(diagnostic),
                        DiagnosticKind::EmissionManagedPublicationUnsupported,
                    );
                }
                DiagnosticKind::EmissionGenerationManifestInvalid => {
                    assert_goal_state_diagnostic_kind(
                        &DiagnosticBag::single(diagnostic),
                        DiagnosticKind::EmissionGenerationManifestInvalid,
                    );
                }
                _ => panic!("test case must remain a managed-publication failure"),
            }
        }
    }
}
