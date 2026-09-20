use std::sync::Arc;

use crate::{
    BinarySymbolName, RuntimeAbiType, RuntimeCapability, RuntimeNativeSignature,
    RuntimeRoleArtifact,
};

macro_rules! define_runtime_roles {
    ($( $role:ident {
        $documentation:literal, $name:literal,
        native: ($($symbol:ident = $native:literal, [$($native_parameter:ident),*] -> $native_result:ident)?),
        $(service: $service:ident,)?
        call_hook: ($($hook:ident)?),
        compiler: $abi:ident [$($parameter:ident),*] -> $result:ident,
        owner: $owner:ident, availability: $availability:ident,
        bootstrap: ($($bootstrap:literal)?), host_control: $host_control:literal,
        capabilities: [$($capability:ident),*],
        effects: [$($effect:ident),*]
    })+) => {
        /// Closed execution ABI role understood by lowering, backends, and product hosts.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum RuntimeAbiRole { $( #[doc = $documentation] $role, )+ }

        impl RuntimeAbiRole {
            /// Every private execution ABI role in stable order.
            pub const ALL: [Self; [$(stringify!($role)),+].len()] = [$(Self::$role,)+];

            /// Returns this role's stable textual name.
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$role => $name,)+ }
            }

            /// Resolves one stable textual role name.
            #[deny(unreachable_patterns)]
            pub fn from_name(name: &str) -> Option<Self> {
                match name { $($name => Some(Self::$role),)+ _ => None }
            }

            /// Returns the compiler-owned semantic contract of this role.
            pub const fn contract(self) -> RuntimeRoleContract {
                match self { $(Self::$role => RuntimeRoleContract::new(self, &[$(RuntimeRoleContractEffect::$effect,)*]),)+ }
            }

            /// Returns the capabilities required by this operation.
            pub const fn required_capabilities(self) -> &'static [RuntimeCapability] {
                match self { $(Self::$role => &[$(RuntimeCapability::$capability,)*],)+ }
            }

            /// Returns the role's stable native symbol, if supplied by a linked artifact.
            pub const fn native_symbol(self) -> Option<&'static str> {
                match self { $(Self::$role => define_runtime_roles!(@symbol $($symbol)?),)+ }
            }

            /// Returns the exact native callable contract for a linked role.
            pub const fn native_signature(self) -> Option<RuntimeNativeSignature> {
                match self {
                    $(Self::$role => define_runtime_roles!(@native $([$($native_parameter),*] -> $native_result)?),)+
                }
            }

            /// Returns the reference runtime component implementing this role.
            pub const fn artifact_owner(self) -> RuntimeRoleArtifact {
                match self { $(Self::$role => RuntimeRoleArtifact::$owner,)+ }
            }

            /// Returns the implementation boundary supplying this role.
            pub const fn implementation(self) -> RuntimeRoleImplementation {
                match self.artifact_owner() {
                    RuntimeRoleArtifact::Compiler => RuntimeRoleImplementation::CompilerLowering,
                    RuntimeRoleArtifact::Bootstrap | RuntimeRoleArtifact::Observation
                    | RuntimeRoleArtifact::Host | RuntimeRoleArtifact::Callback
                    | RuntimeRoleArtifact::Scheduler | RuntimeRoleArtifact::Cancellation
                    | RuntimeRoleArtifact::Event | RuntimeRoleArtifact::TestHost => RuntimeRoleImplementation::BrayRuntime,
                }
            }

            /// Returns whether this role is available to an ordinary product.
            pub const fn available_to_product(self) -> bool {
                match self { $(Self::$role => define_runtime_roles!(@available $availability),)+ }
            }

            /// Returns the trusted Bray declaration implementing this role.
            pub const fn source_declaration(self) -> Option<&'static str> {
                match self { $(Self::$role => define_runtime_roles!(@bootstrap $($bootstrap)?),)+ }
            }

            /// Returns the separately built trusted Bray component implementing this role.
            pub const fn source_artifact(self) -> Option<RuntimeRoleArtifact> {
                if self.source_declaration().is_none() {
                    return None;
                }

                Some(match self.artifact_owner() {
                    RuntimeRoleArtifact::Observation => RuntimeRoleArtifact::Observation,
                    _ => RuntimeRoleArtifact::Bootstrap,
                })
            }

            /// Returns the toolchain-owned trusted Bray source binding for this role.
            pub fn source_binding(self) -> Option<crate::SourceRoleBinding<Self>> {
                let declaration = self.source_declaration()?;

                let module = match self.source_artifact()? {
                    RuntimeRoleArtifact::Bootstrap => "bray.runtime.bootstrap",
                    RuntimeRoleArtifact::Observation => "bray.runtime.observation",
                    _ => unreachable!("source roles belong to trusted Bray components"),
                };

                let path = format!("{module}.{declaration}");

                Some(
                    crate::SourceRoleBinding::try_new(self, &path).unwrap_or_else(|| {
                        panic!("runtime catalog source path is invalid: {path}")
                    }),
                )
            }

            /// Returns the control operations required by every executable host.
            pub fn host_controls() -> impl Iterator<Item = Self> {
                Self::ALL.into_iter().filter(|role| match role { $(Self::$role => $host_control,)+ })
            }
        }

        const _: () = {
            $(
                assert!(
                    matches!(RuntimeAbiRole::$role.artifact_owner(), RuntimeRoleArtifact::Compiler)
                        == RuntimeAbiRole::$role.native_signature().is_none(),
                    "runtime catalog ownership and native signature disagree"
                );

                assert!(
                    RuntimeAbiRole::$role.source_declaration().is_none()
                        || RuntimeAbiRole::$role.native_signature().is_some(),
                    "bootstrap role has no native callable signature"
                );

                assert!(
                    RuntimeAbiRole::$role.available_to_product()
                        != matches!(RuntimeAbiRole::$role.artifact_owner(), RuntimeRoleArtifact::TestHost),
                    "runtime catalog availability contradicts artifact ownership"
                );
            )+
        };
    };
    (@native [$($parameter:ident),*] -> $result:ident) => {
        Some(RuntimeNativeSignature::new(&[$(RuntimeAbiType::$parameter,)*], RuntimeAbiType::$result))
    };
    (@native) => { None };
    (@symbol $symbol:ident) => { Some(bray_runtime_abi::symbols::$symbol) };
    (@symbol) => { None };
    (@available All) => { true };
    (@available Test) => { false };
    (@bootstrap $name:literal) => { Some($name) };
    (@bootstrap) => { None };
}

