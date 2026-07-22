use crate::{
    TargetFactKind, TargetFactValue, TargetFacts, TargetIdentity, TargetMachineProperties,
};

/// The language-level identity and machine properties of one compilation target.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetProfile {
    identity: TargetIdentity,
    machine: TargetMachineProperties,
    facts: TargetFacts,
}

/// A contradiction between facts supplied for one target profile.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TargetProfileBuildError {
    /// A target-sized integer cannot represent one public `usize` target fact.
    TargetFactNotRepresentable,
    /// The maximum storage alignment cannot represent the target pointer alignment.
    StorageAlignmentBelowPointerAlignment,
    /// The maximum storage alignment cannot represent the target stack alignment.
    StorageAlignmentBelowStackAlignment,
    /// The maximum allocation alignment cannot represent the target pointer alignment.
    AllocationAlignmentBelowPointerAlignment,
    /// A complex scalar is available while its real component is unavailable.
    ComplexScalarMissingComponent,
    /// A callable ABI accepts a scalar unavailable on the target.
    AbiAcceptsUnavailableScalar,
    /// A callable ABI accepts an alignment above the target storage maximum.
    AbiAlignmentAboveStorageMaximum,
}

impl std::fmt::Display for TargetProfileBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TargetFactNotRepresentable => {
                formatter.write_str("a target usize fact is not representable")
            }
            Self::StorageAlignmentBelowPointerAlignment => {
                formatter.write_str("maximum storage alignment is below pointer alignment")
            }
            Self::StorageAlignmentBelowStackAlignment => {
                formatter.write_str("maximum storage alignment is below stack alignment")
            }
            Self::AllocationAlignmentBelowPointerAlignment => {
                formatter.write_str("maximum allocation alignment is below pointer alignment")
            }
            Self::ComplexScalarMissingComponent => {
                formatter.write_str("a complex scalar is available without its real component")
            }
            Self::AbiAcceptsUnavailableScalar => {
                formatter.write_str("a callable ABI accepts an unavailable scalar")
            }
            Self::AbiAlignmentAboveStorageMaximum => {
                formatter.write_str("a callable ABI alignment exceeds the storage maximum")
            }
        }
    }
}

impl std::error::Error for TargetProfileBuildError {}

impl TargetProfile {
    /// Creates a target profile when its complete fact groups are mutually consistent.
    pub fn try_new(
        identity: TargetIdentity,
        machine: TargetMachineProperties,
        facts: TargetFacts,
    ) -> Result<Self, TargetProfileBuildError> {
        let pointer_alignment = u64::from(machine.pointer_alignment_bytes().get());
        let stack_alignment = u64::from(machine.stack_alignment_bytes().get());
        let maximum_usize = maximum_usize(machine.pointer_width_bits().get());
        let alignments = facts.alignments();

        if alignments.max_storage().get() > maximum_usize
            || alignments.max_allocation().get() > maximum_usize
        {
            return Err(TargetProfileBuildError::TargetFactNotRepresentable);
        }

        if alignments.max_storage().get() < pointer_alignment {
            return Err(TargetProfileBuildError::StorageAlignmentBelowPointerAlignment);
        }

        if alignments.max_storage().get() < stack_alignment {
            return Err(TargetProfileBuildError::StorageAlignmentBelowStackAlignment);
        }

        if alignments.max_allocation().get() < pointer_alignment {
            return Err(TargetProfileBuildError::AllocationAlignmentBelowPointerAlignment);
        }

        let scalars = facts.scalars();

        if (scalars.complex32() && !scalars.real16())
            || (scalars.complex256() && !scalars.real128())
        {
            return Err(TargetProfileBuildError::ComplexScalarMissingComponent);
        }

        let abis = facts.abis();

        if !abis.is_supported_by(scalars) {
            return Err(TargetProfileBuildError::AbiAcceptsUnavailableScalar);
        }

        for contract in [abis.c_contract(), abis.system_contract()]
            .into_iter()
            .flatten()
        {
            if contract.max_alignment().get() > alignments.max_storage().get() {
                return Err(TargetProfileBuildError::AbiAlignmentAboveStorageMaximum);
            }
        }

        Ok(Self {
            identity,
            machine,
            facts,
        })
    }

