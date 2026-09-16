use std::path::PathBuf;
use std::sync::Arc;

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticNote,
    DiagnosticNoteKind, SeverityKind,
};

use crate::{
    ArtifactContentBuildError, BackendArtifactContribution, BackendArtifactKind, CodegenRequest,
};

/// Structured reason one backend operation could not produce a complete artifact set.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CodegenFailure {
    /// The selected backend does not support the requested target.
    UnsupportedTarget,
    /// The selected backend rejected the target and retained its exact report.
    UnsupportedTargetReport { report: Arc<str> },
    /// One exact target constraint cannot represent the requested value.
    UnsupportedTargetValue {
        constraint: &'static str,
        actual: Arc<str>,
    },
    /// The selected backend does not support one requested artifact kind.
    UnsupportedArtifact(BackendArtifactKind),
    /// Backend configuration is internally inconsistent or invalid.
    InvalidConfiguration,
    /// Backend configuration is invalid and retains the exact rejected input report.
    InvalidConfigurationReport { report: Arc<str> },
    /// Code generation exceeded an available resource or worker budget.
    ResourceExhausted,
    /// One exact code generation resource cannot represent the requested value.
    ResourceLimit {
        resource: &'static str,
        actual: Arc<str>,
    },
    /// The backend library failed while processing a valid request and retained its exact report.
    BackendLibrary { report: Arc<str> },
    /// A backend support program completed unsuccessfully with its exact captured output.
    BackendToolExited {
        program: PathBuf,
        exit: bray_diagnostics::DiagnosticExternalToolExit,
    },
    /// Serialization failed for one requested artifact kind.
    ArtifactConstruction(BackendArtifactKind),
    /// Backend serialization failed for one requested artifact kind.
    ArtifactSerialization {
        artifact: BackendArtifactKind,
        report: Arc<str>,
    },
    /// Serialized bytes could not form immutable artifact content.
    InvalidArtifactContent {
        artifact: BackendArtifactKind,
        cause: ArtifactContentBuildError,
    },
}

