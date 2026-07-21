use std::num::NonZeroU32;
use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;
use bray_symbols::ProductIdentity;

use crate::{
    BinarySymbolName, ProtectedAsyncFrameId, RuntimeAbiRole, RuntimeAbiVersion, RuntimeArtifactId,
    RuntimeRoleBinding, RuntimeRoleImplementation,
};

/// Execution-lane predicate that reachable code requires the product to satisfy.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutionLaneRequirement {
    /// Execution may perform blocking work.
    Blocking,
    /// Execution may perform CPU-bound compute work.
    Compute,
    /// Execution must occur on the distinguished initial process thread.
    MainThread,
}

/// Runtime facility required by reachable product code.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuntimeFeature {
    /// Baseline cooperative task execution.
    CooperativeExecution,
    /// Thread-local task lanes.
    LocalLanes,
    /// Task lanes that may migrate between compatible worker threads.
    MigratableLanes,
    /// Blocking execution lanes.
    BlockingLanes,
    /// CPU-bound compute execution lanes.
    ComputeLanes,
    /// Distinguished main-thread execution lane.
    MainThreadLane,
    /// Reactor-backed external event integration.
    Reactor,
    /// Product-host cleanup-incident reporting.
    CleanupIncidentReporting,
}

/// Product-wide hard execution capacity selected independently of library budgets.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecutionCapacityLimits {
    tasks: Option<NonZeroU32>,
    threads: Option<NonZeroU32>,
    processes: Option<NonZeroU32>,
}

impl ExecutionCapacityLimits {
    /// Creates hard task, native-thread, and child-process capacity limits.
    pub const fn new(
        tasks: Option<NonZeroU32>,
        threads: Option<NonZeroU32>,
        processes: Option<NonZeroU32>,
    ) -> Self {
        Self {
            tasks,
            threads,
            processes,
        }
    }

    /// Returns the simultaneous task capacity, when constrained.
    pub const fn tasks(self) -> Option<NonZeroU32> {
        self.tasks
    }

    /// Returns the native-thread creation capacity, when constrained.
    pub const fn threads(self) -> Option<NonZeroU32> {
        self.threads
    }

    /// Returns the child-process creation capacity, when constrained.
    pub const fn processes(self) -> Option<NonZeroU32> {
        self.processes
    }
}

/// How the compiler-generated host enters the source product root.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RootExecution {
    /// Execute the source entry body directly as the root run.
    Synchronous,
    /// Transfer the protected entry frame into a host-owned root task.
    Asynchronous {
        /// Protected representation of the async entry callable.
        frame: ProtectedAsyncFrameId,
    },
}

/// Complete selected execution contract for one compiler-generated executable host stub.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ExecutableHostContract {
    product: ProductIdentity,
    native_entry: BinarySymbolName,
    root: RootExecution,
    abi_version: RuntimeAbiVersion,
    runtime_artifact: Option<RuntimeArtifactId>,
    role_bindings: Arc<[RuntimeRoleBinding]>,
    runtime_features: Arc<[RuntimeFeature]>,
    lane_requirements: Arc<[ExecutionLaneRequirement]>,
    capacity_limits: ExecutionCapacityLimits,
}

/// Builder for one compiler-generated executable-host contract.
#[derive(Debug)]
pub struct ExecutableHostContractBuilder {
    product: ProductIdentity,
    native_entry: BinarySymbolName,
    root: RootExecution,
    abi_version: RuntimeAbiVersion,
    runtime_artifact: Option<RuntimeArtifactId>,
    role_bindings: Vec<RuntimeRoleBinding>,
    runtime_features: Vec<RuntimeFeature>,
    lane_requirements: Vec<ExecutionLaneRequirement>,
    capacity_limits: ExecutionCapacityLimits,
}

