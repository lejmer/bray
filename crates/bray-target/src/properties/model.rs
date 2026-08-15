use std::num::NonZeroU64;

use bray_base::NonEmptySharedStr;

use super::{
    TargetAbiSupport, TargetAbiScalars, TargetAtomicSupport, TargetCDataModel,
    TargetForeignAbiContract, TargetScalarSupport,
};

/// Stable identity details of one target profile.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetPlatformIdentity {
    vendor: NonEmptySharedStr,
    system: NonEmptySharedStr,
    environment: NonEmptySharedStr,
    abi: NonEmptySharedStr,
}

impl TargetPlatformIdentity {
    /// Creates complete identity properties when every spelling is nonempty.
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
pub struct TargetAddressSpaces {
    host: bool,
    device: bool,
}

impl TargetAddressSpaces {
    /// Creates address-space properties when at least one address space is available.
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
pub struct TargetAlignmentLimits {
    max_storage: NonZeroU64,
    max_allocation: NonZeroU64,
}

impl TargetAlignmentLimits {
    /// Creates alignment properties when both maxima are powers of two and allocation does not exceed storage.
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
pub struct TargetOperationSupport {
    raw_memory: bool,
    allocation: bool,
}

impl TargetOperationSupport {
    /// Creates target operation capability properties.
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

/// Complete language-defined properties not derived from target identity or machine properties.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetProperties {
    identity: TargetPlatformIdentity,
    scalars: TargetScalarSupport,
    atomics: TargetAtomicSupport,
    abis: TargetAbiSupport,
    c_abi: TargetCDataModel,
    address_spaces: TargetAddressSpaces,
    alignments: TargetAlignmentLimits,
    operations: TargetOperationSupport,
    dynamic_loading: bool,
}

impl TargetProperties {
    /// Creates a complete set of independently supplied language target properties.
    pub const fn new(
        identity: TargetPlatformIdentity,
        scalars: TargetScalarSupport,
        atomics: TargetAtomicSupport,
        abis: TargetAbiSupport,
        c_abi: TargetCDataModel,
        address_spaces: TargetAddressSpaces,
        alignments: TargetAlignmentLimits,
        operations: TargetOperationSupport,
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

    /// Creates the portable baseline property set with the supplied C data model.
    pub fn try_portable(
        vendor: &str,
        system: &str,
        environment: &str,
        abi: &str,
        c_abi: TargetCDataModel,
    ) -> Option<Self> {
        let identity = TargetPlatformIdentity::try_new(vendor, system, environment, abi)?;
        let address_spaces = TargetAddressSpaces::try_new(true, false)?;
        let maximum_alignment = NonZeroU64::new(1 << 29).unwrap_or(NonZeroU64::MIN);
        let alignments = TargetAlignmentLimits::try_new(maximum_alignment, maximum_alignment)?;

        let foreign_abi = TargetForeignAbiContract::new(
            TargetAbiScalars::required(),
            true,
            true,
            true,
            true,
            maximum_alignment,
        );

        Some(Self::new(
            identity,
            TargetScalarSupport::default(),
            TargetAtomicSupport::default(),
            TargetAbiSupport::new(Some(foreign_abi), Some(foreign_abi)),
            c_abi,
            address_spaces,
            alignments,
            TargetOperationSupport::default(),
        ))
    }

    /// Returns these properties with the supplied compiler-provided operation capabilities.
    pub const fn with_operations(mut self, operations: TargetOperationSupport) -> Self {
        self.operations = operations;

        self
    }

    /// Returns these properties with the supplied atomic representation contracts.
    pub const fn with_atomics(mut self, atomics: TargetAtomicSupport) -> Self {
        self.atomics = atomics;

        self
    }

    /// Returns these properties with the supplied dynamic-loading capability.
    pub const fn with_dynamic_loading(mut self, dynamic_loading: bool) -> Self {
        self.dynamic_loading = dynamic_loading;

        self
    }

    /// Returns stable target identity properties.
    pub const fn identity(&self) -> &TargetPlatformIdentity {
        &self.identity
    }

    /// Returns target-conditional scalar availability properties.
    pub const fn scalars(&self) -> TargetScalarSupport {
        self.scalars
    }

    /// Returns atomic representation availability properties.
    pub const fn atomics(&self) -> TargetAtomicSupport {
        self.atomics
    }

    /// Returns callable ABI availability and acceptance contracts.
    pub const fn abis(&self) -> TargetAbiSupport {
        self.abis
    }

    /// Returns the target's exact C scalar data model.
    pub const fn c_abi(&self) -> TargetCDataModel {
        self.c_abi
    }

    /// Returns address-space availability properties.
    pub const fn address_spaces(&self) -> TargetAddressSpaces {
        self.address_spaces
    }

    /// Returns maximum supported alignment properties.
    pub const fn alignments(&self) -> TargetAlignmentLimits {
        self.alignments
    }

    /// Returns compiler-known operation capability properties.
    pub const fn operations(&self) -> TargetOperationSupport {
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
    use crate::{TargetAlignmentLimits, TargetPropertyKind, TargetPropertyValue, TargetPlatformIdentity};

    #[test]
    fn derived_properties_cannot_contradict_machine_properties() {
        let profile = test_target_profile();

        assert_eq!(
            profile.property(TargetPropertyKind::IdentityArchitecture),
            TargetPropertyValue::String("x86_64")
        );

        assert_eq!(
            profile.property(TargetPropertyKind::PointerBits),
            TargetPropertyValue::Usize(64)
        );

        assert_eq!(
            profile.property(TargetPropertyKind::EndianLittle),
            TargetPropertyValue::Boolean(true)
        );

        assert_eq!(
            profile.property(TargetPropertyKind::EndianBig),
            TargetPropertyValue::Boolean(false)
        );

        assert_eq!(
            profile.property(TargetPropertyKind::PlatformDynamicLoading),
            TargetPropertyValue::Boolean(false)
        );
    }

    #[test]
    fn required_scalar_properties_are_always_available() {
        let profile = test_target_profile();

        assert_eq!(
            profile.property(TargetPropertyKind::ScalarI32),
            TargetPropertyValue::Boolean(true)
        );

        assert_eq!(
            profile.property(TargetPropertyKind::ScalarR16),
            TargetPropertyValue::Boolean(false)
        );
    }

    #[test]
    fn incomplete_or_contradictory_property_groups_are_rejected() {
        assert!(TargetPlatformIdentity::try_new("", "linux", "gnu", "gnu").is_none());

        let storage = NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN);
        let allocation = NonZeroU64::new(16).unwrap_or(NonZeroU64::MIN);

        assert_eq!(TargetAlignmentLimits::try_new(storage, allocation), None);
    }
}
