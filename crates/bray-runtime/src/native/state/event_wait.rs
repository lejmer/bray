use std::sync::Mutex;
use triomphe::Arc;

use bray_runtime_model::ProtectedFrameStateId;

use crate::event::{EventNotification, ReservedEventWait};
use crate::task::TaskWaitWake;
use crate::{
    RuntimeEvent, RuntimeEventError, RuntimeEventGeneration, RuntimeEventRegistration,
    SchedulerError, TaskStartError, TaskWakeHandle,
};

/// One task's admission-owned event registration and delayed-notification guard.
pub(super) struct EventWait {
    reserved: ReservedEventWait,
    wake: Arc<TaskWaitWake<(usize, RuntimeEventGeneration)>>,
    pending: Mutex<Option<RuntimeEventRegistration>>,
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
        state: ProtectedFrameStateId,
    ) -> Result<(), RuntimeEventError> {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if pending.is_some() {
            return Err(RuntimeEventError::AlreadyRegistered);
        }

        let (generation, _) = event.observation()?;

        let source = (identity, generation);
        self.wake.arm(source, state);

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
                *pending = Some(registration);

                Ok(())
            }
            Err(error) => {
                self.wake.clear();

                Err(error)
            }
        }
    }

    pub(super) fn disarm(&self) -> Result<(), SchedulerError> {
        // Serialize withdrawal with rearming and reject already-selected old notifications.
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        self.wake.disarm()?;
        pending.take();

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;

    use bray_platform::RuntimeThreadScope;
    use bray_runtime_model::{ProtectedFrameStateId, RuntimeCapability};

    use super::EventWait;
    use crate::test_support::{TestFrame, register_task, with_allocation_failure};
    use crate::{FrameSuspension, RuntimeEvent, Scheduler, SchedulerLimits, TaskControlBlock};

    #[test]
    fn admitted_event_storage_rearms_without_allocation_and_rejects_old_notifications() {
        let wait = EventWait::reserve().unwrap();
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
        let observed = event.observation().unwrap().0;

        with_allocation_failure(|| {
            wait.register(&event, 1, initial).unwrap();
            event.signal().unwrap();

            let ready = scheduler
                .take_ready(registration.lane(initial).unwrap())
                .unwrap()
                .unwrap();

            // A previously selected notification can arrive after dequeue and after rearming.
            wait.wake.notify((1, observed));
            wait.disarm().unwrap();
            wait.register(&replacement, 2, resumed).unwrap();
            wait.wake.notify((1, observed));
            ready.suspend(FrameSuspension::new(resumed)).unwrap();

            assert!(
                scheduler
                    .take_ready(registration.lane(resumed).unwrap())
                    .unwrap()
                    .is_none()
            );

            replacement.signal().unwrap();

            let ready = scheduler
                .take_ready(registration.lane(resumed).unwrap())
                .unwrap()
                .unwrap();

            wait.disarm().unwrap();

            // Reusing the same event distinguishes its previous observed generation.
            let replacement_observed = replacement.observation().unwrap().0;
            wait.register(&replacement, 2, resumed).unwrap();
            wait.wake.notify((2, observed));
            ready.suspend(FrameSuspension::new(resumed)).unwrap();

            assert!(
                scheduler
                    .take_ready(registration.lane(resumed).unwrap())
                    .unwrap()
                    .is_none()
            );

            replacement.close().unwrap();

            let ready = scheduler
                .take_ready(registration.lane(resumed).unwrap())
                .unwrap()
                .unwrap();

            wait.disarm().unwrap();
            wait.wake.notify((2, replacement_observed));
            ready.complete().unwrap();

            assert!(
                scheduler
                    .take_ready(registration.lane(resumed).unwrap())
                    .unwrap()
                    .is_none()
            );
        });
    }
}