/// Atomic completion state of one backend operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CodegenStatus {
    /// Every requested required artifact was generated and validated.
    Complete(Arc<[BackendArtifactContribution]>),
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
    /// Returns a complete outcome from backend-produced contributions.
    pub fn complete(
        contributions: impl IntoIterator<Item = BackendArtifactContribution>,
        diagnostics: DiagnosticBag,
    ) -> Self {
        let mut contributions: Vec<_> = contributions.into_iter().collect();

        contributions.sort_unstable_by(|left, right| left.id().cmp(right.id()));

        Self {
            status: CodegenStatus::Complete(contributions.into()),
            diagnostics,
        }
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
    pub fn artifacts(&self) -> Option<&[BackendArtifactContribution]> {
        match &self.status {
            CodegenStatus::Complete(artifacts) => Some(artifacts),
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

    /// Retains an exact backend-configuration rejection report.
    pub fn invalid_configuration(error: impl std::fmt::Debug) -> Self {
        Self::InvalidConfigurationReport {
            report: Arc::from(format!("{error:?}")),
        }
    }

    /// Retains an exact backend report for an unsupported target.
    pub fn unsupported_target_report(error: impl std::fmt::Display) -> Self {
        Self::UnsupportedTargetReport {
            report: Arc::from(error.to_string()),
        }
    }

    /// Retains an exact value that exceeds one target constraint.
    pub fn unsupported_target_value(
        constraint: &'static str,
        actual: impl std::fmt::Display,
    ) -> Self {
        Self::UnsupportedTargetValue {
            constraint,
            actual: Arc::from(actual.to_string()),
        }
    }

    /// Retains an exact value that exceeds one code generation resource.
    pub fn resource_limit(resource: &'static str, actual: impl std::fmt::Display) -> Self {
        Self::ResourceLimit {
            resource,
            actual: Arc::from(actual.to_string()),
        }
    }
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
        CodegenFailure::UnsupportedTarget
        | CodegenFailure::UnsupportedTargetReport { .. }
        | CodegenFailure::UnsupportedTargetValue { .. } => {
            (DiagnosticKind::CodegenUnsupportedTarget, None)
        }
        CodegenFailure::UnsupportedArtifact(artifact) => {
            (DiagnosticKind::CodegenUnsupportedArtifact, Some(*artifact))
        }
        CodegenFailure::InvalidConfiguration
        | CodegenFailure::InvalidConfigurationReport { .. } => {
            (DiagnosticKind::CodegenInvalidConfiguration, None)
        }
        CodegenFailure::ResourceExhausted | CodegenFailure::ResourceLimit { .. } => {
            (DiagnosticKind::CodegenResourceExhausted, None)
        }
        CodegenFailure::BackendLibrary { .. } => {
            (DiagnosticKind::CodegenBackendLibraryFailed, None)
        }
        CodegenFailure::BackendToolExited { .. } => {
            (DiagnosticKind::CodegenBackendToolExited, None)
        }
        CodegenFailure::ArtifactConstruction(artifact)
        | CodegenFailure::ArtifactSerialization { artifact, .. }
        | CodegenFailure::InvalidArtifactContent { artifact, .. } => (
            DiagnosticKind::CodegenArtifactConstructionFailed,
            Some(*artifact),
        ),
    };

    let mut diagnostic = Diagnostic::new(DiagnosticId::new(0), kind, SeverityKind::Error)
        .with_arg(DiagnosticArg::codegen_backend_identity(backend))
        .with_arg(DiagnosticArg::target_triple(target));

    if let Some(artifact) = artifact {
        diagnostic = diagnostic.with_arg(DiagnosticArg::artifact_kind(artifact.diagnostic_kind()));
    }

    match failure {
        CodegenFailure::BackendLibrary { report }
        | CodegenFailure::InvalidConfigurationReport { report } => {
            diagnostic =
                diagnostic.with_arg(DiagnosticArg::codegen_backend_report(report.as_ref()));
        }
        CodegenFailure::UnsupportedTargetReport { report }
        | CodegenFailure::ArtifactSerialization { report, .. } => {
            diagnostic =
                diagnostic.with_arg(DiagnosticArg::codegen_backend_report(report.as_ref()));
        }
        CodegenFailure::UnsupportedTargetValue { constraint, actual } => {
            diagnostic = diagnostic.with_arg(DiagnosticArg::codegen_backend_report(format!(
                "{constraint}={actual}"
            )));
        }
        CodegenFailure::ResourceLimit { resource, actual } => {
            diagnostic = diagnostic.with_arg(DiagnosticArg::codegen_backend_report(format!(
                "{resource}={actual}"
            )));
        }
        CodegenFailure::InvalidArtifactContent { cause, .. } => {
            diagnostic =
                diagnostic.with_arg(DiagnosticArg::codegen_backend_report(format!("{cause:?}")));
        }
        CodegenFailure::BackendToolExited { program, exit } => {
            diagnostic = diagnostic
                .with_arg(DiagnosticArg::file_path(program))
                .with_arg(DiagnosticArg::external_tool_exit(exit.clone()));
        }
        _ => {}
    }

    if matches!(
        failure,
        CodegenFailure::InvalidConfiguration
            | CodegenFailure::InvalidConfigurationReport { .. }
            | CodegenFailure::ResourceExhausted
            | CodegenFailure::ResourceLimit { .. }
            | CodegenFailure::BackendLibrary { .. }
            | CodegenFailure::BackendToolExited { .. }
            | CodegenFailure::ArtifactConstruction(_)
            | CodegenFailure::ArtifactSerialization { .. }
            | CodegenFailure::InvalidArtifactContent { .. }
    ) {
        diagnostic = diagnostic.with_note(DiagnosticNote::new(
            DiagnosticNoteKind::ReportCompilerDefect,
        ));
    }

    diagnostic
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticBag, DiagnosticKind};
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::{CodegenFailure, CodegenOutcome, CodegenStatus};
    use crate::test_support::{codegen_request, contribution};

    #[test]
    fn completed_outcomes_retain_backend_produced_artifacts() {
        let fixture = codegen_request();
        let first = contribution(fixture.required_artifact().clone());
        let second = contribution(fixture.optional_artifact().clone());

        let outcome = CodegenOutcome::complete([second, first], DiagnosticBag::new());

        let Some(artifacts) = outcome.artifacts() else {
            panic!("complete outcome must retain backend artifacts");
        };

        assert_eq!(artifacts.len(), 2);
        assert_eq!(artifacts[0].id(), fixture.required_artifact());
        assert_eq!(artifacts[1].id(), fixture.optional_artifact());
    }

    #[test]
    fn failed_and_cancelled_outcomes_cannot_expose_partial_artifacts() {
        let fixture = codegen_request();

        let failed = CodegenOutcome::failed(
            fixture.request(),
            CodegenFailure::backend_library("test backend failure"),
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
    fn payload_bearing_codegen_failures_keep_exact_diagnostic_context() {
        let fixture = codegen_request();

        let outcome = CodegenOutcome::failed(
            fixture.request(),
            CodegenFailure::resource_limit("callbr_argument_count", usize::MAX),
            DiagnosticBag::new(),
        );

        let report = outcome
            .diagnostics()
            .iter()
            .flat_map(|diagnostic| diagnostic.args())
            .find_map(|argument| match argument.value() {
                bray_diagnostics::DiagnosticArgValue::CodegenBackendReport(report) => {
                    Some(report.as_str())
                }
                _ => None,
            });

        let expected = format!("callbr_argument_count={}", usize::MAX);

        assert_eq!(report, Some(expected.as_str()));
    }
}