    /// Returns the stable identity of this target profile.
    pub const fn identity(&self) -> &TargetIdentity {
        &self.identity
    }

    /// Returns the target's validated machine properties.
    pub const fn machine(&self) -> &TargetMachineProperties {
        &self.machine
    }

    /// Returns the language-defined target facts not derived from machine properties.
    pub const fn facts(&self) -> &TargetFacts {
        &self.facts
    }

    /// Returns one language-defined target fact.
    pub fn fact(&self, kind: TargetFactKind) -> TargetFactValue<'_> {
        TargetFactValue::for_profile(self, kind)
    }
}

const fn maximum_usize(pointer_width_bits: u16) -> u64 {
    if pointer_width_bits >= u64::BITS as u16 {
        u64::MAX
    } else {
        (1_u64 << pointer_width_bits) - 1
    }
}

#[cfg(test)]
mod tests {
    use std::num::{NonZeroU16, NonZeroU32, NonZeroU64};

    use crate::test_support::{test_target_facts, test_target_machine, test_target_profile};
    use crate::{
        Endianness, ObjectFormat, TargetAbiFacts, TargetAbiScalarFacts, TargetAddressSpaceFacts,
        TargetAlignmentFacts, TargetArchitecture, TargetAtomicFacts, TargetFacts,
        TargetForeignAbiFacts, TargetIdentity, TargetIdentityFacts, TargetMachineProperties,
        TargetOperationFacts, TargetProfile, TargetProfileBuildError, TargetScalarFacts,
    };

    #[test]
    fn profiles_keep_identity_and_machine_properties_together() {
        let profile = test_target_profile();

        assert_eq!(profile.identity().as_str(), "x86_64-unknown-linux-gnu");
        assert_eq!(profile.machine().pointer_width_bits().get(), 64);

        assert_eq!(
            profile.fact(crate::TargetFactKind::PointerBytes),
            crate::TargetFactValue::Usize(8)
        );
    }

    #[test]
    fn profiles_reject_alignment_facts_that_contradict_machine_properties() {
        let Some(identity) = TargetIdentity::try_new("x86_64-unknown-linux-gnu") else {
            panic!("test target identity must be valid");
        };

        let Some(identity_facts) = TargetIdentityFacts::try_new("unknown", "linux", "gnu", "gnu")
        else {
            panic!("test target identity facts must be valid");
        };

        let maximum = NonZeroU64::new(4).unwrap_or(NonZeroU64::MIN);

        let Some(alignments) = TargetAlignmentFacts::try_new(maximum, maximum) else {
            panic!("test alignment facts must be internally valid");
        };

        let Some(address_spaces) = TargetAddressSpaceFacts::try_new(true, false) else {
            panic!("test target must expose one address space");
        };

        let baseline = test_target_facts();

        let facts = TargetFacts::new(
            identity_facts,
            baseline.scalars(),
            TargetAtomicFacts::default(),
            baseline.abis(),
            address_spaces,
            alignments,
            TargetOperationFacts::default(),
        );

        assert_eq!(
            TargetProfile::try_new(identity, test_target_machine(), facts),
            Err(TargetProfileBuildError::StorageAlignmentBelowPointerAlignment)
        );
    }

    #[test]
    fn profiles_reject_storage_maxima_below_stack_alignment() {
        let machine = test_target_machine();
        let maximum = NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN);

        let alignments = TargetAlignmentFacts::try_new(maximum, maximum)
            .unwrap_or_else(|| panic!("test alignments must be valid"));

        let facts = facts_with(TargetScalarFacts::default(), None, alignments);

