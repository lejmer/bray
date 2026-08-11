use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an exact package or product identity participating in a dependency cycle.
    pub fn project_dependency_cycle_member(
        member: crate::DiagnosticProjectDependencyCycleMember,
    ) -> Self {
        Self::new(
            DiagnosticArgName::ProjectDependencyCycleMember,
            DiagnosticArgValue::ProjectDependencyCycleMember(member),
        )
    }
}
