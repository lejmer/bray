use std::sync::Arc;

use crate::BinarySymbolName;

/// Closed binary execution ABI role understood by lowering, backends, and product hosts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuntimeAbiRole {
    /// Begin and own the executable root run.
    RootExecution,
    /// Execute one synchronous entry callback behind the product panic boundary.
    SynchronousRootExecution,
    /// Request cancellation of the root run from its host.
    RootCancellationRequest,
    /// Allocate stable task-owned storage.
    TaskAllocation,
    /// Publish a newly initialized task for execution.
    TaskStart,
    /// Enter or resume one protected async frame.
    FrameResume,
    /// Register a suspended frame with an event source.
    SuspensionRegistration,
    /// Wake a suspended task.
    Wake,
    /// Request cancellation of a child task.
    TaskCancellationRequest,
    /// Observe cancellation requested for the current run.
    CurrentRunCancellationObservation,
    /// Transfer current-run cancellation to the nearest run boundary.
    CurrentRunCancellationPropagation,
    /// Register and resolve a task join.
    JoinRegistration,
    /// Publish one terminal run outcome.
    TerminalPublication,
    /// Integrate one runtime event source.
    RuntimeEvent,
    /// Select a lane compatible with checked execution requirements.
    CompatibleLaneSelection,
    /// Transfer ownership of one cleanup incident.
    CleanupIncidentTransfer,
    /// Report and destroy product-host cleanup incidents.
    CleanupIncidentReporting,
    /// Initialize the distinguished main-thread execution lane.
    MainThreadLaneStartup,
    /// Drive work assigned to the distinguished main-thread lane.
    MainThreadLaneDrive,
    /// Observe the root terminal record without creating a source task.
    RootTerminalObservation,
    /// Release runtime-owned root completion storage after host resolution.
    RootCompletionResolution,
    /// Report and resolve one root panic payload.
    PanicReporting,
    /// Report one recoverable entrypoint failure value before host resolution.
    EntryFailureReporting,
    /// Select the catalog entry admitted by the test runner.
    TestEntrySelection,
    /// Shut runtime and product-host infrastructure down in checked order.
    StructuredShutdown,
    /// Broadcast cancellation to tasks reachable from one frame.
    FrameTaskBroadcast,
    /// Resolve async and ordinary lifecycle state retained by one frame.
    FrameLifecycleResolution,
    /// Move a frame's completed result into its destination.
    FrameCompletionMove,
    /// Infallibly destroy terminal frame storage.
    FrameDestruction,
    /// Initialize one generator accumulation.
    GeneratorBegin,
    /// Append one value to generator accumulation.
    GeneratorPush,
    /// Finish generator accumulation and publish its value.
    GeneratorFinish,
    /// Broadcast task cleanup through initialized generator elements.
    GeneratorCleanupBroadcast,
    /// Destroy initialized generator elements and release accumulation storage.
    GeneratorDestruction,
    /// Construct one owned panic report.
    PanicReportConstruction,
    /// Propagate one owned panic report to the nearest native run boundary.
    PanicPropagation,
    /// Create one inactive erased protected frame.
    FrameCreation,
    /// Move one inactive erased protected frame before first resume.
    InactiveFrameMove,
    /// Compose one erased directly awaited frame into its parent.
    AwaitedFrameComposition,
    /// Infallibly destroy one terminal task control record.
    TaskDestruction,
}

impl RuntimeAbiRole {
    /// Every private execution ABI role in stable order.
    pub const ALL: [Self; 40] = [
        Self::RootExecution,
        Self::SynchronousRootExecution,
        Self::RootCancellationRequest,
        Self::TaskAllocation,
        Self::TaskStart,
        Self::FrameResume,
        Self::SuspensionRegistration,
        Self::Wake,
        Self::TaskCancellationRequest,
        Self::CurrentRunCancellationObservation,
        Self::CurrentRunCancellationPropagation,
        Self::JoinRegistration,
        Self::TerminalPublication,
        Self::RuntimeEvent,
        Self::CompatibleLaneSelection,
        Self::CleanupIncidentTransfer,
        Self::CleanupIncidentReporting,
        Self::MainThreadLaneStartup,
        Self::MainThreadLaneDrive,
        Self::RootTerminalObservation,
        Self::RootCompletionResolution,
        Self::PanicReporting,
        Self::EntryFailureReporting,
        Self::TestEntrySelection,
        Self::StructuredShutdown,
        Self::FrameTaskBroadcast,
        Self::FrameLifecycleResolution,
        Self::FrameCompletionMove,
        Self::FrameDestruction,
        Self::GeneratorBegin,
        Self::GeneratorPush,
        Self::GeneratorFinish,
        Self::GeneratorCleanupBroadcast,
        Self::GeneratorDestruction,
        Self::PanicReportConstruction,
        Self::PanicPropagation,
        Self::FrameCreation,
        Self::InactiveFrameMove,
        Self::AwaitedFrameComposition,
        Self::TaskDestruction,
    ];

