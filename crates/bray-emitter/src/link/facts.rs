use std::sync::Arc;

use bray_linker::{
    BinarySymbolName, LinkInputSpec, LinkPolicy, LinkSearchPath, LinkStartupMode, LinkTarget,
    LinkerDriverIdentity,
};
use bray_runtime_interface::RuntimeArtifact;

/// Already resolved product, target, and host facts needed for native link planning.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ProductLinkFacts {
    pub(super) target: LinkTarget,
    pub(super) driver: LinkerDriverIdentity,
    pub(super) startup_mode: LinkStartupMode,
    pub(super) policy: LinkPolicy,
    pub(super) startup_inputs: Arc<[LinkInputSpec]>,
    pub(super) native_inputs: Arc<[LinkInputSpec]>,
    pub(super) termination_inputs: Arc<[LinkInputSpec]>,
    pub(super) runtime: Option<RuntimeArtifact>,
    pub(super) entry_point: Option<BinarySymbolName>,
    pub(super) exported_symbols: Arc<[BinarySymbolName]>,
    pub(super) retained_symbols: Arc<[BinarySymbolName]>,
    pub(super) search_paths: Arc<[LinkSearchPath]>,
}

impl ProductLinkFacts {
    /// Creates product link facts from the selected target, driver, and platform policy.
    pub fn new(
        target: LinkTarget,
        driver: LinkerDriverIdentity,
        startup_mode: LinkStartupMode,
        policy: LinkPolicy,
    ) -> Self {
        Self {
            target,
            driver,
            startup_mode,
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
    pub fn with_runtime(mut self, runtime: RuntimeArtifact) -> Self {
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
