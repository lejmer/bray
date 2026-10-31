use bray_diagnostics::DiagnosticProjectGeneratorFailure as Failure;

use super::super::source::format_english_io_error_kind;

pub(crate) fn format_english_project_generator_failure(failure: &Failure) -> String {
    match failure {
        Failure::UnknownReference { name } => {
            format!("'{name}' does not name a declared generator or output")
        }
        Failure::ToolNotExecutable { tool } => {
            format!("tool '{tool}' must name a declared executable product with executable output")
        }
        Failure::ToolTargetMismatch {
            target,
            host_target: Some(host_target),
        } => {
            format!("tool target '{target}' does not match compiler host target '{host_target}'")
        }
        Failure::ToolTargetMismatch {
            target,
            host_target: None,
        } => {
            format!("the compiler host cannot execute tool target '{target}'")
        }
        Failure::InvalidEnvironment { name } => format!(
            "environment variable '{name}' has a name or value that cannot be passed to a process"
        ),
        Failure::OutputCollision { path } => {
            format!("output '{path}' collides with another declared or written output")
        }
        Failure::InvalidOutput { path } => format!(
            "output '{path}' must be a portable relative file path with the exact .bray extension"
        ),
        Failure::UndeclaredOutput { path } => format!("output '{path}' is not declared"),
        Failure::MissingOutput { path } => format!("declared output '{path}' was not produced"),
        Failure::NonUtf8Output { path } => format!("output '{path}' is not UTF-8 source text"),
        Failure::InputNotRegular { path } => format!(
            "input '{path}' must be a regular file beneath its package with no symbolic links"
        ),
        Failure::InputIo { path, kind } => format!(
            "cannot read input '{path}': {}",
            format_english_io_error_kind(*kind)
        ),
        Failure::DependencyCycle => {
            "product, host-tool, and generated-source dependencies form a cycle".into()
        }
        Failure::ExecutorUnavailable => "the host has not supplied a sandbox executor".into(),
        Failure::ToolUnavailable { tool } => {
            format!("the host has not supplied the exact tool artifact '{tool}'")
        }
        Failure::Cancelled => "generation was cancelled before publication".into(),
        Failure::SandboxUnavailable => {
            "the executor cannot enforce the required filesystem and network isolation".into()
        }
        Failure::AccessDenied { resource } => format!("the sandbox denied access to '{resource}'"),
        Failure::ExecutionFailed { code: Some(code) } => {
            format!("the generator exited with code {code}")
        }
        Failure::ExecutionFailed { code: None } => {
            "the generator was terminated without an exit code".into()
        }
        Failure::ExecutorIo { kind } => format!(
            "the sandbox executor failed: {}",
            format_english_io_error_kind(*kind)
        ),
    }
}
