use std::num::NonZeroU32;
use std::sync::Arc;

use bray_symbols::{ProductIdentity, TypeId, UnionVariantSymbolId};
use bray_target::TargetIdentity;

use crate::role::canonical_role_bindings;
use crate::{
    BinarySymbolName, ExecutionLaneRequirement, PanicAbiIdentity, ProtectedAsyncFrameId,
    RuntimeAbiRole, RuntimeAbiVersion, RuntimeArtifactId, RuntimeCapability,
    RuntimeCompatibilityError, RuntimeContract, RuntimeIdentity, RuntimeRequirements,
    RuntimeRoleBinding, RuntimeRoleImplementation,
};

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

/// Checked source result mapped by one compiler-generated executable host.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExecutableEntryResult {
    /// `unit` maps normal completion to a successful process exit.
    Unit,
    /// `i32` supplies the native process exit result.
    I32,
    /// `Result<unit, E>` maps its success variant to success and its error variant to failure.
    Fallible {
        /// Exact concrete result-union type.
        ty: TypeId,
        /// Exact concrete recoverable error type.
        error: TypeId,
        /// Compiler-known success variant of the result union.
        success_variant: UnionVariantSymbolId,
    },
}

/// Stable position of one source entry in an executable host contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExecutableHostEntryId(u32);

impl ExecutableHostEntryId {
    /// Creates an entry identity from its zero-based contract slot.
    pub const fn new(slot: u32) -> Self {
        Self(slot)
    }

    /// Returns the zero-based contract slot.
    pub const fn slot(self) -> u32 {
        self.0
    }
}

/// One source entry executed by a compiler-generated native host.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ExecutableHostEntry {
    root: RootExecution,
    root_frame_adapter: Option<BinarySymbolName>,
    result: ExecutableEntryResult,
}

impl ExecutableHostEntry {
    /// Creates one synchronous source entry and its checked result mapping.
    pub const fn synchronous(result: ExecutableEntryResult) -> Self {
        Self {
            root: RootExecution::Synchronous,
            root_frame_adapter: None,
            result,
        }
    }

    /// Creates one asynchronous source entry and its generated frame-transfer adapter.
    pub fn asynchronous(
        frame: ProtectedAsyncFrameId,
        root_frame_adapter: BinarySymbolName,
        result: ExecutableEntryResult,
    ) -> Self {
        Self {
            root: RootExecution::Asynchronous { frame },
            root_frame_adapter: Some(root_frame_adapter),
            result,
        }
    }

    /// Returns how this source entry becomes a root run.
    pub const fn root(&self) -> RootExecution {
        self.root
    }

    /// Returns the generated asynchronous frame-transfer adapter, when required.
    pub fn root_frame_adapter(&self) -> Option<&BinarySymbolName> {
        self.root_frame_adapter.as_ref()
    }

    /// Returns the checked source result mapping for this entry.
    pub const fn result(&self) -> ExecutableEntryResult {
        self.result
    }
}

/// Complete selected execution contract for one compiler-generated executable host stub.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ExecutableHostContract {
    data: Arc<ExecutableHostContractData>,
}

#[derive(Debug, Eq, Hash, PartialEq)]
struct ExecutableHostContractData {
    product: ProductIdentity,
    native_entry: BinarySymbolName,
    entries: Arc<[ExecutableHostEntry]>,
    requirements: RuntimeRequirements,
    runtime: Option<RuntimeContract>,
    host_role_bindings: Arc<[RuntimeRoleBinding]>,
    capacity_limits: ExecutionCapacityLimits,
}

/// Builder for one compiler-generated executable-host contract.
#[derive(Debug)]
pub struct ExecutableHostContractBuilder {
    product: ProductIdentity,
    native_entry: BinarySymbolName,
    entries: Vec<ExecutableHostEntry>,
    requirements: RuntimeRequirements,
    runtime: Option<RuntimeContract>,
    host_role_bindings: Vec<RuntimeRoleBinding>,
    capacity_limits: ExecutionCapacityLimits,
}

impl ExecutableHostContractBuilder {
    /// Starts a host contract without source entries.
    pub const fn empty(
        product: ProductIdentity,
        native_entry: BinarySymbolName,
        requirements: RuntimeRequirements,
    ) -> Self {
        Self {
            product,
            native_entry,
            entries: Vec::new(),
            requirements,
            runtime: None,
            host_role_bindings: Vec::new(),
            capacity_limits: ExecutionCapacityLimits::new(None, None, None),
        }
    }

