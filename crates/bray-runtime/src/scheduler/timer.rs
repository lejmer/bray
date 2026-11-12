use std::sync::{Arc, Weak};
use std::time::Duration;

use bray_runtime_abi::NativeEventCallback;

use super::contract::SchedulerError;
use super::deadline::{self, BrayDeadlines};
use super::engine::{Scheduler, SchedulerData, TaskWakeHandle, WakeOwner};
use crate::event::NativeCallback;

impl Scheduler {
    /// Registers a relative monotonic deadline without claiming the task's result.
    pub fn register_timer(
        &self,
        duration: Duration,
        wake: &TaskWakeHandle,
    ) -> Result<TimerRegistration, SchedulerError> {
        let (task, registration, scheduler) = match &wake.owner {
            WakeOwner::Scheduler {
                task,
                registration,
                scheduler,
            } => (task, registration, scheduler),
            WakeOwner::Native(cancellation) => {
                return Err(SchedulerError::UnknownTask(
                    crate::TaskId::from_bray_handle(cancellation.native_reference().task()),
                ));
            }
        };

        if !Weak::ptr_eq(scheduler, &Arc::downgrade(&self.data)) {
            return Err(SchedulerError::UnknownTask(*task));
        }

        // The timer callback retains the same cancellation owner until its
        // committed wake or withdrawal, independently of this registry lookup.
        let cancellation = {
            let state = self.lock_state()?;

            state
                .tasks
                .get(task)
                .filter(|task| task.registration == *registration)
                .ok_or(SchedulerError::UnknownTask(*task))?
                .cancellation
                .clone()
        };

        let deadline = deadline::deadline_after(duration)?;

        let resources = NativeCallback::task(cancellation, wake.clone())
            .map_err(SchedulerError::TimerResources)?;

        let descriptor = resources.retain_timer();

        // Source admission invokes no callbacks. Serialize publication with task
        // retirement so no deadline can be published after its scope is withdrawn.
        let result = (|| {
            let deadlines = self.deadlines()?;
            let state = self.lock_state()?;

            if !state
                .tasks
                .get(task)
                .is_some_and(|task| task.registration == *registration)
            {
                return Err(SchedulerError::UnknownTask(*task));
            }

            deadlines.register(deadline, *registration, &descriptor)
        })();

        self.finish_registration(result, descriptor)
    }

    fn finish_registration(
        &self,
        result: Result<u64, SchedulerError>,
        descriptor: NativeEventCallback,
    ) -> Result<TimerRegistration, SchedulerError> {
        match result {
            Ok(identity) => Ok(TimerRegistration {
                identity,
                owner: TimerOwner::Scheduler(Arc::clone(&self.data)),
            }),
            Err(error) => {
                // Rejection leaves the descriptor with its native producer.
                let status = (descriptor.call)(descriptor.context, 0);

                assert!(
                    status.is_success(),
                    "rejected deadline releases its callback resources"
                );

                Err(error)
            }
        }
    }

    fn deadlines(&self) -> Result<&BrayDeadlines, SchedulerError> {
        if let Some(deadlines) = self.data.deadlines.get() {
            return Ok(deadlines);
        }

        let prepared =
            BrayDeadlines::new(&self.data.source.reference, self.data.limits.timers().get())?;

        let _ = self.data.deadlines.set(prepared);

        Ok(self
            .data
            .deadlines
            .get()
            .expect("a competing deadline owner is published"))
    }

    pub(super) fn promote_elapsed_timers(&self) -> Result<(), SchedulerError> {
        self.data
            .deadlines
            .get()
            .map_or(Ok(()), BrayDeadlines::dispatch)
    }

    pub(super) fn timer_remaining(&self) -> Result<Option<Duration>, SchedulerError> {
        self.data
            .deadlines
            .get()
            .map_or(Ok(None), BrayDeadlines::remaining)
    }

    pub(crate) fn close_timers(&self) {
        super::native::close_deadlines(&self.data.source.reference);
    }
}

/// Ownership of one pending deadline. Dropping withdraws it unless delivery committed.
#[derive(Debug)]
pub struct TimerRegistration {
    identity: u64,
    owner: TimerOwner,
}

#[derive(Debug)]
enum TimerOwner {
    Scheduler(Arc<SchedulerData>),
    Execution(Arc<crate::event::Resources>),
}

impl TimerRegistration {
    pub(crate) fn execution(identity: u64, owner: Arc<crate::event::Resources>) -> Self {
        assert!(
            matches!(owner.as_ref(), crate::event::Resources::Timeout { .. }),
            "an execution timer retains its actual timeout resources"
        );

        Self {
            identity,
            owner: TimerOwner::Execution(owner),
        }
    }
}

impl Drop for TimerRegistration {
    fn drop(&mut self) {
        match &self.owner {
            TimerOwner::Scheduler(scheduler) => scheduler
                .deadlines
                .get()
                .expect("a registered timer retains its deadline owner")
                .withdraw(self.identity),
            TimerOwner::Execution(owner) => {
                let crate::event::Resources::Timeout { runtime, .. } = owner.as_ref() else {
                    unreachable!("execution registration retains its timeout owner");
                };

                crate::timeout::withdraw(runtime, self.identity);
            }
        }
    }
}
