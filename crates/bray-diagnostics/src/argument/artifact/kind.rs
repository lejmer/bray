use super::super::{DiagnosticArg, DiagnosticArgName, DiagnosticArgValue};

impl DiagnosticArg {
    /// Creates an artifact-kind argument.
    pub const fn artifact_kind(kind: DiagnosticArtifactKind) -> Self {
        Self::new(
            DiagnosticArgName::ArtifactKind,
            DiagnosticArgValue::ArtifactKind(kind),
        )
    }

    /// Creates a same-kind artifact ordinal argument.
    pub const fn artifact_ordinal(ordinal: u32) -> Self {
        Self::new(
            DiagnosticArgName::ArtifactOrdinal,
            DiagnosticArgValue::ArtifactOrdinal(ordinal),
        )
    }
}

/// Locale-neutral compiler artifact category used by diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticArtifactKind {
    /// Human-readable target assembly.
    Assembly,
    /// Human-readable backend low-level IR.
    BackendIr,
    /// Backend binary IR or bitcode.
    BackendBitcode,
    /// Relocatable native object.
    RelocatableObject,
    /// Backend-owned directly executable module.
    ExecutableModule,
    /// Separately stored debug data.
    DebugCompanion,
    /// Compiled package interface.
    PackageInterface,
    /// Compiled package implementation payloads.
    PackageImplementation,
    /// Compiler-owned dependency metadata.
    DependencyMetadata,
    /// Final executable product.
    Executable,
    /// Final static library product.
    StaticLibrary,
    /// Final shared library product.
    SharedLibrary,
    /// Target-required linked companion.
    LinkedCompanion,
}

impl DiagnosticArtifactKind {
    /// Returns the stable machine key for this artifact category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Assembly => "assembly",
            Self::BackendIr => "backend_ir",
            Self::BackendBitcode => "backend_bitcode",
            Self::RelocatableObject => "relocatable_object",
            Self::ExecutableModule => "executable_module",
            Self::DebugCompanion => "debug_companion",
            Self::PackageInterface => "package_interface",
            Self::PackageImplementation => "package_implementation",
            Self::DependencyMetadata => "dependency_metadata",
            Self::Executable => "executable",
            Self::StaticLibrary => "static_library",
            Self::SharedLibrary => "shared_library",
            Self::LinkedCompanion => "linked_companion",
        }
    }
}
