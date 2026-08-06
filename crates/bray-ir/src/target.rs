use std::hash::{Hash, Hasher};

use bray_base::StableDigestHasher;
use bray_runtime_interface::RuntimeAbiVersion;
use bray_target::{TargetIdentity, TargetMachineProperties, TargetProfile};

/// Target facts that affect MIR representation and operation selection.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MirTargetFacts {
    profile: TargetProfile,
    runtime_abi: RuntimeAbiVersion,
}

impl MirTargetFacts {
    /// Creates the target contract for one MIR unit.
    pub const fn new(profile: TargetProfile, runtime_abi: RuntimeAbiVersion) -> Self {
        Self {
            profile,
            runtime_abi,
        }
    }

    /// Returns the exact compilation target identity.
    pub const fn identity(&self) -> &TargetIdentity {
        self.profile.identity()
    }

    /// Returns the target machine properties used by lowering.
    pub const fn machine(&self) -> &TargetMachineProperties {
        self.profile.machine()
    }

    /// Returns the selected language-level target profile.
    pub const fn profile(&self) -> &TargetProfile {
        &self.profile
    }

    /// Returns the selected private runtime ABI version.
    pub const fn runtime_abi(&self) -> RuntimeAbiVersion {
        self.runtime_abi
    }

    /// Returns a stable identity for every target fact that can affect MIR.
    pub fn compatibility_digest(&self) -> [u8; 32] {
        let mut digest = StableDigestHasher::new();

        digest.write(b"bray.mir-target-facts.v1");
        self.profile.hash(&mut digest);
        self.runtime_abi.hash(&mut digest);

        digest.finalize()
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_interface::RuntimeAbiVersion;

    use super::MirTargetFacts;

    #[test]
    fn compatibility_identity_covers_target_profile_and_runtime_abi() {
        let profile = bray_target::test_support::test_target_profile();
        let baseline = MirTargetFacts::new(profile.clone(), RuntimeAbiVersion::new(1, 0));
        let different_abi = MirTargetFacts::new(profile, RuntimeAbiVersion::new(1, 1));

        let different_target = MirTargetFacts::new(
            bray_target::NativeTarget::X86_64WindowsMsvc.profile(),
            RuntimeAbiVersion::new(1, 0),
        );

        assert_ne!(
            baseline.compatibility_digest(),
            different_abi.compatibility_digest()
        );

        assert_ne!(
            baseline.compatibility_digest(),
            different_target.compatibility_digest()
        );
    }
}