bray_runtime_abi::runtime_role_catalog!(define_runtime_roles);

/// Runtime service class required by one operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuntimeServiceClass {
    /// Runtime-local behavior that does not require a scheduler.
    Host,
    /// Independent execution behavior backed by a scheduler.
    Execution,
}

macro_rules! define_service_demand {
    ($( $role:ident {
        $documentation:literal, $name:literal,
        native: ($($native:tt)*),
        $(service: $service:ident,)?
        call_hook: ($($hook:ident)?),
        compiler: $abi:ident [$($parameter:ident),*] -> $result:ident,
        owner: $owner:ident, availability: $availability:ident,
        bootstrap: ($($bootstrap:literal)?), host_control: $host_control:literal,
        capabilities: [$($capability:ident),*],
        effects: [$($effect:ident),*]
    })+) => {
        impl RuntimeAbiRole {
            /// Returns the runtime service class required by this operation.
            pub const fn service_class(self) -> Option<RuntimeServiceClass> {
                match self {
                    $(Self::$role => define_service_demand!(@service $($service)?),)+
                }
            }
        }
    };
    (@service Host) => { Some(RuntimeServiceClass::Host) };
    (@service Execution) => { Some(RuntimeServiceClass::Execution) };
    (@service) => { None };
}

bray_runtime_abi::runtime_role_catalog!(define_service_demand);

/// One build-authorized association between a Bray declaration and a private runtime role.
///
/// Runtime artifact tooling supplies these bindings directly to the compiler. Package manifests
/// cannot declare them, so an ordinary package cannot acquire runtime authority by spelling a
/// matching declaration or binary symbol.
pub type RuntimeRoleSourceBinding = crate::SourceRoleBinding<RuntimeAbiRole>;