impl ExecutableHostContractBuilder {
    /// Starts a host contract from its product, native entry, root mode, and ABI version.
    pub const fn new(
        product: ProductIdentity,
        native_entry: BinarySymbolName,
        root: RootExecution,
        abi_version: RuntimeAbiVersion,
    ) -> Self {
        Self {
            product,
            native_entry,
            root,
            abi_version,
            runtime_artifact: None,
            role_bindings: Vec::new(),
            runtime_features: Vec::new(),
            lane_requirements: Vec::new(),
            capacity_limits: ExecutionCapacityLimits::new(None, None, None),
        }
    }

    /// Selects the separately linked async-runtime artifact.
    pub fn select_runtime(&mut self, runtime: RuntimeArtifactId) {
        self.runtime_artifact = Some(runtime);
    }

    /// Adds one exact private ABI role binding.
    pub fn push_role_binding(&mut self, binding: RuntimeRoleBinding) {
        self.role_bindings.push(binding);
    }

    /// Adds one required runtime facility.
    pub fn require_runtime_feature(&mut self, feature: RuntimeFeature) {
        self.runtime_features.push(feature);
    }

    /// Adds one reachable execution-lane requirement.
    pub fn require_lane(&mut self, lane: ExecutionLaneRequirement) {
        self.lane_requirements.push(lane);
    }

    /// Replaces product-wide hard execution capacity limits.
    pub fn set_capacity_limits(&mut self, limits: ExecutionCapacityLimits) {
        self.capacity_limits = limits;
    }

    /// Completes the host contract after validating selected runtime coverage.
    pub fn finish(self) -> Result<ExecutableHostContract, ExecutableHostContractBuildError> {
        let role_bindings = canonical_role_bindings(self.role_bindings)?;
        let runtime_features = sorted_unique_shared_slice(self.runtime_features);
        let lane_requirements = sorted_unique_shared_slice(self.lane_requirements);

        validate_root_contract(
            self.root,
            self.runtime_artifact.as_ref(),
            &role_bindings,
            &runtime_features,
        )?;

        if self.runtime_artifact.is_none()
            && role_bindings
                .iter()
                .any(|binding| binding.implementation() == RuntimeRoleImplementation::BrayRuntime)
        {
            return Err(ExecutableHostContractBuildError::RuntimeBindingWithoutArtifact);
        }

        Ok(ExecutableHostContract {
            product: self.product,
            native_entry: self.native_entry,
            root: self.root,
            abi_version: self.abi_version,
            runtime_artifact: self.runtime_artifact,
            role_bindings,
            runtime_features,
            lane_requirements,
            capacity_limits: self.capacity_limits,
        })
    }
}

impl ExecutableHostContract {
    /// Returns the product owned and observed by this host stub.
    pub const fn product(&self) -> &ProductIdentity {
        &self.product
    }

    /// Returns the binary symbol name of the compiler-generated native process entry point.
    pub const fn native_entry(&self) -> &BinarySymbolName {
        &self.native_entry
    }

    /// Returns how the source entry body becomes the root run.
    pub const fn root(&self) -> RootExecution {
        self.root
    }

    /// Returns the selected private execution ABI version.
    pub const fn abi_version(&self) -> RuntimeAbiVersion {
        self.abi_version
    }

    /// Returns the selected async-runtime artifact, when reachable behavior needs one.
    pub const fn runtime_artifact(&self) -> Option<&RuntimeArtifactId> {
        self.runtime_artifact.as_ref()
    }

    /// Returns exact private role bindings in canonical role order.
    pub fn role_bindings(&self) -> &[RuntimeRoleBinding] {
        &self.role_bindings
    }

    /// Returns the selected binding for one closed ABI role.
    pub fn role_binding(&self, role: RuntimeAbiRole) -> Option<&RuntimeRoleBinding> {
        self.role_bindings
            .binary_search_by_key(&role, RuntimeRoleBinding::role)
            .ok()
            .map(|index| &self.role_bindings[index])
    }

    /// Returns required runtime facilities in canonical order.
    pub fn runtime_features(&self) -> &[RuntimeFeature] {
        &self.runtime_features
    }

