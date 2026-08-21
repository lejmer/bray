use std::num::NonZeroU32;
use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;
use bray_runtime_interface::RuntimeAbiVersion;
use bray_symbols::ProductKind;
use bray_target::{CodeModel, RelocationModel, TargetMachineProperties};

use crate::{
    AssemblySyntaxKind, BackendArtifactKind, BackendBitcodeSemantics, CodegenTarget,
    DebugInformationMode, DebugInformationOutputMode, OptimizationLevel, SizePreference,
};

/// Stable revision of one backend's complete capability declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendCapabilityRevision(NonZeroU32);

impl BackendCapabilityRevision {
    /// Creates a nonzero capability revision.
    pub const fn try_new(revision: u32) -> Option<Self> {
        match NonZeroU32::new(revision) {
            Some(revision) => Some(Self(revision)),
            None => None,
        }
    }

    /// Returns the stable numeric revision.
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// Reproducibility strength requested from or guaranteed by a backend.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReproducibilityLevel {
    /// Equivalent inputs produce semantically equivalent outputs.
    Semantic,
    /// Equivalent inputs produce byte-for-byte identical required artifacts.
    #[default]
    ByteForByte,
}

/// One exact target-machine configuration supported by a backend.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendTargetConfiguration {
    machine: TargetMachineProperties,
    relocation_model: RelocationModel,
    code_model: CodeModel,
}

impl BackendTargetConfiguration {
    /// Creates one indivisible supported target configuration.
    pub const fn new(
        machine: TargetMachineProperties,
        relocation_model: RelocationModel,
        code_model: CodeModel,
    ) -> Self {
        Self {
            machine,
            relocation_model,
            code_model,
        }
    }

    /// Returns the exact supported machine properties.
    pub const fn machine(&self) -> &TargetMachineProperties {
        &self.machine
    }

    /// Returns the relocation policy supported with this machine and code model.
    pub const fn relocation_model(&self) -> RelocationModel {
        self.relocation_model
    }

    /// Returns the code model supported with this machine and relocation policy.
    pub const fn code_model(&self) -> CodeModel {
        self.code_model
    }
}

/// Exact target-machine configurations supported by one backend.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct BackendTargetCapabilities {
    configurations: Arc<[BackendTargetConfiguration]>,
}

impl BackendTargetCapabilities {
    /// Creates canonical indivisible target-machine capabilities.
    pub fn new(configurations: impl IntoIterator<Item = BackendTargetConfiguration>) -> Self {
        Self {
            configurations: sorted_unique_shared_slice(configurations),
        }
    }

    /// Returns supported target configurations in canonical order.
    pub fn configurations(&self) -> &[BackendTargetConfiguration] {
        &self.configurations
    }

    /// Returns whether the exact target-machine configuration is declared.
    pub fn supports(&self, target: &CodegenTarget) -> bool {
        self.configurations
            .binary_search_by(|configuration| {
                configuration
                    .machine()
                    .cmp(target.machine())
                    .then_with(|| {
                        configuration
                            .relocation_model()
                            .cmp(&target.relocation_model())
                    })
                    .then_with(|| configuration.code_model().cmp(&target.code_model()))
            })
            .is_ok()
    }

    fn supports_machine(&self, machine: &TargetMachineProperties) -> bool {
        self.configurations
            .iter()
            .any(|configuration| configuration.machine() == machine)
    }
}

/// Runtime shapes and private ABI versions supported by one backend.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct BackendRuntimeCapabilities {
    abi_versions: Arc<[RuntimeAbiVersion]>,
    protected_frames: bool,
    executable_hosts: bool,
}

impl BackendRuntimeCapabilities {
    /// Creates canonical runtime capabilities.
    pub fn new(
        abi_versions: impl IntoIterator<Item = RuntimeAbiVersion>,
        protected_frames: bool,
        executable_hosts: bool,
    ) -> Self {
        Self {
            abi_versions: sorted_unique_shared_slice(abi_versions),
            protected_frames,
            executable_hosts,
        }
    }

    /// Returns provided private runtime ABI versions.
    pub fn abi_versions(&self) -> &[RuntimeAbiVersion] {
        &self.abi_versions
    }

    /// Returns whether protected async frames are supported.
    pub const fn supports_protected_frames(&self) -> bool {
        self.protected_frames
    }

    /// Returns whether executable-host units are supported.
    pub const fn supports_executable_hosts(&self) -> bool {
        self.executable_hosts
    }

    pub(crate) fn supports(
        &self,
        abi: RuntimeAbiVersion,
        protected_frames: bool,
        executable_hosts: bool,
    ) -> bool {
        self.abi_versions
            .iter()
            .any(|provided| provided.supports(abi))
            && (!protected_frames || self.protected_frames)
            && (!executable_hosts || self.executable_hosts)
    }
}

/// Optimization policies supported by one backend.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct BackendOptimizationCapabilities {
    levels: Arc<[OptimizationLevel]>,
    size_preferences: Arc<[SizePreference]>,
}

