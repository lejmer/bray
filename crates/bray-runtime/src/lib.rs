//! Target-independent protected-frame and execution-runtime services.

#![forbid(unsafe_code)]

mod cancellation;
mod context;
mod frame;
mod lane;
mod outcome;
mod root;
mod scheduler;
mod shutdown;
mod task;

#[cfg(test)]
mod test_support;

pub use cancellation::{CancellationContext, CancellationShield};
pub use context::{
    TaskExecutionContext, current_run_cancellation_requested,
    current_task_execution_context,
};
pub use frame::{
    ErasedProtectedFrame, ErasedSendableProtectedFrame, FrameContext, FrameExit, FrameProgress,
    FrameSuspension, ProtectedFrame, RuntimePanic, SendableProtectedFrame, erase_protected_frame,
    erase_sendable_protected_frame, resume_direct,
};
pub use lane::{
    ExecutionLane, ExecutionLanePlacement, ExecutionLaneSelectionError, ExecutionWorkload,
};
pub use outcome::{RunOutcome, RunOutcomeKind};
pub use root::{
    RootCancellationHandle, RootExecutionError, execute_async_root,
    execute_synchronous_root,
};
pub use scheduler::{
    ReadyTask, Scheduler, SchedulerError, SchedulerLimits, TaskRegistration, TaskWakeHandle,
};
pub use shutdown::{
    CleanupIncident, CleanupIncidentOrigin, CleanupIncidentProducer,
    CleanupReportSink, finish_product_shutdown,
};
pub use task::{
    JoinWake, TaskControlBlock, TaskFailureKind, TaskId, TaskObservationError, TaskResumeError,
    TaskResumeStatus, TaskStartError, TaskState,
};
