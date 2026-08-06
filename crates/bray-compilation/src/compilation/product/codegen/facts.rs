use std::sync::Arc;

use bray_codegen::{CodegenMappings, CodegenOptions, CodegenTarget, CodegenUnit};
use bray_emitter::{EmissionBackend, ProductLinkFacts};
use bray_runtime_interface::ExecutableHostContract;

/// Compilation-owned native product facts consumed by emission.
#[derive(Clone, Debug)]
pub struct NativeProductFacts {
    pub(super) backend: EmissionBackend,
    pub(super) target: CodegenTarget,
    pub(super) options: CodegenOptions,
    pub(super) host: Option<ExecutableHostContract>,
    pub(super) test_catalog: Option<bray_test_protocol::TestCatalog>,
    pub(super) link: Option<ProductLinkFacts>,
    pub(super) units: Arc<[CodegenUnit]>,
    pub(super) mappings: Arc<[CodegenMappings]>,
}

impl NativeProductFacts {
    /// Returns the selected backend and demanded unit keys.
    pub const fn backend(&self) -> &EmissionBackend {
        &self.backend
    }

    /// Returns the selected code generation target.
    pub const fn target(&self) -> &CodegenTarget {
        &self.target
    }

    /// Returns the selected code generation options.
    pub const fn options(&self) -> &CodegenOptions {
        &self.options
    }

    /// Returns the executable host when the product has a process root.
    pub const fn executable_host(&self) -> Option<&ExecutableHostContract> {
        self.host.as_ref()
    }

    /// Returns the immutable test catalog published with a native test host.
    pub const fn test_catalog(&self) -> Option<&bray_test_protocol::TestCatalog> {
        self.test_catalog.as_ref()
    }

    /// Returns native link facts when link planning was requested.
    pub const fn link(&self) -> Option<&ProductLinkFacts> {
        self.link.as_ref()
    }

    pub(in crate::compilation) fn units(&self) -> &[CodegenUnit] {
        &self.units
    }

    pub(in crate::compilation) fn mappings(&self) -> &[CodegenMappings] {
        &self.mappings
    }
}
