use std::sync::Mutex;
use triomphe::Arc;

use bray_runtime_abi::{NativeRuntimeStatus, NativeTaskHandle};
use bray_runtime_model::ProtectedFrameStateId;

use crate::task::{JoinNotification, TaskWaitWake};
use crate::{JoinWaitRegistration, SchedulerError, TaskControlBlock, TaskWakeHandle};

/// Admission-owned notification storage for a frame's one suspended continuation.
pub(super) struct ContinuationWait {
    wake: Arc<TaskWaitWake<crate::TaskId>>,
    pending: Mutex<
        Option<(
            NativeTaskHandle,
            ProtectedFrameStateId,
            JoinWaitRegistration,
        )>,
    >,
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

        if let Some((existing, existing_state, registration)) = pending.as_ref()
            && (*existing != handle || *existing_state != state)
            && registration.is_pending()
        {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        self.wake.arm(task.id(), state);

        if let Some((existing, existing_state, registration)) = pending.as_ref()
            && *existing == handle
            && *existing_state == state
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

        *pending = Some((handle, state, registration));

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
