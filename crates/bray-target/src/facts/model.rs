use std::num::NonZeroU64;

use bray_base::NonEmptySharedStr;

use super::{
    TargetAbiFacts, TargetAbiScalarFacts, TargetAtomicFacts, TargetCAbiFacts,
    TargetForeignAbiFacts, TargetScalarFacts,
};

/// Stable identity details of one target profile.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetIdentityFacts {
    vendor: NonEmptySharedStr,
    system: NonEmptySharedStr,
    environment: NonEmptySharedStr,
    abi: NonEmptySharedStr,
}

impl TargetIdentityFacts {
    /// Creates complete identity facts when every spelling is nonempty.
    pub fn try_new(
        vendor: impl Into<std::sync::Arc<str>>,
        system: impl Into<std::sync::Arc<str>>,
        environment: impl Into<std::sync::Arc<str>>,
        abi: impl Into<std::sync::Arc<str>>,
    ) -> Option<Self> {
        Some(Self {
            vendor: NonEmptySharedStr::try_new(vendor)?,
            system: NonEmptySharedStr::try_new(system)?,
            environment: NonEmptySharedStr::try_new(environment)?,
            abi: NonEmptySharedStr::try_new(abi)?,
        })
    }

    /// Returns the target vendor spelling.
    pub fn vendor(&self) -> &str {
        self.vendor.as_str()
    }

    /// Returns the target operating-system spelling.
    pub fn system(&self) -> &str {
        self.system.as_str()
    }

    /// Returns the target environment spelling.
    pub fn environment(&self) -> &str {
        self.environment.as_str()
    }

    /// Returns the target ABI-family spelling.
    pub fn abi(&self) -> &str {
        self.abi.as_str()
    }
}

/// Address spaces available on one target.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetAddressSpaceFacts {
    host: bool,
    device: bool,
}

impl TargetAddressSpaceFacts {
    /// Creates address-space facts when at least one address space is available.
    pub const fn try_new(host: bool, device: bool) -> Option<Self> {
        if !host && !device {
            return None;
        }

        Some(Self { host, device })
    }

    /// Returns whether the host address space is available.
    pub const fn host(self) -> bool {
        self.host
    }

    /// Returns whether the device address space is available.
    pub const fn device(self) -> bool {
        self.device
    }
}

/// Maximum supported storage and allocation alignments.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetAlignmentFacts {
    max_storage: NonZeroU64,
    max_allocation: NonZeroU64,
}

impl TargetAlignmentFacts {
    /// Creates alignment facts when both maxima are powers of two and allocation does not exceed storage.
    pub const fn try_new(max_storage: NonZeroU64, max_allocation: NonZeroU64) -> Option<Self> {
        if !max_storage.get().is_power_of_two()
            || !max_allocation.get().is_power_of_two()
            || max_allocation.get() > max_storage.get()
        {
            return None;
        }

        Some(Self {
            max_storage,
            max_allocation,
        })
    }

    /// Returns the maximum supported storage alignment in bytes.
    pub const fn max_storage(self) -> NonZeroU64 {
        self.max_storage
    }

    /// Returns the maximum supported allocation alignment in bytes.
    pub const fn max_allocation(self) -> NonZeroU64 {
        self.max_allocation
    }
}

/// Target support for compiler-known raw-memory and allocation operations.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetOperationFacts {
    raw_memory: bool,
    allocation: bool,
}

impl TargetOperationFacts {
    /// Creates target operation capability facts.
    pub const fn new(raw_memory: bool, allocation: bool) -> Self {
        Self {
            raw_memory,
            allocation,
        }
    }

    /// Returns whether compiler-known raw-memory operations are available.
    pub const fn raw_memory(self) -> bool {
        self.raw_memory
    }

    /// Returns whether compiler-known allocation operations are available.
    pub const fn allocation(self) -> bool {
        self.allocation
    }
}

/// Complete language-defined facts not derived from target identity or machine properties.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetFacts {
    identity: TargetIdentityFacts,
    scalars: TargetScalarFacts,
    atomics: TargetAtomicFacts,
    abis: TargetAbiFacts,
    c_abi: TargetCAbiFacts,
    address_spaces: TargetAddressSpaceFacts,
    alignments: TargetAlignmentFacts,
    operations: TargetOperationFacts,
    dynamic_loading: bool,
}

impl TargetFacts {
    /// Creates a complete set of independently supplied language target facts.
    pub const fn new(
        identity: TargetIdentityFacts,
        scalars: TargetScalarFacts,
        atomics: TargetAtomicFacts,
        abis: TargetAbiFacts,
        c_abi: TargetCAbiFacts,
        address_spaces: TargetAddressSpaceFacts,
        alignments: TargetAlignmentFacts,
        operations: TargetOperationFacts,
    ) -> Self {
        Self {
            identity,
            scalars,
            atomics,
            abis,
            c_abi,
            address_spaces,
            alignments,
            operations,
            dynamic_loading: false,
        }
    }

