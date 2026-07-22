use std::num::{NonZeroU16, NonZeroU32};

use bray_compiler_known::AvailabilityRule;
use bray_runtime_interface::RuntimeAbiVersion;
use bray_symbols::AvailableCompilerKnownSymbols;
use bray_target::{
    Endianness, ObjectFormat, TargetArchitecture, TargetIdentity, TargetMachineProperties,
    TargetProfile,
};

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
        let Some(identity) = TargetIdentity::try_new("x86_64-unknown-linux-gnu") else {
            panic!("the compiler baseline target identity must be valid");
        };

        let pointer_width = NonZeroU16::new(64).unwrap_or(NonZeroU16::MIN);
        let pointer_alignment = NonZeroU32::new(8).unwrap_or(NonZeroU32::MIN);
        let stack_alignment = NonZeroU32::new(16).unwrap_or(NonZeroU32::MIN);

        let Some(machine) = TargetMachineProperties::try_new(
            TargetArchitecture::X86_64,
            ObjectFormat::Elf,
            Endianness::Little,
            pointer_width,
            pointer_alignment,
            stack_alignment,
        ) else {
            panic!("the compiler baseline target machine must be valid");
        };

        let facts = match bray_target::TargetFacts::try_portable("unknown", "linux", "gnu", "gnu") {
            Some(facts) => facts,
            None => panic!("the compiler baseline target facts must be valid"),
        };

        let profile = match TargetProfile::try_new(identity, machine, facts) {
            Ok(profile) => profile,
            Err(error) => panic!("the compiler baseline target profile must be valid: {error:?}"),
        };

        Self::new(profile, RuntimeAbiVersion::new(1, 0))
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
}

impl Default for SelectedTarget {
    fn default() -> Self {
        Self::baseline()
    }
}

/// Target facts and compiler-known declarations available to one compilation.
#[derive(Clone, Debug, Eq, PartialEq)]
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
        assert!(!target.supports(AvailabilityRule::RawMemory));
        assert!(target.supports(AvailabilityRule::ForeignAbi));
    }
}
