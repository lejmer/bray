use crate::{
    TargetIdentity, TargetMachineProperties, TargetProperties, TargetPropertyKind,
    TargetPropertyValue, TargetScalarKind,
};

/// The language-level identity and machine properties of one compilation target.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetProfile {
    identity: TargetIdentity,
    machine: TargetMachineProperties,
    properties: TargetProperties,
}

/// A contradiction between properties supplied for one target profile.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TargetProfileBuildError {
    /// A target-sized integer cannot represent one public `usize` target property.
    TargetPropertyNotRepresentable,
    /// The maximum storage alignment cannot represent the target pointer alignment.
    StorageAlignmentBelowPointerAlignment,
    /// The maximum storage alignment cannot represent the target stack alignment.
    StorageAlignmentBelowStackAlignment,
    /// The maximum allocation alignment cannot represent the target pointer alignment.
    AllocationAlignmentBelowPointerAlignment,
    /// A complex scalar is available while its real component is unavailable.
    ComplexScalarMissingComponent,
    /// A scalar alignment exceeds the target storage maximum.
    ScalarAlignmentAboveStorageMaximum,
    /// An atomic representation alignment exceeds the target storage maximum.
    AtomicAlignmentAboveStorageMaximum,
    /// A callable ABI accepts a scalar unavailable on the target.
    AbiAcceptsUnavailableScalar,
    /// A C scalar maps to a Bray scalar unavailable on the target.
    CAbiMapsUnavailableScalar,
    /// C scalar mappings exist without an available C callable ABI.
    CScalarMappingWithoutCAbi,
    /// A C scalar mapping cannot use the target's C by-value transparent-wrapper contract.
    CAbiRejectsMappedScalar,
    /// A callable ABI accepts an alignment above the target storage maximum.
    AbiAlignmentAboveStorageMaximum,
}

impl std::fmt::Display for TargetProfileBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TargetPropertyNotRepresentable => {
                formatter.write_str("a target usize property is not representable")
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
            Self::ScalarAlignmentAboveStorageMaximum => {
                formatter.write_str("a scalar alignment exceeds the storage maximum")
            }
            Self::AtomicAlignmentAboveStorageMaximum => {
                formatter.write_str("an atomic alignment exceeds the storage maximum")
            }
            Self::AbiAcceptsUnavailableScalar => {
                formatter.write_str("a callable ABI accepts an unavailable scalar")
            }
            Self::CAbiMapsUnavailableScalar => {
                formatter.write_str("the C ABI maps to an unavailable scalar")
            }
            Self::CScalarMappingWithoutCAbi => {
                formatter.write_str("C scalar mappings exist without a C callable ABI")
            }
            Self::CAbiRejectsMappedScalar => formatter.write_str(
                "a C scalar mapping is incompatible with the C ABI transparent-wrapper contract",
            ),
            Self::AbiAlignmentAboveStorageMaximum => {
                formatter.write_str("a callable ABI alignment exceeds the storage maximum")
            }
        }
    }
}

impl std::error::Error for TargetProfileBuildError {}

