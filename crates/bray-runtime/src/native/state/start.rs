use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use bray_runtime_abi::{NativeRuntimeStatus, NativeTaskHandle};
use bray_runtime_model::ProtectedFrameStateId;

use crate::task::TaskAdmission;
use crate::{CancellationContext, ExecutionLanePlacement, erase_sendable_protected_frame};

use super::super::frame::{NativeFrame, NativeFrameTransfer};
use super::core::{NativeRuntime, NativeTaskSlot, StartedTask};

struct TaskStart<'runtime> {
    runtime: &'runtime NativeRuntime,
    handle: NativeTaskHandle,
    committed: bool,
    admission: crate::task::TaskAdmissionKind,
}

impl<'runtime> TaskStart<'runtime> {
    fn claim(runtime: &'runtime NativeRuntime, handle: NativeTaskHandle) -> Option<Self> {
        let mut tasks = runtime
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let slot = tasks.get_mut(&handle)?;

        let NativeTaskSlot::Allocated(admission) = *slot else {
            return None;
        };

        *slot = NativeTaskSlot::Starting(admission);

        Some(Self {
            runtime,
            handle,
            committed: false,
            admission,
        })
    }
}

impl Drop for TaskStart<'_> {
    fn drop(&mut self) {
        if self.committed {
            return;
        }

        let mut tasks = self
            .runtime
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let removed = tasks.remove(&self.handle);

        if let Some(slot) = &removed {
            self.runtime.release_admission(slot);
        }

        drop(tasks);

        // A failed frame can reenter the runtime while resolving owned captures.
        drop(removed);
    }
}

impl NativeRuntime {
    pub(in crate::native) fn start(
        &self,
        handle: NativeTaskHandle,
        frame: &mut NativeFrameTransfer,
        cleanup_parent: Option<Arc<super::super::frame::NativeTerminalState>>,
    ) -> NativeRuntimeStatus {
        let Some(mut start) = TaskStart::claim(self, handle) else {
            return NativeRuntimeStatus::UNKNOWN_TASK;
        };

        let Some(descriptor) = NativeFrame::checked_descriptor(frame.frame()) else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        let Ok(admission) = TaskAdmission::reserve(CancellationContext::root()) else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let state = ProtectedFrameStateId::new(0);

        let Ok(registration) = self.scheduler.register_admitted_task(
            admission.id(),
            descriptor.clone(),
            self.thread.runtime().id(),
            state,
            admission.cancellation_context(),
            start.admission,
        ) else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        if self.cleanup_workloads.get()
            && !self.main_thread_lane
            && registration
                .lane(state)
                .is_ok_and(|lane| matches!(lane.placement(), ExecutionLanePlacement::MainThread(_)))
        {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        let mut tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if !matches!(tasks.get(&handle), Some(NativeTaskSlot::Starting(_))) {
            return NativeRuntimeStatus::UNKNOWN_TASK;
        }

        // Workers acquire this table before obtaining executable state. The wake may become
        // visible now, but no worker can enter the frame before its ownership is published.
        if registration.wake_handle().wake(state).is_err() {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        let frame = NativeFrame::new(frame.take(), descriptor);
        let terminal = frame.terminal_state();

        let task = Arc::new(StartedTask {
            admission: start.admission,
            task: admission.publish(erase_sendable_protected_frame(frame)),
            registration,
            waits: Mutex::new(Vec::new()),
            event_wait: Mutex::new(None),
            observation_claimed: AtomicBool::new(false),
            terminal,
            cleanup_parent,
        });

        tasks.insert(handle, NativeTaskSlot::Started(task));
        start.committed = true;

        NativeRuntimeStatus::SUCCESS
    }
}
