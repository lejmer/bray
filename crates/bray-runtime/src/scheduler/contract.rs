use std::num::NonZeroUsize;

use crate::{ExecutionLaneSelectionError, TaskId};

/// Execution and pending-timer limits selected by the product host.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SchedulerLimits {
    tasks: NonZeroUsize,
    timers: NonZeroUsize,
}

impl SchedulerLimits {
    /// Sets the running-task quota and pending-timer capacity.
    pub const fn new(tasks: NonZeroUsize, timers: NonZeroUsize) -> Self {
        Self { tasks, timers }
    }

    /// Returns the maximum number of simultaneously executing tasks.
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
    /// Native binding or pending-deadline storage could not be admitted.
    ReadyStorageUnavailable(std::collections::TryReserveError),
    /// Stable Bray scheduler backing could not be allocated.
    BackingUnavailable,
    /// Scheduler registration generations or indices were exhausted.
    RegistrationIdentityExhausted,
    /// The hard timer capacity was reached.
    TimerCapacityReached,
    /// Pending timer identities cannot be represented.
    TimerIdentityExhausted,
    /// The task identity is already registered.
    TaskAlreadyRegistered(TaskId),
    /// The task is no longer registered with this scheduler.
    UnknownTask(TaskId),
    /// Checked requirements could not select a compatible runtime lane.
    LaneSelection(ExecutionLaneSelectionError),
    /// Cancellation-wake identities were exhausted.
    CancellationWakeIdentityExhausted,
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
