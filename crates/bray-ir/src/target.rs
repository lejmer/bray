use bray_target::{TargetIdentity, TargetMachineProperties};

/// Target facts that affect MIR representation and operation selection.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MirTargetFacts {
    identity: TargetIdentity,
    machine: TargetMachineProperties,
}

impl MirTargetFacts {
    /// Creates the target contract for one MIR unit.
    pub const fn new(identity: TargetIdentity, machine: TargetMachineProperties) -> Self {
        Self { identity, machine }
    }

    /// Returns the exact compilation target identity.
    pub const fn identity(&self) -> &TargetIdentity {
        &self.identity
    }

    /// Returns the target machine properties used by lowering.
    pub const fn machine(&self) -> &TargetMachineProperties {
        &self.machine
    }
}