/// Semantic effect fixed by one closed private ABI role.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuntimeRoleContractEffect {
    /// Initialize one loaded runtime artifact instance.
    InitializeRuntime,
    /// Establish a new root run owned by the product host.
    EstablishRootRun,
    /// Transfer ownership of a protected frame.
    TransferFrame,
    /// Allocate runtime-owned task storage.
    AllocateTask,
    /// Publish work for execution by a compatible lane.
    PublishWork,
    /// Execute a protected callback root.
    ExecuteCallbackRoot,
    /// Observe the current exact thread attachment.
    ObserveThreadAttachment,
    /// Transfer one cleanup entry to the current exact thread attachment.
    RegisterThreadCleanup,
    /// Control loaded-product entry, retention, attachment, and closure obligations.
    ControlProductHost,
    /// Register a suspended continuation.
    RegisterContinuation,
    /// Create one runtime-owned task event.
    CreateTaskEvent,
    /// Signal one runtime-owned task event.
    SignalTaskEvent,
    /// Release one runtime-owned task event.
    ReleaseTaskEvent,
    /// Establish release-to-acquire visibility.
    EstablishVisibility,
    /// Request cancellation of another run.
    RequestCancellation,
    /// Observe cancellation of the current run.
    ObserveCancellation,
    /// Publish one terminal run state.
    PublishTerminalState,
    /// Acquire one terminal run state.
    AcquireTerminalState,
    /// Release runtime-owned completion storage after payload resolution.
    ReleaseRootCompletion,
    /// Report and destroy one owned panic report.
    ReportPanic,
    /// Destroy one handled panic report without reporting it.
    DestroyPanicReport,
    /// Report one borrowed recoverable entry failure value.
    ReportEntryFailure,
    /// Select one admitted test entry from the runner command.
    SelectTestEntry,
    /// Transfer ownership of a cleanup incident.
    TransferCleanupIncident,
    /// Report and destroy owned cleanup incidents.
    ReportCleanupIncidents,
    /// Broadcast cancellation to frame-owned tasks.
    BroadcastFrameTasks,
    /// Resolve frame-owned lifecycle state.
    ResolveFrameLifecycle,
    /// Move an initialized completion result.
    MoveCompletion,
    /// Infallibly destroy terminal frame storage.
    DestroyFrame,
    /// Initialize generator-owned accumulation storage.
    InitializeGenerator,
    /// Transfer one yielded value into generator-owned accumulation storage.
    AppendGeneratorValue,
    /// Finish generator accumulation and transfer its completed value.
    FinishGenerator,
    /// Invoke task-cleanup callbacks for initialized generator elements.
    BroadcastGeneratorCleanup,
    /// Finalize and destroy initialized generator elements, then release their storage.
    DestroyGenerator,
    /// Construct one owned panic report.
    ConstructPanicReport,
    /// Propagate one owned panic report without resuming the failed continuation.
    PropagatePanic,
    /// Propagate cancellation without resuming the cancelled continuation.
    PropagateCancellation,
    /// Create one inactive protected frame value.
    CreateFrame,
    /// Move one inactive frame before first resume.
    MoveFrame,
    /// Compose one directly awaited child frame.
    ComposeAwaitedFrame,
    /// Infallibly destroy one terminal task control record.
    DestroyTask,
    /// Shut product execution infrastructure down.
    StructuredShutdown,
}

/// Compiler-owned semantic record for one closed private ABI role.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RuntimeRoleContract {
    role: RuntimeAbiRole,
    effects: &'static [RuntimeRoleContractEffect],
}

impl RuntimeRoleContract {
    const fn new(role: RuntimeAbiRole, effects: &'static [RuntimeRoleContractEffect]) -> Self {
        Self { role, effects }
    }

    /// Returns the exact ABI role whose signature and behavior this record defines.
    pub const fn role(self) -> RuntimeAbiRole {
        self.role
    }

    /// Returns the role's immutable semantic effects.
    pub const fn effects(self) -> &'static [RuntimeRoleContractEffect] {
        self.effects
    }
}

/// Implementation boundary supplying one private ABI role.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuntimeRoleImplementation {
    /// Operation synthesized directly by compiler lowering or code generation.
    CompilerLowering,
    /// Operation supplied by a separately linked Bray runtime artifact.
    BrayRuntime,
    /// Operation supplied by a direct target-platform binding.
    PlatformBinding,
    /// Operation supplied by a narrow native ABI-normalization shim.
    NativeShim,
}

impl RuntimeRoleImplementation {
    /// Returns this implementation boundary's stable textual name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CompilerLowering => "compiler_lowering",
            Self::BrayRuntime => "bray_runtime",
            Self::PlatformBinding => "platform_binding",
            Self::NativeShim => "native_shim",
        }
    }

    /// Resolves one stable textual implementation-boundary name.
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "compiler_lowering" => Some(Self::CompilerLowering),
            "bray_runtime" => Some(Self::BrayRuntime),
            "platform_binding" => Some(Self::PlatformBinding),
            "native_shim" => Some(Self::NativeShim),
            _ => None,
        }
    }
}

/// Exact binary binding selected for one private execution ABI role.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuntimeRoleBinding {
    role: RuntimeAbiRole,
    symbol_name: BinarySymbolName,
    implementation: RuntimeRoleImplementation,
}

impl RuntimeRoleBinding {
    /// Creates one role binding after product and target selection.
    pub const fn new(
        role: RuntimeAbiRole,
        symbol_name: BinarySymbolName,
        implementation: RuntimeRoleImplementation,
    ) -> Self {
        Self {
            role,
            symbol_name,
            implementation,
        }
    }