    /// Returns reachable execution-lane requirements in canonical order.
    pub fn lane_requirements(&self) -> &[ExecutionLaneRequirement] {
        &self.lane_requirements
    }

    /// Returns product-wide execution capacity limits.
    pub const fn capacity_limits(&self) -> ExecutionCapacityLimits {
        self.capacity_limits
    }
}

/// A contract violation that prevents executable-host construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExecutableHostContractBuildError {
    /// More than one binary binding was supplied for one closed ABI role.
    DuplicateRole(RuntimeAbiRole),
    /// An async root has no selected async-runtime artifact.
    MissingAsyncRuntime,
    /// A role names the Bray runtime mechanism without selecting a runtime artifact.
    RuntimeBindingWithoutArtifact,
    /// An async root does not require the distinguished main-thread lane.
    MissingMainThreadLaneFeature,
    /// A mandatory executable-host role has no selected binding.
    MissingRole(RuntimeAbiRole),
}

fn canonical_role_bindings(
    bindings: impl IntoIterator<Item = RuntimeRoleBinding>,
) -> Result<Arc<[RuntimeRoleBinding]>, ExecutableHostContractBuildError> {
    let mut bindings: Vec<_> = bindings.into_iter().collect();

    bindings.sort_unstable_by_key(RuntimeRoleBinding::role);

    if let Some(pair) = bindings
        .windows(2)
        .find(|pair| pair[0].role() == pair[1].role())
    {
        return Err(ExecutableHostContractBuildError::DuplicateRole(
            pair[0].role(),
        ));
    }

    Ok(bindings.into())
}

fn validate_root_contract(
    root: RootExecution,
    runtime_artifact: Option<&RuntimeArtifactId>,
    role_bindings: &[RuntimeRoleBinding],
    runtime_features: &[RuntimeFeature],
) -> Result<(), ExecutableHostContractBuildError> {
    let required_roles = [
        RuntimeAbiRole::RootExecution,
        RuntimeAbiRole::RootCancellationRequest,
        RuntimeAbiRole::CleanupIncidentReporting,
        RuntimeAbiRole::RootTerminalObservation,
        RuntimeAbiRole::StructuredShutdown,
    ];

    for role in required_roles {
        if !has_role(role_bindings, role) {
            return Err(ExecutableHostContractBuildError::MissingRole(role));
        }
    }

    if root == RootExecution::Synchronous {
        return Ok(());
    }

    if runtime_artifact.is_none() {
        return Err(ExecutableHostContractBuildError::MissingAsyncRuntime);
    }

    if runtime_features
        .binary_search(&RuntimeFeature::MainThreadLane)
        .is_err()
    {
        return Err(ExecutableHostContractBuildError::MissingMainThreadLaneFeature);
    }

    for role in [
        RuntimeAbiRole::MainThreadLaneStartup,
        RuntimeAbiRole::MainThreadLaneDrive,
    ] {
        if !has_role(role_bindings, role) {
            return Err(ExecutableHostContractBuildError::MissingRole(role));
        }
    }

    Ok(())
}

