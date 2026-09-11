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
    /// Each notification restarts the search from a clone of the supplied iterator.
    pub fn wait_ready_from(
        &self,
        lanes: impl Iterator<Item = ExecutionLane> + Clone,
        deadline: Option<MonotonicDeadline>,
    ) -> Result<Option<ReadyTask>, SchedulerError> {
        self.wait_ready_matching(deadline, |state| {
            lanes
                .clone()
                .find_map(|lane| pop_ready(&self.data, state, lane))
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

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use bray_platform::{MonotonicClock, RuntimeThreadScope};
    use bray_runtime_model::{ProtectedFrameStateId, RuntimeCapability};

    use crate::test_support::{TestFrame, register_task};
    use crate::{
        ExecutionLane, ExecutionLanePlacement, ExecutionWorkload, Scheduler, SchedulerLimits,
        TaskControlBlock,
    };

    #[test]
    fn lane_iteration_restarts_after_notification() {
        let runtime = RuntimeThreadScope::enter().unwrap();
        let origin = runtime.runtime().id();

        let scheduler = Scheduler::new(
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MigratableLanes,
            ],
            origin,
            SchedulerLimits::new(NonZeroUsize::new(2).unwrap(), NonZeroUsize::new(1).unwrap()),
        );

        let task = TaskControlBlock::start(TestFrame::completing(1)).unwrap();
        let registration = register_task(&scheduler, &task, origin);
        let selected = registration.lane(ProtectedFrameStateId::new(0)).unwrap();

        let absent = ExecutionLane::new(
            ExecutionLanePlacement::OriginThread(origin),
            ExecutionWorkload::Cooperative,
        );

        let visits = AtomicUsize::new(0);

        let (observed, ready_to_wake) = std::sync::mpsc::sync_channel(1);

        let lanes = [absent, selected].into_iter().inspect(|_| {
            if visits.fetch_add(1, Ordering::Relaxed) == 1 {
                observed.send(()).unwrap();
            }
        });

        std::thread::scope(|scope| {
            let wake = registration.wake_handle();

            scope.spawn(move || {
                ready_to_wake.recv_timeout(Duration::from_secs(5)).unwrap();
                wake.wake().unwrap();
            });

            let ready = scheduler
                .wait_ready_from(lanes, MonotonicClock.deadline_after(Duration::from_secs(5)))
                .unwrap()
                .unwrap();

            assert_eq!(ready.task(), task.id());
            assert!(visits.load(Ordering::Relaxed) >= 4);
            ready.complete().unwrap();
        });
    }
}
