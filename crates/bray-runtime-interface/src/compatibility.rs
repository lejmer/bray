use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;
use bray_target::TargetIdentity;

use crate::{
    ExecutionLaneRequirement, PanicAbiIdentity, ProtectedFrameAbiOperation,
    ProtectedFrameAbiVersions, RuntimeAbiRole, RuntimeAbiVersion, RuntimeCapability,
    RuntimeIdentity,
};

/// Runtime facilities and compatibility requirements for reachable code.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuntimeRequirements {
    runtime: Option<RuntimeIdentity>,
    abi_version: RuntimeAbiVersion,
    frame_abi: Option<ProtectedFrameAbiVersions>,
    target: TargetIdentity,
    panic_abi: PanicAbiIdentity,
    roles: Arc<[RuntimeAbiRole]>,
    capabilities: Arc<[RuntimeCapability]>,
    lanes: Arc<[ExecutionLaneRequirement]>,
}
impl RuntimeRequirements {
    /// Creates canonical requirements for one target-specific product or library surface.
    #[expect(
        clippy::too_many_arguments,
        reason = "the contract keeps each independent compatibility dimension explicit"
    )]
    pub fn new(
        runtime: Option<RuntimeIdentity>,
        abi_version: RuntimeAbiVersion,
        frame_abi: Option<ProtectedFrameAbiVersions>,
        target: TargetIdentity,
        panic_abi: PanicAbiIdentity,
        roles: impl IntoIterator<Item = RuntimeAbiRole>,
        capabilities: impl IntoIterator<Item = RuntimeCapability>,
        lanes: impl IntoIterator<Item = ExecutionLaneRequirement>,
    ) -> Self {
        Self {
            runtime,
            abi_version,
            frame_abi,
            target,
            panic_abi,
            roles: sorted_unique_shared_slice(roles),
            capabilities: sorted_unique_shared_slice(capabilities),
            lanes: sorted_unique_shared_slice(lanes),
        }
    }

    /// Merges reachable requirement sets when their compatibility contracts agree.
    pub fn try_merge(
        requirements: impl IntoIterator<Item = Self>,
    ) -> Result<Option<Self>, RuntimeRequirementsMergeError> {
        let mut requirements: Vec<_> = requirements.into_iter().collect();

        requirements.sort_unstable();

        let mut requirements = requirements.into_iter();

        let Some(mut merged) = requirements.next() else {
            return Ok(None);
        };

        for requirement in requirements {
            merged.merge(requirement)?;
        }

        Ok(Some(merged))
    }

    /// Returns the required runtime identity when product policy selects one exactly.
    pub const fn runtime(&self) -> Option<&RuntimeIdentity> {
        self.runtime.as_ref()
    }

    /// Returns the minimum compatible private runtime ABI version.
    pub const fn abi_version(&self) -> RuntimeAbiVersion {
        self.abi_version
    }

    /// Returns protected-frame operation requirements when reachable code has a frame.
    pub const fn frame_abi(&self) -> Option<ProtectedFrameAbiVersions> {
        self.frame_abi
    }

    /// Returns the exact target identity.
    pub const fn target(&self) -> &TargetIdentity {
        &self.target
    }

    /// Returns the exact target panic ABI identity.
    pub const fn panic_abi(&self) -> &PanicAbiIdentity {
        &self.panic_abi
    }

    /// Returns required private ABI roles in canonical order.
    pub fn roles(&self) -> &[RuntimeAbiRole] {
        &self.roles
    }

    /// Returns whether the product requires one private runtime ABI role.
    pub fn requires_role(&self, role: RuntimeAbiRole) -> bool {
        self.roles.contains(&role)
    }

    /// Returns required runtime capabilities in canonical order.
    pub fn capabilities(&self) -> &[RuntimeCapability] {
        &self.capabilities
    }

    /// Returns reachable lane requirements in canonical order.
    pub fn lanes(&self) -> &[ExecutionLaneRequirement] {
        &self.lanes
    }

    /// Returns whether reachable behavior requires a linked runtime implementation.
    pub fn requires_implementation(&self) -> bool {
        self.runtime.is_some()
            || self.frame_abi.is_some()
            || !self.roles.is_empty()
            || !self.capabilities.is_empty()
            || !self.lanes.is_empty()
    }

    fn merge(&mut self, other: Self) -> Result<(), RuntimeRequirementsMergeError> {
        self.runtime = merge_optional_exact(
            self.runtime.take(),
            other.runtime,
            RuntimeRequirementsMergeError::RuntimeIdentityMismatch,
        )?;

        self.abi_version = merge_version(
            self.abi_version,
            other.abi_version,
            RuntimeRequirementsMergeError::RuntimeAbiMismatch,
        )?;

        self.frame_abi = match (self.frame_abi, other.frame_abi) {
            (Some(left), Some(right)) => Some(
                left.merge(right)
                    .map_err(RuntimeRequirementsMergeError::FrameAbiMismatch)?,
            ),
            (left @ Some(_), None) | (None, left @ Some(_)) => left,
            (None, None) => None,
        };

        if self.target != other.target {
            return Err(RuntimeRequirementsMergeError::TargetMismatch);
        }

        if self.panic_abi != other.panic_abi {
            return Err(RuntimeRequirementsMergeError::PanicAbiMismatch);
        }

        self.roles = sorted_unique_shared_slice(
            self.roles
                .iter()
                .copied()
                .chain(other.roles.iter().copied()),
        );

        self.capabilities = sorted_unique_shared_slice(
            self.capabilities
                .iter()
                .copied()
                .chain(other.capabilities.iter().copied()),
        );

        self.lanes = sorted_unique_shared_slice(
            self.lanes
                .iter()
                .copied()
                .chain(other.lanes.iter().copied()),
        );

        Ok(())
    }
}

