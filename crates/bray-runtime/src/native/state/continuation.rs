use std::sync::Mutex;
use triomphe::Arc;

use bray_runtime_abi::{NativeRuntimeStatus, NativeTaskHandle};
use bray_runtime_model::ProtectedFrameStateId;

use crate::task::{JoinNotification, TaskWaitWake};
use crate::{JoinWaitRegistration, SchedulerError, TaskControlBlock, TaskWakeHandle};

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
        state: ProtectedFrameStateId,
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

        self.wake.arm(task.id(), state);

        // Cancellation drains the same child at a new resume state using its admitted waiter.
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

    pub(super) fn clear(&self) {
        self.pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();

        self.wake.clear();
    }

    pub(super) fn disarm(&self) -> Result<(), SchedulerError> {
        self.wake.disarm()
    }
}

#[cfg(test)]
mod tests {
    use super::ContinuationWait;
    use crate::test_support::{TestFrame, with_allocation_failure};
    use crate::{CancellationContext, ProtectedFrame, TaskControlBlock};
    use bray_runtime_abi::{NativeRuntimeStatus, NativeTaskHandle};
    use bray_runtime_model::ProtectedFrameStateId;

    #[test]
    fn cancellation_retargets_the_pending_child_wait_without_new_admission() {
        let descriptor = TestFrame::suspending_then_completing(0)
            .descriptor()
            .clone();

        let child =
            TaskControlBlock::<usize>::prepare(CancellationContext::root().unwrap(), descriptor)
                .unwrap();

        let wait = ContinuationWait::reserve().unwrap();
        let child_handle = NativeTaskHandle::new(1).unwrap();
        let replacement = NativeTaskHandle::new(2).unwrap();
        let ordinary = ProtectedFrameStateId::new(0);
        let cleanup = ProtectedFrameStateId::new(1);

        assert_eq!(
            wait.register(child_handle, &child, ordinary),
            NativeRuntimeStatus::SUCCESS
        );

        with_allocation_failure(|| {
            assert_eq!(
                wait.register(replacement, &child, cleanup),
                NativeRuntimeStatus::RUNTIME_FAILURE
            );

            assert_eq!(
                wait.register(child_handle, &child, cleanup),
                NativeRuntimeStatus::SUCCESS
            );
        });

        let pending = wait.pending.lock().unwrap();

        let (handle, registration) = pending.as_ref().unwrap();

        assert_eq!(*handle, child_handle);
        assert!(registration.is_pending());
    }
}
