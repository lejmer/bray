use bray_diagnostics::DiagnosticBag;

use crate::{ArtifactId, EmittedArtifactSet};

/// Structured reason one emission operation could not publish a complete product.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EmissionFailure {
    /// The host request could not be validated.
    InvalidRequest,
    /// Complete deterministic planning could not be established.
    Planning,
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
    diagnostics: DiagnosticBag,
}

impl EmissionOutcome {
    pub(crate) const fn complete(
        artifacts: EmittedArtifactSet,
        diagnostics: DiagnosticBag,
    ) -> Self {
        Self {
            status: EmissionStatus::Complete,
            artifacts,
            diagnostics,
        }
    }

    /// Creates a failed outcome without a partial product success claim.
    pub(crate) const fn failed(
        failure: EmissionFailure,
        artifacts: EmittedArtifactSet,
        diagnostics: DiagnosticBag,
    ) -> Self {
        Self {
            status: EmissionStatus::Failed(failure),
            artifacts,
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

    pub(crate) fn with_prior_diagnostics(mut self, diagnostics: &DiagnosticBag) -> Self {
        self.diagnostics = diagnostics.merged(&self.diagnostics);

        self
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticBag;

    use super::{EmissionFailure, EmissionOutcome, EmissionStatus};
    use crate::test_support::{emission_plan, emitted_artifact};

    #[test]
    fn outcomes_separate_product_status_from_published_artifacts() {
        let plan = emission_plan();
        let artifact = emitted_artifact(&plan);

        let artifacts = crate::EmittedArtifactSet::from_publication(&plan, [artifact]);
        let complete = EmissionOutcome::complete(artifacts, DiagnosticBag::new());

        assert!(matches!(complete.status(), EmissionStatus::Complete));
        assert_eq!(complete.artifacts().artifacts().len(), 1);

        let failed = EmissionOutcome::failed(
            EmissionFailure::Planning,
            crate::EmittedArtifactSet::from_publication(&plan, []),
            DiagnosticBag::new(),
        );

        let cancelled = EmissionOutcome::cancelled(
            crate::EmittedArtifactSet::from_publication(&plan, []),
            DiagnosticBag::new(),
        );

        assert!(failed.artifacts().artifacts().is_empty());
        assert!(cancelled.artifacts().artifacts().is_empty());
    }
}
