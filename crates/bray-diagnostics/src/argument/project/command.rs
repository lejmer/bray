use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an exact project-command failure argument.
    pub fn project_command_failure(failure: crate::DiagnosticProjectCommandFailure) -> Self {
        Self::new(
            DiagnosticArgName::ProjectCommandFailure,
            DiagnosticArgValue::ProjectCommandFailure(failure),
        )
    }
}