/// A contradiction between independently collected runtime requirements.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RuntimeRequirementsMergeError {
    /// Two requirements select different exact runtime implementations.
    RuntimeIdentityMismatch,
    /// Required private runtime ABI versions have incompatible major versions.
    RuntimeAbiMismatch,
    /// One protected-frame operation has incompatible major versions.
    FrameAbiMismatch(ProtectedFrameAbiOperation),
    /// Requirements were produced for different compilation targets.
    TargetMismatch,
    /// Requirements select different panic ABIs.
    PanicAbiMismatch,
}

fn merge_optional_exact<T: Eq>(
    left: Option<T>,
    right: Option<T>,
    error: RuntimeRequirementsMergeError,
) -> Result<Option<T>, RuntimeRequirementsMergeError> {
    match (left, right) {
        (Some(left), Some(right)) if left != right => Err(error),
        (Some(left), Some(_)) | (Some(left), None) => Ok(Some(left)),
        (None, right) => Ok(right),
    }
}

fn merge_version<E>(
    left: RuntimeAbiVersion,
    right: RuntimeAbiVersion,
    error: E,
) -> Result<RuntimeAbiVersion, E> {
    if left.major() != right.major() {
        return Err(error);
    }

    Ok(RuntimeAbiVersion::new(
        left.major(),
        left.minor().max(right.minor()),
    ))
}

#[cfg(test)]
mod tests {
    use bray_target::TargetIdentity;

    use super::{
        ProtectedFrameAbiOperation, ProtectedFrameAbiVersions, RuntimeRequirements,
        RuntimeRequirementsMergeError,
    };
    use crate::{
        PanicAbiIdentity, RuntimeAbiRole, RuntimeAbiVersion, RuntimeCapability, RuntimeIdentity,
    };

