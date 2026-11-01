use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::sync::{Arc, Weak};

use bray_platform::MonotonicDeadline;

use super::contract::SchedulerError;
use super::engine::{Scheduler, SchedulerData, TaskWakeHandle};
use crate::{TaskId, TaskWakeCause};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct Timer {
    deadline: MonotonicDeadline,
    identity: u64,
    pub(super) task: TaskId,
    registration: u64,
}

impl Scheduler {
    /// Registers an ordered deadline wake with the existing timer owner.
    pub fn register_timer(
        &self,
        deadline: MonotonicDeadline,
        wake: &TaskWakeHandle,
    ) -> Result<TimerRegistration, SchedulerError> {
        if !Weak::ptr_eq(&wake.scheduler, &Arc::downgrade(&self.data)) {
            return Err(SchedulerError::UnknownTask(wake.task));
        }

        let mut prepared = BinaryHeap::new();

        loop {
            let mut state = self.lock_state()?;

            if !state
                .tasks
                .get(&wake.task)
                .is_some_and(|task| task.registration == wake.registration)
            {
                return Err(SchedulerError::UnknownTask(wake.task));
            }

            if state.timers.len() == self.data.limits.timers().get() {
                return Err(SchedulerError::TimerCapacityReached);
            }

            let identity = state.next_timer;

            let next = identity
                .checked_add(1)
                .ok_or(SchedulerError::TimerIdentityExhausted)?;

            let required = state.timers.len() + 1;

            if state.timers.capacity() < required {
                if prepared.capacity() < required {
                    drop(state);

                    prepared
                        .try_reserve(required)
                        .map_err(SchedulerError::ReadyStorageUnavailable)?;

                    continue;
                }

                prepared.extend(state.timers.drain());
                std::mem::swap(&mut prepared, &mut state.timers);
            }

            state.next_timer = next;

            state.timers.push(Reverse(Timer {
                task: wake.task,
                registration: wake.registration,
                deadline,
                identity,
            }));

            drop(state);
            self.wake_waiters();

            return Ok(TimerRegistration {
                identity,
                scheduler: Arc::downgrade(&self.data),
            });
        }
    }

    pub(super) fn promote_elapsed_timers(&self) -> Result<(), SchedulerError> {
        loop {
            let timer = {
                let mut state = self.lock_state()?;

                if !state
                    .timers
                    .peek()
                    .is_some_and(|Reverse(timer)| timer.deadline.has_elapsed())
                {
                    return Ok(());
                }

                state
                    .timers
                    .pop()
                    .expect("the elapsed deadline remains present")
                    .0
            };

            let wake = TaskWakeHandle {
                task: timer.task,
                registration: timer.registration,
                scheduler: Arc::downgrade(&self.data),
            };

            match wake.wake_with_cause(TaskWakeCause::Timer) {
                Ok(_) | Err(SchedulerError::UnknownTask(_)) => {}
                Err(error) => return Err(error),
            }
        }
    }

    pub(super) fn next_deadline(&self) -> Result<Option<MonotonicDeadline>, SchedulerError> {
        Ok(self
            .lock_state()?
            .timers
            .peek()
            .map(|Reverse(timer)| timer.deadline))
    }
}

/// Ownership of a pending deadline wake. Dropping withdraws its registration.
#[derive(Debug)]
pub struct TimerRegistration {
    identity: u64,
    scheduler: Weak<SchedulerData>,
}

impl Drop for TimerRegistration {
    fn drop(&mut self) {
        if let Some(scheduler) = self.scheduler.upgrade() {
            let mut state = scheduler
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            state
                .timers
                .retain(|Reverse(timer)| timer.identity != self.identity);
        }
    }
}
