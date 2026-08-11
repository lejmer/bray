use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an exact command-selection problem argument.
    pub fn project_selection_problem(problem: crate::DiagnosticProjectSelectionProblem) -> Self {
        Self::new(
            DiagnosticArgName::ProjectSelectionProblem,
            DiagnosticArgValue::ProjectSelectionProblem(problem),
        )
    }
}