    /// Product-host control roles shared by synchronous and asynchronous roots.
    pub const EXECUTABLE_HOST_CONTROL: [Self; 7] = [
        Self::RootCancellationRequest,
        Self::CleanupIncidentReporting,
        Self::RootTerminalObservation,
        Self::RootCompletionResolution,
        Self::PanicReporting,
        Self::EntryFailureReporting,
        Self::StructuredShutdown,
    ];

    /// Returns this role's stable textual name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RootExecution => "root_execution",
            Self::SynchronousRootExecution => "synchronous_root_execution",
            Self::RootCancellationRequest => "root_cancellation_request",
            Self::TaskAllocation => "task_allocation",
            Self::TaskStart => "task_start",
            Self::FrameResume => "frame_resume",
            Self::SuspensionRegistration => "suspension_registration",
            Self::Wake => "wake",
            Self::TaskCancellationRequest => "task_cancellation_request",
            Self::CurrentRunCancellationObservation => "current_run_cancellation_observation",
            Self::CurrentRunCancellationPropagation => "current_run_cancellation_propagation",
            Self::JoinRegistration => "join_registration",
            Self::TerminalPublication => "terminal_publication",
            Self::RuntimeEvent => "runtime_event",
            Self::CompatibleLaneSelection => "compatible_lane_selection",
            Self::CleanupIncidentTransfer => "cleanup_incident_transfer",
            Self::CleanupIncidentReporting => "cleanup_incident_reporting",
            Self::MainThreadLaneStartup => "main_thread_lane_startup",
            Self::MainThreadLaneDrive => "main_thread_lane_drive",
            Self::RootTerminalObservation => "root_terminal_observation",
            Self::RootCompletionResolution => "root_completion_resolution",
            Self::PanicReporting => "panic_reporting",
            Self::EntryFailureReporting => "entry_failure_reporting",
            Self::TestEntrySelection => "test_entry_selection",
            Self::StructuredShutdown => "structured_shutdown",
            Self::FrameTaskBroadcast => "frame_task_broadcast",
            Self::FrameLifecycleResolution => "frame_lifecycle_resolution",
            Self::FrameCompletionMove => "frame_completion_move",
            Self::FrameDestruction => "frame_destruction",
            Self::GeneratorBegin => "generator_begin",
            Self::GeneratorPush => "generator_push",
            Self::GeneratorFinish => "generator_finish",
            Self::GeneratorCleanupBroadcast => "generator_cleanup_broadcast",
            Self::GeneratorDestruction => "generator_destruction",
            Self::PanicReportConstruction => "panic_report_construction",
            Self::PanicPropagation => "panic_propagation",
            Self::FrameCreation => "frame_creation",
            Self::InactiveFrameMove => "inactive_frame_move",
            Self::AwaitedFrameComposition => "awaited_frame_composition",
            Self::TaskDestruction => "task_destruction",
        }
    }

    /// Resolves one stable textual role name.
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.as_str() == name)
    }

    /// Returns the compiler-owned semantic contract of this closed ABI role.
    pub const fn contract(self) -> RuntimeRoleContract {
        RuntimeRoleContract::new(self, role_effects(self))
    }
}

/// Semantic effect fixed by one closed private ABI role.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuntimeRoleContractEffect {
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
    /// Register a suspended continuation.
    RegisterContinuation,
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

