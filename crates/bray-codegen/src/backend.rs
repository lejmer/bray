use std::sync::Arc;

use bray_base::NonEmptySharedStr;

use crate::{BackendCapabilities, CodegenFailure, CodegenOutcome, CodegenRequest, CodegenTarget};

/// Stable compiler-facing identity of one backend implementation and compatible toolchain.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendIdentity {
    name: NonEmptySharedStr,
    revision: NonEmptySharedStr,
    toolchain_revision: NonEmptySharedStr,
}

impl BackendIdentity {
    /// Creates an identity when every canonical component is non-empty.
    pub fn try_new(
        name: impl Into<Arc<str>>,
        revision: impl Into<Arc<str>>,
        toolchain_revision: impl Into<Arc<str>>,
    ) -> Option<Self> {
        Some(Self {
            name: NonEmptySharedStr::try_new(name)?,
            revision: NonEmptySharedStr::try_new(revision)?,
            toolchain_revision: NonEmptySharedStr::try_new(toolchain_revision)?,
        })
    }

    /// Returns the stable backend name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the Bray backend implementation revision.
    pub fn revision(&self) -> &str {
        self.revision.as_str()
    }

    /// Returns the compatible backend-library or toolchain revision.
    pub fn toolchain_revision(&self) -> &str {
        self.toolchain_revision.as_str()
    }
}

/// Coarse backend service boundary for one complete codegen-unit operation.
///
/// Generated contributions crossing this boundary must be immutable.
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
    use super::{BackendIdentity, CodeGenerator};
    use crate::{
        ArtifactSpool, BackendArtifactContribution, BackendArtifactRequest, CodegenOutcome,
        CodegenRequest, CodegenTarget, CodegenUnit,
    };

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
