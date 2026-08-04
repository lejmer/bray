//! Target-independent protected-frame and execution-runtime services.

#![deny(unsafe_code)]

mod cancellation;
#[cfg(test)]
mod conformance;
mod context;
mod event;
mod frame;
mod lane;
mod native;
mod observation;
mod outcome;
mod root;
mod scheduler;
mod shutdown;
mod task;

#[cfg(test)]
mod test_support;

pub use cancellation::{CancellationContext, CancellationShield};
pub use context::{
    TaskExecutionContext, current_run_cancellation_observable,
    current_run_cancellation_requested, current_task_execution_context,
};
pub use event::{
    RuntimeEvent, RuntimeEventError, RuntimeEventGeneration, RuntimeEventRegistration,
    RuntimeEventWake,
};
pub use frame::{
    ErasedProtectedFrame, ErasedSendableProtectedFrame, FrameContext, FrameExit, FrameProgress,
    FrameSuspension, FrameSuspensionKind, ProtectedFrame, RuntimePanic, SendableProtectedFrame,
    erase_protected_frame, erase_sendable_protected_frame, resume_direct,
};
pub use lane::{
    ExecutionLane, ExecutionLanePlacement, ExecutionLaneSelectionError, ExecutionWorkload,
};
pub use observation::{
    CancellationObservation, ScheduledTaskSnapshot, ScheduledTaskState, SchedulerSnapshot,
    TaskSnapshot, TaskStartSite, TaskWakeCause,
};
pub use outcome::{RunOutcome, RunOutcomeKind};
pub use root::{
    RootCancellationHandle, RootExecutionError, execute_async_root, execute_synchronous_root,
};
pub use scheduler::{
    ReadyTask, Scheduler, SchedulerError, SchedulerLimits, TaskRegistration, TaskWakeHandle,
    TimerRegistration,
};
pub use shutdown::{
    CleanupIncident, CleanupIncidentOrigin, CleanupIncidentProducer, CleanupReportSink,
    finish_product_shutdown,
};
pub use task::{
    JoinWaitRegistration, JoinWake, TaskControlBlock, TaskFailureKind, TaskId,
    TaskObservationError, TaskResumeError, TaskResumeStatus, TaskStartError, TaskState,
};
