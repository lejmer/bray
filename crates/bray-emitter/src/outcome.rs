use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticEmissionArtifact,
    DiagnosticEmissionFailure, DiagnosticId, DiagnosticKind, SeverityKind,
};

use crate::{ArtifactId, EmittedArtifactSet, PublishedProductGeneration};

/// Structured reason one emission operation could not publish a complete product.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmissionFailure {
    /// One required planned contribution was not available.
    MissingContribution(ArtifactId),
    /// One contribution did not match its planned identity or producer.
    InvalidContribution(ArtifactId),
    /// A complete artifact could not be published to its planned sink.
    Publication(ArtifactId),
    /// Native linking could not produce the planned linked artifacts.
    Linking,
    /// Completed work did not satisfy the complete product contract.
    IncompleteProduct,
}

/// Atomic completion state of one product emission operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmissionStatus {
    /// Every required external artifact was completely published and validated.
    Complete,
    /// Emission failed without claiming partial product success.
    Failed(EmissionFailure),
    /// Cancellation was observed before product success was published.
    Cancelled,
}

/// Immutable product emission result and locale-neutral diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmissionOutcome {
    status: EmissionStatus,
    artifacts: EmittedArtifactSet,
    generation: Option<PublishedProductGeneration>,
    diagnostics: DiagnosticBag,
}

impl EmissionOutcome {
    pub(crate) fn complete(
        artifacts: EmittedArtifactSet,
        generation: Option<PublishedProductGeneration>,
        diagnostics: DiagnosticBag,
    ) -> Self {
        if diagnostics.has_errors() {
            return Self::failed(EmissionFailure::IncompleteProduct, artifacts, diagnostics);
        }

        Self {
            status: EmissionStatus::Complete,
            artifacts,
            generation,
            diagnostics,
        }
    }

    /// Creates a failed outcome without a partial product success claim.
    pub(crate) fn failed(
        failure: EmissionFailure,
        artifacts: EmittedArtifactSet,
        diagnostics: DiagnosticBag,
    ) -> Self {
        let diagnostics = if diagnostics_explain_failure(&failure, &diagnostics) {
            diagnostics
        } else {
            diagnostics.merged(&terminal_failure_diagnostics(&failure, &artifacts))
        };

        Self {
            status: EmissionStatus::Failed(failure),
            artifacts,
            generation: None,
            diagnostics,
        }
    }

    /// Creates a canceled outcome without a partial product success claim.
    pub(crate) const fn cancelled(
        artifacts: EmittedArtifactSet,
        diagnostics: DiagnosticBag,
    ) -> Self {
        Self {
            status: EmissionStatus::Cancelled,
            artifacts,
            generation: None,
            diagnostics,
        }
    }

    /// Returns the atomic product completion state.
    pub const fn status(&self) -> &EmissionStatus {
        &self.status
    }

    /// Returns emitter-owned structured diagnostics.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Returns every artifact published before the operation reached its final status.
    pub const fn artifacts(&self) -> &EmittedArtifactSet {
        &self.artifacts
    }

    /// Returns the complete managed generation after successful filesystem publication.
    pub const fn generation(&self) -> Option<&PublishedProductGeneration> {
        self.generation.as_ref()
    }

    /// Prepends diagnostics completed before emitter publication without contradicting success.
    pub fn with_prior_diagnostics(mut self, diagnostics: &DiagnosticBag) -> Self {
        let merged = diagnostics.merged(&self.diagnostics);

        if matches!(self.status, EmissionStatus::Complete) && merged.has_errors() {
            return Self::failed(EmissionFailure::IncompleteProduct, self.artifacts, merged);
        }

        self.diagnostics = merged;

        self
    }
}

fn diagnostics_explain_failure(failure: &EmissionFailure, diagnostics: &DiagnosticBag) -> bool {
    diagnostics.iter().any(|diagnostic| {
        diagnostic.severity() == SeverityKind::Error
            && match failure {
                EmissionFailure::MissingContribution(_) => {
                    diagnostic.kind() == DiagnosticKind::EmissionMissingContribution
                }
                EmissionFailure::InvalidContribution(_) => matches!(
                    diagnostic.kind(),
                    DiagnosticKind::EmissionInvalidContribution
                        | DiagnosticKind::EmissionArtifactReadFailed
                        | DiagnosticKind::EmissionArtifactLengthMismatch
                        | DiagnosticKind::EmissionArtifactDigestMismatch
                ),
                EmissionFailure::Publication(_) => matches!(
                    diagnostic.kind(),
                    DiagnosticKind::EmissionArtifactOpenFailed
                        | DiagnosticKind::EmissionArtifactWriteFailed
                        | DiagnosticKind::EmissionArtifactFlushFailed
                        | DiagnosticKind::EmissionArtifactCommitFailed
                        | DiagnosticKind::EmissionManagedPublicationUnsupported
                        | DiagnosticKind::EmissionGenerationCollision
                        | DiagnosticKind::EmissionGenerationManifestInvalid
                ),
                EmissionFailure::Linking => is_linker_failure_diagnostic(diagnostic.kind()),
                EmissionFailure::IncompleteProduct => {
                    diagnostic.kind() == DiagnosticKind::EmissionFailed
                }
            }
    })
}

