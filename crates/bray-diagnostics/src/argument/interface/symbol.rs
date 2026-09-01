use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an exact symbol-category argument.
    pub fn actual_symbol_kind(kind: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::ActualSymbolKind,
            DiagnosticArgValue::SymbolKind(kind.into()),
        )
    }

    /// Creates the symbol category required by an artifact contract.
    pub fn expected_symbol_kind(kind: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedSymbolKind,
            DiagnosticArgValue::SymbolKind(kind.into()),
        )
    }

    /// Creates an exact imported symbol-graph construction problem argument.
    pub fn interface_symbol_graph_problem(
        problem: crate::DiagnosticInterfaceSymbolGraphProblem,
    ) -> Self {
        Self::new(
            DiagnosticArgName::InterfaceSymbolGraphProblem,
            DiagnosticArgValue::InterfaceSymbolGraphProblem(problem),
        )
    }

    /// Creates an exact imported semantic-content problem argument.
    pub fn interface_semantic_problem(problem: crate::DiagnosticInterfaceSemanticProblem) -> Self {
        Self::new(
            DiagnosticArgName::InterfaceSemanticProblem,
            DiagnosticArgValue::InterfaceSemanticProblem(problem),
        )
    }

    /// Creates an exact package-interface validation failure argument.
    pub fn interface_validation_failure(
        failure: crate::DiagnosticInterfaceValidationFailure,
    ) -> Self {
        Self::new(
            DiagnosticArgName::InterfaceValidationFailure,
            DiagnosticArgValue::InterfaceValidationFailure(failure),
        )
    }

    /// Creates an exact recursive package-interface symbol identity argument.
    pub fn interface_symbol_identity(identity: crate::DiagnosticInterfaceSymbolIdentity) -> Self {
        Self::new(
            DiagnosticArgName::InterfaceSymbolIdentity,
            DiagnosticArgValue::InterfaceSymbolIdentity(identity),
        )
    }
}
