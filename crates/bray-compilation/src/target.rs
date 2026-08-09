use std::num::NonZeroU16;

use bray_codegen::{CodegenTarget, CodegenTargetBuildError};
use bray_compiler_known::AvailabilityRule;
use bray_runtime_interface::RuntimeAbiVersion;
use bray_symbols::AvailableCompilerKnownSymbols;
use bray_target::{NativeTarget, TargetIdentity, TargetProfile};

/// The target profile and product ABI selected for one compilation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SelectedTarget {
    profile: TargetProfile,
    runtime_abi: RuntimeAbiVersion,
}

impl SelectedTarget {
    /// Creates a selected target from validated language and runtime facts.
    pub const fn new(profile: TargetProfile, runtime_abi: RuntimeAbiVersion) -> Self {
        Self {
            profile,
            runtime_abi,
        }
    }

    /// Creates the compiler's deterministic baseline target.
    pub fn baseline() -> Self {
        Self::for_native(NativeTarget::X86_64LinuxGnu)
    }

    /// Creates a selected target from one native toolchain profile.
    pub fn for_native(target: NativeTarget) -> Self {
        Self::new(target.profile(), RuntimeAbiVersion::new(1, 0))
    }

    /// Returns the native selected target with the supplied canonical identity.
    pub fn for_identity(identity: &TargetIdentity) -> Option<Self> {
        NativeTarget::for_identity(identity).map(Self::for_native)
    }

    /// Returns the native toolchain target represented by this selection.
    pub fn native_target(&self) -> Option<NativeTarget> {
        NativeTarget::for_profile(&self.profile)
    }

    /// Returns the selected language-level target profile.
    pub const fn profile(&self) -> &TargetProfile {
        &self.profile
    }

    /// Returns the selected private runtime ABI version.
    pub const fn runtime_abi(&self) -> RuntimeAbiVersion {
        self.runtime_abi
    }

    /// Returns whether the target satisfies one compiler-known availability rule.
    pub const fn supports(&self, rule: AvailabilityRule) -> bool {
        let facts = self.profile.facts();

        match rule {
            AvailabilityRule::Always => true,
            AvailabilityRule::Real16 => facts.scalars().real16(),
            AvailabilityRule::Real128 => facts.scalars().real128(),
            AvailabilityRule::Complex32 => facts.scalars().complex32(),
            AvailabilityRule::Complex256 => facts.scalars().complex256(),
            AvailabilityRule::RawMemory => facts.operations().raw_memory(),
            AvailabilityRule::Atomics => facts.atomics().any(),
            AvailabilityRule::ForeignAbi => facts.abis().c() || facts.abis().system(),
            AvailabilityRule::AddressSpaces => facts.address_spaces().device(),
            AvailabilityRule::Allocation => facts.operations().allocation(),
        }
    }

    /// Returns the width used by target-sized integer literals and constants.
    pub const fn integer_width_bits(&self) -> NonZeroU16 {
        self.profile.machine().pointer_width_bits()
    }

    /// Returns the backend-neutral code generation target.
    pub fn codegen_target(&self) -> Result<CodegenTarget, CodegenTargetBuildError> {
        self.native_target()
            .map(CodegenTarget::for_native)
            .ok_or(CodegenTargetBuildError::UnsupportedProfile)
    }
}

impl Default for SelectedTarget {
    fn default() -> Self {
        Self::baseline()
    }
}

/// Target facts and compiler-known declarations available to one compilation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SelectedTargetContext {
    target: SelectedTarget,
    available_compiler_known_symbols: AvailableCompilerKnownSymbols,
}

impl SelectedTargetContext {
    pub(crate) const fn new(
        target: SelectedTarget,
        available_compiler_known_symbols: AvailableCompilerKnownSymbols,
    ) -> Self {
        Self {
            target,
            available_compiler_known_symbols,
        }
    }

    /// Returns the selected target and product ABI facts.
    pub const fn target(&self) -> &SelectedTarget {
        &self.target
    }

    /// Returns the compiler-known declarations available for this target.
    pub const fn available_compiler_known_symbols(&self) -> &AvailableCompilerKnownSymbols {
        &self.available_compiler_known_symbols
    }
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::AvailabilityRule;
    use bray_runtime_interface::RuntimeAbiVersion;
    use bray_target::NativeTarget;

    use super::SelectedTarget;

    #[test]
    fn baseline_target_is_explicit_and_host_independent() {
        let target = SelectedTarget::baseline();

        assert_eq!(
            target.profile().identity().as_str(),
            "x86_64-unknown-linux-gnu"
        );

        assert_eq!(target.integer_width_bits().get(), 64);
        assert_eq!(target.runtime_abi(), RuntimeAbiVersion::new(1, 0));
    }

    #[test]
    fn declaration_availability_is_derived_from_the_selected_profile() {
        let target = SelectedTarget::baseline();

        assert!(target.supports(AvailabilityRule::Always));
        assert!(!target.supports(AvailabilityRule::Real16));
        assert!(target.supports(AvailabilityRule::RawMemory));
        assert!(target.supports(AvailabilityRule::Allocation));
        assert!(target.supports(AvailabilityRule::ForeignAbi));
    }

    #[test]
    fn every_native_target_produces_its_exact_codegen_contract() {
        for native in NativeTarget::ALL {
            let selected = SelectedTarget::for_native(native);

            let codegen = selected
                .codegen_target()
                .unwrap_or_else(|error| panic!("native target must support codegen: {error:?}"));

            assert_eq!(selected.profile().identity().as_str(), native.as_str());
            assert_eq!(codegen.identity().as_str(), native.as_str());
            assert_eq!(codegen.machine().architecture(), native.architecture());
            assert_eq!(codegen.machine().object_format(), native.object_format());
        }
    }
}
