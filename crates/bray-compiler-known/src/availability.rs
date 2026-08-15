define_catalog_enum! {
    /// A language-defined predicate controlling catalog entry availability.
    ///
    /// The process-wide catalog retains every entry. Compilation-owned target
    /// selected-target queries evaluate these rules without mutating catalog descriptors.
    #[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
    pub enum AvailabilityRule {
        /// The entry is available on every supported target.
        #[default]
        Always => "Always",
        /// The target supports the optional 16-bit real scalar form.
        Real16 => "Real16",
        /// The target supports the optional 128-bit real scalar form.
        Real128 => "Real128",
        /// The target supports the optional 32-bit complex scalar form.
        Complex32 => "Complex32",
        /// The target supports the optional 256-bit complex scalar form.
        Complex256 => "Complex256",
        /// The target supports the required raw-memory operation.
        RawMemory => "RawMemory",
        /// The target supports the required atomic operation.
        Atomics => "Atomics",
        /// The target supports the required foreign ABI operation.
        ForeignAbi => "ForeignAbi",
        /// The target supports the required address-space operation.
        AddressSpaces => "AddressSpaces",
        /// The target supports the required allocation operation.
        Allocation => "Allocation",
    }
}

#[cfg(test)]
mod tests {
    use super::AvailabilityRule;

    #[test]
    fn availability_defaults_to_always() {
        assert_eq!(AvailabilityRule::default(), AvailabilityRule::Always);
    }

    #[test]
    fn all_rules_follow_canonical_declaration_order() {
        assert_eq!(
            AvailabilityRule::ALL,
            &[
                AvailabilityRule::Always,
                AvailabilityRule::Real16,
                AvailabilityRule::Real128,
                AvailabilityRule::Complex32,
                AvailabilityRule::Complex256,
                AvailabilityRule::RawMemory,
                AvailabilityRule::Atomics,
                AvailabilityRule::ForeignAbi,
                AvailabilityRule::AddressSpaces,
                AvailabilityRule::Allocation,
            ]
        );
    }
}
