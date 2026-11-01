use std::sync::{Arc, Weak};
use std::time::Duration;

use bray_runtime_abi::NativeEventCallback;

use super::contract::SchedulerError;
use super::deadline::{self, BrayDeadlines};
use super::engine::{Scheduler, SchedulerData, TaskWakeHandle};
use crate::event::NativeCallback;

impl Scheduler {
    /// Registers a relative monotonic deadline without claiming the task's result.
    pub fn register_timer(
        &self,
        duration: Duration,
        wake: &TaskWakeHandle,
    ) -> Result<TimerRegistration, SchedulerError> {
        if !Weak::ptr_eq(&wake.scheduler, &Arc::downgrade(&self.data)) {
            return Err(SchedulerError::UnknownTask(wake.task));
        }

        let (native, cancellation) = {
            let state = self.lock_state()?;

            let task = state.tasks.get(&wake.task)
                .filter(|task| task.registration == wake.registration)
                .ok_or(SchedulerError::UnknownTask(wake.task))?;

            (task.native.clone(), task.cancellation.clone())
        };

        let deadline = deadline::deadline_after(duration)?;

        let resources = if native.is_none() {
            Some(NativeCallback::task(cancellation, wake.clone()).map_err(SchedulerError::TimerResources)?)
        } else {
            None
        };

        let descriptor = native.as_ref().map_or_else(
            || resources.as_ref().expect("host task admits callback resources").retain_timer(),
            |native| native.callback.retain_timer(),
        );

        // Source admission invokes no callbacks. Serialize publication with task
        // retirement so no deadline can be published after its scope is withdrawn.
        let result = (|| {
            let deadlines = self.deadlines()?;
            let state = self.lock_state()?;

            if !state.tasks.get(&wake.task).is_some_and(|task| task.registration == wake.registration) {
                return Err(SchedulerError::UnknownTask(wake.task));
            }

            deadlines.register(deadline, wake.registration, &descriptor)
        })();

        self.finish_registration(result, descriptor)
    }

    pub(crate) fn register_callback(
        &self,
        deadline_ns: u64,
        descriptor: NativeEventCallback,
    ) -> Result<TimerRegistration, SchedulerError> {
        let result = self.deadlines().and_then(|deadlines| deadlines.register(deadline_ns, 0, &descriptor));

        self.finish_registration(result, descriptor)
    }

    fn finish_registration(
        &self,
        result: Result<u64, SchedulerError>,
        descriptor: NativeEventCallback,
    ) -> Result<TimerRegistration, SchedulerError> {
        match result {
            Ok(identity) => Ok(TimerRegistration { identity, scheduler: Arc::clone(&self.data) }),
            Err(error) => {
                // Rejection leaves the descriptor with its native producer.
                let status = (descriptor.call)(descriptor.context, 0);

                assert!(status.is_success(), "rejected deadline releases its callback resources");

                Err(error)
            }
        }
    }

    fn deadlines(&self) -> Result<&BrayDeadlines, SchedulerError> {
        if let Some(deadlines) = self.data.deadlines.get() {
            return Ok(deadlines);
        }

        let prepared = BrayDeadlines::new(&self.data.source.reference, self.data.limits.timers().get())?;
        let _ = self.data.deadlines.set(prepared);

        Ok(self.data.deadlines.get().expect("a competing deadline owner is published"))
    }

    pub(super) fn promote_elapsed_timers(&self) -> Result<(), SchedulerError> {
        self.data.deadlines.get().map_or(Ok(()), BrayDeadlines::dispatch)
    }

    pub(super) fn timer_remaining(&self) -> Result<Option<Duration>, SchedulerError> {
        self.data.deadlines.get().map_or(Ok(None), BrayDeadlines::remaining)
    }

    pub(crate) fn close_timers(&self) {
        super::native::close_deadlines(&self.data.source.reference);
    }
}

/// Ownership of one pending deadline. Dropping withdraws it unless delivery committed.
#[derive(Debug)]
pub struct TimerRegistration {
    identity: u64,
    scheduler: Arc<SchedulerData>,
}

impl Drop for TimerRegistration {
    fn drop(&mut self) {
        self.scheduler.deadlines.get().expect("a registered timer retains its deadline owner").withdraw(self.identity);
    }
}