    /// Starts a host contract from its product, native entry, first executable entry, and requirements.
    pub fn new(
        product: ProductIdentity,
        native_entry: BinarySymbolName,
        entry: ExecutableHostEntry,
        requirements: RuntimeRequirements,
    ) -> Self {
        let mut builder = Self::empty(product, native_entry, requirements);
        builder.entries.push(entry);

        builder
    }

    /// Selects the validated target-specific execution runtime.
    pub fn select_runtime(&mut self, runtime: RuntimeContract) {
        self.runtime = Some(runtime);
    }

    /// Adds another source entry in deterministic execution order.
    pub fn push_entry(&mut self, entry: ExecutableHostEntry) {
        self.entries.push(entry);
    }

    /// Adds one private ABI role supplied outside the selected runtime artifact.
    pub fn push_role_binding(&mut self, binding: RuntimeRoleBinding) {
        self.host_role_bindings.push(binding);
    }

    /// Replaces product-wide hard execution capacity limits.
    pub fn set_capacity_limits(&mut self, limits: ExecutionCapacityLimits) {
        self.capacity_limits = limits;
    }

    /// Completes the host contract after validating selected runtime coverage.
    pub fn finish(self) -> Result<ExecutableHostContract, ExecutableHostContractBuildError> {
        let host_role_bindings = canonical_role_bindings(self.host_role_bindings)
            .map_err(ExecutableHostContractBuildError::DuplicateRole)?;

        if let Some(binding) = host_role_bindings
            .iter()
            .find(|binding| binding.implementation() == RuntimeRoleImplementation::BrayRuntime)
        {
            return Err(ExecutableHostContractBuildError::RuntimeOwnedHostBinding(
                binding.role(),
            ));
        }

        if let Some(runtime) = &self.runtime {
            runtime
                .validate(&self.requirements)
                .map_err(ExecutableHostContractBuildError::IncompatibleRuntime)?;

            if let Some(role) = host_role_bindings
                .iter()
                .map(RuntimeRoleBinding::role)
                .find(|role| {
                    self.requirements.roles().binary_search(role).is_ok()
                        && runtime.role_binding(*role).is_some()
                })
            {
                return Err(ExecutableHostContractBuildError::DuplicateRole(role));
            }
        } else if self.requirements.requires_implementation() {
            return Err(ExecutableHostContractBuildError::MissingRuntime);
        }

        if !has_role_binding(
            RuntimeAbiRole::StructuredShutdown,
            self.runtime.as_ref(),
            &host_role_bindings,
        ) {
            return Err(ExecutableHostContractBuildError::MissingRole(
                RuntimeAbiRole::StructuredShutdown,
            ));
        }

        for entry in &self.entries {
            validate_root_contract(
                entry.root(),
                &self.requirements,
                self.runtime.as_ref(),
                &host_role_bindings,
            )?;
        }

        Ok(ExecutableHostContract {
            data: Arc::new(ExecutableHostContractData {
                product: self.product,
                native_entry: self.native_entry,
                entries: self.entries.into(),
                requirements: self.requirements,
                runtime: self.runtime,
                host_role_bindings,
                capacity_limits: self.capacity_limits,
            }),
        })
    }
}

impl ExecutableHostContract {
    /// Returns the product owned and observed by this host stub.
    pub fn product(&self) -> &ProductIdentity {
        &self.data.product
    }

    /// Returns the binary symbol name of the compiler-generated native process entry point.
    pub fn native_entry(&self) -> &BinarySymbolName {
        &self.data.native_entry
    }

    /// Returns source entries in deterministic execution order.
    pub fn entries(&self) -> &[ExecutableHostEntry] {
        &self.data.entries
    }

    /// Returns the source entry at an exact contract position.
    pub fn entry(&self, entry: ExecutableHostEntryId) -> Option<&ExecutableHostEntry> {
        usize::try_from(entry.slot())
            .ok()
            .and_then(|entry| self.data.entries.get(entry))
    }

    /// Returns the complete reachable runtime requirements.
    pub fn requirements(&self) -> &RuntimeRequirements {
        &self.data.requirements
    }

    /// Returns the minimum private execution ABI version required by the product.
    pub fn abi_version(&self) -> RuntimeAbiVersion {
        self.requirements().abi_version()
    }

