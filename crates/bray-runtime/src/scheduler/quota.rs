use std::sync::Arc;

use super::engine::{SchedulerData, TaskRegistration};
use super::native;
use super::wait::Parking;

/// A lent execution slot with its exclusive enclosing dispatch retained.
pub(crate) struct ExecutionPark {
    scheduler: Arc<SchedulerData>,
    registration: u64,
}

impl TaskRegistration {
    pub(crate) fn park_execution(&self) -> Option<ExecutionPark> {
        let scheduler = self.scheduler.upgrade()?;

        let status = native::bray_runtime_scheduler_slot_release(
            &scheduler.source.reference,
            self.registration,
        );

        match status {
            0 | 2 => None,
            1 => Some(ExecutionPark {
                scheduler,
                registration: self.registration,
            }),
            _ => panic!("Bray slot release returned unknown status {status}"),
        }
    }
}

impl Drop for ExecutionPark {
    fn drop(&mut self) {
        loop {
            let parking = Parking::register(&self.scheduler);

            let status = native::bray_runtime_scheduler_slot_acquisition(
                &self.scheduler.source.reference,
                self.registration,
            );

            match status {
                1 | 2 => return,
                0 => parking.wait(None),
                _ => panic!("Bray slot acquisition returned unknown status {status}"),
            }
        }
    }
}