impl BackendOptimizationCapabilities {
    /// Creates canonical optimization capabilities.
    pub fn new(
        levels: impl IntoIterator<Item = OptimizationLevel>,
        size_preferences: impl IntoIterator<Item = SizePreference>,
    ) -> Self {
        Self {
            levels: sorted_unique_shared_slice(levels),
            size_preferences: sorted_unique_shared_slice(size_preferences),
        }
    }

    /// Returns supported optimization levels.
    pub fn levels(&self) -> &[OptimizationLevel] {
        &self.levels
    }

    /// Returns supported size preferences.
    pub fn size_preferences(&self) -> &[SizePreference] {
        &self.size_preferences
    }

    pub(crate) fn supports(&self, level: OptimizationLevel, size: SizePreference) -> bool {
        self.levels.binary_search(&level).is_ok()
            && self.size_preferences.binary_search(&size).is_ok()
    }
}

/// Emission and debug policies supported by one backend.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub struct BackendOutputCapabilities {
    artifacts: Arc<[BackendArtifactKind]>,
    debug_information: Arc<[DebugInformationMode]>,
    debug_output: Arc<[DebugInformationOutputMode]>,
    assembly_syntax: Arc<[AssemblySyntaxKind]>,
    bitcode_semantics: Arc<[BackendBitcodeSemantics]>,
}

impl BackendOutputCapabilities {
    /// Creates canonical output capabilities.
    pub fn new(
        artifacts: impl IntoIterator<Item = BackendArtifactKind>,
        debug_information: impl IntoIterator<Item = DebugInformationMode>,
        debug_output: impl IntoIterator<Item = DebugInformationOutputMode>,
        assembly_syntax: impl IntoIterator<Item = AssemblySyntaxKind>,
        bitcode_semantics: impl IntoIterator<Item = BackendBitcodeSemantics>,
    ) -> Self {
        Self {
            artifacts: sorted_unique_shared_slice(artifacts),
            debug_information: sorted_unique_shared_slice(debug_information),
            debug_output: sorted_unique_shared_slice(debug_output),
            assembly_syntax: sorted_unique_shared_slice(assembly_syntax),
            bitcode_semantics: sorted_unique_shared_slice(bitcode_semantics),
        }
    }

    /// Returns supported artifact kinds.
    pub fn artifacts(&self) -> &[BackendArtifactKind] {
        &self.artifacts
    }

    /// Returns supported debug-information modes.
    pub fn debug_information(&self) -> &[DebugInformationMode] {
        &self.debug_information
    }

    /// Returns supported debug-information output placements.
    pub fn debug_output(&self) -> &[DebugInformationOutputMode] {
        &self.debug_output
    }

    /// Returns supported assembly syntax kinds.
    pub fn assembly_syntax(&self) -> &[AssemblySyntaxKind] {
        &self.assembly_syntax
    }

    /// Returns supported serialized backend-bitcode contracts.
    pub fn bitcode_semantics(&self) -> &[BackendBitcodeSemantics] {
        &self.bitcode_semantics
    }
}

/// Immutable complete capability declaration of one backend implementation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BackendCapabilities {
    revision: BackendCapabilityRevision,
    product_kinds: Arc<[ProductKind]>,
    targets: BackendTargetCapabilities,
    runtime: BackendRuntimeCapabilities,
    optimization: BackendOptimizationCapabilities,
    outputs: BackendOutputCapabilities,
    reproducibility: ReproducibilityLevel,
}

impl BackendCapabilities {
    /// Creates a complete capability declaration in canonical deterministic order.
    pub fn new(
        revision: BackendCapabilityRevision,
        product_kinds: impl IntoIterator<Item = ProductKind>,
        targets: BackendTargetCapabilities,
        runtime: BackendRuntimeCapabilities,
        optimization: BackendOptimizationCapabilities,
        outputs: BackendOutputCapabilities,
        reproducibility: ReproducibilityLevel,
    ) -> Self {
        Self {
            revision,
            product_kinds: sorted_unique_shared_slice(product_kinds),
            targets,
            runtime,
            optimization,
            outputs,
            reproducibility,
        }
    }

    /// Returns the stable capability declaration revision.
    pub const fn revision(&self) -> BackendCapabilityRevision {
        self.revision
    }

    /// Returns supported product kinds.
    pub fn product_kinds(&self) -> &[ProductKind] {
        &self.product_kinds
    }

    /// Returns supported target-machine policies.
    pub const fn targets(&self) -> &BackendTargetCapabilities {
        &self.targets
    }

    /// Returns supported runtime contracts.
    pub const fn runtime(&self) -> &BackendRuntimeCapabilities {
        &self.runtime
    }

    /// Returns supported optimization policies.
    pub const fn optimization(&self) -> &BackendOptimizationCapabilities {
        &self.optimization
    }

