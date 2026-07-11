/// A language-defined predicate controlling catalog entry availability.
///
/// The process-wide catalog retains every entry. Compilation-owned target
/// facts evaluate these rules without mutating catalog descriptors.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AvailabilityRule {
    /// The entry is available on every supported target.
    #[default]
    Always,
    /// The target supports the optional 16-bit real scalar form.
    Real16,
    /// The target supports the optional 128-bit real scalar form.
    Real128,
    /// The target supports the optional 32-bit complex scalar form.
    Complex32,
    /// The target supports the optional 256-bit complex scalar form.
    Complex256,
    /// The target supports the required raw-memory operation.
    RawMemory,
    /// The target supports the required atomic operation.
    Atomics,
    /// The target supports the required foreign ABI operation.
    ForeignAbi,
    /// The target supports the required address-space operation.
    AddressSpaces,
    /// The target supports the required allocation operation.
    Allocation,
}

impl AvailabilityRule {
    #[cfg(any(test, feature = "generation"))]
    pub(crate) fn from_catalog_spelling(spelling: &str) -> Option<Self> {
        match spelling {
            "Always" => Some(Self::Always),
            "Real16" => Some(Self::Real16),
            "Real128" => Some(Self::Real128),
            "Complex32" => Some(Self::Complex32),
            "Complex256" => Some(Self::Complex256),
            "RawMemory" => Some(Self::RawMemory),
            "Atomics" => Some(Self::Atomics),
            "ForeignAbi" => Some(Self::ForeignAbi),
            "AddressSpaces" => Some(Self::AddressSpaces),
            "Allocation" => Some(Self::Allocation),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::AvailabilityRule;

    #[test]
    fn availability_defaults_to_always() {
        assert_eq!(AvailabilityRule::default(), AvailabilityRule::Always);
    }
}
