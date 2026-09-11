use std::sync::atomic::Ordering;

use bray_runtime_abi::{NativeRuntimeStatus, NativeTaskAllocation, NativeTaskHandle};

use super::super::binding::CleanupWorkloadScope;
use super::super::core::{NativeRuntime, NativeRuntimeCore, NativeTaskSlot};

impl NativeRuntime {
    pub(in crate::native) fn with_cleanup_driving<T>(&self, callback: impl FnOnce() -> T) -> T {
        let previous = self.cleanup_workloads.replace(true);

        let _workloads = CleanupWorkloadScope {
            runtime: self,
            previous,
        };

        callback()
    }

    pub(in crate::native) fn allocate(&self) -> NativeTaskAllocation {
        self.allocate_kind(crate::task::TaskAdmissionKind::Independent)
    }

    pub(in crate::native) fn allocate_continuation(&self) -> NativeTaskAllocation {
        self.allocate_kind(crate::task::TaskAdmissionKind::Continuation)
    }

}

impl NativeRuntimeCore {
    pub(in crate::native) fn retain_failed_run(
        &self,
        handle: NativeTaskHandle,
        run: triomphe::Arc<crate::native::run::NativeRun>,
    ) {
        let mut tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Only the owning cleanup driver can release this admitted, unpublished slot.
        let slot = tasks
            .get_mut(&handle)
            .expect("admitted host run retains its native slot");

        if let NativeTaskSlot::Started(task) | NativeTaskSlot::Terminal { _task: task, .. } = slot {
            // Observation can fail while workers still dispatch this task. Keep its registration
            // intact, and carry containment through the same StartedTask into terminal observation.
            task.retained_failure.store(true, Ordering::Release);
        }

        if matches!(
            slot,
            NativeTaskSlot::Allocated(_) | NativeTaskSlot::Starting(_)
        ) {
            *slot = NativeTaskSlot::FailedRun {
                admission: slot.admission(),
                _run: run,
            };
        }
    }

    pub(in crate::native) fn release_task_reservation(&self, handle: NativeTaskHandle) {
        let mut tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if !matches!(tasks.get(&handle), Some(NativeTaskSlot::Allocated(_))) {
            return;
        }

        let removed = tasks.remove(&handle);

        if let Some(slot) = &removed {
            self.release_admission(slot);
        }

        drop(tasks);
        drop(removed);
    }

    pub(in crate::native) fn allocate_kind(
        &self,
        kind: crate::task::TaskAdmissionKind,
    ) -> NativeTaskAllocation {
        let mut tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if kind == crate::task::TaskAdmissionKind::Independent
            && self.independent_tasks.load(Ordering::Relaxed) >= self.task_capacity.get()
        {
            return NativeTaskAllocation::failure(NativeRuntimeStatus::RUNTIME_FAILURE);
        }

        let next = self.next_task.load(Ordering::Relaxed);

        let Some(handle) = NativeTaskHandle::new(next) else {
            return NativeTaskAllocation::failure(NativeRuntimeStatus::RUNTIME_FAILURE);
        };

        let Some(next) = next.checked_add(1) else {
            return NativeTaskAllocation::failure(NativeRuntimeStatus::RUNTIME_FAILURE);
        };

        if crate::allocation::reserve_map_entries(&mut tasks, 1).is_err() {
            return NativeTaskAllocation::failure(NativeRuntimeStatus::ALLOCATION_FAILURE);
        }

        self.next_task.store(next, Ordering::Relaxed);
        tasks.insert(handle, NativeTaskSlot::Allocated(kind));

        if kind == crate::task::TaskAdmissionKind::Independent {
            self.independent_tasks.fetch_add(1, Ordering::Relaxed);
        }

        NativeTaskAllocation::success(handle)
    }
}
