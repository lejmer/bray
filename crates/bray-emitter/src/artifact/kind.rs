use bray_codegen::{BackendArtifactId, BackendArtifactKind, BackendIdentity};
use bray_diagnostics::DiagnosticArtifactKind;
use bray_linker::{LinkedArtifactKind, LinkedArtifactRequirement};
use bray_target::TargetOutputKind;

use crate::{DependencyMetadataProducerId, LinkerProducerId};

/// External or staging artifact categories understood by emission policy.
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    /// Human-readable target assembly.
    Assembly,
    /// Human-readable backend low-level IR.
    BackendIr,
    /// Backend-owned binary IR or bitcode.
    BackendBitcode,
    /// Relocatable native object.
    RelocatableObject,
    /// Backend-owned directly executable target module.
    ExecutableModule,
    /// Backend-owned separately stored debug data.
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
    /// Target-required companion to a linked product.
    LinkedCompanion,
}

impl ArtifactKind {
    pub(crate) const fn machine_key(self) -> &'static str {
        self.diagnostic_kind().as_str()
    }

    /// Returns the locale-neutral diagnostic artifact category.
    pub const fn diagnostic_kind(self) -> DiagnosticArtifactKind {
        match self {
            Self::Assembly => DiagnosticArtifactKind::Assembly,
            Self::BackendIr => DiagnosticArtifactKind::BackendIr,
            Self::BackendBitcode => DiagnosticArtifactKind::BackendBitcode,
            Self::RelocatableObject => DiagnosticArtifactKind::RelocatableObject,
            Self::ExecutableModule => DiagnosticArtifactKind::ExecutableModule,
            Self::DebugCompanion => DiagnosticArtifactKind::DebugCompanion,
            Self::PackageInterface => DiagnosticArtifactKind::PackageInterface,
            Self::PackageImplementation => DiagnosticArtifactKind::PackageImplementation,
            Self::DependencyMetadata => DiagnosticArtifactKind::DependencyMetadata,
            Self::Executable => DiagnosticArtifactKind::Executable,
            Self::StaticLibrary => DiagnosticArtifactKind::StaticLibrary,
            Self::SharedLibrary => DiagnosticArtifactKind::SharedLibrary,
            Self::LinkedCompanion => DiagnosticArtifactKind::LinkedCompanion,
        }
    }

    /// Returns the backend artifact category when code generation owns its bytes.
    pub const fn backend_kind(self) -> Option<BackendArtifactKind> {
        match self {
            Self::Assembly => Some(BackendArtifactKind::Assembly),
            Self::BackendIr => Some(BackendArtifactKind::BackendIr),
            Self::BackendBitcode => Some(BackendArtifactKind::BackendBitcode),
            Self::RelocatableObject => Some(BackendArtifactKind::RelocatableObject),
            Self::ExecutableModule => Some(BackendArtifactKind::ExecutableModule),
            Self::DebugCompanion => Some(BackendArtifactKind::DebugCompanion),
            Self::PackageInterface
            | Self::PackageImplementation
            | Self::DependencyMetadata
            | Self::Executable
            | Self::StaticLibrary
            | Self::SharedLibrary
            | Self::LinkedCompanion => None,
        }
    }

    pub(crate) const fn target_output_kind(self) -> TargetOutputKind {
        match self {
            Self::Assembly => TargetOutputKind::Assembly,
            Self::BackendIr => TargetOutputKind::BackendIr,
            Self::BackendBitcode => TargetOutputKind::BackendBitcode,
            Self::RelocatableObject => TargetOutputKind::RelocatableObject,
            Self::ExecutableModule => TargetOutputKind::ExecutableModule,
            Self::DebugCompanion => TargetOutputKind::DebugCompanion,
            Self::PackageInterface => TargetOutputKind::PackageInterface,
            Self::PackageImplementation => TargetOutputKind::PackageImplementation,
            Self::DependencyMetadata => TargetOutputKind::DependencyMetadata,
            Self::Executable => TargetOutputKind::Executable,
            Self::StaticLibrary => TargetOutputKind::StaticLibrary,
            Self::SharedLibrary => TargetOutputKind::SharedLibrary,
            Self::LinkedCompanion => TargetOutputKind::LinkedCompanion,
        }
    }

    pub(crate) const fn supports_role(self, role: ArtifactRole) -> bool {
        match self {
            Self::Assembly | Self::BackendIr => matches!(role, ArtifactRole::Inspection),
            Self::BackendBitcode | Self::RelocatableObject => {
                matches!(role, ArtifactRole::Inspection | ArtifactRole::LinkInput)
            }
            Self::ExecutableModule
            | Self::PackageInterface
            | Self::Executable
            | Self::StaticLibrary
            | Self::SharedLibrary => matches!(role, ArtifactRole::Product),
            Self::DebugCompanion
            | Self::PackageImplementation
            | Self::DependencyMetadata
            | Self::LinkedCompanion => {
                matches!(role, ArtifactRole::Companion)
            }
        }
    }

    pub(crate) fn accepts_linked_kind(self, linked: LinkedArtifactKind) -> bool {
        match self {
            Self::Executable => linked == LinkedArtifactKind::Executable,
            Self::StaticLibrary => linked == LinkedArtifactKind::StaticLibrary,
            Self::SharedLibrary => linked == LinkedArtifactKind::SharedLibrary,
            Self::LinkedCompanion => matches!(
                linked,
                LinkedArtifactKind::ImportLibrary
                    | LinkedArtifactKind::DebugCompanion
                    | LinkedArtifactKind::PlatformCompanion
            ),
            Self::Assembly
            | Self::BackendIr
            | Self::BackendBitcode
            | Self::RelocatableObject
            | Self::ExecutableModule
            | Self::DebugCompanion
            | Self::PackageInterface
            | Self::PackageImplementation
            | Self::DependencyMetadata => false,
        }
    }
}

