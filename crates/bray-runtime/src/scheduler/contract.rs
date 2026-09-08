use std::num::NonZeroUsize;

use bray_platform::PlatformError;
use bray_runtime_model::ProtectedFrameStateId;

use crate::{ExecutionLaneSelectionError, TaskId};

/// Hard scheduler capacities selected by the product host.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SchedulerLimits {
    tasks: NonZeroUsize,
    timers: NonZeroUsize,
}

impl SchedulerLimits {
    /// Creates explicit task and timer capacities.
    pub const fn new(tasks: NonZeroUsize, timers: NonZeroUsize) -> Self {
        Self { tasks, timers }
    }

    /// Returns the maximum registered task count.
    pub const fn tasks(self) -> NonZeroUsize {
        self.tasks
    }

    /// Returns the maximum pending timer count.
    pub const fn timers(self) -> NonZeroUsize {
        self.timers
    }
}

/// A scheduler operation that could not preserve its runtime contract.
#[derive(Debug)]
pub enum SchedulerError {
    /// The hard registered-task capacity was reached.
    TaskCapacityReached,
    /// The hard timer capacity was reached.
    TimerCapacityReached,
    /// Pending timer identities cannot be represented.
    TimerIdentityExhausted,
    /// The task identity is already registered.
    TaskAlreadyRegistered(TaskId),
    /// Terminal completion requires ownership of the task's current running dispatch.
    TaskNotRunning(TaskId),
    /// The task is no longer registered with this scheduler.
    UnknownTask(TaskId),
    /// A wake named a state absent from the task's protected-frame descriptor.
    UnknownFrameState(ProtectedFrameStateId),
    /// A queued or running task was woken for a different suspension state.
    ConflictingWakeState {
        /// State already retained by the scheduler.
        retained: ProtectedFrameStateId,
        /// State supplied by the conflicting wake.
        requested: ProtectedFrameStateId,
    },
    /// Checked requirements could not select a compatible runtime lane.
    LaneSelection(ExecutionLaneSelectionError),
    /// Cancellation-wake identities were exhausted.
    CancellationWakeIdentityExhausted,
    /// A native wait or wake mechanism failed.
    Platform(PlatformError),
    /// Scheduler state was poisoned by an unexpected runtime panic.
    SynchronizationPoisoned,
}

impl From<ExecutionLaneSelectionError> for SchedulerError {
    fn from(error: ExecutionLaneSelectionError) -> Self {
        Self::LaneSelection(error)
    }
}

impl From<crate::cancellation::CancellationWakeRegistrationError> for SchedulerError {
    fn from(_: crate::cancellation::CancellationWakeRegistrationError) -> Self {
        Self::CancellationWakeIdentityExhausted
    }
}

impl From<PlatformError> for SchedulerError {
    fn from(error: PlatformError) -> Self {
        Self::Platform(error)
    }
}
