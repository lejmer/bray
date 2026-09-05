use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;
use bray_target::TargetIdentity;

use crate::role::canonical_role_bindings;
use crate::{
    ExecutionLaneRequirement, PanicAbiIdentity, ProtectedFrameAbiOperation,
    ProtectedFrameAbiVersions, RuntimeAbiRole, RuntimeAbiVersion, RuntimeArtifactId,
    RuntimeCapability, RuntimeIdentity, RuntimeRequirements, RuntimeRoleBinding,
    RuntimeRoleImplementation,
};

/// Selected target-specific execution-runtime implementation contract.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeContract {
    identity: RuntimeIdentity,
    artifact: RuntimeArtifactId,
    abi_version: RuntimeAbiVersion,
    frame_abi: ProtectedFrameAbiVersions,
    target: TargetIdentity,
    panic_abi: PanicAbiIdentity,
    capabilities: Arc<[RuntimeCapability]>,
    role_bindings: Arc<[RuntimeRoleBinding]>,
}

impl RuntimeContract {
    /// Creates a selected runtime contract after validating its published role surface.
    #[expect(
        clippy::too_many_arguments,
        reason = "the contract keeps each independent compatibility dimension explicit"
    )]
    pub fn try_new(
        identity: RuntimeIdentity,
        artifact: RuntimeArtifactId,
        abi_version: RuntimeAbiVersion,
        frame_abi: ProtectedFrameAbiVersions,
        target: TargetIdentity,
        panic_abi: PanicAbiIdentity,
        capabilities: impl IntoIterator<Item = RuntimeCapability>,
        role_bindings: impl IntoIterator<Item = RuntimeRoleBinding>,
    ) -> Result<Self, RuntimeContractBuildError> {
        let role_bindings = canonical_role_bindings(role_bindings)
            .map_err(RuntimeContractBuildError::DuplicateRole)?;

        if let Some(binding) = role_bindings.iter().find(|binding| {
            binding.implementation() == RuntimeRoleImplementation::CompilerLowering
                || binding.role().native_symbol().is_none()
        }) {
            return Err(RuntimeContractBuildError::CompilerOwnedRole(binding.role()));
        }

        let capabilities = sorted_unique_shared_slice(capabilities);

        if capabilities
            .binary_search(&RuntimeCapability::CooperativeExecution)
            .is_err()
        {
            return Err(RuntimeContractBuildError::MissingCooperativeExecution);
        }

        Ok(Self {
            identity,
            artifact,
            abi_version,
            frame_abi,
            target,
            panic_abi,
            capabilities,
            role_bindings,
        })
    }

    /// Validates that this runtime supplies one complete reachable requirement set.
    pub fn validate(
        &self,
        requirements: &RuntimeRequirements,
    ) -> Result<(), RuntimeCompatibilityError> {
        if let Some(required) = requirements.runtime()
            && required != &self.identity
        {
            return Err(RuntimeCompatibilityError::RuntimeIdentity);
        }

        if !self.abi_version.supports(requirements.abi_version()) {
            return Err(RuntimeCompatibilityError::RuntimeAbi);
        }

        if &self.target != requirements.target() {
            return Err(RuntimeCompatibilityError::Target);
        }

        if &self.panic_abi != requirements.panic_abi() {
            return Err(RuntimeCompatibilityError::PanicAbi);
        }

        if let Some(required) = requirements.frame_abi()
            && let Some(operation) = self.frame_abi.first_incompatible(required)
        {
            return Err(RuntimeCompatibilityError::FrameAbi(operation));
        }

        if let Some(capability) = requirements
            .capabilities()
            .iter()
            .find(|capability| self.capabilities.binary_search(capability).is_err())
        {
            return Err(RuntimeCompatibilityError::MissingCapability(*capability));
        }

        if let Some(capability) = requirements
            .lanes()
            .iter()
            .map(|lane| lane_capability(*lane))
            .find(|capability| self.capabilities.binary_search(capability).is_err())
        {
            return Err(RuntimeCompatibilityError::MissingCapability(capability));
        }

        if let Some(role) = requirements
            .roles()
            .iter()
            .find(|role| self.role_binding(**role).is_none())
        {
            return Err(RuntimeCompatibilityError::MissingRole(*role));
        }

        Ok(())
    }

    /// Returns the stable runtime implementation identity.
    pub const fn identity(&self) -> &RuntimeIdentity {
        &self.identity
    }

    /// Returns the separately linked runtime artifact identity.
    pub const fn artifact(&self) -> &RuntimeArtifactId {
        &self.artifact
    }

    /// Returns the provided private runtime ABI version.
    pub const fn abi_version(&self) -> RuntimeAbiVersion {
        self.abi_version
    }

    /// Returns the provided protected-frame operation versions.
    pub const fn frame_abi(&self) -> ProtectedFrameAbiVersions {
        self.frame_abi
    }

    /// Returns the exact supported target identity.
    pub const fn target(&self) -> &TargetIdentity {
        &self.target
    }

    /// Returns the exact supported panic ABI identity.
    pub const fn panic_abi(&self) -> &PanicAbiIdentity {
        &self.panic_abi
    }

    /// Returns published runtime capabilities in canonical order.
    pub fn capabilities(&self) -> &[RuntimeCapability] {
        &self.capabilities
    }

    /// Returns published role bindings in canonical role order.
    pub fn role_bindings(&self) -> &[RuntimeRoleBinding] {
        &self.role_bindings
    }

    /// Returns the published binding for one closed private ABI role.
    pub fn role_binding(&self, role: RuntimeAbiRole) -> Option<&RuntimeRoleBinding> {
        self.role_bindings
            .binary_search_by_key(&role, RuntimeRoleBinding::role)
            .ok()
            .map(|index| &self.role_bindings[index])
    }
}