impl From<BackendArtifactKind> for ArtifactKind {
    fn from(kind: BackendArtifactKind) -> Self {
        match kind {
            BackendArtifactKind::RelocatableObject => Self::RelocatableObject,
            BackendArtifactKind::Assembly => Self::Assembly,
            BackendArtifactKind::BackendIr => Self::BackendIr,
            BackendArtifactKind::BackendBitcode => Self::BackendBitcode,
            BackendArtifactKind::ExecutableModule => Self::ExecutableModule,
            BackendArtifactKind::DebugCompanion => Self::DebugCompanion,
        }
    }
}

impl From<TargetOutputKind> for ArtifactKind {
    fn from(kind: TargetOutputKind) -> Self {
        match kind {
            TargetOutputKind::Assembly => Self::Assembly,
            TargetOutputKind::BackendIr => Self::BackendIr,
            TargetOutputKind::BackendBitcode => Self::BackendBitcode,
            TargetOutputKind::RelocatableObject => Self::RelocatableObject,
            TargetOutputKind::ExecutableModule => Self::ExecutableModule,
            TargetOutputKind::DebugCompanion => Self::DebugCompanion,
            TargetOutputKind::PackageInterface => Self::PackageInterface,
            TargetOutputKind::PackageImplementation => Self::PackageImplementation,
            TargetOutputKind::DependencyMetadata => Self::DependencyMetadata,
            TargetOutputKind::Executable => Self::Executable,
            TargetOutputKind::StaticLibrary => Self::StaticLibrary,
            TargetOutputKind::SharedLibrary => Self::SharedLibrary,
            TargetOutputKind::LinkedCompanion => Self::LinkedCompanion,
        }
    }
}

/// Whether one planned artifact is mandatory for complete product emission.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ArtifactRequirement {
    /// Product emission cannot complete without this artifact.
    Required,
    /// Product emission may complete when this artifact is unavailable.
    Optional,
}

impl ArtifactRequirement {
    pub(crate) const fn linked(self) -> LinkedArtifactRequirement {
        match self {
            Self::Required => LinkedArtifactRequirement::Required,
            Self::Optional => LinkedArtifactRequirement::Optional,
        }
    }
}

/// Role one artifact has in the emission lifecycle.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ArtifactRole {
    /// Final externally visible compiler product.
    Product,
    /// Externally requested inspection output.
    Inspection,
    /// Private input staged for the native linker.
    LinkInput,
    /// Externally visible companion to another product artifact.
    Companion,
}

/// Typed owner of one planned artifact contribution.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ArtifactProducer {
    /// Exact backend contribution and backend implementation expected by the plan.
    Backend {
        /// Logical codegen contribution identity.
        artifact: BackendArtifactId,
        /// Backend implementation and toolchain identity.
        backend: BackendIdentity,
    },
    /// Completed package-interface record.
    PackageInterface,
    /// Package implementation artifact.
    PackageImplementation,
    /// Compiler-owned dependency metadata record.
    DependencyMetadata(DependencyMetadataProducerId),
    /// Native linker output.
    Linker(LinkerProducerId),
}

impl ArtifactProducer {
    /// Returns the exact backend contribution identity when code generation owns the bytes.
    pub const fn backend_artifact(&self) -> Option<&BackendArtifactId> {
        match self {
            Self::Backend { artifact, .. } => Some(artifact),
            Self::PackageInterface
            | Self::PackageImplementation
            | Self::DependencyMetadata(_)
            | Self::Linker(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_codegen::BackendArtifactKind;
    use bray_target::TargetOutputKind;

    use super::ArtifactKind;

    #[test]
    fn backend_artifact_kinds_have_lossless_emitter_projections() {
        let backend_kinds = [
            BackendArtifactKind::RelocatableObject,
            BackendArtifactKind::Assembly,
            BackendArtifactKind::BackendIr,
            BackendArtifactKind::BackendBitcode,
            BackendArtifactKind::ExecutableModule,
            BackendArtifactKind::DebugCompanion,
        ];

        for backend_kind in backend_kinds {
            assert_eq!(
                ArtifactKind::from(backend_kind).backend_kind(),
                Some(backend_kind)
            );
        }
    }

    #[test]
    fn target_output_kinds_have_lossless_emitter_projections() {
        let output_kinds = [
            TargetOutputKind::Assembly,
            TargetOutputKind::BackendIr,
            TargetOutputKind::BackendBitcode,
            TargetOutputKind::RelocatableObject,
            TargetOutputKind::ExecutableModule,
            TargetOutputKind::DebugCompanion,
            TargetOutputKind::PackageInterface,
            TargetOutputKind::PackageImplementation,
            TargetOutputKind::DependencyMetadata,
            TargetOutputKind::Executable,
            TargetOutputKind::StaticLibrary,
            TargetOutputKind::SharedLibrary,
            TargetOutputKind::LinkedCompanion,
        ];

        for output_kind in output_kinds {
            let artifact = ArtifactKind::from(output_kind);

            assert_eq!(artifact.target_output_kind(), output_kind);
        }
    }
}
