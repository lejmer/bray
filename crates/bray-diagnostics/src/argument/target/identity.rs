use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates the target identity required by a runtime artifact contract.
    pub fn expected_target_identity(identity: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedTargetIdentity,
            DiagnosticArgValue::TargetIdentity(identity.into()),
        )
    }

    /// Creates the target identity supplied by a runtime artifact.
    pub fn actual_target_identity(identity: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::ActualTargetIdentity,
            DiagnosticArgValue::TargetIdentity(identity.into()),
        )
    }

    /// Creates an exact target-triple argument.
    pub fn target_triple(target: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::TargetTriple,
            DiagnosticArgValue::TargetTriple(target.into()),
        )
    }

    /// Creates the target triple required by a diagnostic contract.
    pub fn expected_target_triple(target: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedTargetTriple,
            DiagnosticArgValue::TargetTriple(target.into()),
        )
    }

    /// Creates the target triple that conflicted with a diagnostic contract.
    pub fn actual_target_triple(target: impl Into<String>) -> Self {
        Self::new(
            DiagnosticArgName::ActualTargetTriple,
            DiagnosticArgValue::TargetTriple(target.into()),
        )
    }
}
