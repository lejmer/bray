use bray_diagnostics::DiagnosticBag;

use crate::{
    BackendArtifactContribution, BackendArtifactKind, BackendArtifactSet,
    BackendArtifactSetBuildError, CodegenRequest, CodegenRuntimeMetadata,
};

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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CodegenStatus {
    /// Every requested required artifact was generated and validated.
    Complete(Box<BackendArtifactSet>),
    /// Generation failed without returning partial artifacts.
    Failed(CodegenFailure),
    /// Cancellation was observed before artifacts were completed.
    Cancelled,
}

/// Immutable result and structured diagnostics from one backend operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CodegenOutcome {
    status: CodegenStatus,
    diagnostics: DiagnosticBag,
}

impl CodegenOutcome {
    /// Returns a complete outcome after validating contributions against the request.
    pub fn try_complete(
        request: CodegenRequest<'_>,
        contributions: impl IntoIterator<Item = BackendArtifactContribution>,
        runtime_metadata: CodegenRuntimeMetadata,
        diagnostics: DiagnosticBag,
    ) -> Result<Self, BackendArtifactSetBuildError> {
        let artifacts = BackendArtifactSet::try_new(request, contributions, runtime_metadata)?;

        Ok(Self {
            status: CodegenStatus::Complete(Box::new(artifacts)),
            diagnostics,
        })
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
    pub fn artifacts(&self) -> Option<&BackendArtifactSet> {
        match &self.status {
            CodegenStatus::Complete(artifacts) => Some(artifacts.as_ref()),
            CodegenStatus::Failed(_) | CodegenStatus::Cancelled => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticBag;

    use super::{CodegenFailure, CodegenOutcome, CodegenStatus};
    use crate::CodegenRuntimeMetadata;
    use crate::test_support::{codegen_request, contribution};

    #[test]
    fn completed_outcomes_publish_only_authoritatively_validated_artifacts() {
        let fixture = codegen_request();
        let artifact = contribution(&fixture, fixture.required_artifact().clone());

        let Ok(outcome) = CodegenOutcome::try_complete(
            fixture.request(),
            [artifact],
            CodegenRuntimeMetadata::default(),
            DiagnosticBag::new(),
        ) else {
            panic!("requested test contribution must complete generation");
        };

        let Some(artifacts) = outcome.artifacts() else {
            panic!("complete outcome must retain validated artifacts");
        };

        assert_eq!(artifacts.unit(), fixture.request().unit().key());
        assert_eq!(artifacts.backend(), fixture.request().backend());
        assert_eq!(artifacts.target(), fixture.request().target().identity());
    }

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
