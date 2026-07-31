use std::sync::Arc;

use bray_base::{NonEmptySharedStr, sorted_unique_shared_slice};
use bray_runtime_interface::PanicAbiIdentity;
use bray_target::{
    CodeModel, RelocationModel, TargetIdentity, TargetMachineProperties, TargetProfile,
};

use super::{TargetAbi, TargetCompatibility, TargetDataLayout, TargetSymbolConvention};

/// Complete target semantics that no backend may rediscover or override.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetContract {
    data_layout: TargetDataLayout,
    abi: TargetAbi,
    panic_abi: PanicAbiIdentity,
    symbols: TargetSymbolConvention,
    compatibility: TargetCompatibility,
}

impl TargetContract {
    /// Composes independently validated layout, ABI, symbol, and compatibility facts.
    pub const fn new(
        data_layout: TargetDataLayout,
        abi: TargetAbi,
        panic_abi: PanicAbiIdentity,
        symbols: TargetSymbolConvention,
        compatibility: TargetCompatibility,
    ) -> Self {
        Self {
            data_layout,
            abi,
            panic_abi,
            symbols,
            compatibility,
        }
    }
}

/// Backend-private machine selection that does not change language-visible target semantics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetMachineSelection {
    relocation_model: RelocationModel,
    code_model: CodeModel,
    cpu: NonEmptySharedStr,
    features: Arc<[NonEmptySharedStr]>,
}

impl TargetMachineSelection {
    /// Creates a machine selection from non-empty canonical CPU and feature names.
    pub fn try_new<Features, Feature>(
        relocation_model: RelocationModel,
        code_model: CodeModel,
        cpu: impl Into<Arc<str>>,
        features: Features,
    ) -> Result<Self, CodegenTargetBuildError>
    where
        Features: IntoIterator<Item = Feature>,
        Feature: Into<Arc<str>>,
    {
        let Some(cpu) = NonEmptySharedStr::try_new(cpu) else {
            return Err(CodegenTargetBuildError::EmptyCpu);
        };

        let features: Option<Vec<_>> = features
            .into_iter()
            .map(NonEmptySharedStr::try_new)
            .collect();

        let Some(features) = features else {
            return Err(CodegenTargetBuildError::EmptyFeature);
        };

        Ok(Self {
            relocation_model,
            code_model,
            cpu,
            features: sorted_unique_shared_slice(features),
        })
    }
}

/// Validated backend-neutral target configuration for code generation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenTarget {
    profile: TargetProfile,
    triple: NonEmptySharedStr,
    contract: TargetContract,
    selection: TargetMachineSelection,
}

impl CodegenTarget {
    /// Creates a complete target from validated semantic and machine-selection components.
    pub fn try_new(
        profile: TargetProfile,
        triple: impl Into<Arc<str>>,
        contract: TargetContract,
        selection: TargetMachineSelection,
    ) -> Result<Self, CodegenTargetBuildError> {
        let Some(triple) = NonEmptySharedStr::try_new(triple) else {
            return Err(CodegenTargetBuildError::EmptyTriple);
        };

        Ok(Self {
            profile,
            triple,
            contract,
            selection,
        })
    }

    /// Returns the stable target identity used by codegen facts and artifacts.
    pub const fn identity(&self) -> &TargetIdentity {
        self.profile.identity()
    }

    /// Returns the language-level target profile used by code generation.
    pub const fn profile(&self) -> &TargetProfile {
        &self.profile
    }

    /// Returns the canonical target triple.
    pub fn triple(&self) -> &str {
        self.triple.as_str()
    }

    /// Returns validated pointer, alignment, platform, and byte-order facts.
    pub const fn machine(&self) -> &TargetMachineProperties {
        self.profile.machine()
    }

    /// Returns validated scalar, aggregate, and address-space layout facts.
    pub const fn data_layout(&self) -> &TargetDataLayout {
        &self.contract.data_layout
    }

    /// Returns exact callable ABI mappings.
    pub const fn abi(&self) -> &TargetAbi {
        &self.contract.abi
    }

    /// Returns the exact panic ABI selected for generated code.
    pub const fn panic_abi(&self) -> &PanicAbiIdentity {
        &self.contract.panic_abi
    }

    /// Returns target symbol spelling and linkage rules.
    pub const fn symbols(&self) -> &TargetSymbolConvention {
        &self.contract.symbols
    }

    /// Returns target-profile and backend-contract compatibility metadata.
    pub const fn compatibility(&self) -> &TargetCompatibility {
        &self.contract.compatibility
    }

    /// Returns the selected relocation policy.
    pub const fn relocation_model(&self) -> RelocationModel {
        self.selection.relocation_model
    }

    /// Returns the selected code model.
    pub const fn code_model(&self) -> CodeModel {
        self.selection.code_model
    }

    /// Returns the canonical target CPU name.
    pub fn cpu(&self) -> &str {
        self.selection.cpu.as_str()
    }

    /// Returns enabled target features in canonical deterministic order.
    pub fn features(&self) -> impl ExactSizeIterator<Item = &str> {
        self.selection
            .features
            .iter()
            .map(NonEmptySharedStr::as_str)
    }

    pub(crate) fn matches_mir_target(&self, target: &bray_ir::MirTargetFacts) -> bool {
        target.identity() == self.identity() && target.machine() == self.machine()
    }
}

/// A contract violation that prevents creation of a codegen target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CodegenTargetBuildError {
    /// The selected profile is not a native toolchain target.
    UnsupportedProfile,
    /// The canonical target triple is empty.
    EmptyTriple,
    /// The canonical target CPU name is empty.
    EmptyCpu,
    /// One target feature has an empty canonical name.
    EmptyFeature,
}

#[cfg(test)]
mod tests {
    use crate::test_support::codegen_target;

    #[test]
    fn targets_canonicalize_machine_features() {
        let target = codegen_target();

        assert_eq!(target.features().collect::<Vec<_>>(), ["avx", "sse4.2"]);
    }
}