    /// Returns the exact compilation target identity.
    pub fn target(&self) -> &TargetIdentity {
        self.requirements().target()
    }

    /// Returns the exact target panic ABI.
    pub fn panic_abi(&self) -> &PanicAbiIdentity {
        self.requirements().panic_abi()
    }

    /// Returns the selected runtime implementation, when one is required.
    pub fn runtime(&self) -> Option<&RuntimeContract> {
        self.data.runtime.as_ref()
    }

    /// Returns the selected runtime identity, when one is required.
    pub fn runtime_identity(&self) -> Option<&RuntimeIdentity> {
        self.runtime().map(RuntimeContract::identity)
    }

    /// Returns the selected runtime artifact, when one is required.
    pub fn runtime_artifact(&self) -> Option<&RuntimeArtifactId> {
        self.runtime().map(RuntimeContract::artifact)
    }

    /// Returns the selected binding for one closed ABI role.
    pub fn role_binding(&self, role: RuntimeAbiRole) -> Option<&RuntimeRoleBinding> {
        self.data
            .host_role_bindings
            .binary_search_by_key(&role, RuntimeRoleBinding::role)
            .ok()
            .map(|index| &self.data.host_role_bindings[index])
            .or_else(|| {
                self.data
                    .runtime
                    .as_ref()
                    .and_then(|runtime| runtime.role_binding(role))
            })
    }

    /// Returns required runtime capabilities in canonical order.
    pub fn runtime_capabilities(&self) -> &[RuntimeCapability] {
        self.requirements().capabilities()
    }

    /// Returns reachable execution-lane requirements in canonical order.
    pub fn lane_requirements(&self) -> &[ExecutionLaneRequirement] {
        self.requirements().lanes()
    }

    /// Returns product-wide execution capacity limits.
    pub fn capacity_limits(&self) -> ExecutionCapacityLimits {
        self.data.capacity_limits
    }
}

/// Selects the executable host spelling for a runtime role, falling back to the native ABI.
pub fn selected_runtime_role_symbol(
    host: Option<&ExecutableHostContract>,
    role: RuntimeAbiRole,
) -> Option<BinarySymbolName> {
    host.and_then(|host| host.role_binding(role))
        .map(|binding| binding.symbol_name().clone())
        .or_else(|| role.native_symbol().and_then(BinarySymbolName::try_new))
}

/// A contract violation that prevents executable-host construction.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExecutableHostContractBuildError {
    /// More than one binary binding was supplied for one closed ABI role.
    DuplicateRole(RuntimeAbiRole),
    /// Reachable requirements need an execution runtime but none was selected.
    MissingRuntime,
    /// A host binding claims a role owned by the selected Bray runtime.
    RuntimeOwnedHostBinding(RuntimeAbiRole),
    /// The selected runtime cannot satisfy reachable product requirements.
    IncompatibleRuntime(RuntimeCompatibilityError),
    /// An async root does not require the distinguished main-thread lane.
    MissingMainThreadLaneCapability,
    /// An async root has no protected-frame operation compatibility contract.
    MissingProtectedFrameAbi,
    /// A mandatory executable-host role has no selected binding.
    MissingRole(RuntimeAbiRole),
}

fn validate_root_contract(
    root: RootExecution,
    requirements: &RuntimeRequirements,
    runtime: Option<&RuntimeContract>,
    host_role_bindings: &[RuntimeRoleBinding],
) -> Result<(), ExecutableHostContractBuildError> {
    let root_role = if root == RootExecution::Synchronous
        && requirements
            .roles()
            .contains(&RuntimeAbiRole::SynchronousRootExecution)
    {
        RuntimeAbiRole::SynchronousRootExecution
    } else {
        RuntimeAbiRole::RootExecution
    };

    for role in std::iter::once(root_role).chain(RuntimeAbiRole::host_controls()) {
        if !has_role_binding(role, runtime, host_role_bindings) {
            return Err(ExecutableHostContractBuildError::MissingRole(role));
        }
    }

    if root == RootExecution::Synchronous {
        return Ok(());
    }

    if runtime.is_none() {
        return Err(ExecutableHostContractBuildError::MissingRuntime);
    }

    if requirements.frame_abi().is_none() {
        return Err(ExecutableHostContractBuildError::MissingProtectedFrameAbi);
    }

    if requirements
        .capabilities()
        .binary_search(&RuntimeCapability::MainThreadLane)
        .is_err()
    {
        return Err(ExecutableHostContractBuildError::MissingMainThreadLaneCapability);
    }

    for role in [
        RuntimeAbiRole::MainThreadLaneStartup,
        RuntimeAbiRole::MainThreadLaneDrive,
    ] {
        if !has_role_binding(role, runtime, host_role_bindings) {
            return Err(ExecutableHostContractBuildError::MissingRole(role));
        }
    }

    Ok(())
}