fn has_role(bindings: &[RuntimeRoleBinding], role: RuntimeAbiRole) -> bool {
    bindings
        .binary_search_by_key(&role, RuntimeRoleBinding::role)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use bray_symbols::{PackageIdentity, ProductIdentity};

    use super::{
        ExecutableHostContract, ExecutableHostContractBuildError, ExecutableHostContractBuilder,
        RootExecution, RuntimeFeature,
    };
    use crate::{
        BinarySymbolName, ProtectedAsyncFrameId, RuntimeAbiRole, RuntimeAbiVersion,
        RuntimeArtifactId, RuntimeRoleBinding, RuntimeRoleImplementation,
    };

    #[test]
    fn async_hosts_require_runtime_and_main_thread_contracts() {
        let root = RootExecution::Asynchronous {
            frame: ProtectedAsyncFrameId::new([7; 32]),
        };

        assert_eq!(
            host(root, None, base_bindings(), []),
            Err(ExecutableHostContractBuildError::MissingAsyncRuntime)
        );

        let Some(runtime) = RuntimeArtifactId::try_new("runtime.test") else {
            panic!("test runtime identity must be valid");
        };

        assert_eq!(
            host(root, Some(runtime), base_bindings(), []),
            Err(ExecutableHostContractBuildError::MissingMainThreadLaneFeature)
        );
    }

    #[test]
    fn complete_async_hosts_publish_canonical_role_bindings() {
        let mut bindings = base_bindings();
        bindings.push(binding(RuntimeAbiRole::MainThreadLaneDrive));
        bindings.push(binding(RuntimeAbiRole::MainThreadLaneStartup));

        let Some(runtime) = RuntimeArtifactId::try_new("runtime.test") else {
            panic!("test runtime identity must be valid");
        };

        let Ok(host) = host(
            RootExecution::Asynchronous {
                frame: ProtectedAsyncFrameId::new([7; 32]),
            },
            Some(runtime),
            bindings,
            [RuntimeFeature::MainThreadLane],
        ) else {
            panic!("complete async host contract must validate");
        };

        assert_eq!(
            host.role_bindings()[0].role(),
            RuntimeAbiRole::RootExecution
        );

        assert!(
            host.role_binding(RuntimeAbiRole::MainThreadLaneDrive)
                .is_some()
        );
    }

    #[test]
    fn runtime_role_bindings_require_a_selected_runtime_artifact() {
        let mut bindings = base_bindings();
        bindings.push(binding_with_implementation(
            RuntimeAbiRole::TaskStart,
            RuntimeRoleImplementation::BrayRuntime,
        ));

        assert_eq!(
            host(RootExecution::Synchronous, None, bindings, []),
            Err(ExecutableHostContractBuildError::RuntimeBindingWithoutArtifact)
        );
    }

    fn host(
        root: RootExecution,
        runtime: Option<RuntimeArtifactId>,
        bindings: impl IntoIterator<Item = RuntimeRoleBinding>,
        features: impl IntoIterator<Item = RuntimeFeature>,
    ) -> Result<ExecutableHostContract, ExecutableHostContractBuildError> {
        let Some(package) = PackageIdentity::try_new("example.app") else {
            panic!("test package identity must be valid");
        };
        let Some(product) = ProductIdentity::try_new(package, "application") else {
            panic!("test product identity must be valid");
        };
        let Some(entry) = BinarySymbolName::try_new("_bray_host_start") else {
            panic!("test host entry symbol name must be valid");
        };

        let mut builder =
            ExecutableHostContractBuilder::new(product, entry, root, RuntimeAbiVersion::new(1, 0));

        if let Some(runtime) = runtime {
            builder.select_runtime(runtime);
        }

        for binding in bindings {
            builder.push_role_binding(binding);
        }

        for feature in features {
            builder.require_runtime_feature(feature);
        }

        builder.finish()
    }

    fn base_bindings() -> Vec<RuntimeRoleBinding> {
        [
            RuntimeAbiRole::StructuredShutdown,
            RuntimeAbiRole::RootTerminalObservation,
            RuntimeAbiRole::CleanupIncidentReporting,
            RuntimeAbiRole::RootCancellationRequest,
            RuntimeAbiRole::RootExecution,
        ]
        .into_iter()
        .map(binding)
        .collect()
    }

    fn binding(role: RuntimeAbiRole) -> RuntimeRoleBinding {
        binding_with_implementation(role, RuntimeRoleImplementation::CompilerLowering)
    }

    fn binding_with_implementation(
        role: RuntimeAbiRole,
        implementation: RuntimeRoleImplementation,
    ) -> RuntimeRoleBinding {
        let Some(symbol_name) = BinarySymbolName::try_new(format!("role_{role:?}")) else {
            panic!("test role symbol name must be valid");
        };

        RuntimeRoleBinding::new(role, symbol_name, implementation)
    }
}
