use std::sync::Arc;

use bray_base::{shared_str, sorted_unique_shared_slice};

use crate::{
    AssemblySyntaxKind, BackendArtifactKind, CodegenFailure, CodegenOutcome, CodegenRequest,
    CodegenTarget, DebugInformationMode, ObjectFormat, TargetArchitecture,
};

/// Stable compiler-facing identity of one backend implementation and compatible toolchain.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendIdentity {
    name: Arc<str>,
    revision: Arc<str>,
    toolchain_revision: Arc<str>,
}

impl BackendIdentity {
    /// Creates an identity when every canonical component is non-empty.
    pub fn try_new(
        name: impl Into<Arc<str>>,
        revision: impl Into<Arc<str>>,
        toolchain_revision: impl Into<Arc<str>>,
    ) -> Option<Self> {
        let name = shared_str(name);
        let revision = shared_str(revision);
        let toolchain_revision = shared_str(toolchain_revision);

        if name.is_empty() || revision.is_empty() || toolchain_revision.is_empty() {
            return None;
        }

        Some(Self {
            name,
            revision,
            toolchain_revision,
        })
    }

    /// Returns the stable backend name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the Bray backend implementation revision.
    pub fn revision(&self) -> &str {
        &self.revision
    }

    /// Returns the compatible backend-library or toolchain revision.
    pub fn toolchain_revision(&self) -> &str {
        &self.toolchain_revision
    }
}

/// One architecture and object-format combination supported by a backend.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendTargetPlatform {
    architecture: TargetArchitecture,
    object_format: ObjectFormat,
}

impl BackendTargetPlatform {
    /// Creates a supported target platform declaration.
    pub const fn new(architecture: TargetArchitecture, object_format: ObjectFormat) -> Self {
        Self {
            architecture,
            object_format,
        }
    }

    /// Returns the supported processor architecture.
    pub const fn architecture(&self) -> &TargetArchitecture {
        &self.architecture
    }

    /// Returns the supported object format.
    pub const fn object_format(&self) -> &ObjectFormat {
        &self.object_format
    }
}

/// Immutable capabilities declared by one backend implementation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BackendCapabilities {
    target_platforms: Arc<[BackendTargetPlatform]>,
    artifact_kinds: Arc<[BackendArtifactKind]>,
    debug_information_modes: Arc<[DebugInformationMode]>,
    assembly_syntax_kinds: Arc<[AssemblySyntaxKind]>,
}

impl BackendCapabilities {
    /// Creates capabilities in canonical deterministic order.
    pub fn new(
        target_platforms: impl IntoIterator<Item = BackendTargetPlatform>,
        artifact_kinds: impl IntoIterator<Item = BackendArtifactKind>,
        debug_information_modes: impl IntoIterator<Item = DebugInformationMode>,
        assembly_syntax_kinds: impl IntoIterator<Item = AssemblySyntaxKind>,
    ) -> Self {
        Self {
            target_platforms: sorted_unique_shared_slice(target_platforms),
            artifact_kinds: sorted_unique_shared_slice(artifact_kinds),
            debug_information_modes: sorted_unique_shared_slice(debug_information_modes),
            assembly_syntax_kinds: sorted_unique_shared_slice(assembly_syntax_kinds),
        }
    }

    /// Returns supported architecture and object-format combinations.
    pub fn target_platforms(&self) -> &[BackendTargetPlatform] {
        &self.target_platforms
    }

    /// Returns supported artifact kinds in canonical order.
    pub fn artifact_kinds(&self) -> &[BackendArtifactKind] {
        &self.artifact_kinds
    }

    /// Returns supported debug-information modes in canonical order.
    pub fn debug_information_modes(&self) -> &[DebugInformationMode] {
        &self.debug_information_modes
    }

    /// Returns supported human-readable assembly syntax kinds in canonical order.
    pub fn assembly_syntax_kinds(&self) -> &[AssemblySyntaxKind] {
        &self.assembly_syntax_kinds
    }

    /// Returns whether the backend declares support for this architecture and object format.
    ///
    /// Complete target validation remains backend-specific through [`CodeGenerator::validate_target`].
    pub fn supports_platform(&self, target: &CodegenTarget) -> bool {
        let machine = target.machine();

        self.target_platforms.binary_search_by(|platform| {
            platform
                .architecture()
                .cmp(&machine.architecture())
                .then_with(|| platform.object_format().cmp(&machine.object_format()))
        }) == Ok(0)
    }

