use bray_platform::{MonotonicDeadline, RuntimeThread, RuntimeThreadId};

use super::contract::SchedulerError;
use super::dispatch::{self, ReadyTask};
use super::engine::{Scheduler, SchedulerData};
use super::native;
use crate::ExecutionLane;

impl Scheduler {
    /// Waits for ready work with fair turns across the acquired lanes.
    /// The caller retains the current thread's attachment throughout the wait.
    pub fn wait_ready(
        &self,
        thread: &RuntimeThread,
        lanes: impl IntoIterator<Item = ExecutionLane>,
        deadline: Option<MonotonicDeadline>,
    ) -> Result<Option<ReadyTask>, SchedulerError> {
        let thread = thread.id();

        let lanes = lanes
            .into_iter()
            .fold(0, |mask, lane| mask | native::lane_bit(lane, thread));

        self.wait_ready_matching(thread, lanes, deadline)
    }

    fn wait_ready_matching(
        &self,
        thread: RuntimeThreadId,
        lanes: u32,
        deadline: Option<MonotonicDeadline>,
    ) -> Result<Option<ReadyTask>, SchedulerError> {
        loop {
            let parking = Parking::register(&self.data);

            self.promote_elapsed_timers()?;

            if let Some(task) = dispatch::pop_ready(&self.data, thread, lanes)? {
                return Ok(Some(task));
            }

            if deadline.is_some_and(MonotonicDeadline::has_elapsed) {
                return Ok(None);
            }

            let wait_deadline = match (deadline, self.next_deadline()?) {
                (Some(first), Some(second)) => Some(first.min(second)),
                (deadline, None) | (None, deadline) => deadline,
            };

            parking.wait(wait_deadline);
        }
    }
}

// Registration precedes every condition observation. Its borrowed domain stays
// live until withdrawal acknowledges any broadcast committed while checking.
pub(super) struct Parking<'a> {
    scheduler: &'a SchedulerData,
    group: u32,
}

impl<'a> Parking<'a> {
    pub(super) fn register(scheduler: &'a SchedulerData) -> Self {
        let group =
            native::bray_runtime_scheduler_parking_registration(&scheduler.source.reference);

        Self { scheduler, group }
    }

    pub(super) fn wait(&self, deadline: Option<MonotonicDeadline>) {
        let milliseconds = deadline.map_or(u32::MAX - 1, |deadline| {
            let remaining = deadline.remaining().as_nanos().div_ceil(1_000_000);

            u32::try_from(remaining)
                .unwrap_or(u32::MAX - 1)
                .min(u32::MAX - 1)
        });

        native::bray_runtime_scheduler_parking(
            &self.scheduler.source.reference,
            self.group,
            milliseconds,
        );
    }
}

impl Drop for Parking<'_> {
    fn drop(&mut self) {
        native::bray_runtime_scheduler_parking_withdrawal(
            &self.scheduler.source.reference,
            self.group,
        );
    }
}
