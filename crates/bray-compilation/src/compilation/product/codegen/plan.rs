use std::sync::Arc;

use bray_codegen::{CodegenMappings, CodegenOptions, CodegenTarget, CodegenUnit};
use bray_emitter::{EmissionBackend, ProductLinkInputs};
use bray_linker::LinkSearchPath;
use bray_runtime_interface::ExecutableHostContract;

/// Compilation-owned native product inputs consumed by emission.
#[derive(Clone, Debug, Hash)]
pub struct NativeProductPlan {
    pub(super) backend: EmissionBackend,
    pub(super) target: CodegenTarget,
    pub(super) options: CodegenOptions,
    pub(super) host: Option<ExecutableHostContract>,
    pub(super) test_catalog: Option<bray_test_protocol::TestCatalog>,
    pub(super) link: Option<ProductLinkInputs>,
    pub(super) units: Arc<[CodegenUnit]>,
    pub(super) mappings: Arc<[CodegenMappings]>,
    pub(super) static_instances: Arc<[bray_codegen::CodegenStaticInstanceKey]>,
    pub(super) product_host: Option<bray_codegen::CodegenProductHostMapping>,
}

impl NativeProductPlan {
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

    /// Returns native link inputs when link planning was requested.
    pub const fn link(&self) -> Option<&ProductLinkInputs> {
        self.link.as_ref()
    }

    /// Appends host-selected search paths to this product's native link inputs.
    pub fn with_additional_link_search_paths(
        mut self,
        search_paths: impl IntoIterator<Item = LinkSearchPath>,
    ) -> Self {
        self.link = self
            .link
            .map(|link| link.with_additional_search_paths(search_paths));

        self
    }

    pub(in crate::compilation) fn units(&self) -> &[CodegenUnit] {
        &self.units
    }

    pub(in crate::compilation) fn mappings(&self) -> &[CodegenMappings] {
        &self.mappings
    }

    /// Returns the deterministic static-instance table contributed by this product.
    pub fn static_instances(&self) -> &[bray_codegen::CodegenStaticInstanceKey] {
        &self.static_instances
    }

    /// Returns the compiler-generated loaded-product host contract.
    pub const fn product_host(&self) -> Option<&bray_codegen::CodegenProductHostMapping> {
        self.product_host.as_ref()
    }
}