fn has_role_binding(
    role: RuntimeAbiRole,
    runtime: Option<&RuntimeContract>,
    host_role_bindings: &[RuntimeRoleBinding],
) -> bool {
    host_role_bindings
        .binary_search_by_key(&role, RuntimeRoleBinding::role)
        .is_ok()
        || runtime.is_some_and(|runtime| runtime.role_binding(role).is_some())
}

#[cfg(test)]
mod tests {
    use bray_symbols::{PackageIdentity, ProductIdentity};
    use bray_target::TargetIdentity;

    use super::{
        ExecutableEntryResult, ExecutableHostContract, ExecutableHostContractBuildError,
        ExecutableHostContractBuilder, ExecutableHostEntry, RootExecution, RuntimeCapability,
    };
    use crate::{
        BinarySymbolName, PanicAbiIdentity, ProtectedAsyncFrameId, ProtectedFrameAbiVersions,
        RuntimeAbiRole, RuntimeAbiVersion, RuntimeArtifactId, RuntimeContract, RuntimeIdentity,
        RuntimeRequirements, RuntimeRoleBinding, RuntimeRoleImplementation,
    };

    #[test]
    fn async_hosts_require_runtime_and_infer_role_capabilities() {
        let root = RootExecution::Asynchronous {
            frame: ProtectedAsyncFrameId::new([7; 32]),
        };

        assert_eq!(
            host(root, synchronous_requirements(), None, base_bindings()),
            Err(ExecutableHostContractBuildError::MissingRuntime)
        );

        let requirements = runtime_requirements([]);

        assert!(
            host(
                root,
                requirements,
                Some(runtime_contract()),
                base_bindings()
            ).is_ok()
        );
    }

    #[test]
    fn runtime_roles_cannot_be_left_unselected() {
        let requirements = RuntimeRequirements::new(
            None,
            RuntimeAbiVersion::new(1, 0),
            None,
            target(),
            panic_abi(),
            [RuntimeAbiRole::TaskStart],
            [],
            [],
        );

        assert_eq!(
            host(
                RootExecution::Synchronous,
                requirements,
                None,
                base_bindings()
            ),
            Err(ExecutableHostContractBuildError::MissingRuntime)
        );
    }

    #[test]
    fn synchronous_hosts_report_cleanup_incidents_without_a_runtime() {
        let Ok(host) = host(
            RootExecution::Synchronous,
            synchronous_requirements(),
            None,
            base_bindings(),
        ) else {
            panic!("synchronous host contract must not require an execution runtime");
        };

        assert_eq!(host.runtime(), None);

        assert_eq!(
            host.role_binding(RuntimeAbiRole::CleanupIncidentReporting)
                .map(RuntimeRoleBinding::implementation),
            Some(RuntimeRoleImplementation::CompilerLowering)
        );
    }

    #[test]
    fn complete_async_hosts_resolve_runtime_and_compiler_role_bindings() {
        let root = RootExecution::Asynchronous {
            frame: ProtectedAsyncFrameId::new([7; 32]),
        };

        let requirements = runtime_requirements([RuntimeCapability::MainThreadLane]);

        let Ok(host) = host(
            root,
            requirements,
            Some(runtime_contract()),
            base_bindings(),
        ) else {
            panic!("complete async host contract must validate");
        };

        assert_eq!(
            host.role_binding(RuntimeAbiRole::RootExecution)
                .map(RuntimeRoleBinding::implementation),
            Some(RuntimeRoleImplementation::CompilerLowering)
        );

        assert_eq!(
            host.role_binding(RuntimeAbiRole::MainThreadLaneDrive)
                .map(RuntimeRoleBinding::implementation),
            Some(RuntimeRoleImplementation::BrayRuntime)
        );
    }