    /// Returns supported output policies.
    pub const fn outputs(&self) -> &BackendOutputCapabilities {
        &self.outputs
    }

    /// Returns the strongest reproducibility guarantee provided by the backend.
    pub const fn reproducibility(&self) -> ReproducibilityLevel {
        self.reproducibility
    }

    /// Returns whether the complete selected target policy is supported.
    pub fn supports_target(&self, target: &CodegenTarget) -> bool {
        self.targets.supports(target)
    }

    /// Returns whether the target machine properties are declared.
    pub fn supports_target_machine(&self, machine: &TargetMachineProperties) -> bool {
        self.targets.supports_machine(machine)
    }

    /// Returns whether an artifact kind is supported.
    pub fn supports_artifact(&self, kind: BackendArtifactKind) -> bool {
        self.outputs.artifacts.binary_search(&kind).is_ok()
    }

    /// Returns whether a debug-information mode is supported.
    pub fn supports_debug_information(&self, mode: DebugInformationMode) -> bool {
        self.outputs.debug_information.binary_search(&mode).is_ok()
    }

    /// Returns whether a debug-information output placement is supported.
    pub fn supports_debug_output(&self, output: DebugInformationOutputMode) -> bool {
        self.outputs.debug_output.binary_search(&output).is_ok()
    }

    /// Returns whether an assembly syntax kind is supported.
    pub fn supports_assembly_syntax_kind(&self, syntax: AssemblySyntaxKind) -> bool {
        self.outputs.assembly_syntax.binary_search(&syntax).is_ok()
    }

    /// Returns whether the selected serialized backend-bitcode contract is supported.
    pub fn supports_bitcode_semantics(&self, semantics: BackendBitcodeSemantics) -> bool {
        self.outputs
            .bitcode_semantics
            .binary_search(&semantics)
            .is_ok()
    }

    pub(crate) fn supports_product(&self, product: ProductKind) -> bool {
        self.product_kinds.binary_search(&product).is_ok()
    }
}

impl Default for BackendCapabilities {
    fn default() -> Self {
        Self::new(
            BackendCapabilityRevision(NonZeroU32::MIN),
            [],
            BackendTargetCapabilities::default(),
            BackendRuntimeCapabilities::default(),
            BackendOptimizationCapabilities::default(),
            BackendOutputCapabilities::default(),
            ReproducibilityLevel::Semantic,
        )
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_interface::RuntimeAbiVersion;
    use bray_symbols::ProductKind;
    use bray_target::{CodeModel, NativeTarget, RelocationModel};

    use super::{
        BackendCapabilities, BackendCapabilityRevision, BackendOptimizationCapabilities,
        BackendOutputCapabilities, BackendRuntimeCapabilities, BackendTargetCapabilities,
        BackendTargetConfiguration, ReproducibilityLevel,
    };
    use crate::{
        AssemblySyntaxKind, BackendArtifactKind, BackendBitcodeSemantics, DebugInformationMode,
        DebugInformationOutputMode, OptimizationLevel, SizePreference,
    };

    #[test]
    fn capabilities_are_complete_canonical_records() {
        let machine = NativeTarget::X86_64LinuxGnu.profile().machine().clone();

        let revision = BackendCapabilityRevision::try_new(2)
            .unwrap_or_else(|| panic!("test revision must be valid"));

        let capabilities = BackendCapabilities::new(
            revision,
            [ProductKind::Library, ProductKind::Library],
            BackendTargetCapabilities::new([
                BackendTargetConfiguration::new(
                    machine.clone(),
                    RelocationModel::Static,
                    CodeModel::Small,
                ),
                BackendTargetConfiguration::new(machine, RelocationModel::Static, CodeModel::Small),
            ]),
            BackendRuntimeCapabilities::new(
                [RuntimeAbiVersion::new(1, 0), RuntimeAbiVersion::new(1, 0)],
                true,
                false,
            ),
            BackendOptimizationCapabilities::new(
                [OptimizationLevel::Full, OptimizationLevel::None],
                [SizePreference::None],
            ),
            BackendOutputCapabilities::new(
                [BackendArtifactKind::Assembly, BackendArtifactKind::Assembly],
                [DebugInformationMode::None],
                [DebugInformationOutputMode::Omit],
                [AssemblySyntaxKind::TargetDefault],
                [BackendBitcodeSemantics::Plain],
            ),
            ReproducibilityLevel::ByteForByte,
        );

        assert_eq!(capabilities.revision(), revision);
        assert_eq!(capabilities.product_kinds(), &[ProductKind::Library]);
        assert_eq!(capabilities.targets().configurations().len(), 1);
        assert_eq!(capabilities.runtime().abi_versions().len(), 1);

        assert_eq!(
            capabilities.optimization().levels(),
            &[OptimizationLevel::None, OptimizationLevel::Full]
        );

        assert_eq!(
            capabilities.outputs().artifacts(),
            &[BackendArtifactKind::Assembly]
        );
    }
}