    /// Returns whether the backend declares support for this artifact kind.
    pub fn supports_artifact(&self, kind: BackendArtifactKind) -> bool {
        self.artifact_kinds.binary_search(&kind).is_ok()
    }

    /// Returns whether the backend supports the requested debug-information mode.
    pub fn supports_debug_information(&self, mode: DebugInformationMode) -> bool {
        self.debug_information_modes.binary_search(&mode).is_ok()
    }

    /// Returns whether the backend supports one assembly serialization syntax kind.
    pub fn supports_assembly_syntax_kind(&self, syntax_kind: AssemblySyntaxKind) -> bool {
        self.assembly_syntax_kinds
            .binary_search(&syntax_kind)
            .is_ok()
    }
}

/// Coarse backend service boundary for one complete codegen-unit operation.
///
/// Implementations own all mutable backend modules and low-level IR internally. Only immutable
/// contributions cross this boundary.
pub trait CodeGenerator: Send + Sync {
    /// Returns the stable backend and toolchain identity used by codegen fact keys.
    fn identity(&self) -> &BackendIdentity;

    /// Returns the backend's immutable declared capabilities.
    fn capabilities(&self) -> &BackendCapabilities;

    /// Validates the complete target contract against backend-specific machine support.
    fn validate_target(&self, target: &CodegenTarget) -> Result<(), CodegenFailure>;

    /// Generates every requested artifact for one validated codegen unit.
    fn generate(&self, request: CodegenRequest<'_>) -> CodegenOutcome;
}

#[cfg(test)]
mod tests {
    use super::{BackendCapabilities, BackendIdentity, BackendTargetPlatform, CodeGenerator};
    use crate::{
        ArtifactSpool, AssemblySyntaxKind, BackendArtifactContribution, BackendArtifactKind,
        BackendArtifactRequest, CodegenOutcome, CodegenRequest, CodegenTarget, CodegenUnit,
        DebugInformationMode, ObjectFormat, TargetArchitecture,
    };

    #[test]
    fn capabilities_are_canonical_sets() {
        let platform = BackendTargetPlatform::new(TargetArchitecture::X86_64, ObjectFormat::Elf);
        let capabilities = BackendCapabilities::new(
            [platform.clone(), platform],
            [
                BackendArtifactKind::Assembly,
                BackendArtifactKind::RelocatableObject,
                BackendArtifactKind::Assembly,
            ],
            [
                DebugInformationMode::Full,
                DebugInformationMode::None,
                DebugInformationMode::Full,
            ],
            [AssemblySyntaxKind::TargetDefault, AssemblySyntaxKind::Intel],
        );

        assert_eq!(capabilities.target_platforms().len(), 1);
        assert_eq!(
            capabilities.artifact_kinds(),
            &[
                BackendArtifactKind::RelocatableObject,
                BackendArtifactKind::Assembly,
            ]
        );
        assert_eq!(
            capabilities.debug_information_modes(),
            &[DebugInformationMode::None, DebugInformationMode::Full]
        );
        assert_eq!(
            capabilities.assembly_syntax_kinds(),
            &[AssemblySyntaxKind::TargetDefault, AssemblySyntaxKind::Intel]
        );
    }

    #[test]
    fn backend_contracts_are_send_and_sync() {
        fn assert_send_sync<T: ?Sized + Send + Sync>() {}

        assert_send_sync::<dyn CodeGenerator>();
        assert_send_sync::<CodegenRequest<'static>>();
        assert_send_sync::<CodegenUnit>();
        assert_send_sync::<CodegenTarget>();
        assert_send_sync::<BackendArtifactRequest>();
        assert_send_sync::<BackendArtifactContribution>();
        assert_send_sync::<ArtifactSpool>();
        assert_send_sync::<CodegenOutcome>();
    }

    #[test]
    fn backend_identities_require_every_cache_compatibility_component() {
        assert_eq!(BackendIdentity::try_new("", "bray-1", "llvm-22"), None);
        assert_eq!(BackendIdentity::try_new("llvm", "", "llvm-22"), None);
        assert_eq!(BackendIdentity::try_new("llvm", "bray-1", ""), None);
    }
}
