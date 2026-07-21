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
}