pub(crate) const fn lane_capability(lane: ExecutionLaneRequirement) -> RuntimeCapability {
    match lane {
        ExecutionLaneRequirement::Blocking => RuntimeCapability::BlockingLanes,
        ExecutionLaneRequirement::Compute => RuntimeCapability::ComputeLanes,
        ExecutionLaneRequirement::MainThread => RuntimeCapability::MainThreadLane,
    }
}

/// A malformed runtime publication that cannot participate in product selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeContractBuildError {
    /// More than one binding was published for one closed ABI role.
    DuplicateRole(RuntimeAbiRole),
    /// The runtime publication claims a role owned by compiler-generated code.
    CompilerOwnedRole(RuntimeAbiRole),
    /// The runtime does not publish baseline cooperative execution.
    MissingCooperativeExecution,
}

/// Deterministic incompatibility between reachable requirements and a selected runtime.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum RuntimeCompatibilityError {
    /// Product policy requires another runtime implementation.
    RuntimeIdentity,
    /// The selected runtime ABI version cannot satisfy the required version.
    RuntimeAbi,
    /// The runtime was built for another target.
    Target,
    /// The runtime and product use different panic ABIs.
    PanicAbi,
    /// One protected-frame operation uses an incompatible ABI version.
    FrameAbi(ProtectedFrameAbiOperation),
    /// A reachable runtime capability is unavailable.
    MissingCapability(RuntimeCapability),
    /// A reachable private ABI role has no runtime binding.
    MissingRole(RuntimeAbiRole),
}

#[cfg(test)]
mod tests {
    use bray_target::TargetIdentity;

    use super::{RuntimeCompatibilityError, RuntimeContract, RuntimeContractBuildError};
    use crate::{
        BinarySymbolName, ExecutionLaneRequirement, PanicAbiIdentity, ProtectedFrameAbiVersions,
        RuntimeAbiRole, RuntimeAbiVersion, RuntimeArtifactId, RuntimeCapability, RuntimeIdentity,
        RuntimeRequirements, RuntimeRoleBinding, RuntimeRoleImplementation,
    };

