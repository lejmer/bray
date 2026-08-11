use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an exact project-manifest field argument.
    pub const fn project_manifest_field(field: crate::DiagnosticProjectManifestField) -> Self {
        Self::new(
            DiagnosticArgName::ProjectManifestField,
            DiagnosticArgValue::ProjectManifestField(field),
        )
    }

    /// Creates an actual manifest or interface revision argument.
    pub const fn actual_revision(revision: u64) -> Self {
        Self::new(
            DiagnosticArgName::ActualRevision,
            DiagnosticArgValue::Revision(revision),
        )
    }

    /// Creates an expected manifest or interface revision argument.
    pub const fn expected_revision(revision: u64) -> Self {
        Self::new(
            DiagnosticArgName::ExpectedRevision,
            DiagnosticArgValue::Revision(revision),
        )
    }
}