    #[test]
    fn requirement_merging_is_canonical_and_keeps_strongest_compatible_versions() {
        let first = requirements(
            RuntimeAbiVersion::new(1, 0),
            [RuntimeAbiRole::TaskStart],
            [RuntimeCapability::CooperativeExecution],
        );

        let second = requirements(
            RuntimeAbiVersion::new(1, 2),
            [RuntimeAbiRole::FrameResume],
            [RuntimeCapability::Reactor],
        );

        let Ok(Some(merged)) = RuntimeRequirements::try_merge([first, second]) else {
            panic!("compatible runtime requirements must merge");
        };

        assert_eq!(merged.abi_version(), RuntimeAbiVersion::new(1, 2));

        assert_eq!(
            merged.roles(),
            [RuntimeAbiRole::TaskStart, RuntimeAbiRole::FrameResume]
        );

        assert!(merged.requires_role(RuntimeAbiRole::TaskStart));
        assert!(!merged.requires_role(RuntimeAbiRole::Wake));

        assert_eq!(
            merged.capabilities(),
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::Reactor
            ]
        );
    }

    #[test]
    fn requirement_merging_rejects_each_exact_compatibility_mismatch() {
        let baseline = requirements(RuntimeAbiVersion::new(1, 0), [], []);
        let incompatible = requirements(RuntimeAbiVersion::new(2, 0), [], []);

        assert_eq!(
            RuntimeRequirements::try_merge([baseline, incompatible]),
            Err(RuntimeRequirementsMergeError::RuntimeAbiMismatch)
        );

        let baseline = requirements(RuntimeAbiVersion::new(1, 0), [], []);
        let mut incompatible = requirements(RuntimeAbiVersion::new(1, 0), [], []);

        incompatible.frame_abi = Some(ProtectedFrameAbiVersions::new(
            RuntimeAbiVersion::new(2, 0),
            RuntimeAbiVersion::new(1, 0),
            RuntimeAbiVersion::new(1, 0),
            RuntimeAbiVersion::new(1, 0),
            RuntimeAbiVersion::new(1, 0),
        ));

        assert_eq!(
            RuntimeRequirements::try_merge([baseline, incompatible]),
            Err(RuntimeRequirementsMergeError::FrameAbiMismatch(
                ProtectedFrameAbiOperation::Resume
            ))
        );
    }

    #[test]
    fn requirement_merge_failures_do_not_depend_on_discovery_order() {
        let baseline = requirements(RuntimeAbiVersion::new(1, 0), [], []);
        let incompatible_version = requirements(RuntimeAbiVersion::new(2, 0), [], []);
        let mut incompatible_target = requirements(RuntimeAbiVersion::new(1, 0), [], []);

        incompatible_target.target = TargetIdentity::try_new("aarch64-unknown-linux-gnu")
            .unwrap_or_else(|| panic!("test target identity must be valid"));

        let requirements = [baseline, incompatible_version, incompatible_target];

        // The shallow clone lets both orders use the exact same immutable requirement set.
        let forward = RuntimeRequirements::try_merge(requirements.clone());
        let reverse = RuntimeRequirements::try_merge(requirements.into_iter().rev());

        assert_eq!(forward, reverse);
    }

    fn requirements<const R: usize, const C: usize>(
        version: RuntimeAbiVersion,
        roles: [RuntimeAbiRole; R],
        capabilities: [RuntimeCapability; C],
    ) -> RuntimeRequirements {
        let Some(runtime) = RuntimeIdentity::try_new("bray.runtime.test") else {
            panic!("test runtime identity must be valid");
        };

        let Some(target) = TargetIdentity::try_new("x86_64-unknown-linux-gnu") else {
            panic!("test target identity must be valid");
        };

        let Some(panic_abi) = PanicAbiIdentity::try_new("bray.panic.test") else {
            panic!("test panic ABI identity must be valid");
        };

        RuntimeRequirements::new(
            Some(runtime),
            version,
            Some(ProtectedFrameAbiVersions::uniform(RuntimeAbiVersion::new(
                1, 0,
            ))),
            target,
            panic_abi,
            roles,
            capabilities,
            [],
        )
    }
}