    #[test]
    fn runtime_contracts_reject_duplicate_and_compiler_owned_roles() {
        assert_eq!(
            runtime([runtime_binding(RuntimeAbiRole::FrameResume)]),
            Err(RuntimeContractBuildError::CompilerOwnedRole(
                RuntimeAbiRole::FrameResume
            )),
        );

        let role = runtime_binding(RuntimeAbiRole::TaskStart);

        assert_eq!(
            runtime([role.clone(), role]),
            Err(RuntimeContractBuildError::DuplicateRole(
                RuntimeAbiRole::TaskStart
            ))
        );

        let compiler_owned = binding(
            RuntimeAbiRole::TaskStart,
            RuntimeRoleImplementation::CompilerLowering,
        );

        assert_eq!(
            runtime([compiler_owned]),
            Err(RuntimeContractBuildError::CompilerOwnedRole(
                RuntimeAbiRole::TaskStart
            ))
        );
    }

    #[test]
    fn compatibility_failures_are_typed_and_deterministic() {
        let Ok(runtime) = runtime([
            runtime_binding(RuntimeAbiRole::TaskStart),
            runtime_binding(RuntimeAbiRole::TaskAllocation),
        ]) else {
            panic!("test runtime contract must be valid");
        };

        let cases = [
            (
                requirements(
                    Some(runtime_identity("other.runtime")),
                    RuntimeAbiVersion::new(1, 0),
                    frame_abi(RuntimeAbiVersion::new(1, 0)),
                    target("x86_64-unknown-linux-gnu"),
                    panic_abi("bray.panic.test"),
                    [],
                    [],
                ),
                RuntimeCompatibilityError::RuntimeIdentity,
            ),
            (
                requirements(
                    None,
                    RuntimeAbiVersion::new(2, 0),
                    frame_abi(RuntimeAbiVersion::new(1, 0)),
                    target("x86_64-unknown-linux-gnu"),
                    panic_abi("bray.panic.test"),
                    [],
                    [],
                ),
                RuntimeCompatibilityError::RuntimeAbi,
            ),
            (
                requirements(
                    None,
                    RuntimeAbiVersion::new(1, 0),
                    frame_abi(RuntimeAbiVersion::new(1, 0)),
                    target("other-target"),
                    panic_abi("bray.panic.test"),
                    [],
                    [],
                ),
                RuntimeCompatibilityError::Target,
            ),
            (
                requirements(
                    None,
                    RuntimeAbiVersion::new(1, 0),
                    frame_abi(RuntimeAbiVersion::new(1, 0)),
                    target("x86_64-unknown-linux-gnu"),
                    panic_abi("other-panic"),
                    [],
                    [],
                ),
                RuntimeCompatibilityError::PanicAbi,
            ),
            (
                requirements(
                    None,
                    RuntimeAbiVersion::new(1, 0),
                    frame_abi(RuntimeAbiVersion::new(2, 0)),
                    target("x86_64-unknown-linux-gnu"),
                    panic_abi("bray.panic.test"),
                    [],
                    [],
                ),
                RuntimeCompatibilityError::FrameAbi(crate::ProtectedFrameAbiOperation::Resume),
            ),
            (
                requirements(
                    None,
                    RuntimeAbiVersion::new(1, 0),
                    frame_abi(RuntimeAbiVersion::new(1, 0)),
                    target("x86_64-unknown-linux-gnu"),
                    panic_abi("bray.panic.test"),
                    [],
                    [RuntimeCapability::Reactor],
                ),
                RuntimeCompatibilityError::MissingCapability(RuntimeCapability::Reactor),
            ),
            (
                requirements(
                    None,
                    RuntimeAbiVersion::new(1, 0),
                    frame_abi(RuntimeAbiVersion::new(1, 0)),
                    target("x86_64-unknown-linux-gnu"),
                    panic_abi("bray.panic.test"),
                    [RuntimeAbiRole::Wake],
                    [],
                ),
                RuntimeCompatibilityError::MissingRole(RuntimeAbiRole::Wake),
            ),
        ];

        for (requirements, expected) in cases {
            assert_eq!(runtime.validate(&requirements), Err(expected));
            assert_eq!(runtime.validate(&requirements), Err(expected));
        }

        let missing_lane = RuntimeRequirements::new(
            None,
            RuntimeAbiVersion::new(1, 0),
            None,
            target("x86_64-unknown-linux-gnu"),
            panic_abi("bray.panic.test"),
            [],
            [],
            [ExecutionLaneRequirement::Compute],
        );

        assert_eq!(
            runtime.validate(&missing_lane),
            Err(RuntimeCompatibilityError::MissingCapability(
                RuntimeCapability::ComputeLanes
            ))
        );
    }

