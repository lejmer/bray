use crate::{
    MirCall, MirFrameReference, MirFrameStateId, MirFrameStorageSource, MirOperand, MirPlace,
    MirRuntimeReference, MirStorageId,
};
use bray_bound_tree::{BoundCallResult, BoundUnitKey};
use bray_runtime_interface::{ExecutableHostEntryId, ProtectedAsyncFrameId, RootExecution};
use bray_symbols::TypeId;

/// Terminal state published for one task run.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirTaskTerminalState {
    /// The task completed with a value.
    Completed(MirOperand),
    /// Complete a capture entry with `unit`, preserving the ordinary body's result storage.
    CapturesCompleted,
    /// The task observed current-run cancellation.
    Cancelled,
    /// The task panicked with ownership of a panic report.
    Panicked(MirOperand),
}

/// Checked work captured by one newly constructed inactive future.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirFrameInitializer {
    /// Invoke an async callable when the frame is first driven.
    Callable(MirCall),
    /// Resolve generated lifecycle work when the frame is first driven.
    Lifecycle {
        /// Lifecycle step implemented by the generated frame.
        role: crate::MirGeneratedLifecycleRole,
        /// Concrete or substituted owner type.
        ty: TypeId,
        /// Receiver borrowed by the deferred lifecycle computation.
        receiver: MirOperand,
        /// Inactive future and its normal completion type.
        result: bray_bound_tree::BoundFutureConstruction,
    },
}

impl MirFrameInitializer {
    /// Returns the source-visible future type produced by this initializer.
    pub const fn future_type(&self) -> Option<TypeId> {
        match self {
            Self::Callable(call) => match call.result() {
                BoundCallResult::LazyFuture(result) => Some(result.future_type()),
                BoundCallResult::Immediate(_) => None,
            },
            Self::Lifecycle { result, .. } => Some(result.future_type()),
        }
    }
}

/// Explicit protected-frame and task operation selected by checked lowering.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirAsyncOperation {
    /// Try to create an inactive protected frame, returning whether ownership transferred.
    CreateFrame {
        /// Fresh admission or previously secured cleanup capacity.
        storage: MirFrameStorageSource,
        /// Static or existential frame representation.
        frame: MirFrameReference,
        /// Deferred work captured by the frame.
        initializer: MirFrameInitializer,
        /// Future storage initialized only when creation succeeds. Inputs remain owned by the caller on failure.
        destination: MirPlace,
    },
    /// Enter or resume a protected frame state.
    ResumeFrame {
        /// Stable frame representation.
        frame: ProtectedAsyncFrameId,
        /// State being entered.
        state: MirFrameStateId,
        /// Stable frame storage.
        storage: MirStorageId,
        /// Selected private ABI role.
        runtime: MirRuntimeReference,
    },
    /// Compose a child frame directly into its active parent.
    ComposeAwaitedFrame {
        /// Parent protected frame.
        parent: ProtectedAsyncFrameId,
        /// Static or existential child frame representation.
        child: MirFrameReference,
        /// Inactive child frame value.
        frame: MirOperand,
        /// Ordinary execution or cleanup of the inactive captures.
        entry: crate::MirFrameEntry,
    },
    /// Synchronously destroy the captures of a successfully quiesced inactive frame.
    DestroyInactiveCaptures {
        /// Inactive frame whose capture ownership is consumed.
        frame: MirOperand,
        /// Selected private destruction ABI role.
        runtime: MirRuntimeReference,
    },
    /// Attempt to publish a task, returning whether its ownership was transferred.
    StartTask {
        /// Static or existential frame representation.
        frame: MirFrameReference,
        /// Inactive frame borrowed for admission and consumed only on success.
        value: MirOperand,
        /// Task storage initialized only on success.
        destination: MirPlace,
        /// Selected private task-allocation ABI role.
        allocation: MirRuntimeReference,
        /// Selected private task-start ABI role.
        start: MirRuntimeReference,
    },
    /// Request cancellation of an owned task.
    RequestTaskCancellation {
        /// Owned task control state.
        task: MirOperand,
        /// Selected private cancellation ABI role.
        runtime: MirRuntimeReference,
    },
    /// Observe whether cancellation was requested for the current run.
    ObserveCurrentRunCancellation {
        /// Selected private cancellation-observation ABI role.
        runtime: MirRuntimeReference,
    },
    /// Transfer a task's terminal result after its completion suspension has resumed.
    ResolveTask {
        /// Owned task control state.
        task: MirOperand,
        /// Exact compiler-known variants used to form the terminal result.
        variants: crate::MirRunResultVariants,
        /// Selected private terminal-resolution ABI role.
        runtime: MirRuntimeReference,
    },
    /// Exclusively borrow a completed task value, returning a nullable mutable borrow.
    BorrowTaskCompletion {
        /// Borrowed task control state retaining the terminal outcome.
        task: MirOperand,
        /// Selected private completion-borrow ABI role.
        runtime: MirRuntimeReference,
    },
    /// Restore the original task owner's access after its completion borrow ends.
    ReleaseTaskCompletionBorrow {
        /// Task control state whose non-null completion borrow is being released.
        task: MirOperand,
        /// Selected private completion-borrow release ABI role.
        runtime: MirRuntimeReference,
    },
    /// Moves the awaited child's terminal outcome into a represented `RunResult`.
    ResolveAwaitedFrame {
        /// Exact compiler-known variants used to preserve completion, panic, and cancellation.
        variants: crate::MirRunResultVariants,
        /// Selected private awaited-resolution ABI role.
        runtime: MirRuntimeReference,
    },
    /// Publish exactly one task terminal state.
    PublishTerminalState {
        /// Terminal state being published.
        state: MirTaskTerminalState,
        /// Selected private publication ABI role.
        runtime: MirRuntimeReference,
    },
    /// Execute phase-one cancellation broadcast for a protected frame.
    ExecuteCleanupBroadcast {
        /// Stable frame representation.
        frame: ProtectedAsyncFrameId,
        /// Selected private broadcast ABI role.
        runtime: MirRuntimeReference,
    },
    /// Execute phase-two lifecycle resolution for a protected frame.
    ExecuteLifecycleResolution {
        /// Stable frame representation.
        frame: ProtectedAsyncFrameId,
        /// Selected private lifecycle ABI role.
        runtime: MirRuntimeReference,
    },
    /// Transfer ownership of a cleanup incident.
    TransferCleanupIncident {
        /// The lifecycle call or callable frame creation whose Error is being transferred.
        invocation: crate::MirOperationId,
        /// Incident value being transferred.
        incident: MirOperand,
        /// Selected private transfer ABI role.
        runtime: MirRuntimeReference,
    },
    /// Destroy terminal task control state exactly once.
    DestroyTerminalTask {
        /// Terminal task control state.
        task: MirOperand,
        /// Retained completion to abandon in place; absent when its owner already consumed it.
        completion: Option<TypeId>,
    },
}

