use bray_diagnostics::DiagnosticProjectGeneratorFailure as Failure;
use serde::Serialize;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticProjectGeneratorFailureJson {
    UnknownReference {
        name: String,
    },
    ToolNotExecutable {
        tool: String,
    },
    ToolTargetMismatch {
        target: String,
        host_target: Option<String>,
    },
    InvalidEnvironment {
        name: String,
    },
    OutputCollision {
        path: String,
    },
    InvalidOutput {
        path: String,
    },
    UndeclaredOutput {
        path: String,
    },
    MissingOutput {
        path: String,
    },
    NonUtf8Output {
        path: String,
    },
    InputNotRegular {
        path: String,
    },
    InputIo {
        path: String,
        error_kind: &'static str,
    },
    DependencyCycle,
    ExecutorUnavailable,
    ToolUnavailable {
        tool: String,
    },
    Cancelled,
    SandboxUnavailable,
    AccessDenied {
        resource: String,
    },
    ExecutionFailed {
        code: Option<i32>,
    },
    ExecutorIo {
        error_kind: &'static str,
    },
}

impl From<&Failure> for DiagnosticProjectGeneratorFailureJson {
    fn from(failure: &Failure) -> Self {
        match failure {
            Failure::UnknownReference { name } => Self::UnknownReference { name: name.clone() },
            Failure::ToolNotExecutable { tool } => Self::ToolNotExecutable { tool: tool.clone() },
            Failure::ToolTargetMismatch {
                target,
                host_target,
            } => Self::ToolTargetMismatch {
                target: target.clone(),
                host_target: host_target.clone(),
            },
            Failure::InvalidEnvironment { name } => Self::InvalidEnvironment { name: name.clone() },
            Failure::OutputCollision { path } => Self::OutputCollision { path: path.clone() },
            Failure::InvalidOutput { path } => Self::InvalidOutput { path: path.clone() },
            Failure::UndeclaredOutput { path } => Self::UndeclaredOutput { path: path.clone() },
            Failure::MissingOutput { path } => Self::MissingOutput { path: path.clone() },
            Failure::NonUtf8Output { path } => Self::NonUtf8Output { path: path.clone() },
            Failure::InputNotRegular { path } => Self::InputNotRegular { path: path.clone() },
            Failure::InputIo { path, kind } => Self::InputIo {
                path: path.clone(),
                error_kind: kind.as_str(),
            },
            Failure::DependencyCycle => Self::DependencyCycle,
            Failure::ExecutorUnavailable => Self::ExecutorUnavailable,
            Failure::ToolUnavailable { tool } => Self::ToolUnavailable { tool: tool.clone() },
            Failure::Cancelled => Self::Cancelled,
            Failure::SandboxUnavailable => Self::SandboxUnavailable,
            Failure::AccessDenied { resource } => Self::AccessDenied {
                resource: resource.clone(),
            },
            Failure::ExecutionFailed { code } => Self::ExecutionFailed { code: *code },
            Failure::ExecutorIo { kind } => Self::ExecutorIo {
                error_kind: kind.as_str(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticIoErrorKind, DiagnosticProjectGeneratorFailure};
    use serde_json::json;

    use super::DiagnosticProjectGeneratorFailureJson;

    #[test]
    fn structured_generator_failures_preserve_identity_and_exact_io_cause() {
        let failure = DiagnosticProjectGeneratorFailure::InputIo {
            path: "schema.json".into(),
            kind: DiagnosticIoErrorKind::NotFound,
        };

        assert_eq!(
            serde_json::to_value(DiagnosticProjectGeneratorFailureJson::from(&failure))
                .expect("JSON projection"),
            json!({"kind": "input_io", "path": "schema.json", "error_kind": "not_found"})
        );
    }
}