    fn host(
        root: RootExecution,
        requirements: RuntimeRequirements,
        runtime: Option<RuntimeContract>,
        bindings: impl IntoIterator<Item = RuntimeRoleBinding>,
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

        let host_entry = match root {
            RootExecution::Synchronous => {
                ExecutableHostEntry::synchronous(ExecutableEntryResult::Unit)
            }
            RootExecution::Asynchronous { frame } => ExecutableHostEntry::asynchronous(
                frame,
                BinarySymbolName::try_new("test_root_frame_adapter")
                    .unwrap_or_else(|| panic!("test frame adapter must be valid")),
                ExecutableEntryResult::Unit,
            ),
        };

        let mut builder =
            ExecutableHostContractBuilder::new(product, entry, host_entry, requirements);

        if let Some(runtime) = runtime {
            builder.select_runtime(runtime);
        }

        for binding in bindings {
            builder.push_role_binding(binding);
        }

        builder.finish()
    }

    fn synchronous_requirements() -> RuntimeRequirements {
        RuntimeRequirements::new(
            None,
            RuntimeAbiVersion::new(1, 0),
            None,
            target(),
            panic_abi(),
            [],
            [],
            [],
        )
    }

    fn runtime_requirements<const N: usize>(
        additional: [RuntimeCapability; N],
    ) -> RuntimeRequirements {
        RuntimeRequirements::new(
            Some(runtime_identity()),
            RuntimeAbiVersion::new(1, 0),
            Some(ProtectedFrameAbiVersions::uniform(RuntimeAbiVersion::new(
                1, 0,
            ))),
            target(),
            panic_abi(),
            [
                RuntimeAbiRole::MainThreadLaneStartup,
                RuntimeAbiRole::MainThreadLaneDrive,
            ],
            [RuntimeCapability::CooperativeExecution]
                .into_iter()
                .chain(additional),
            [],
        )
    }

    fn runtime_contract() -> RuntimeContract {
        RuntimeContract::try_new(
            runtime_identity(),
            RuntimeArtifactId::try_new("runtime.test")
                .unwrap_or_else(|| panic!("test runtime artifact must be valid")),
            RuntimeAbiVersion::new(1, 0),
            ProtectedFrameAbiVersions::uniform(RuntimeAbiVersion::new(1, 0)),
            target(),
            panic_abi(),
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MainThreadLane,
            ],
            [
                runtime_binding(RuntimeAbiRole::MainThreadLaneStartup),
                runtime_binding(RuntimeAbiRole::MainThreadLaneDrive),
            ],
        )
        .unwrap_or_else(|error| panic!("test runtime contract must be valid: {error:?}"))
    }

    fn base_bindings() -> Vec<RuntimeRoleBinding> {
        [
            RuntimeAbiRole::StructuredShutdown,
            RuntimeAbiRole::RootTerminalObservation,
            RuntimeAbiRole::CleanupIncidentReporting,
            RuntimeAbiRole::RootCancellationRequest,
            RuntimeAbiRole::RootExecution,
            RuntimeAbiRole::RootCompletionResolution,
            RuntimeAbiRole::PanicReporting,
            RuntimeAbiRole::EntryFailureReporting,
        ]
        .into_iter()
        .map(compiler_binding)
        .collect()
    }

    fn runtime_binding(role: RuntimeAbiRole) -> RuntimeRoleBinding {
        binding(role, RuntimeRoleImplementation::BrayRuntime)
    }

    fn compiler_binding(role: RuntimeAbiRole) -> RuntimeRoleBinding {
        binding(role, RuntimeRoleImplementation::CompilerLowering)
    }

    fn binding(
        role: RuntimeAbiRole,
        implementation: RuntimeRoleImplementation,
    ) -> RuntimeRoleBinding {
        let Some(symbol_name) = BinarySymbolName::try_new(format!("role_{role:?}")) else {
            panic!("test role symbol name must be valid");
        };

        RuntimeRoleBinding::new(role, symbol_name, implementation)
    }

    fn runtime_identity() -> RuntimeIdentity {
        RuntimeIdentity::try_new("bray.runtime.test")
            .unwrap_or_else(|| panic!("test runtime identity must be valid"))
    }

    fn target() -> TargetIdentity {
        TargetIdentity::try_new("x86_64-unknown-linux-gnu")
            .unwrap_or_else(|| panic!("test target identity must be valid"))
    }

    fn panic_abi() -> PanicAbiIdentity {
        PanicAbiIdentity::try_new("bray.panic.test")
            .unwrap_or_else(|| panic!("test panic ABI identity must be valid"))
    }
}
