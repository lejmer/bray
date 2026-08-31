use std::sync::Arc;
use std::path::PathBuf;

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArtifactKind, DiagnosticBag,
    DiagnosticCodegenVerificationStage, DiagnosticId, DiagnosticKind, DiagnosticNote,
    DiagnosticNoteKind, SeverityKind,
};

use crate::{
    BackendArtifactContribution, BackendArtifactKind, BackendArtifactSet,
    BackendArtifactSetBuildError, CodegenRequest, CodegenRuntimeMetadata,
};

/// Structured reason one backend operation could not produce a complete artifact set.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CodegenFailure {
    /// The selected backend does not support the requested target.
    UnsupportedTarget,
    /// The selected backend does not support one requested artifact kind.
    UnsupportedArtifact(BackendArtifactKind),
    /// Backend configuration is internally inconsistent or invalid.
    InvalidConfiguration,
    /// Code generation exceeded an available resource or worker budget.
    ResourceExhausted,
    /// The backend library failed while processing a valid request and retained its exact report.
    BackendLibrary { report: Arc<str> },
    /// A backend support program completed unsuccessfully with its exact captured output.
    BackendToolExited {
        program: PathBuf,
        exit: bray_diagnostics::DiagnosticExternalToolExit,
    },
    /// Generated backend IR violated a backend module invariant.
    GeneratedModuleInvariant,
    /// The backend rejected generated native-code input and retained its exact report.
    BackendRejectedModule {
        stage: DiagnosticCodegenVerificationStage,
        report: Arc<str>,
    },
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
    ) -> Result<Self, CodegenOutcomeBuildError> {
        if diagnostics.has_errors() {
            return Err(CodegenOutcomeBuildError::ErrorDiagnostics(diagnostics));
        }

        let artifacts = BackendArtifactSet::try_new(request, contributions, runtime_metadata)
            .map_err(CodegenOutcomeBuildError::InvalidArtifacts)?;

        Ok(Self {
            status: CodegenStatus::Complete(Box::new(artifacts)),
            diagnostics,
        })
    }

    /// Creates a failed outcome with exact backend-request context and no partial artifacts.
    pub fn failed(
        request: CodegenRequest<'_>,
        failure: CodegenFailure,
        diagnostics: DiagnosticBag,
    ) -> Self {
        let terminal = codegen_failure_diagnostic(
            request.backend().name(),
            request.target().identity().as_str(),
            &failure,
        );

        let is_explained = diagnostics.iter().any(|diagnostic| {
            diagnostic.severity() == SeverityKind::Error
                && diagnostic.kind() == terminal.kind()
                && diagnostic.args() == terminal.args()
        });

        let diagnostics = if is_explained {
            diagnostics
        } else {
            diagnostics.merged(&DiagnosticBag::single(terminal))
        };

        Self {
            status: CodegenStatus::Failed(failure),
            diagnostics,
        }
    }

    /// Creates a cancelled outcome that retains diagnostics completed before cancellation.
    pub const fn cancelled(diagnostics: DiagnosticBag) -> Self {
        Self {
            status: CodegenStatus::Cancelled,
            diagnostics,
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

impl CodegenFailure {
    /// Retains the exact report returned by a backend library operation.
    pub fn backend_library(error: impl std::fmt::Display) -> Self {
        Self::BackendLibrary {
            report: Arc::from(error.to_string()),
        }
    }
}

/// A contract violation that prevents construction of a successful code generation outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CodegenOutcomeBuildError {
    /// One or more produced backend artifacts violate the authoritative request.
    InvalidArtifacts(BackendArtifactSetBuildError),
    /// Error diagnostics contradict a successful code generation status.
    ErrorDiagnostics(DiagnosticBag),
}

/// Returns a structured terminal diagnostic for one exact backend request failure.
pub fn codegen_failure_diagnostics(
    request: CodegenRequest<'_>,
    failure: &CodegenFailure,
) -> DiagnosticBag {
    DiagnosticBag::single(codegen_failure_diagnostic(
        request.backend().name(),
        request.target().identity().as_str(),
        failure,
    ))
}

/// Returns one structured terminal diagnostic for a backend failure with exact backend, target,
/// and affected-artifact context.
pub fn codegen_failure_diagnostic(
    backend: &str,
    target: &str,
    failure: &CodegenFailure,
) -> Diagnostic {
    let (kind, artifact) = match failure {
        CodegenFailure::UnsupportedTarget => (DiagnosticKind::CodegenUnsupportedTarget, None),
        CodegenFailure::UnsupportedArtifact(artifact) => {
            (DiagnosticKind::CodegenUnsupportedArtifact, Some(*artifact))
        }
        CodegenFailure::InvalidConfiguration => (DiagnosticKind::CodegenInvalidConfiguration, None),
        CodegenFailure::ResourceExhausted => (DiagnosticKind::CodegenResourceExhausted, None),
        CodegenFailure::BackendLibrary { .. } => {
            (DiagnosticKind::CodegenBackendLibraryFailed, None)
        }
        CodegenFailure::BackendToolExited { .. } => {
            (DiagnosticKind::CodegenBackendToolExited, None)
        }
        CodegenFailure::GeneratedModuleInvariant => {
            (DiagnosticKind::CodegenGeneratedModuleInvalid, None)
        }
        CodegenFailure::BackendRejectedModule { .. } => {
            (DiagnosticKind::CodegenBackendRejectedModule, None)
        }
        CodegenFailure::ArtifactConstruction(artifact) => (
            DiagnosticKind::CodegenArtifactConstructionFailed,
            Some(*artifact),
        ),
    };

    let mut diagnostic = Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error)
        .with_arg(DiagnosticArg::codegen_backend_identity(backend))
        .with_arg(DiagnosticArg::target_triple(target));

    if let Some(artifact) = artifact {
        diagnostic = diagnostic.with_arg(DiagnosticArg::artifact_kind(diagnostic_artifact_kind(
            artifact,
        )));
    }

    match failure {
        CodegenFailure::BackendLibrary { report } => {
            diagnostic = diagnostic.with_arg(DiagnosticArg::codegen_backend_report(report.as_ref()));
        }
        CodegenFailure::BackendToolExited { program, exit } => {
            diagnostic = diagnostic
                .with_arg(DiagnosticArg::file_path(program))
                .with_arg(DiagnosticArg::external_tool_exit(exit.clone()));
        }
        CodegenFailure::BackendRejectedModule { stage, report } => {
            diagnostic = diagnostic
                .with_arg(DiagnosticArg::codegen_verification_stage(*stage))
                .with_arg(DiagnosticArg::codegen_backend_report(report.as_ref()));
        }
        _ => {}
    }

    if matches!(
        failure,
        CodegenFailure::InvalidConfiguration
            | CodegenFailure::ResourceExhausted
            | CodegenFailure::BackendLibrary { .. }
            | CodegenFailure::BackendToolExited { .. }
            | CodegenFailure::GeneratedModuleInvariant
            | CodegenFailure::BackendRejectedModule { .. }
            | CodegenFailure::ArtifactConstruction(_)
    ) {
        diagnostic = diagnostic.with_note(DiagnosticNote::new(
            DiagnosticNoteKind::ReportCompilerDefect,
        ));
    }

    diagnostic
}