    #[test]
    fn compatible_runtime_contracts_cover_roles_capabilities_and_target() {
        let Ok(runtime) = runtime([
            runtime_binding(RuntimeAbiRole::TaskAllocation),
            runtime_binding(RuntimeAbiRole::TaskStart),
        ]) else {
            panic!("test runtime contract must be valid");
        };

        let requirements = requirements(
            Some(runtime_identity("bray.runtime.test")),
            RuntimeAbiVersion::new(1, 0),
            frame_abi(RuntimeAbiVersion::new(1, 0)),
            target("x86_64-unknown-linux-gnu"),
            panic_abi("bray.panic.test"),
            [RuntimeAbiRole::TaskStart],
            [RuntimeCapability::CooperativeExecution],
        );

        assert_eq!(runtime.validate(&requirements), Ok(()));
    }

    fn runtime<const N: usize>(
        roles: [RuntimeRoleBinding; N],
    ) -> Result<RuntimeContract, RuntimeContractBuildError> {
        RuntimeContract::try_new(
            runtime_identity("bray.runtime.test"),
            runtime_artifact(),
            RuntimeAbiVersion::new(1, 1),
            ProtectedFrameAbiVersions::uniform(RuntimeAbiVersion::new(1, 0)),
            target("x86_64-unknown-linux-gnu"),
            panic_abi("bray.panic.test"),
            [RuntimeCapability::CooperativeExecution],
            roles,
        )
    }

    fn requirements<const R: usize, const C: usize>(
        runtime: Option<RuntimeIdentity>,
        version: RuntimeAbiVersion,
        frame_abi: ProtectedFrameAbiVersions,
        target: TargetIdentity,
        panic_abi: PanicAbiIdentity,
        roles: [RuntimeAbiRole; R],
        capabilities: [RuntimeCapability; C],
    ) -> RuntimeRequirements {
        RuntimeRequirements::new(
            runtime,
            version,
            Some(frame_abi),
            target,
            panic_abi,
            roles,
            capabilities,
            [],
        )
    }

    fn frame_abi(resume: RuntimeAbiVersion) -> ProtectedFrameAbiVersions {
        ProtectedFrameAbiVersions::new(
            resume,
            RuntimeAbiVersion::new(1, 0),
            RuntimeAbiVersion::new(1, 0),
            RuntimeAbiVersion::new(1, 0),
            RuntimeAbiVersion::new(1, 0),
        )
    }

    fn runtime_binding(role: RuntimeAbiRole) -> RuntimeRoleBinding {
        binding(role, RuntimeRoleImplementation::BrayRuntime)
    }

    fn binding(
        role: RuntimeAbiRole,
        implementation: RuntimeRoleImplementation,
    ) -> RuntimeRoleBinding {
        let Some(symbol) = BinarySymbolName::try_new(format!("role_{role:?}")) else {
            panic!("test ABI symbol must be valid");
        };

        RuntimeRoleBinding::new(role, symbol, implementation)
    }

    fn runtime_identity(value: &str) -> RuntimeIdentity {
        RuntimeIdentity::try_new(value)
            .unwrap_or_else(|| panic!("test runtime identity must be valid"))
    }

    fn runtime_artifact() -> RuntimeArtifactId {
        RuntimeArtifactId::try_new("bray.runtime.test.artifact")
            .unwrap_or_else(|| panic!("test runtime artifact must be valid"))
    }

    fn target(value: &str) -> TargetIdentity {
        TargetIdentity::try_new(value)
            .unwrap_or_else(|| panic!("test target identity must be valid"))
    }

    fn panic_abi(value: &str) -> PanicAbiIdentity {
        PanicAbiIdentity::try_new(value)
            .unwrap_or_else(|| panic!("test panic ABI identity must be valid"))
    }
}
