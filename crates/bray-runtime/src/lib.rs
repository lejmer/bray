//! Target-independent protected-frame and execution-runtime services.

#![forbid(unsafe_code)]

mod frame;
mod outcome;
mod task;

#[cfg(test)]
mod test_support;

pub use frame::{
    ErasedProtectedFrame, ErasedSendableProtectedFrame, FrameContext, FrameExit,
    FrameProgress, FrameSuspension, ProtectedFrame, RuntimePanic,
    SendableProtectedFrame, erase_protected_frame, erase_sendable_protected_frame,
    resume_direct,
};
pub use outcome::{RunOutcome, RunOutcomeKind};
pub use task::{
    JoinWake, TaskControlBlock, TaskFailureKind, TaskId, TaskObservationError,
    TaskResumeError, TaskResumeStatus, TaskStartError, TaskState,
};
