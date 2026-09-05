use crate::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

/// Locale-neutral operation attempted on managed build storage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticStorageOperation {
    /// Inspect managed build state.
    Inspect,
    /// Create managed build state.
    Create,
    /// Read managed build state.
    Read,
    /// Write managed build state.
    Write,
    /// Flush managed build state.
    Flush,
    /// Lock managed build state.
    Lock,
    /// Rename managed build state.
    Rename,
    /// Remove managed build state.
    Remove,
}

impl DiagnosticStorageOperation {
    /// Returns the stable machine key for the operation.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inspect => "inspect",
            Self::Create => "create",
            Self::Read => "read",
            Self::Write => "write",
            Self::Flush => "flush",
            Self::Lock => "lock",
            Self::Rename => "rename",
            Self::Remove => "remove",
        }
    }
}

impl DiagnosticArg {
    /// Creates a managed build-storage operation argument.
    pub const fn storage_operation(operation: DiagnosticStorageOperation) -> Self {
        Self::new(
            DiagnosticArgName::StorageOperation,
            DiagnosticArgValue::StorageOperation(operation),
        )
    }
}

/// Specific validation failure in a retained product generation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticRetainedGenerationProblem {
    /// The publication reference uses a different revision.
    ReferenceRevision { expected: u32, actual: u32 },
    /// A locator or content identity violates its encoding contract.
    ReferenceIdentity,
    /// The manifest uses a different revision.
    ManifestRevision { expected: u32, actual: u32 },
    /// The manifest names a different selected product.
    ProductIdentity,
    /// The requested artifact is absent or ambiguous.
    ArtifactSelection,
    /// The manifest records an unsupported digest algorithm.
    DigestAlgorithm,
    /// A recorded content digest is not a lowercase 32-byte hexadecimal value.
    DigestEncoding,
    /// The recorded logical permission disagrees with the artifact kind.
    LogicalPermission,
    /// Read-only status differs from the published artifact's metadata.
    ReadOnly { expected: bool, actual: bool },
    /// Unix permission bits differ from the published artifact's metadata.
    UnixMode {
        expected: Option<u32>,
        actual: Option<u32>,
    },
    /// An artifact violates its recorded path or identity contract.
    ArtifactContract,
}

impl DiagnosticRetainedGenerationProblem {
    /// Returns the stable machine key for this failure.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReferenceRevision { .. } => "reference_revision",
            Self::ReferenceIdentity => "reference_identity",
            Self::ManifestRevision { .. } => "manifest_revision",
            Self::ProductIdentity => "product_identity",
            Self::ArtifactSelection => "artifact_selection",
            Self::ArtifactContract => "artifact_contract",
            Self::DigestAlgorithm => "digest_algorithm",
            Self::DigestEncoding => "digest_encoding",
            Self::LogicalPermission => "logical_permission",
            Self::ReadOnly { .. } => "read_only",
            Self::UnixMode { .. } => "unix_mode",
        }
    }

    /// Returns the required and recorded revisions when the failure concerns a schema revision.
    pub const fn revisions(self) -> Option<(u32, u32)> {
        match self {
            Self::ReferenceRevision { expected, actual }
            | Self::ManifestRevision { expected, actual } => Some((expected, actual)),
            _ => None,
        }
    }
}

impl DiagnosticArg {
    /// Creates a retained-generation validation argument.
    pub const fn retained_generation_problem(problem: DiagnosticRetainedGenerationProblem) -> Self {
        Self::new(
            DiagnosticArgName::RetainedGenerationProblem,
            DiagnosticArgValue::RetainedGenerationProblem(problem),
        )
    }
}
