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

    /// Returns public definitions and lifecycle records preserved during product optimization.
    pub fn preservation_roots(
        &self,
    ) -> impl Iterator<Item = &bray_runtime_interface::BinarySymbolName> {
        product_preservation_roots(&self.mappings, self.product_host.as_ref())
    }

    /// Returns the compiler-generated loaded-product host contract.
    pub const fn product_host(&self) -> Option<&bray_codegen::CodegenProductHostMapping> {
        self.product_host.as_ref()
    }
}

pub(super) fn product_preservation_roots<'plan>(
    mappings: &'plan [CodegenMappings],
    product_host: Option<&'plan bray_codegen::CodegenProductHostMapping>,
) -> impl Iterator<Item = &'plan bray_runtime_interface::BinarySymbolName> {
    let definitions = mappings
        .iter()
        .flat_map(bray_codegen::CodegenMappings::symbols)
        .filter(|symbol| is_preservation_root(symbol.linkage()))
        .map(bray_codegen::CodegenSymbolMapping::name);

    let native_data = mappings
        .iter()
        .flat_map(bray_codegen::CodegenMappings::native_storages)
        .filter(|mapping| {
            mapping.direction() == bray_symbols::ForeignCallableDirection::Export
        })
        .map(bray_codegen::CodegenNativeStaticMapping::symbol);

    let lifecycle = product_host.into_iter().flat_map(|host| {
        [host.descriptor_symbol(), host.control_symbol()]
            .into_iter()
            .chain(
                host.statics()
                    .iter()
                    .map(bray_codegen::CodegenProductHostStatic::host_symbol),
            )
    });

    definitions.chain(native_data).chain(lifecycle)
}

const fn is_preservation_root(linkage: bray_codegen::CodegenLinkage) -> bool {
    matches!(
        linkage,
        bray_codegen::CodegenLinkage::External
            | bray_codegen::CodegenLinkage::Weak
            | bray_codegen::CodegenLinkage::Export
    )
}

#[cfg(test)]
mod tests {
    use bray_codegen::CodegenLinkage;

    use super::is_preservation_root;

    #[test]
    fn preservation_roots_include_only_public_definitions() {
        assert!(is_preservation_root(CodegenLinkage::External));
        assert!(is_preservation_root(CodegenLinkage::Weak));
        assert!(is_preservation_root(CodegenLinkage::Export));

        assert!(!is_preservation_root(CodegenLinkage::Private));
        assert!(!is_preservation_root(CodegenLinkage::Internal));
        assert!(!is_preservation_root(CodegenLinkage::LinkOnce));
        assert!(!is_preservation_root(CodegenLinkage::Common));
        assert!(!is_preservation_root(CodegenLinkage::Import));
    }
}
