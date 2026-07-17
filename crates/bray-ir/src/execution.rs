use bray_execution::{ExecutableHostContract, ProtectedAsyncFrameId};

/// Execution representation owned by one MIR unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MirUnitExecution {
    /// Ordinary synchronous control flow with no protected async frame.
    Synchronous,
    /// Compiler-protected inactive and resumable async-frame representation.
    ProtectedAsyncFrame(ProtectedAsyncFrameId),
    /// Compiler-generated native host stub for one executable or test root.
    ExecutableHost(ExecutableHostContract),
}

/// Typed semantic category reserved for future MIR instruction nodes.
///
/// These categories prevent later lowering and backends from inferring execution semantics from
/// source method names or standard-library paths. They do not prescribe scheduler mechanics or
/// operand representation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirExecutionOperationKind {
    /// Create an inactive protected frame in a supplied result place.
    CreateFrame,
    /// Move an inactive frame before its first resume.
    MoveInactiveFrame,
    /// Enter or resume protected frame state.
    ResumeFrame,
    /// Suspend the current task with a checked resume state.
    SuspendTask,
    /// Compose a direct-awaited child frame without a task boundary.
    ComposeAwaitedFrame,
    /// Commit a directly awaited child's completion value.
    CommitAwaitedCompletion,
    /// Enter current-run cancellation cleanup.
    EnterCancellationCleanup,
    /// Request cancellation of an owned task.
    RequestTaskCancellation,
    /// Start a task by transferring an inactive frame into stable task storage.
    StartTask,
    /// Register and resolve terminal task observation.
    ResolveTask,
    /// Publish completed, cancelled, or panicked task state.
    PublishTerminalState,
    /// Execute checked phase-one task cancellation broadcast.
    ExecuteCleanupBroadcast,
    /// Execute checked phase-two lifecycle resolution.
    ExecuteLifecycleResolution,
    /// Transfer ownership of a cleanup incident.
    TransferCleanupIncident,
    /// Forward an observed panic or cancellation into the current run.
    ForwardCurrentRun,
    /// Destroy terminal task control state exactly once.
    DestroyTerminalTask,
}