    /// Returns the closed semantic ABI role.
    pub const fn role(&self) -> RuntimeAbiRole {
        self.role
    }

    /// Returns the exact binary symbol name selected for the role.
    pub const fn symbol_name(&self) -> &BinarySymbolName {
        &self.symbol_name
    }

    /// Returns the mechanism boundary supplying the role.
    pub const fn implementation(&self) -> RuntimeRoleImplementation {
        self.implementation
    }
}

pub(crate) fn canonical_role_bindings(
    bindings: impl IntoIterator<Item = RuntimeRoleBinding>,
) -> Result<Arc<[RuntimeRoleBinding]>, RuntimeAbiRole> {
    let mut bindings: Vec<_> = bindings.into_iter().collect();

    bindings.sort_unstable_by_key(RuntimeRoleBinding::role);

    if let Some(pair) = bindings
        .windows(2)
        .find(|pair| pair[0].role() == pair[1].role())
    {
        return Err(pair[0].role());
    }

    Ok(bindings.into())
}

#[cfg(test)]
mod tests {
    use super::{RuntimeAbiRole, RuntimeRoleContractEffect, RuntimeServiceClass};

    #[test]
    fn complete_execution_catalog_has_unique_and_exhaustive_projections() {
        let mut names = std::collections::BTreeSet::new();
        let mut symbols = std::collections::BTreeSet::new();

        for role in RuntimeAbiRole::ALL {
            assert!(names.insert(role.as_str()), "{role:?}");
            assert_eq!(RuntimeAbiRole::from_name(role.as_str()), Some(role));
            assert_eq!(role.contract().role(), role);

            assert_eq!(
                role.native_symbol().is_some(),
                role.native_signature().is_some()
            );

            if let Some(symbol) = role.native_symbol() {
                assert!(symbols.insert(symbol), "{role:?}");
            }
        }

        for role in crate::PlatformServiceRole::ALL {
            assert!(symbols.insert(role.native_symbol()), "{role:?}");
        }
    }

    #[test]
    fn role_catalog_carries_bootstrap_availability_and_capability_requirements() {
        assert_eq!(
            RuntimeAbiRole::MainThreadLaneStartup.required_capabilities(),
            [
                crate::RuntimeCapability::CooperativeExecution,
                crate::RuntimeCapability::MainThreadLane
            ],
        );

        assert_eq!(
            RuntimeAbiRole::ThreadStaticCleanupRegistration.source_declaration(),
            None,
        );

        let binding = RuntimeAbiRole::RuntimeInitialization
            .source_binding()
            .unwrap_or_else(|| panic!("runtime initialization must have a source binding"));

        assert_eq!(binding.role(), RuntimeAbiRole::RuntimeInitialization);

        assert_eq!(
            binding.dotted_path(),
            "bray.runtime.bootstrap.runtime_initialization"
        );

        assert_eq!(
            RuntimeAbiRole::SynchronousRootExecution.service_class(),
            Some(RuntimeServiceClass::Host),
        );

        assert_eq!(
            RuntimeAbiRole::RootExecution.service_class(),
            Some(RuntimeServiceClass::Execution),
        );

        assert!(!RuntimeAbiRole::TestEntrySelection.available_to_product());

        assert_eq!(
            RuntimeAbiRole::GeneratorBegin.implementation(),
            super::RuntimeRoleImplementation::CompilerLowering
        );
    }

    #[test]
    fn runtime_service_inventory_is_exhaustive_and_natively_typed() {
        let mut host = 0;
        let mut execution = 0;

        for role in RuntimeAbiRole::ALL {
            match role.service_class() {
                Some(RuntimeServiceClass::Host) => host += 1,
                Some(RuntimeServiceClass::Execution) => execution += 1,
                None => continue,
            }

            assert!(role.native_signature().is_some(), "{role:?}");
        }

        assert_eq!(host, 22);
        assert_eq!(execution, 22);
    }

    #[test]
    fn role_contracts_are_closed_and_typed() {
        let contract = RuntimeAbiRole::TaskStart.contract();

        assert_eq!(contract.role(), RuntimeAbiRole::TaskStart);

        assert_eq!(
            contract.effects(),
            [
                RuntimeRoleContractEffect::TransferFrame,
                RuntimeRoleContractEffect::PublishWork
            ]
        );

        assert_eq!(
            RuntimeAbiRole::RootExecution.contract().effects(),
            [
                RuntimeRoleContractEffect::EstablishRootRun,
                RuntimeRoleContractEffect::TransferFrame,
            ]
        );
    }
}
