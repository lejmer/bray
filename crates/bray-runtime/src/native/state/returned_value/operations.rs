use bray_runtime_abi::{
    NativeCleanupExecution, NativeRootHandle, NativeRunState, NativeRuntimeStatus,
    NativeTaskHandle, NativeValueCleanup,
};

use super::super::core::{NativeRuntime, NativeTaskSlot};
use super::super::start::NativeRunReservation;
use crate::native::run::{NativeHostSequence, NativeValueSequence};
use crate::task::TaskAdmissionKind;

impl NativeRuntime {
    /// Admits the result destination and its independent cleanup owner before entry execution.
    pub(in crate::native) fn admit_returned_value(
        &self,
        product: usize,
        size: usize,
        alignment: usize,
        error_offset: usize,
        broadcast: Option<NativeValueCleanup>,
        lifecycle: NativeValueCleanup,
    ) -> Result<(NativeTaskHandle, usize), NativeRuntimeStatus> {
        if error_offset > size
            || lifecycle.execution() != NativeCleanupExecution::ASYNCHRONOUS
            || lifecycle.metadata().is_none()
            || broadcast.is_some_and(|cleanup| {
                !matches!(
                    cleanup.execution(),
                    NativeCleanupExecution::NONE | NativeCleanupExecution::SYNCHRONOUS
                ) || cleanup.metadata().is_some()
            })
        {
            return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
        }

        // This entry keeps the formed product, its statics, and execution alive during metadata
        // callbacks and throughout the returned value's ownership interval.
        let entry = crate::product::ProductEntry::acquire(product)?;
        let storage = crate::native::storage::NativeStorage::new(size, alignment)?;
        let address = storage.address();
        let descriptor = crate::native::run::cleanup_descriptor(lifecycle.metadata())?;
        let terminal = crate::native::frame::NativeTerminalState::reserve()?;

        let mut reservation =
            NativeRunReservation::prepare(descriptor, terminal, TaskAdmissionKind::Continuation)?;

        reservation.reserve(&self.scheduler)?;

        let sequence = crate::allocation::reserve_storage::<NativeHostSequence>()
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        reservation.run().install_sequence(Box::write(
            sequence,
            NativeHostSequence::Value(NativeValueSequence::new(
                storage,
                error_offset,
                broadcast,
                lifecycle,
                entry,
            )),
        ));

        let allocation = self.allocate_continuation();
        let handle = allocation.task().ok_or(allocation.status())?;

        let mut tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let slot = tasks
            .get_mut(&handle)
            .expect("newly allocated result admission retains its slot");

        assert!(matches!(
            slot,
            NativeTaskSlot::Allocated(TaskAdmissionKind::Continuation)
        ));

        *slot = NativeTaskSlot::ReturnedValue(reservation);

        Ok((handle, address))
    }

    /// Releases a destination after entry execution produced no owned error.
    pub(in crate::native) fn release_returned_value(
        &self,
        handle: NativeTaskHandle,
    ) -> NativeRuntimeStatus {
        let removed = {
            let mut tasks = self
                .tasks
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            if !matches!(tasks.get(&handle), Some(NativeTaskSlot::ReturnedValue(_))) {
                return NativeRuntimeStatus::UNKNOWN_TASK;
            }

            let removed = tasks.remove(&handle);

            if let Some(slot) = &removed {
                self.release_admission(slot);
            }

            removed
        };

        // Product entry release may progress closure and execute callbacks.
        drop(removed);

        NativeRuntimeStatus::SUCCESS
    }

    /// Transfers the prepared result into its admitted cleanup run, without allocating a task.
    pub(in crate::native) fn resolve_returned_value(
        &self,
        handle: NativeTaskHandle,
    ) -> NativeRuntimeStatus {
        let reservation = {
            let mut tasks = self
                .tasks
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            let Some(slot @ NativeTaskSlot::ReturnedValue(_)) = tasks.get_mut(&handle) else {
                return NativeRuntimeStatus::UNKNOWN_TASK;
            };

            let NativeTaskSlot::ReturnedValue(reservation) = std::mem::replace(
                slot,
                NativeTaskSlot::Allocated(TaskAdmissionKind::Continuation),
            ) else {
                unreachable!("returned value is exclusively claimed through its native slot");
            };

            reservation
        };

        let run = triomphe::Arc::clone(reservation.run());

        let status = if run.begin_value_cleanup() {
            self.start_run(handle, reservation)
        } else {
            NativeRuntimeStatus::INVALID_ARGUMENT
        };

        if !status.is_success() {
            self.retain_failed_run(handle, run);

            return status;
        }

        let root = NativeRootHandle::new(handle.raw()).expect("native task handle is nonzero");

        if self.observe_root(root).state() != NativeRunState::COMPLETED {
            self.retain_failed_run(handle, run);

            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        self.resolve_root_completion(root)
    }
}
