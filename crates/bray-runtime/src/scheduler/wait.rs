use bray_platform::{MonotonicDeadline, NativeWaitOutcome};

use crate::ExecutionLane;

use super::contract::SchedulerError;
use super::dispatch::{earliest_deadline, pop_ready};
use super::engine::{ReadyTask, Scheduler, SchedulerState};

impl Scheduler {
    /// Waits for the next compatible task or the supplied deadline.
    pub fn wait_ready(
        &self,
        lane: ExecutionLane,
        deadline: Option<MonotonicDeadline>,
    ) -> Result<Option<ReadyTask>, SchedulerError> {
        self.wait_ready_matching(deadline, |state| pop_ready(&self.data, state, lane))
    }

    /// Waits for the next task compatible with the first ready lane in the supplied order.
    pub fn wait_ready_from(
        &self,
        lanes: &[ExecutionLane],
        deadline: Option<MonotonicDeadline>,
    ) -> Result<Option<ReadyTask>, SchedulerError> {
        self.wait_ready_matching(deadline, |state| {
            lanes
                .iter()
                .find_map(|lane| pop_ready(&self.data, state, *lane))
        })
    }

    fn wait_ready_matching(
        &self,
        deadline: Option<MonotonicDeadline>,
        mut take_ready: impl FnMut(&mut SchedulerState) -> Option<ReadyTask>,
    ) -> Result<Option<ReadyTask>, SchedulerError> {
        loop {
            let observed = self.data.event.observe()?;

            let wait_deadline = {
                let mut state = self.lock_state()?;

                self.promote_elapsed_timers(&mut state)?;

                if let Some(task) = take_ready(&mut state) {
                    return Ok(Some(task));
                }

                earliest_deadline(deadline, state.timers.keys().next().copied())
            };

            match self.data.event.wait(observed, wait_deadline)? {
                NativeWaitOutcome::Woken(_) => {}
                NativeWaitOutcome::TimedOut(_) => {
                    let mut state = self.lock_state()?;

                    self.promote_elapsed_timers(&mut state)?;

                    if let Some(task) = take_ready(&mut state) {
                        return Ok(Some(task));
                    }

                    if deadline.is_some_and(MonotonicDeadline::has_elapsed) {
                        return Ok(None);
                    }
                }
            }
        }
    }
}
