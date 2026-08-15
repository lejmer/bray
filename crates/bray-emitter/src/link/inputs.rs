use std::sync::Arc;

use bray_linker::{BinarySymbolName, LinkInputSpec, LinkPolicy, LinkSearchPath, LinkTarget};
use bray_runtime_interface::RuntimeArtifactSelection;

/// Already resolved product, target, and host inputs needed for native link planning.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProductLinkInputs {
    pub(super) target: LinkTarget,
    pub(super) policy: LinkPolicy,
    pub(super) startup_inputs: Arc<[LinkInputSpec]>,
    pub(super) native_inputs: Arc<[LinkInputSpec]>,
    pub(super) termination_inputs: Arc<[LinkInputSpec]>,
    pub(super) runtime: Option<RuntimeArtifactSelection>,
    pub(super) entry_point: Option<BinarySymbolName>,
    pub(super) exported_symbols: Arc<[BinarySymbolName]>,
    pub(super) retained_symbols: Arc<[BinarySymbolName]>,
    pub(super) search_paths: Arc<[LinkSearchPath]>,
}

impl ProductLinkInputs {
    /// Creates product link inputs from the selected target and platform policy.
    pub fn new(target: LinkTarget, policy: LinkPolicy) -> Self {
        Self {
            target,
            policy,
            startup_inputs: Arc::from([]),
            native_inputs: Arc::from([]),
            termination_inputs: Arc::from([]),
            runtime: None,
            entry_point: None,
            exported_symbols: Arc::from([]),
            retained_symbols: Arc::from([]),
            search_paths: Arc::from([]),
        }
    }

    /// Supplies target startup inputs in driver-visible order.
    pub fn with_startup_inputs(mut self, inputs: impl IntoIterator<Item = LinkInputSpec>) -> Self {
        self.startup_inputs = inputs.into_iter().collect::<Vec<_>>().into();

        self
    }

    /// Supplies native archives, libraries, and frameworks in resolved order.
    pub fn with_native_inputs(mut self, inputs: impl IntoIterator<Item = LinkInputSpec>) -> Self {
        self.native_inputs = inputs.into_iter().collect::<Vec<_>>().into();

        self
    }

    /// Supplies target termination inputs in driver-visible order.
    pub fn with_termination_inputs(
        mut self,
        inputs: impl IntoIterator<Item = LinkInputSpec>,
    ) -> Self {
        self.termination_inputs = inputs.into_iter().collect::<Vec<_>>().into();

        self
    }

    /// Supplies the exact selected runtime archive and compatibility contract.
    pub fn with_runtime(mut self, runtime: RuntimeArtifactSelection) -> Self {
        self.runtime = Some(runtime);

        self
    }

    /// Supplies the selected native entry point for a shared library.
    pub fn with_entry_point(mut self, entry_point: BinarySymbolName) -> Self {
        self.entry_point = Some(entry_point);

        self
    }

    /// Supplies exported binary symbols.
    pub fn with_exported_symbols(
        mut self,
        symbols: impl IntoIterator<Item = BinarySymbolName>,
    ) -> Self {
        self.exported_symbols = symbols.into_iter().collect::<Vec<_>>().into();

        self
    }

    /// Supplies binary definitions that dead stripping must retain.
    pub fn with_retained_symbols(
        mut self,
        symbols: impl IntoIterator<Item = BinarySymbolName>,
    ) -> Self {
        self.retained_symbols = symbols.into_iter().collect::<Vec<_>>().into();

        self
    }

    /// Supplies library and framework search paths in driver-visible order.
    pub fn with_search_paths(
        mut self,
        search_paths: impl IntoIterator<Item = LinkSearchPath>,
    ) -> Self {
        self.search_paths = search_paths.into_iter().collect::<Vec<_>>().into();

        self
    }

    /// Appends library and framework search paths after the selected product paths.
    pub fn with_additional_search_paths(
        mut self,
        search_paths: impl IntoIterator<Item = LinkSearchPath>,
    ) -> Self {
        let mut combined = self.search_paths.to_vec();

        combined.extend(search_paths);
        self.search_paths = combined.into();

        self
    }
}