impl TargetProfile {
    /// Creates a target profile when its complete property groups are mutually consistent.
    pub fn try_new(
        identity: TargetIdentity,
        machine: TargetMachineProperties,
        properties: TargetProperties,
    ) -> Result<Self, TargetProfileBuildError> {
        let pointer_alignment = u64::from(machine.pointer_alignment_bytes().get());
        let stack_alignment = u64::from(machine.stack_alignment_bytes().get());
        let maximum_usize = maximum_usize(machine.pointer_width_bits().get());
        let alignments = properties.alignments();

        if alignments.max_storage().get() > maximum_usize
            || alignments.max_allocation().get() > maximum_usize
        {
            return Err(TargetProfileBuildError::TargetPropertyNotRepresentable);
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

        let scalars = properties.scalars();

        if (scalars.complex32() && !scalars.real16())
            || (scalars.complex256() && !scalars.real128())
        {
            return Err(TargetProfileBuildError::ComplexScalarMissingComponent);
        }

        if TargetScalarKind::ALL
            .into_iter()
            .any(|kind| scalars.alignment(kind).get() > alignments.max_storage().get())
        {
            return Err(TargetProfileBuildError::ScalarAlignmentAboveStorageMaximum);
        }

        if crate::TargetAtomicRepresentation::ALL
            .into_iter()
            .any(|representation| {
                properties
                    .atomics()
                    .representation(representation)
                    .required_alignment()
                    .get()
                    > alignments.max_storage().get()
            })
        {
            return Err(TargetProfileBuildError::AtomicAlignmentAboveStorageMaximum);
        }

        let abis = properties.abis();

        if !abis.is_supported_by(scalars) {
            return Err(TargetProfileBuildError::AbiAcceptsUnavailableScalar);
        }

        if !properties.c_abi().is_supported_by(scalars) {
            return Err(TargetProfileBuildError::CAbiMapsUnavailableScalar);
        }

        match abis.c_contract() {
            Some(contract) if !properties.c_abi().is_supported_by_c_abi(contract) => {
                return Err(TargetProfileBuildError::CAbiRejectsMappedScalar);
            }
            None if !properties.c_abi().is_empty() => {
                return Err(TargetProfileBuildError::CScalarMappingWithoutCAbi);
            }
            Some(_) | None => {}
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
            properties,
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

    /// Returns the language-defined target properties not derived from machine properties.
    pub const fn properties(&self) -> &TargetProperties {
        &self.properties
    }

    /// Returns one language-defined target property.
    pub fn property(&self, kind: TargetPropertyKind) -> TargetPropertyValue<'_> {
        TargetPropertyValue::for_profile(self, kind)
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

    use crate::test_support::{test_target_machine, test_target_profile, test_target_properties};
    use crate::{
        Endianness, ObjectFormat, TargetAbiScalars, TargetAbiSupport, TargetAddressSpaces,
        TargetAlignmentLimits, TargetArchitecture, TargetAtomicSupport, TargetCDataModel,
        TargetCScalarKind, TargetForeignAbiContract, TargetIdentity, TargetMachineProperties,
        TargetOperationSupport, TargetPlatformIdentity, TargetProfile, TargetProfileBuildError,
        TargetProperties, TargetScalarKind, TargetScalarSupport,
    };

    #[test]
    fn profiles_keep_identity_and_machine_properties_together() {
        let profile = test_target_profile();

        assert_eq!(profile.identity().as_str(), "x86_64-unknown-linux-gnu");
        assert_eq!(profile.machine().pointer_width_bits().get(), 64);

        assert_eq!(
            profile.property(crate::TargetPropertyKind::PointerBytes),
            crate::TargetPropertyValue::Usize(8)
        );
    }

    #[test]
    fn profiles_reject_alignment_properties_that_contradict_machine_properties() {
        let Some(identity) = TargetIdentity::try_new("x86_64-unknown-linux-gnu") else {
            panic!("test target identity must be valid");
        };

        let Some(platform_identity) =
            TargetPlatformIdentity::try_new("unknown", "linux", "gnu", "gnu")
        else {
            panic!("test target platform identity must be valid");
        };

        let maximum = NonZeroU64::new(4).unwrap_or(NonZeroU64::MIN);

        let Some(alignments) = TargetAlignmentLimits::try_new(maximum, maximum) else {
            panic!("test alignment limits must be internally valid");
        };

        let Some(address_spaces) = TargetAddressSpaces::try_new(true, false) else {
            panic!("test target must expose one address space");
        };

        let baseline = test_target_properties();

        let properties = TargetProperties::new(
            platform_identity,
            baseline.scalars(),
            TargetAtomicSupport::default(),
            baseline.abis(),
            baseline.c_abi(),
            address_spaces,
            alignments,
            TargetOperationSupport::default(),
        );

        assert_eq!(
            TargetProfile::try_new(identity, test_target_machine(), properties),
            Err(TargetProfileBuildError::StorageAlignmentBelowPointerAlignment)
        );
    }

    #[test]
    fn profiles_reject_storage_maxima_below_stack_alignment() {
        let machine = test_target_machine();
        let maximum = NonZeroU64::new(8).unwrap_or(NonZeroU64::MIN);

        let alignments = TargetAlignmentLimits::try_new(maximum, maximum)
            .unwrap_or_else(|| panic!("test alignments must be valid"));

        let properties = properties_with(TargetScalarSupport::default(), None, alignments);

        assert_eq!(
            TargetProfile::try_new(test_identity(), machine, properties),
            Err(TargetProfileBuildError::StorageAlignmentBelowStackAlignment)
        );
    }

    #[test]
    fn profiles_reject_usize_properties_that_the_target_cannot_represent() {
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

        let alignments = TargetAlignmentLimits::try_new(maximum, maximum)
            .unwrap_or_else(|| panic!("test alignments must be valid"));

        let properties = properties_with(TargetScalarSupport::default(), None, alignments);

        assert_eq!(
            TargetProfile::try_new(test_identity(), machine, properties),
            Err(TargetProfileBuildError::TargetPropertyNotRepresentable)
        );
    }

    #[test]
    fn profiles_reject_scalar_and_abi_cross_group_contradictions() {
        let maximum = NonZeroU64::new(1 << 29).unwrap_or(NonZeroU64::MIN);

        let alignments = TargetAlignmentLimits::try_new(maximum, maximum)
            .unwrap_or_else(|| panic!("test alignments must be valid"));

        let inconsistent_scalars = TargetScalarSupport::new(false, false, true, false);
        let properties = properties_with(inconsistent_scalars, None, alignments);

        assert_eq!(
            TargetProfile::try_new(test_identity(), test_target_machine(), properties),
            Err(TargetProfileBuildError::ComplexScalarMissingComponent)
        );

        let abi = TargetForeignAbiContract::new(
            TargetAbiScalars::all(),
            true,
            true,
            true,
            true,
            true,
            maximum,
        );

        let properties = properties_with(
            TargetScalarSupport::default(),
            Some(TargetAbiSupport::new(Some(abi), None)),
            alignments,
        );

        assert_eq!(
            TargetProfile::try_new(test_identity(), test_target_machine(), properties),
            Err(TargetProfileBuildError::AbiAcceptsUnavailableScalar)
        );
    }

    #[test]
    fn profiles_reject_abi_alignment_above_storage_maximum() {
        let storage_maximum = NonZeroU64::new(16).unwrap_or(NonZeroU64::MIN);
        let abi_maximum = NonZeroU64::new(32).unwrap_or(NonZeroU64::MIN);

        let alignments = TargetAlignmentLimits::try_new(storage_maximum, storage_maximum)
            .unwrap_or_else(|| panic!("test alignments must be valid"));

        let abi = TargetForeignAbiContract::new(
            TargetAbiScalars::required(),
            true,
            true,
            true,
            true,
            true,
            abi_maximum,
        );

        let properties = properties_with(
            TargetScalarSupport::default(),
            Some(TargetAbiSupport::new(Some(abi), None)),
            alignments,
        );

        assert_eq!(
            TargetProfile::try_new(test_identity(), test_target_machine(), properties),
            Err(TargetProfileBuildError::AbiAlignmentAboveStorageMaximum)
        );
    }

    #[test]
    fn profiles_require_mapped_c_scalars_to_match_the_c_callable_abi() {
        let baseline = test_target_properties();
        let maximum = baseline.alignments().max_storage();
        let no_c_abi = TargetAbiSupport::new(None, baseline.abis().system_contract());

        let properties = TargetProperties::new(
            baseline.identity().clone(),
            TargetScalarSupport::new(false, true, false, false),
            baseline.atomics(),
            no_c_abi,
            baseline.c_abi(),
            baseline.address_spaces(),
            baseline.alignments(),
            baseline.operations(),
        );

        assert_eq!(
            TargetProfile::try_new(test_identity(), test_target_machine(), properties),
            Err(TargetProfileBuildError::CScalarMappingWithoutCAbi)
        );

        let c_abi =
            TargetCDataModel::try_new(&[(TargetCScalarKind::LongDouble, TargetScalarKind::R128)])
                .unwrap_or_else(|| panic!("test C scalar mapping must be valid"));

        let contract = TargetForeignAbiContract::new(
            TargetAbiScalars::required(),
            true,
            true,
            true,
            true,
            true,
            maximum,
        );

        let properties = TargetProperties::new(
            baseline.identity().clone(),
            TargetScalarSupport::new(false, true, false, false),
            baseline.atomics(),
            TargetAbiSupport::new(Some(contract), baseline.abis().system_contract()),
            c_abi,
            baseline.address_spaces(),
            baseline.alignments(),
            baseline.operations(),
        );

        assert_eq!(
            TargetProfile::try_new(test_identity(), test_target_machine(), properties),
            Err(TargetProfileBuildError::CAbiRejectsMappedScalar)
        );

        let c_abi = TargetCDataModel::try_new(&[(TargetCScalarKind::Int, TargetScalarKind::I32)])
            .unwrap_or_else(|| panic!("test C scalar mapping must be valid"));

        let contract = TargetForeignAbiContract::new(
            TargetAbiScalars::required(),
            true,
            true,
            true,
            false,
            true,
            maximum,
        );

        let properties = TargetProperties::new(
            baseline.identity().clone(),
            baseline.scalars(),
            baseline.atomics(),
            TargetAbiSupport::new(Some(contract), baseline.abis().system_contract()),
            c_abi,
            baseline.address_spaces(),
            baseline.alignments(),
            baseline.operations(),
        );

        assert_eq!(
            TargetProfile::try_new(test_identity(), test_target_machine(), properties),
            Err(TargetProfileBuildError::CAbiRejectsMappedScalar)
        );

        let c_abi = TargetCDataModel::try_new(&[])
            .unwrap_or_else(|| panic!("empty C scalar mapping must be valid"));

        let properties = TargetProperties::new(
            baseline.identity().clone(),
            baseline.scalars(),
            baseline.atomics(),
            TargetAbiSupport::new(Some(contract), baseline.abis().system_contract()),
            c_abi,
            baseline.address_spaces(),
            baseline.alignments(),
            baseline.operations(),
        );

        assert!(TargetProfile::try_new(test_identity(), test_target_machine(), properties).is_ok());
    }

    #[test]
    fn profiles_reject_scalar_alignment_above_storage_maximum() {
        let storage_maximum = NonZeroU64::new(16).unwrap_or(NonZeroU64::MIN);
        let scalar_alignment = NonZeroU64::new(32).unwrap_or(NonZeroU64::MIN);

        let alignments = TargetAlignmentLimits::try_new(storage_maximum, storage_maximum)
            .unwrap_or_else(|| panic!("test alignments must be valid"));

        let scalars = TargetScalarSupport::default()
            .try_with_alignment(TargetScalarKind::I128, scalar_alignment)
            .unwrap_or_else(|| panic!("test scalar alignment must be valid"));

        let properties = properties_with(scalars, None, alignments);

        assert_eq!(
            TargetProfile::try_new(test_identity(), test_target_machine(), properties),
            Err(TargetProfileBuildError::ScalarAlignmentAboveStorageMaximum)
        );
    }

    fn test_identity() -> TargetIdentity {
        TargetIdentity::try_new("x86_64-unknown-linux-gnu")
            .unwrap_or_else(|| panic!("test target identity must be valid"))
    }

    fn properties_with(
        scalars: TargetScalarSupport,
        abis: Option<TargetAbiSupport>,
        alignments: TargetAlignmentLimits,
    ) -> TargetProperties {
        let baseline = test_target_properties();

        TargetProperties::new(
            baseline.identity().clone(),
            scalars,
            baseline.atomics(),
            abis.unwrap_or_else(|| baseline.abis()),
            baseline.c_abi(),
            baseline.address_spaces(),
            alignments,
            baseline.operations(),
        )
    }
}