const fn diagnostic_artifact_kind(kind: BackendArtifactKind) -> DiagnosticArtifactKind {
    match kind {
        BackendArtifactKind::RelocatableObject => DiagnosticArtifactKind::RelocatableObject,
        BackendArtifactKind::Assembly => DiagnosticArtifactKind::Assembly,
        BackendArtifactKind::BackendIr => DiagnosticArtifactKind::BackendIr,
        BackendArtifactKind::BackendBitcode => DiagnosticArtifactKind::BackendBitcode,
        BackendArtifactKind::ExecutableModule => DiagnosticArtifactKind::ExecutableModule,
        BackendArtifactKind::DebugCompanion => DiagnosticArtifactKind::DebugCompanion,
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
    use bray_testing::assert_goal_state_diagnostic_kind;

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
        let fixture = codegen_request();

        let failed = CodegenOutcome::failed(
            fixture.request(),
            CodegenFailure::GeneratedModuleInvariant,
            DiagnosticBag::new(),
        );

        let cancelled = CodegenOutcome::cancelled(DiagnosticBag::new());

        assert!(matches!(failed.status(), CodegenStatus::Failed(_)));
        assert_eq!(failed.artifacts(), None);
        assert!(failed.diagnostics().has_errors());

        assert!(matches!(cancelled.status(), CodegenStatus::Cancelled));
        assert_eq!(cancelled.artifacts(), None);
    }

    #[test]
    fn every_terminal_codegen_failure_publishes_its_exact_diagnostic() {
        let fixture = codegen_request();

        let unsupported_target = CodegenOutcome::failed(
            fixture.request(),
            CodegenFailure::UnsupportedTarget,
            DiagnosticBag::new(),
        );

        assert_goal_state_diagnostic_kind(
            unsupported_target.diagnostics(),
            DiagnosticKind::CodegenUnsupportedTarget,
        );

        let unsupported_artifact = CodegenOutcome::failed(
            fixture.request(),
            CodegenFailure::UnsupportedArtifact(crate::BackendArtifactKind::Assembly),
            DiagnosticBag::new(),
        );

        assert_goal_state_diagnostic_kind(
            unsupported_artifact.diagnostics(),
            DiagnosticKind::CodegenUnsupportedArtifact,
        );

        let invalid_configuration = CodegenOutcome::failed(
            fixture.request(),
            CodegenFailure::InvalidConfiguration,
            DiagnosticBag::new(),
        );

        assert_goal_state_diagnostic_kind(
            invalid_configuration.diagnostics(),
            DiagnosticKind::CodegenInvalidConfiguration,
        );

        let resource_exhausted = CodegenOutcome::failed(
            fixture.request(),
            CodegenFailure::ResourceExhausted,
            DiagnosticBag::new(),
        );

        assert_goal_state_diagnostic_kind(
            resource_exhausted.diagnostics(),
            DiagnosticKind::CodegenResourceExhausted,
        );

        let backend_library = CodegenOutcome::failed(
            fixture.request(),
            CodegenFailure::backend_library("backend test failure"),
            DiagnosticBag::new(),
        );

        assert_goal_state_diagnostic_kind(
            backend_library.diagnostics(),
            DiagnosticKind::CodegenBackendLibraryFailed,
        );

        let backend_tool_exit = CodegenOutcome::failed(
            fixture.request(),
            CodegenFailure::BackendToolExited {
                program: std::path::PathBuf::from("opt"),
                exit: bray_diagnostics::DiagnosticExternalToolExit::new(
                    Some(1),
                    &[],
                    b"optimizer failure",
                ),
            },
            DiagnosticBag::new(),
        );

        assert_goal_state_diagnostic_kind(
            backend_tool_exit.diagnostics(),
            DiagnosticKind::CodegenBackendToolExited,
        );

        let generated_module = CodegenOutcome::failed(
            fixture.request(),
            CodegenFailure::GeneratedModuleInvariant,
            DiagnosticBag::new(),
        );

        assert_goal_state_diagnostic_kind(
            generated_module.diagnostics(),
            DiagnosticKind::CodegenGeneratedModuleInvalid,
        );

        let rejected_module = CodegenOutcome::failed(
            fixture.request(),
            CodegenFailure::BackendRejectedModule {
                stage: bray_diagnostics::DiagnosticCodegenVerificationStage::BeforeOptimization,
                report: std::sync::Arc::from("value representation mismatch"),
            },
            DiagnosticBag::new(),
        );

        assert_goal_state_diagnostic_kind(
            rejected_module.diagnostics(),
            DiagnosticKind::CodegenBackendRejectedModule,
        );

        let artifact_construction = CodegenOutcome::failed(
            fixture.request(),
            CodegenFailure::ArtifactConstruction(crate::BackendArtifactKind::BackendBitcode),
            DiagnosticBag::new(),
        );

        assert_goal_state_diagnostic_kind(
            artifact_construction.diagnostics(),
            DiagnosticKind::CodegenArtifactConstructionFailed,
        );
    }

    #[test]
    fn successful_outcomes_reject_error_diagnostics() {
        let fixture = codegen_request();
        let artifact = contribution(&fixture, fixture.required_artifact().clone());

        let diagnostics = DiagnosticBag::single(bray_diagnostics::Diagnostic::new(
            bray_diagnostics::DiagnosticId::new(0),
            bray_diagnostics::DiagnosticKind::CodegenGeneratedModuleInvalid,
            bray_diagnostics::SeverityKind::Error,
        ));

        assert!(matches!(
            CodegenOutcome::try_complete(
                fixture.request(),
                [artifact],
                CodegenRuntimeMetadata::default(),
                diagnostics.clone(),
            ),
            Err(super::CodegenOutcomeBuildError::ErrorDiagnostics(actual)) if actual == diagnostics
        ));
    }
}
