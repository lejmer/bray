use std::sync::Mutex;
use triomphe::Arc;

use bray_runtime_abi::{NativeRuntimeStatus, NativeTaskHandle};

use crate::task::{JoinNotification, TaskWaitWake};
use crate::{JoinWaitRegistration, TaskControlBlock, TaskWakeHandle};

/// Admission-owned notification storage for a frame's one suspended continuation.
pub(super) struct ContinuationWait {
    wake: Arc<TaskWaitWake<crate::TaskId>>,
    pending: Mutex<Option<(NativeTaskHandle, JoinWaitRegistration)>>,
}

impl ContinuationWait {
    pub(super) fn reserve() -> Result<Self, crate::TaskStartError> {
        Ok(Self {
            wake: TaskWaitWake::reserve()?,
            pending: Mutex::new(None),
        })
    }

    pub(super) fn bind(&self, wake: TaskWakeHandle) {
        self.wake.bind(wake);
    }

    pub(super) fn register(
        &self,
        handle: NativeTaskHandle,
        task: &TaskControlBlock<usize>,
    ) -> NativeRuntimeStatus {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some((existing, registration)) = pending.as_ref()
            && *existing != handle
            && registration.is_pending()
        {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        self.wake.arm(task.id());

        // Cancellation drains the same child after cancellation delivery using its admitted waiter.
        if let Some((existing, registration)) = pending.as_ref()
            && *existing == handle
            && registration.is_pending()
        {
            return NativeRuntimeStatus::SUCCESS;
        }

        pending.take();

        let wake = JoinNotification::Continuation(Arc::clone(&self.wake));

        let registration = match task.register_owner_waiter(wake) {
            Ok(registration) => registration,
            Err(_) => return NativeRuntimeStatus::RUNTIME_FAILURE,
        };

        *pending = Some((handle, registration));

        NativeRuntimeStatus::SUCCESS
    }

    pub(super) fn is_ready(&self) -> bool {
        let pending = self
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        !self.wake.is_armed()
            || pending
                .as_ref()
                .is_none_or(|(_, registration)| !registration.is_pending())
    }

    pub(super) fn clear(&self) {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();

        self.wake.clear();
    }

    pub(super) fn disarm(&self) {
        self.wake.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::ContinuationWait;
    use crate::test_support::{TestFrame, register_task, with_allocation_failure};
    use crate::{CancellationContext, ProtectedFrame, TaskControlBlock};
    use crate::{Scheduler, SchedulerLimits};
    use bray_platform::RuntimeThreadScope;
    use bray_runtime_abi::{NativeRuntimeStatus, NativeTaskHandle};
    use bray_runtime_model::RuntimeCapability;
    use std::num::NonZeroUsize;

    #[test]
    fn cancellation_retargets_the_pending_child_wait_without_new_admission() {
        let descriptor = TestFrame::suspending_then_completing(0)
            .descriptor()
            .clone();

        let child =
            TaskControlBlock::<usize>::prepare(CancellationContext::root().unwrap(), descriptor)
                .unwrap();

        let wait = ContinuationWait::reserve().unwrap();
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

        let parent = TaskControlBlock::start(TestFrame::suspending_then_completing(0)).unwrap();
        let registration = register_task(&scheduler, &parent, thread);
        wait.bind(registration.wake_handle());
        let child_handle = NativeTaskHandle::new(1).unwrap();
        let replacement = NativeTaskHandle::new(2).unwrap();

        assert_eq!(
            wait.register(child_handle, &child),
            NativeRuntimeStatus::SUCCESS
        );

        assert!(!wait.is_ready());

        with_allocation_failure(|| {
            wait.disarm();
            assert!(wait.is_ready());

            assert!(
                wait.pending
                    .lock()
                    .unwrap()
                    .as_ref()
                    .unwrap()
                    .1
                    .is_pending()
            );

            assert_eq!(
                wait.register(replacement, &child),
                NativeRuntimeStatus::RUNTIME_FAILURE
            );

            assert_eq!(
                wait.register(child_handle, &child),
                NativeRuntimeStatus::SUCCESS
            );
        });

        assert!(!wait.is_ready());
        let pending = wait.pending.lock().unwrap();

        let (handle, registration) = pending.as_ref().unwrap();

        assert_eq!(*handle, child_handle);
        assert!(registration.is_pending());
        drop(pending);
        assert!(!wait.is_ready());

        drop(child);
        assert!(wait.is_ready());
        wait.clear();
        assert!(wait.is_ready());
    }
}
