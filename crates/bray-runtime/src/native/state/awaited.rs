use std::sync::Arc;

use bray_runtime_abi::{
    NativeInactiveFrame, NativeRunState, NativeRuntimeStatus, NativeTaskHandle,
};

use super::binding::current_native_task;
use super::core::{NativeRuntime, SuspendedWait};

impl NativeRuntime {
    pub(in crate::native) fn compose_awaited(
        &self,
        frame: NativeInactiveFrame,
    ) -> NativeRuntimeStatus {
        let Some(parent) = current_native_task() else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        if self
            .awaited
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .contains_key(&parent)
        {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        let allocation = self.allocate();

        let Some(child) = allocation.task() else {
            return allocation.status();
        };

        let status = self.start(child, frame.into_protected());

        if !status.is_success() {
            return status;
        }

        self.awaited
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(parent, child);

        NativeRuntimeStatus::SUCCESS
    }

    pub(in crate::native) fn resolve_awaited_completion(
        &self,
    ) -> Result<usize, NativeRuntimeStatus> {
        let parent = current_native_task().ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

        let child = self
            .awaited
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&parent)
            .ok_or(NativeRuntimeStatus::UNKNOWN_TASK)?;

        let outcome = self.observe(child);

        if outcome.state() != NativeRunState::COMPLETED || outcome.payload() == 0 {
            self.awaited
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(parent, child);

            return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
        }

        self.resolved_awaits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(parent)
            .or_default()
            .push(child);

        Ok(outcome.payload())
    }

    pub(in crate::native::state) fn register_awaited_wake(
        &self,
        parent: NativeTaskHandle,
    ) -> NativeRuntimeStatus {
        let Some(child) = self
            .awaited
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .get(&parent)
            .copied()
        else {
            return NativeRuntimeStatus::SUCCESS;
        };

        let wake = self.with_started(parent, |task| task.registration.wake_handle());

        let Ok(wake) = wake else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        let registration = self.with_started(child, |task| {
            task.task.register_join_waiter(Arc::new(move || {
                let _ = wake.wake();
            }))
        });

        let Ok(Ok(registration)) = registration else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        self.with_started(parent, |task| {
            let previous = task
                .suspended_wait
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .replace(SuspendedWait::Awaited(registration));

            assert!(
                previous.is_none(),
                "await wait registration must be consumed before replacement"
            );

            NativeRuntimeStatus::SUCCESS
        })
        .unwrap_or_else(|status| status)
    }

    pub(in crate::native::state) fn release_resolved_awaits(&self, parent: NativeTaskHandle) {
        let Some(children) = self
            .resolved_awaits
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&parent)
        else {
            return;
        };

        let mut tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        for child in children {
            tasks.remove(&child);
        }
    }
}
