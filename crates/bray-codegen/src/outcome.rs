use bray_diagnostics::DiagnosticBag;

use crate::{BackendArtifactKind, BackendArtifactSet};

/// Structured reason one backend operation could not produce a complete artifact set.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CodegenFailure {
    /// The selected backend does not support the requested target.
    UnsupportedTarget,
    /// The selected backend does not support one requested artifact kind.
    UnsupportedArtifact(BackendArtifactKind),
    /// Backend configuration is internally inconsistent or invalid.
    InvalidConfiguration,
    /// Code generation exceeded an available resource or worker budget.
    ResourceExhausted,
    /// The backend library failed while processing a valid request.
    BackendLibrary,
    /// Generated backend IR violated a backend module invariant.
    GeneratedModuleInvariant,
    /// Serialization failed for one requested artifact kind.
    ArtifactConstruction(BackendArtifactKind),
}

/// Atomic completion state of one backend operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodegenStatus {
    /// Every requested required artifact was generated and validated.
    Complete(BackendArtifactSet),
    /// Generation failed without publishing partial artifacts.
    Failed(CodegenFailure),
    /// Cancellation was observed before artifact publication.
    Cancelled,
}

/// Immutable result and structured diagnostics from one backend operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodegenOutcome {
    status: CodegenStatus,
    diagnostics: DiagnosticBag,
}

impl CodegenOutcome {
    /// Creates a successful outcome containing a complete artifact set.
    pub const fn complete(artifacts: BackendArtifactSet, diagnostics: DiagnosticBag) -> Self {
        Self {
            status: CodegenStatus::Complete(artifacts),
            diagnostics,
        }
    }

    /// Creates a failed outcome without partial artifacts.
    pub const fn failed(failure: CodegenFailure, diagnostics: DiagnosticBag) -> Self {
        Self {
            status: CodegenStatus::Failed(failure),
            diagnostics,
        }
    }

    /// Creates a cancelled outcome without diagnostics or partial artifacts.
    pub const fn cancelled() -> Self {
        Self {
            status: CodegenStatus::Cancelled,
            diagnostics: DiagnosticBag::new(),
        }
    }

    /// Returns the atomic completion state.
    pub const fn status(&self) -> &CodegenStatus {
        &self.status
    }

    /// Returns diagnostics produced by the completed or failed operation.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Returns the complete artifact set only after successful generation.
    pub const fn artifacts(&self) -> Option<&BackendArtifactSet> {
        match &self.status {
            CodegenStatus::Complete(artifacts) => Some(artifacts),
            CodegenStatus::Failed(_) | CodegenStatus::Cancelled => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticBag;

    use super::{CodegenFailure, CodegenOutcome, CodegenStatus};

    #[test]
    fn failed_and_cancelled_outcomes_cannot_expose_partial_artifacts() {
        let failed = CodegenOutcome::failed(
            CodegenFailure::GeneratedModuleInvariant,
            DiagnosticBag::new(),
        );
        let cancelled = CodegenOutcome::cancelled();

        assert!(matches!(failed.status(), CodegenStatus::Failed(_)));
        assert_eq!(failed.artifacts(), None);
        assert!(matches!(cancelled.status(), CodegenStatus::Cancelled));
        assert_eq!(cancelled.artifacts(), None);
    }
}
