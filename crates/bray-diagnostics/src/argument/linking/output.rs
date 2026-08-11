use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates the ordinal of one linked output staging destination.
    pub const fn link_output_ordinal(ordinal: u32) -> Self {
        Self::new(
            DiagnosticArgName::LinkOutputOrdinal,
            DiagnosticArgValue::Count(ordinal as u64),
        )
    }

    /// Creates the ordinal of a conflicting linked output staging destination.
    pub const fn conflicting_link_output_ordinal(ordinal: u32) -> Self {
        Self::new(
            DiagnosticArgName::ConflictingLinkOutputOrdinal,
            DiagnosticArgValue::Count(ordinal as u64),
        )
    }

    /// Creates the linked artifact category found while building a link plan.
    pub const fn actual_linked_artifact_kind(kind: DiagnosticLinkedArtifactKind) -> Self {
        Self::new(
            DiagnosticArgName::ActualLinkedArtifactKind,
            DiagnosticArgValue::LinkedArtifactKind(kind),
        )
    }

    /// Creates the linked product category found while building a link plan.
    pub const fn actual_linked_product_kind(kind: DiagnosticLinkedProductKind) -> Self {
        Self::new(
            DiagnosticArgName::ActualLinkedProductKind,
            DiagnosticArgValue::LinkedProductKind(kind),
        )
    }
}

/// Locale-neutral category of one linked artifact.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLinkedArtifactKind {
    Executable,
    SharedLibrary,
    StaticLibrary,
    ImportLibrary,
    DebugCompanion,
    PlatformCompanion,
}

impl DiagnosticLinkedArtifactKind {
    /// Returns the stable machine key for this linked artifact category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Executable => "executable",
            Self::SharedLibrary => "shared_library",
            Self::StaticLibrary => "static_library",
            Self::ImportLibrary => "import_library",
            Self::DebugCompanion => "debug_companion",
            Self::PlatformCompanion => "platform_companion",
        }
    }
}

/// Locale-neutral category of one linked product.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLinkedProductKind {
    Executable,
    SharedLibrary,
    StaticLibrary,
}

impl DiagnosticLinkedProductKind {
    /// Returns the stable machine key for this linked product category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Executable => "executable",
            Self::SharedLibrary => "shared_library",
            Self::StaticLibrary => "static_library",
        }
    }
}
