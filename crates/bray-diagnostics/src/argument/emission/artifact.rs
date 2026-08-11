use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an exact emission-artifact operation category.
    pub const fn emission_artifact_operation(kind: DiagnosticEmissionArtifactOperation) -> Self {
        Self::new(
            DiagnosticArgName::EmissionArtifactOperation,
            DiagnosticArgValue::EmissionArtifactOperation(kind),
        )
    }

    /// Creates the backend artifact requirement that could not be satisfied.
    pub const fn artifact_requirement(kind: DiagnosticArtifactRequirement) -> Self {
        Self::new(
            DiagnosticArgName::ArtifactRequirement,
            DiagnosticArgValue::ArtifactRequirement(kind),
        )
    }
}

/// Locale-neutral artifact operation performed by emission.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticEmissionArtifactOperation {
    CreateStagingStorage,
    ReadContribution,
    WriteStagingStorage,
    FlushStagingStorage,
}

impl DiagnosticEmissionArtifactOperation {
    /// Returns the stable machine key for this operation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CreateStagingStorage => "create_staging_storage",
            Self::ReadContribution => "read_contribution",
            Self::WriteStagingStorage => "write_staging_storage",
            Self::FlushStagingStorage => "flush_staging_storage",
        }
    }
}

/// Locale-neutral strength of one backend artifact requirement.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticArtifactRequirement {
    Required,
    Optional,
}

impl DiagnosticArtifactRequirement {
    /// Returns the stable machine key for this requirement strength.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Required => "required",
            Self::Optional => "optional",
        }
    }
}