const fn role_effects(role: RuntimeAbiRole) -> &'static [RuntimeRoleContractEffect] {
    use RuntimeRoleContractEffect as Effect;

    match role {
        RuntimeAbiRole::RootExecution => &[Effect::EstablishRootRun, Effect::TransferFrame],
        RuntimeAbiRole::SynchronousRootExecution => {
            &[Effect::EstablishRootRun, Effect::ExecuteCallbackRoot]
        }
        RuntimeAbiRole::RootCancellationRequest | RuntimeAbiRole::TaskCancellationRequest => {
            &[Effect::RequestCancellation]
        }
        RuntimeAbiRole::TaskAllocation => &[Effect::AllocateTask],
        RuntimeAbiRole::TaskStart => &[Effect::TransferFrame, Effect::PublishWork],
        RuntimeAbiRole::FrameResume => &[Effect::ExecuteCallbackRoot],
        RuntimeAbiRole::SuspensionRegistration => &[Effect::RegisterContinuation],
        RuntimeAbiRole::Wake => &[Effect::PublishWork, Effect::EstablishVisibility],
        RuntimeAbiRole::CurrentRunCancellationObservation => &[Effect::ObserveCancellation],
        RuntimeAbiRole::CurrentRunCancellationPropagation => &[Effect::PropagateCancellation],
        RuntimeAbiRole::JoinRegistration => &[
            Effect::RegisterContinuation,
            Effect::AcquireTerminalState,
            Effect::EstablishVisibility,
        ],
        RuntimeAbiRole::TerminalPublication => {
            &[Effect::PublishTerminalState, Effect::EstablishVisibility]
        }
        RuntimeAbiRole::RuntimeEvent => &[Effect::ExecuteCallbackRoot],
        RuntimeAbiRole::CompatibleLaneSelection
        | RuntimeAbiRole::MainThreadLaneStartup
        | RuntimeAbiRole::MainThreadLaneDrive => &[],
        RuntimeAbiRole::CleanupIncidentTransfer => &[Effect::TransferCleanupIncident],
        RuntimeAbiRole::CleanupIncidentReporting => &[Effect::ReportCleanupIncidents],
        RuntimeAbiRole::RootTerminalObservation => {
            &[Effect::AcquireTerminalState, Effect::EstablishVisibility]
        }
        RuntimeAbiRole::RootCompletionResolution => &[Effect::ReleaseRootCompletion],
        RuntimeAbiRole::PanicReporting => &[Effect::ReportPanic],
        RuntimeAbiRole::EntryFailureReporting => &[Effect::ReportEntryFailure],
        RuntimeAbiRole::TestEntrySelection => &[Effect::SelectTestEntry],
        RuntimeAbiRole::StructuredShutdown => &[Effect::StructuredShutdown],
        RuntimeAbiRole::FrameTaskBroadcast => &[Effect::BroadcastFrameTasks],
        RuntimeAbiRole::FrameLifecycleResolution => &[Effect::ResolveFrameLifecycle],
        RuntimeAbiRole::FrameCompletionMove => &[Effect::MoveCompletion],
        RuntimeAbiRole::FrameDestruction => &[Effect::DestroyFrame],
        RuntimeAbiRole::GeneratorBegin => &[Effect::InitializeGenerator],
        RuntimeAbiRole::GeneratorPush => &[Effect::AppendGeneratorValue],
        RuntimeAbiRole::GeneratorFinish => &[Effect::FinishGenerator],
        RuntimeAbiRole::GeneratorCleanupBroadcast => &[Effect::BroadcastGeneratorCleanup],
        RuntimeAbiRole::GeneratorDestruction => &[Effect::DestroyGenerator],
        RuntimeAbiRole::PanicReportConstruction => &[Effect::ConstructPanicReport],
        RuntimeAbiRole::PanicPropagation => &[Effect::PropagatePanic],
        RuntimeAbiRole::FrameCreation => &[Effect::CreateFrame],
        RuntimeAbiRole::InactiveFrameMove => &[Effect::MoveFrame],
        RuntimeAbiRole::AwaitedFrameComposition => &[Effect::ComposeAwaitedFrame],
        RuntimeAbiRole::TaskDestruction => &[Effect::DestroyTask],
    }
}

#[cfg(test)]
mod tests {
    use super::{RuntimeAbiRole, RuntimeRoleContractEffect};

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