const fn is_linker_failure_diagnostic(kind: DiagnosticKind) -> bool {
    matches!(
        kind,
        DiagnosticKind::LinkerUnsupportedTarget
            | DiagnosticKind::LinkerUnsupportedProduct
            | DiagnosticKind::LinkerUnsupportedInput
            | DiagnosticKind::LinkerUnsupportedInputMode
            | DiagnosticKind::LinkerUnsupportedOutput
            | DiagnosticKind::LinkerUnsupportedSearchPath
            | DiagnosticKind::LinkerUnsupportedLinkModel
            | DiagnosticKind::LinkerUnsupportedDeadStrip
            | DiagnosticKind::LinkerUnsupportedSectionGarbageCollection
            | DiagnosticKind::LinkerUnsupportedDebug
            | DiagnosticKind::LinkerUnsupportedSubsystem
            | DiagnosticKind::LinkerUnsupportedSymbol
            | DiagnosticKind::LinkerUnsupportedStartup
            | DiagnosticKind::LinkerUnsupportedRuntime
            | DiagnosticKind::LinkerDriverUnavailable
            | DiagnosticKind::LinkerDriverIncompatible
            | DiagnosticKind::LinkerInputMissing
            | DiagnosticKind::LinkerResponseFileFailed
            | DiagnosticKind::LinkerInvocationFailed
            | DiagnosticKind::LinkerExternalToolIoFailed
            | DiagnosticKind::LinkerExternalToolContractFailed
            | DiagnosticKind::LinkerExternalToolExitedUnsuccessfully
            | DiagnosticKind::LinkerOutputMissing
            | DiagnosticKind::LinkerOutputInvalid
            | DiagnosticKind::LinkerResourceExhausted
    )
}

fn terminal_failure_diagnostics(
    failure: &EmissionFailure,
    artifacts: &EmittedArtifactSet,
) -> DiagnosticBag {
    let failure = match failure {
        EmissionFailure::MissingContribution(artifact) => {
            DiagnosticEmissionFailure::MissingContribution(diagnostic_artifact(artifact))
        }
        EmissionFailure::InvalidContribution(artifact) => {
            DiagnosticEmissionFailure::InvalidContribution(diagnostic_artifact(artifact))
        }
        EmissionFailure::Publication(artifact) => {
            DiagnosticEmissionFailure::Publication(diagnostic_artifact(artifact))
        }
        EmissionFailure::Linking => DiagnosticEmissionFailure::Linking,
        EmissionFailure::IncompleteProduct => {
            // rust-style: allow(context-erasing-failure-conversion, reason = "the emission diagnostic bag retains the exact causes")
            DiagnosticEmissionFailure::IncompleteProduct
        }
    };

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::EmissionFailed,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::actual_product_identity(
        artifacts.product().to_string(),
    ))
    .with_arg(DiagnosticArg::target_triple(artifacts.target().as_str()))
    .with_arg(DiagnosticArg::emission_failure(failure));

    DiagnosticBag::single(diagnostic)
}

const fn diagnostic_artifact(artifact: &ArtifactId) -> DiagnosticEmissionArtifact {
    DiagnosticEmissionArtifact::new(artifact.kind().diagnostic_kind(), artifact.ordinal())
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::{EmissionFailure, EmissionOutcome, EmissionStatus};
    use crate::test_support::{emission_plan, emitted_artifact};

    #[test]
    fn outcomes_separate_product_status_from_published_artifacts() {
        let plan = emission_plan();
        let artifact = emitted_artifact(&plan);

        let artifacts = crate::EmittedArtifactSet::from_publication(&plan, [artifact]);

        let complete = EmissionOutcome::complete(artifacts, None, DiagnosticBag::new());

        assert!(matches!(complete.status(), EmissionStatus::Complete));
        assert_eq!(complete.artifacts().artifacts().len(), 1);

        let failed = EmissionOutcome::failed(
            EmissionFailure::Linking,
            crate::EmittedArtifactSet::from_publication(&plan, []),
            DiagnosticBag::new(),
        );

        let cancelled = EmissionOutcome::cancelled(
            crate::EmittedArtifactSet::from_publication(&plan, []),
            DiagnosticBag::new(),
        );

        assert!(failed.artifacts().artifacts().is_empty());
        assert!(cancelled.artifacts().artifacts().is_empty());

        assert_goal_state_diagnostic_kind(failed.diagnostics(), DiagnosticKind::EmissionFailed);
    }

    #[test]
    fn failed_outcomes_add_a_contextual_terminal_diagnostic() {
        let plan = emission_plan();

        let outcome = EmissionOutcome::failed(
            EmissionFailure::Linking,
            crate::EmittedArtifactSet::from_publication(&plan, []),
            DiagnosticBag::new(),
        );

        assert_goal_state_diagnostic_kind(outcome.diagnostics(), DiagnosticKind::EmissionFailed);
    }

    #[test]
    fn successful_outcomes_reject_error_diagnostics() {
        let plan = emission_plan();
        let artifacts = crate::EmittedArtifactSet::from_publication(&plan, []);

        let diagnostics = DiagnosticBag::single(bray_diagnostics::Diagnostic::new(
            bray_diagnostics::DiagnosticId::new(0),
            bray_diagnostics::DiagnosticKind::EmissionFailed,
            bray_diagnostics::SeverityKind::Error,
        ));

        let outcome = EmissionOutcome::complete(artifacts, None, diagnostics);

        assert!(matches!(
            outcome.status(),
            EmissionStatus::Failed(EmissionFailure::IncompleteProduct)
        ));
    }
}
