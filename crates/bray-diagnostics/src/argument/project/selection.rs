use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an exact source-generator failure argument.
    pub fn project_generator_failure(failure: crate::DiagnosticProjectGeneratorFailure) -> Self {
        Self::new(
            DiagnosticArgName::ProjectGeneratorFailure,
            DiagnosticArgValue::ProjectGeneratorFailure(failure),
        )
    }

    /// Creates an exact command-selection problem argument.
    pub fn project_selection_problem(problem: crate::DiagnosticProjectSelectionProblem) -> Self {
        Self::new(
            DiagnosticArgName::ProjectSelectionProblem,
            DiagnosticArgValue::ProjectSelectionProblem(problem),
        )
    }
}