        assert_eq!(
            TargetProfile::try_new(test_identity(), machine, facts),
            Err(TargetProfileBuildError::StorageAlignmentBelowStackAlignment)
        );
    }

    #[test]
    fn profiles_reject_usize_facts_that_the_target_cannot_represent() {
        let pointer_width = NonZeroU16::new(32).unwrap_or(NonZeroU16::MIN);
        let pointer_alignment = NonZeroU32::new(4).unwrap_or(NonZeroU32::MIN);
        let stack_alignment = NonZeroU32::new(16).unwrap_or(NonZeroU32::MIN);

        let machine = TargetMachineProperties::try_new(
            TargetArchitecture::X86,
            ObjectFormat::Elf,
            Endianness::Little,
            pointer_width,
            pointer_alignment,
            stack_alignment,
        )
        .unwrap_or_else(|| panic!("test machine must be valid"));

        let maximum = NonZeroU64::new(1 << 40).unwrap_or(NonZeroU64::MIN);

        let alignments = TargetAlignmentFacts::try_new(maximum, maximum)
            .unwrap_or_else(|| panic!("test alignments must be valid"));

        let facts = facts_with(TargetScalarFacts::default(), None, alignments);

        assert_eq!(
            TargetProfile::try_new(test_identity(), machine, facts),
            Err(TargetProfileBuildError::TargetFactNotRepresentable)
        );
    }

    #[test]
    fn profiles_reject_scalar_and_abi_cross_group_contradictions() {
        let maximum = NonZeroU64::new(1 << 29).unwrap_or(NonZeroU64::MIN);

        let alignments = TargetAlignmentFacts::try_new(maximum, maximum)
            .unwrap_or_else(|| panic!("test alignments must be valid"));

        let inconsistent_scalars = TargetScalarFacts::new(false, false, true, false);
        let facts = facts_with(inconsistent_scalars, None, alignments);

        assert_eq!(
            TargetProfile::try_new(test_identity(), test_target_machine(), facts),
            Err(TargetProfileBuildError::ComplexScalarMissingComponent)
        );

        let abi = TargetForeignAbiFacts::new(
            TargetAbiScalarFacts::all(),
            true,
            true,
            true,
            true,
            maximum,
        );

        let facts = facts_with(
            TargetScalarFacts::default(),
            Some(TargetAbiFacts::new(Some(abi), None)),
            alignments,
        );

        assert_eq!(
            TargetProfile::try_new(test_identity(), test_target_machine(), facts),
            Err(TargetProfileBuildError::AbiAcceptsUnavailableScalar)
        );
    }

    #[test]
    fn profiles_reject_abi_alignment_above_storage_maximum() {
        let storage_maximum = NonZeroU64::new(16).unwrap_or(NonZeroU64::MIN);
        let abi_maximum = NonZeroU64::new(32).unwrap_or(NonZeroU64::MIN);

        let alignments = TargetAlignmentFacts::try_new(storage_maximum, storage_maximum)
            .unwrap_or_else(|| panic!("test alignments must be valid"));

        let abi = TargetForeignAbiFacts::new(
            TargetAbiScalarFacts::required(),
            true,
            true,
            true,
            true,
            abi_maximum,
        );

        let facts = facts_with(
            TargetScalarFacts::default(),
            Some(TargetAbiFacts::new(Some(abi), None)),
            alignments,
        );

        assert_eq!(
            TargetProfile::try_new(test_identity(), test_target_machine(), facts),
            Err(TargetProfileBuildError::AbiAlignmentAboveStorageMaximum)
        );
    }

    fn test_identity() -> TargetIdentity {
        TargetIdentity::try_new("x86_64-unknown-linux-gnu")
            .unwrap_or_else(|| panic!("test target identity must be valid"))
    }

    fn facts_with(
        scalars: TargetScalarFacts,
        abis: Option<TargetAbiFacts>,
        alignments: TargetAlignmentFacts,
    ) -> TargetFacts {
        let baseline = test_target_facts();

        TargetFacts::new(
            baseline.identity().clone(),
            scalars,
            baseline.atomics(),
            abis.unwrap_or_else(|| baseline.abis()),
            baseline.address_spaces(),
            alignments,
            baseline.operations(),
        )
    }
}
