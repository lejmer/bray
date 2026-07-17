use crate::BinarySymbolName;

/// Closed binary execution ABI role understood by lowering, backends, and product hosts.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RuntimeAbiRole {
    /// Begin and own the executable root run.
    RootExecution,
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

/// Exact binary binding selected for one private execution ABI role.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuntimeRoleBinding {
    role: RuntimeAbiRole,
    symbol: BinarySymbolName,
    implementation: RuntimeRoleImplementation,
}

impl RuntimeRoleBinding {
    /// Creates one role binding after product and target selection.
    pub const fn new(
        role: RuntimeAbiRole,
        symbol: BinarySymbolName,
        implementation: RuntimeRoleImplementation,
    ) -> Self {
        Self {
            role,
            symbol,
            implementation,
        }
    }

    /// Returns the closed semantic ABI role.
    pub const fn role(&self) -> RuntimeAbiRole {
        self.role
    }

    /// Returns the exact binary symbol selected for the role.
    pub const fn symbol(&self) -> &BinarySymbolName {
        &self.symbol
    }

    /// Returns the mechanism boundary supplying the role.
    pub const fn implementation(&self) -> RuntimeRoleImplementation {
        self.implementation
    }
}
