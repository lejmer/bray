//! Target-independent protected-frame and execution-runtime services.

#![forbid(unsafe_code)]

mod frame;
mod outcome;
mod task;

#[cfg(test)]
mod test_support;

pub use frame::{
    ErasedProtectedFrame, FrameContext, FrameExit, FrameProgress, FrameSuspension,
    ProtectedFrame, RuntimePanic, erase_protected_frame, resume_direct,
};
pub use outcome::{RunOutcome, RunOutcomeKind};
pub use task::{
    JoinWake, TaskControlBlock, TaskId, TaskObservationError, TaskResumeError,
    TaskResumeStatus, TaskStartError, TaskState,
};
