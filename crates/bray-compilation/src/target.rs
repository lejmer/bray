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
    declaration_availability: TargetAvailabilityFacts,
}

impl SelectedTarget {
    /// Creates a selected target from validated language, runtime, and declaration facts.
    pub const fn new(
        profile: TargetProfile,
        runtime_abi: RuntimeAbiVersion,
        declaration_availability: TargetAvailabilityFacts,
    ) -> Self {
        Self {
            profile,
            runtime_abi,
            declaration_availability,
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

        Self::new(
            TargetProfile::new(identity, machine),
            RuntimeAbiVersion::new(1, 0),
            TargetAvailabilityFacts::portable(),
        )
    }

    /// Returns the selected language-level target profile.
    pub const fn profile(&self) -> &TargetProfile {
        &self.profile
    }

    /// Returns the selected private runtime ABI version.
    pub const fn runtime_abi(&self) -> RuntimeAbiVersion {
        self.runtime_abi
    }

    /// Returns the compiler-known declaration capabilities of this target.
    pub const fn declaration_availability(&self) -> TargetAvailabilityFacts {
        self.declaration_availability
    }

    /// Returns a copy with the requested compiler-known declaration capabilities.
    pub const fn with_declaration_availability(
        mut self,
        declaration_availability: TargetAvailabilityFacts,
    ) -> Self {
        self.declaration_availability = declaration_availability;

        self
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

/// Immutable target capability facts used to evaluate compiler-known availability.
///
/// The portable default enables only declarations marked as universally available. Target
/// selection can explicitly enable further closed catalog capabilities.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TargetAvailabilityFacts {
    real16: bool,
    real128: bool,
    complex32: bool,
    complex256: bool,
    raw_memory: bool,
    atomics: bool,
    foreign_abi: bool,
    address_spaces: bool,
    allocation: bool,
}

impl TargetAvailabilityFacts {
    /// Creates the portable baseline that supports only universally available declarations.
    pub const fn portable() -> Self {
        Self {
            real16: false,
            real128: false,
            complex32: false,
            complex256: false,
            raw_memory: false,
            atomics: false,
            foreign_abi: false,
            address_spaces: false,
            allocation: false,
        }
    }

    /// Creates a target fact set that supports every closed availability rule.
    pub const fn all() -> Self {
        Self {
            real16: true,
            real128: true,
            complex32: true,
            complex256: true,
            raw_memory: true,
            atomics: true,
            foreign_abi: true,
            address_spaces: true,
            allocation: true,
        }
    }

    /// Returns a copy with one closed catalog capability enabled or disabled.
    pub const fn with_rule(mut self, rule: AvailabilityRule, available: bool) -> Self {
        match rule {
            AvailabilityRule::Always => {}
            AvailabilityRule::Real16 => self.real16 = available,
            AvailabilityRule::Real128 => self.real128 = available,
            AvailabilityRule::Complex32 => self.complex32 = available,
            AvailabilityRule::Complex256 => self.complex256 = available,
            AvailabilityRule::RawMemory => self.raw_memory = available,
            AvailabilityRule::Atomics => self.atomics = available,
            AvailabilityRule::ForeignAbi => self.foreign_abi = available,
            AvailabilityRule::AddressSpaces => self.address_spaces = available,
            AvailabilityRule::Allocation => self.allocation = available,
        }

        self
    }

    /// Returns whether the selected target satisfies one closed availability rule.
    pub const fn supports(self, rule: AvailabilityRule) -> bool {
        match rule {
            AvailabilityRule::Always => true,
            AvailabilityRule::Real16 => self.real16,
            AvailabilityRule::Real128 => self.real128,
            AvailabilityRule::Complex32 => self.complex32,
            AvailabilityRule::Complex256 => self.complex256,
            AvailabilityRule::RawMemory => self.raw_memory,
            AvailabilityRule::Atomics => self.atomics,
            AvailabilityRule::ForeignAbi => self.foreign_abi,
            AvailabilityRule::AddressSpaces => self.address_spaces,
            AvailabilityRule::Allocation => self.allocation,
        }
    }
}

impl Default for TargetAvailabilityFacts {
    fn default() -> Self {
        Self::portable()
    }
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::AvailabilityRule;
    use bray_runtime_interface::RuntimeAbiVersion;

    use super::{SelectedTarget, TargetAvailabilityFacts};

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
    fn portable_and_complete_target_facts_have_explicit_closed_semantics() {
        let portable = TargetAvailabilityFacts::portable();

        assert!(portable.supports(AvailabilityRule::Always));
        assert!(!portable.supports(AvailabilityRule::Real16));
        assert!(!portable.supports(AvailabilityRule::RawMemory));

        let complete = TargetAvailabilityFacts::all();

        assert!(complete.supports(AvailabilityRule::Real16));
        assert!(complete.supports(AvailabilityRule::RawMemory));
    }

    #[test]
    fn individual_capabilities_can_be_selected_without_affecting_other_rules() {
        let facts = TargetAvailabilityFacts::portable()
            .with_rule(AvailabilityRule::Real16, true)
            .with_rule(AvailabilityRule::RawMemory, false);

        assert!(facts.supports(AvailabilityRule::Always));
        assert!(facts.supports(AvailabilityRule::Real16));
        assert!(!facts.supports(AvailabilityRule::RawMemory));
    }
}
