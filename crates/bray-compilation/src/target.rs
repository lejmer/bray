use bray_compiler_known::AvailabilityRule;

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

    use super::TargetAvailabilityFacts;

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