/// Explicit compiler-generated product-host operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirHostOperation {
    /// Starts the runtime and admits the product execution before static materialization.
    BeginExecution {
        startup: MirRuntimeReference,
        control: MirRuntimeReference,
    },
    /// Materialize one demanded static before executing source entry code.
    MaterializeStatic {
        /// Exact static place whose initializer must complete.
        place: MirPlace,
    },
    /// Select one catalog entry admitted by the test runner.
    SelectTestEntry {
        /// Position of this source entry in the host contract.
        entry: ExecutableHostEntryId,
        /// Selected private test-entry-selection ABI role.
        runtime: MirRuntimeReference,
    },
    /// Establish and execute the selected source root.
    ExecuteRoot {
        /// Position of this source entry in the host contract.
        entry: ExecutableHostEntryId,
        /// Exact source unit selected as the product root.
        root: BoundUnitKey,
        /// Synchronous or protected-frame root execution.
        execution: RootExecution,
        /// Selected private root-execution ABI role.
        runtime: MirRuntimeReference,
    },
    /// Observe the root terminal record.
    ObserveRootTerminal {
        /// Position of this source entry in the host contract.
        entry: ExecutableHostEntryId,
        /// Selected private terminal-observation ABI role.
        runtime: MirRuntimeReference,
    },
    /// Map and release the observed terminal root payload.
    ResolveRootTerminal {
        /// Position of this source entry in the host contract.
        entry: ExecutableHostEntryId,
        /// Recoverable error type whose lifecycle the host resolves after reporting.
        error: Option<TypeId>,
        /// Selected private completion-release ABI role.
        completion: MirRuntimeReference,
        /// Selected private panic-reporting ABI role.
        panic: MirRuntimeReference,
        /// Selected private recoverable-entry-failure reporting ABI role.
        entry_failure: MirRuntimeReference,
    },
    /// Close root selection and begin product-static cleanup.
    BeginStaticCleanup,
    /// Report and destroy cleanup incidents transferred to the host.
    ReportCleanupIncidents {
        /// Selected private cleanup-reporting ABI role.
        runtime: MirRuntimeReference,
    },
    /// Shut product execution infrastructure down in checked order.
    StructuredShutdown {
        /// Selected private shutdown ABI role.
        runtime: MirRuntimeReference,
    },
}