    /// Creates the portable baseline fact set with the supplied C data model.
    pub fn try_portable(
        vendor: &str,
        system: &str,
        environment: &str,
        abi: &str,
        c_abi: TargetCAbiFacts,
    ) -> Option<Self> {
        let identity = TargetIdentityFacts::try_new(vendor, system, environment, abi)?;
        let address_spaces = TargetAddressSpaceFacts::try_new(true, false)?;
        let maximum_alignment = NonZeroU64::new(1 << 29).unwrap_or(NonZeroU64::MIN);
        let alignments = TargetAlignmentFacts::try_new(maximum_alignment, maximum_alignment)?;

        let foreign_abi = TargetForeignAbiFacts::new(
            TargetAbiScalarFacts::required(),
            true,
            true,
            true,
            true,
            maximum_alignment,
        );

        Some(Self::new(
            identity,
            TargetScalarFacts::default(),
            TargetAtomicFacts::default(),
            TargetAbiFacts::new(Some(foreign_abi), Some(foreign_abi)),
            c_abi,
            address_spaces,
            alignments,
            TargetOperationFacts::default(),
        ))
    }

    /// Returns these facts with the supplied compiler-provided operation capabilities.
    pub const fn with_operations(mut self, operations: TargetOperationFacts) -> Self {
        self.operations = operations;

        self
    }

    /// Returns these facts with the supplied atomic representation contracts.
    pub const fn with_atomics(mut self, atomics: TargetAtomicFacts) -> Self {
        self.atomics = atomics;

        self
    }

    /// Returns these facts with the supplied dynamic-loading capability.
    pub const fn with_dynamic_loading(mut self, dynamic_loading: bool) -> Self {
        self.dynamic_loading = dynamic_loading;

        self
    }

    /// Returns stable target identity facts.
    pub const fn identity(&self) -> &TargetIdentityFacts {
        &self.identity
    }

    /// Returns target-conditional scalar availability facts.
    pub const fn scalars(&self) -> TargetScalarFacts {
        self.scalars
    }

    /// Returns atomic representation availability facts.
    pub const fn atomics(&self) -> TargetAtomicFacts {
        self.atomics
    }

    /// Returns callable ABI availability and acceptance contracts.
    pub const fn abis(&self) -> TargetAbiFacts {
        self.abis
    }

    /// Returns the target's exact C scalar data model.
    pub const fn c_abi(&self) -> TargetCAbiFacts {
        self.c_abi
    }

    /// Returns address-space availability facts.
    pub const fn address_spaces(&self) -> TargetAddressSpaceFacts {
        self.address_spaces
    }

    /// Returns maximum supported alignment facts.
    pub const fn alignments(&self) -> TargetAlignmentFacts {
        self.alignments
    }

    /// Returns compiler-known operation capability facts.
    pub const fn operations(&self) -> TargetOperationFacts {
        self.operations
    }

    /// Returns whether the complete dynamic-library platform role family is available.
    pub const fn dynamic_loading(&self) -> bool {
        self.dynamic_loading
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use crate::test_support::test_target_profile;
    use crate::{TargetAlignmentFacts, TargetFactKind, TargetFactValue, TargetIdentityFacts};

    #[test]
    fn derived_facts_cannot_contradict_machine_properties() {
        let profile = test_target_profile();

        assert_eq!(
            profile.fact(TargetFactKind::IdentityArchitecture),
            TargetFactValue::String("x86_64")
        );

        assert_eq!(
            profile.fact(TargetFactKind::PointerBits),
            TargetFactValue::Usize(64)
        );

        assert_eq!(
            profile.fact(TargetFactKind::EndianLittle),
            TargetFactValue::Boolean(true)
        );

        assert_eq!(
            profile.fact(TargetFactKind::EndianBig),
            TargetFactValue::Boolean(false)
        );

        assert_eq!(
            profile.fact(TargetFactKind::PlatformDynamicLoading),
            TargetFactValue::Boolean(false)
        );
    }

    #[test]
    fn required_scalar_facts_are_always_available() {
        let profile = test_target_profile();

        assert_eq!(
            profile.fact(TargetFactKind::ScalarI32),
            TargetFactValue::Boolean(true)
        );

        assert_eq!(
            profile.fact(TargetFactKind::ScalarR16),
            TargetFactValue::Boolean(false)
        );
    }

    #[test]
    fn incomplete_or_contradictory_fact_groups_are_rejected() {
        assert!(TargetIdentityFacts::try_new("", "linux", "gnu", "gnu").is_none());

        let storage = NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN);
        let allocation = NonZeroU64::new(16).unwrap_or(NonZeroU64::MIN);

        assert_eq!(TargetAlignmentFacts::try_new(storage, allocation), None);
    }
}
