use std::sync::Mutex;
use triomphe::Arc;

use crate::event::{EventNotification, ReservedEventWait};
use crate::task::TaskWaitWake;
use crate::{
    RuntimeEvent, RuntimeEventError, RuntimeEventGeneration, RuntimeEventRegistration,
    TaskStartError, TaskWakeHandle,
};

/// One task's admission-owned event registration and delayed-notification guard.
pub(super) struct EventWait {
    reserved: ReservedEventWait,
    wake: Arc<TaskWaitWake<(usize, RuntimeEventGeneration)>>,
    pending: Mutex<
        Option<(
            RuntimeEvent,
            RuntimeEventGeneration,
            RuntimeEventRegistration,
        )>,
    >,
}

impl EventWait {
    pub(super) fn reserve() -> Result<Self, TaskStartError> {
        Ok(Self {
            reserved: ReservedEventWait::reserve().map_err(|_| TaskStartError::AllocationFailed)?,
            wake: TaskWaitWake::reserve()?,
            pending: Mutex::new(None),
        })
    }

    pub(super) fn bind(&self, wake: TaskWakeHandle) {
        self.wake.bind(wake);
    }

    pub(super) fn register(
        &self,
        event: &RuntimeEvent,
        identity: usize,
    ) -> Result<(), RuntimeEventError> {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if pending.is_some() {
            return Err(RuntimeEventError::AlreadyRegistered);
        }

        let (generation, _) = event.observation();

        let source = (identity, generation);
        self.wake.arm(source);

        let registration = event.register_reserved(
            generation,
            &self.reserved,
            EventNotification::Task {
                wake: Arc::clone(&self.wake),
                source,
            },
        );

        match registration {
            Ok(registration) => {
                // Retain the event while suspended so readiness remains observable.
                *pending = Some((event.clone(), generation, registration));

                Ok(())
            }
            Err(error) => {
                self.wake.clear();

                Err(error)
            }
        }
    }

    pub(super) fn is_ready(&self) -> bool {
        let pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some((event, observed, _)) = pending.as_ref() else {
            return true;
        };

        let (generation, closed) = event.observation();

        closed || generation != *observed
    }

    pub(super) fn disarm(&self) {
        // Serialize withdrawal with rearming and reject already-selected old notifications.
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        self.wake.clear();
        pending.take();
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use bray_platform::RuntimeThreadScope;
    use bray_runtime_model::{ProtectedFrameStateId, RuntimeCapability};

    use super::EventWait;
    use crate::test_support::{TestFrame, register_task, with_allocation_failure};
    use crate::{RuntimeEvent, Scheduler, SchedulerLimits, TaskControlBlock};

    #[test]
    fn admitted_event_storage_rearms_without_allocation_and_rejects_old_notifications() {
        let wait = EventWait::reserve().unwrap();
        assert!(wait.is_ready());
        let runtime = RuntimeThreadScope::enter().unwrap();
        let thread = runtime.runtime().id();

        let scheduler = Scheduler::new(
            [
                RuntimeCapability::CooperativeExecution,
                RuntimeCapability::MigratableLanes,
            ],
            thread,
            SchedulerLimits::new(NonZeroUsize::new(1).unwrap(), NonZeroUsize::new(1).unwrap()),
        );

        let task = TaskControlBlock::start(TestFrame::suspending_then_completing(0)).unwrap();
        let registration = register_task(&scheduler, &task, thread);
        with_allocation_failure(|| wait.bind(registration.wake_handle()));
        let event = RuntimeEvent::new();
        let replacement = RuntimeEvent::new();
        let initial = ProtectedFrameStateId::new(0);
        let resumed = ProtectedFrameStateId::new(1);

        let execution = crate::FrameExecutionState::new(
            task.descriptor().frame(),
            task.descriptor().state(resumed).unwrap().clone(),
        );

        let observed = event.observation().0;

        with_allocation_failure(|| {
            wait.register(&event, 1).unwrap();
            assert!(!wait.is_ready());
            event.signal().unwrap();
            assert!(wait.is_ready());

            let ready = scheduler
                .take_ready(registration.lane(initial).unwrap())
                .unwrap()
                .unwrap();

            // A previously selected notification can arrive after dequeue and after rearming.
            wait.wake.notify((1, observed));
            wait.disarm();
            assert!(wait.is_ready());
            wait.register(&replacement, 2).unwrap();
            assert!(!wait.is_ready());
            wait.wake.notify((1, observed));
            ready.suspend(execution.clone()).unwrap();

            // A selected old notification requests a check without completing this wait.
            let ready = scheduler
                .take_ready(registration.lane(resumed).unwrap())
                .unwrap()
                .unwrap();

            assert!(!wait.is_ready());
            ready.suspend(execution.clone()).unwrap();

            assert!(
                scheduler
                    .take_ready(registration.lane(resumed).unwrap())
                    .unwrap()
                    .is_none()
            );

            replacement.signal().unwrap();
            assert!(wait.is_ready());

            let ready = scheduler
                .take_ready(registration.lane(resumed).unwrap())
                .unwrap()
                .unwrap();

            wait.disarm();
            assert!(wait.is_ready());

            // Reusing the same event distinguishes its previous observed generation.
            let replacement_observed = replacement.observation().0;
            wait.register(&replacement, 2).unwrap();
            assert!(!wait.is_ready());
            wait.wake.notify((2, observed));
            ready.suspend(execution.clone()).unwrap();

            assert!(
                scheduler
                    .take_ready(registration.lane(resumed).unwrap())
                    .unwrap()
                    .is_none()
            );

            replacement.close();
            assert!(wait.is_ready());

            let ready = scheduler
                .take_ready(registration.lane(resumed).unwrap())
                .unwrap()
                .unwrap();

            wait.disarm();
            assert!(wait.is_ready());
            wait.wake.notify((2, replacement_observed));

            {
                let execution = ready.execution_state().clone();

                ready.complete(execution)
            }
            .unwrap();

            assert!(
                scheduler
                    .take_ready(registration.lane(resumed).unwrap())
                    .unwrap()
                    .is_none()
            );
        });
    }
}
