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
    Complete(EmittedArtifactSet),
    /// Emission failed without claiming partial product success.
    Failed(EmissionFailure),
    /// Cancellation was observed before product success was published.
    Cancelled,
}

/// Immutable product emission result and locale-neutral diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmissionOutcome {
    status: EmissionStatus,
    diagnostics: DiagnosticBag,
}

impl EmissionOutcome {
    pub(crate) fn try_complete(
        artifacts: EmittedArtifactSet,
        diagnostics: DiagnosticBag,
    ) -> Result<Self, DiagnosticBag> {
        if diagnostics.has_errors() {
            return Err(diagnostics);
        }

        Ok(Self {
            status: EmissionStatus::Complete(artifacts),
            diagnostics,
        })
    }

    /// Creates a failed outcome without a partial product success claim.
    pub(crate) const fn failed(failure: EmissionFailure, diagnostics: DiagnosticBag) -> Self {
        Self {
            status: EmissionStatus::Failed(failure),
            diagnostics,
        }
    }

    /// Creates a canceled outcome without diagnostics or partial product success.
    pub const fn cancelled() -> Self {
        Self {
            status: EmissionStatus::Cancelled,
            diagnostics: DiagnosticBag::new(),
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

    /// Returns the complete publication set only after successful emission.
    pub const fn artifacts(&self) -> Option<&EmittedArtifactSet> {
        match &self.status {
            EmissionStatus::Complete(artifacts) => Some(artifacts),
            EmissionStatus::Failed(_) | EmissionStatus::Cancelled => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};

    use super::{EmissionFailure, EmissionOutcome, EmissionStatus};
    use crate::test_support::{emission_plan, emitted_artifact};

    #[test]
    fn outcomes_expose_artifacts_only_after_complete_plan_validation() {
        let plan = emission_plan();
        let artifact = emitted_artifact(&plan);

        let Ok(artifacts) = crate::EmittedArtifactSet::try_new(&plan, [artifact]) else {
            panic!("matching test artifact must complete emission");
        };

        let Ok(complete) = EmissionOutcome::try_complete(artifacts, DiagnosticBag::new()) else {
            panic!("error-free artifacts must complete emission");
        };

        assert!(matches!(complete.status(), EmissionStatus::Complete(_)));
        assert!(complete.artifacts().is_some());

        let failed = EmissionOutcome::failed(EmissionFailure::Planning, DiagnosticBag::new());
        let cancelled = EmissionOutcome::cancelled();

        assert_eq!(failed.artifacts(), None);
        assert_eq!(cancelled.artifacts(), None);
    }

    #[test]
    fn complete_outcomes_reject_error_diagnostics() {
        let plan = emission_plan();
        let artifact = emitted_artifact(&plan);

        let Ok(artifacts) = crate::EmittedArtifactSet::try_new(&plan, [artifact]) else {
            panic!("matching test artifact must complete emission");
        };

        let diagnostics = DiagnosticBag::single(Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::EmissionArtifactWriteFailed,
            SeverityKind::Error,
        ));

        assert_eq!(
            EmissionOutcome::try_complete(artifacts, diagnostics.clone()),
            Err(diagnostics)
        );
    }
}
