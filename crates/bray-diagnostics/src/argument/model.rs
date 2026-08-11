use std::path::PathBuf;

use super::{DiagnosticArgName, DiagnosticArgValue};

/// Stable typed argument attached to a diagnostic message component.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticArg {
    name: DiagnosticArgName,
    value: DiagnosticArgValue,
}

impl DiagnosticArg {
    /// Creates a typed diagnostic argument.
    pub const fn new(name: DiagnosticArgName, value: DiagnosticArgValue) -> Self {
        Self { name, value }
    }

    /// Creates a byte-count argument from a platform byte count.
    pub fn byte_count(byte_count: usize) -> Option<Self> {
        let byte_count = u64::try_from(byte_count).ok()?;

        Some(Self::new(
            DiagnosticArgName::ByteCount,
            DiagnosticArgValue::ByteCount(byte_count),
        ))
    }

    /// Creates a file-path argument.
    pub fn file_path(path: impl Into<PathBuf>) -> Self {
        Self::new(
            DiagnosticArgName::FilePath,
            DiagnosticArgValue::FilePath(path.into()),
        )
    }

    /// Creates an external package-interface artifact-path argument.
    pub fn artifact_path(path: impl Into<PathBuf>) -> Self {
        Self::new(
            DiagnosticArgName::ArtifactPath,
            DiagnosticArgValue::FilePath(path.into()),
        )
    }

    /// Creates a path selected by a Bray project manifest.
    pub fn project_path(path: impl Into<PathBuf>) -> Self {
        Self::new(
            DiagnosticArgName::ProjectPath,
            DiagnosticArgValue::FilePath(path.into()),
        )
    }
}

impl DiagnosticArg {
    /// Returns the stable argument name.
    pub const fn name(&self) -> DiagnosticArgName {
        self.name
    }

    /// Returns the typed argument value.
    pub const fn value(&self) -> &DiagnosticArgValue {
        &self.value
    }
}
