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
    /// The maximum storage alignment cannot represent the target pointer alignment.
    StorageAlignmentBelowPointerAlignment,
    /// The maximum allocation alignment cannot represent the target pointer alignment.
    AllocationAlignmentBelowPointerAlignment,
}

impl std::fmt::Display for TargetProfileBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::StorageAlignmentBelowPointerAlignment => {
                formatter.write_str("maximum storage alignment is below pointer alignment")
            }
            Self::AllocationAlignmentBelowPointerAlignment => {
                formatter.write_str("maximum allocation alignment is below pointer alignment")
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

        if facts.alignments().max_storage().get() < pointer_alignment {
            return Err(TargetProfileBuildError::StorageAlignmentBelowPointerAlignment);
        }

        if facts.alignments().max_allocation().get() < pointer_alignment {
            return Err(TargetProfileBuildError::AllocationAlignmentBelowPointerAlignment);
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

#[cfg(test)]
mod tests {
    use std::num::NonZeroU64;

    use crate::test_support::{test_target_facts, test_target_machine, test_target_profile};
    use crate::{
        TargetAddressSpaceFacts, TargetAlignmentFacts, TargetAtomicFacts, TargetFacts,
        TargetIdentity, TargetIdentityFacts, TargetProfile, TargetProfileBuildError,
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
        );

        assert_eq!(
            TargetProfile::try_new(identity, test_target_machine(), facts),
            Err(TargetProfileBuildError::StorageAlignmentBelowPointerAlignment)
        );
    }
}
