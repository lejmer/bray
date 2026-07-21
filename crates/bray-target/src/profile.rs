use crate::{TargetIdentity, TargetMachineProperties};

/// The language-level identity and machine properties of one compilation target.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetProfile {
    identity: TargetIdentity,
    machine: TargetMachineProperties,
}

impl TargetProfile {
    /// Creates a target profile from its validated identity and machine properties.
    pub const fn new(identity: TargetIdentity, machine: TargetMachineProperties) -> Self {
        Self { identity, machine }
    }

    /// Returns the stable identity of this target profile.
    pub const fn identity(&self) -> &TargetIdentity {
        &self.identity
    }

    /// Returns the target's validated machine properties.
    pub const fn machine(&self) -> &TargetMachineProperties {
        &self.machine
    }
}

#[cfg(test)]
mod tests {
    use crate::test_support::test_target_profile;

    #[test]
    fn profiles_keep_identity_and_machine_properties_together() {
        let profile = test_target_profile();

        assert_eq!(profile.identity().as_str(), "x86_64-unknown-linux-gnu");
        assert_eq!(profile.machine().pointer_width_bits().get(), 64);
    }
}
