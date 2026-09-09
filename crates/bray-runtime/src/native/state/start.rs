use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use triomphe::{Arc, UniqueArc};

use bray_runtime_abi::{NativeRuntimeStatus, NativeTaskHandle};
use bray_runtime_model::ProtectedFrameStateId;

use crate::{CancellationContext, ExecutionLanePlacement, TaskControlBlock};

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

        let reservation = match NativeTaskReservation::prepare(
            self,
            frame.frame().metadata(),
            start.admission,
            cleanup_parent,
        ) {
            Ok(reservation) => reservation,
            Err(status) => return status,
        };

        let mut tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if !matches!(tasks.get(&handle), Some(NativeTaskSlot::Starting(_))) {
            return NativeRuntimeStatus::UNKNOWN_TASK;
        }

        // Workers acquire this table before obtaining executable state. The wake may become
        // visible now, but no worker can enter the frame before its ownership is published.
        if reservation
            .task
            .registration
            .wake_handle()
            .wake(ProtectedFrameStateId::new(0))
            .is_err()
        {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        tasks.insert(
            handle,
            NativeTaskSlot::Started(reservation.install(frame.take())),
        );

        start.committed = true;

        NativeRuntimeStatus::SUCCESS
    }
}

// A reservation owns the allocations needed to install one native frame on its admitted
// runtime and origin. It contains no live frame context and never makes itself ready.
struct NativeTaskReservation {
    task: UniqueArc<StartedTask>,
    frame_storage: Box<std::mem::MaybeUninit<NativeFrame>>,
    completion: super::super::result_storage::NativeResultStorage,
}

impl NativeTaskReservation {
    fn prepare(
        runtime: &NativeRuntime,
        metadata: &bray_runtime_abi::NativeFrameMetadata,
        admission: crate::task::TaskAdmissionKind,
        cleanup_parent: Option<Arc<super::super::frame::NativeTerminalState>>,
    ) -> Result<Self, NativeRuntimeStatus> {
        let descriptor = NativeFrame::checked_descriptor(metadata)?;

        // Completion can be observed during mandatory cleanup. Secure its destination while
        // the caller still owns the inactive frame, before publishing any executable task.
        let completion = super::super::result_storage::NativeResultStorage::new(
            metadata.completion_size(),
            metadata.completion_alignment(),
        )?;

        let cancellation =
            CancellationContext::root().map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        let control = match TaskControlBlock::prepare(cancellation, descriptor.clone()) {
            Ok(admission) => admission,
            Err(crate::TaskStartError::AllocationFailed) => {
                return Err(NativeRuntimeStatus::ALLOCATION_FAILURE);
            }
            Err(crate::TaskStartError::Cancellation(_)) => {
                return Err(NativeRuntimeStatus::ALLOCATION_FAILURE);
            }
            Err(crate::TaskStartError::IdentityExhausted) => {
                return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
            }
        };

        let frame_storage = crate::frame::reserve_frame_storage::<NativeFrame>()
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        let terminal = super::super::frame::NativeTerminalState::reserve()?;

        let continuation = super::continuation::ContinuationWait::reserve()
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        let event_wait = super::event_wait::EventWait::reserve()
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        let state = ProtectedFrameStateId::new(0);

        let registration = match runtime.scheduler.register_admitted_task(
            control.id(),
            descriptor,
            runtime.thread.runtime().id(),
            state,
            control.cancellation_context(),
            admission,
        ) {
            Ok(registration) => registration,
            Err(crate::SchedulerError::AdmissionAllocation(_)) => {
                return Err(NativeRuntimeStatus::ALLOCATION_FAILURE);
            }
            Err(_) => return Err(NativeRuntimeStatus::RUNTIME_FAILURE),
        };

        if runtime.cleanup_workloads.get()
            && !runtime.main_thread_lane
            && registration
                .lane(state)
                .is_ok_and(|lane| matches!(lane.placement(), ExecutionLanePlacement::MainThread(_)))
        {
            return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
        }

        continuation.bind(registration.wake_handle());
        event_wait.bind(registration.wake_handle());

        #[cfg(test)]
        if crate::test_support::allocation_should_fail() {
            return Err(NativeRuntimeStatus::ALLOCATION_FAILURE);
        }

        let task = UniqueArc::try_new(StartedTask {
            admission,
            task: control,
            registration,
            waits: Mutex::new(Vec::new()),
            continuation,
            event_wait,
            awaited: Mutex::new(None),
            observation_claimed: AtomicBool::new(false),
            terminal,
            cleanup_parent,
        })
        .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        Ok(Self {
            task,
            frame_storage,
            completion,
        })
    }

    fn install(mut self, abi: bray_runtime_abi::NativeProtectedFrame) -> Arc<StartedTask> {
        let frame = NativeFrame::new(
            abi,
            self.task.task.descriptor().clone(),
            self.completion,
            Arc::clone(&self.task.terminal),
        );

        self.task
            .task
            .install_frame(Box::into_pin(Box::write(self.frame_storage, frame)));

        self.task.shareable()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{
        NativeFrameExit, NativeFrameMetadata, NativeFrameProgress, NativeFrameProgressKind,
        NativeProtectedFrame, NativeRuntimeConfiguration, NativeRuntimeStatus,
    };
    use bray_runtime_model::ProtectedFrameStateId;

    use super::NativeTaskReservation;
    use crate::native::state::core::{initialize, shutdown, with_runtime};
    use crate::task::TaskAdmissionKind;
    use crate::test_support::{
        native_origin_frame_state, with_allocation_failure, with_allocation_failure_after,
    };

    static RESUMES: AtomicUsize = AtomicUsize::new(0);
    static RELEASES: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn metadata_admission_rolls_back_without_a_context_and_installation_needs_no_allocation() {
        let metadata = NativeFrameMetadata::new([19; 32], 1, 8, 8, 0, 1, native_origin_frame_state);

        for allowed in 0..64 {
            assert_eq!(
                initialize(NativeRuntimeConfiguration::new(1, 1)),
                NativeRuntimeStatus::SUCCESS
            );

            RESUMES.store(0, Ordering::Relaxed);
            RELEASES.store(0, Ordering::Relaxed);

            let admitted = with_runtime(|runtime| {
                let result = with_allocation_failure_after(allowed, || {
                    NativeTaskReservation::prepare(
                        runtime,
                        &metadata,
                        TaskAdmissionKind::Independent,
                        None,
                    )
                });

                let reservation = match result {
                    Ok(reservation) => reservation,
                    Err(status) => {
                        assert_eq!(
                            status,
                            NativeRuntimeStatus::ALLOCATION_FAILURE,
                            "allocation {allowed}"
                        );

                        assert_eq!(runtime.scheduler.task_count().unwrap(), 0);

                        return false;
                    }
                };

                assert_eq!(runtime.scheduler.task_count().unwrap(), 1);
                let state = ProtectedFrameStateId::new(0);
                let lane = reservation.task.registration.lane(state).unwrap();
                assert!(runtime.scheduler.take_ready(lane).unwrap().is_none());

                // Discarding unused capacity unregisters it without invoking frame code.
                with_allocation_failure(|| drop(reservation));
                assert_eq!(runtime.scheduler.task_count().unwrap(), 0);
                assert_eq!(RESUMES.load(Ordering::Relaxed), 0);
                assert_eq!(RELEASES.load(Ordering::Relaxed), 0);

                let reservation = NativeTaskReservation::prepare(
                    runtime,
                    &metadata,
                    TaskAdmissionKind::Independent,
                    None,
                )
                .unwrap();

                // The context and its callbacks first exist after all task machinery is reserved.
                let abi = NativeProtectedFrame::new(
                    77,
                    metadata,
                    resume,
                    resume,
                    ignore,
                    resolve,
                    move_completion,
                    release,
                );

                with_allocation_failure(|| {
                    let task = reservation.install(abi);
                    let wake = task.registration.wake_handle();
                    wake.wake(state).unwrap();
                    let ready = runtime.scheduler.take_ready(lane).unwrap().unwrap();
                    task.task.resume().unwrap();
                    ready.complete().unwrap();
                    drop(task);
                    assert_eq!(runtime.scheduler.task_count().unwrap(), 0);
                });

                true
            })
            .unwrap();

            assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);

            if admitted {
                assert!(allowed >= 8);
                assert_eq!(RESUMES.load(Ordering::Relaxed), 1);
                assert_eq!(RELEASES.load(Ordering::Relaxed), 1);
                return;
            }
        }

        panic!("metadata admission did not complete its allocation sequence");
    }

    extern "C-unwind" fn resume(context: usize) -> NativeFrameProgress {
        assert_eq!(context, 77);
        RESUMES.fetch_add(1, Ordering::Relaxed);

        NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
    }

    extern "C-unwind" fn release(context: usize) {
        assert_eq!(context, 77);
        RELEASES.fetch_add(1, Ordering::Relaxed);
    }

    extern "C-unwind" fn ignore(_: usize) {}
    extern "C-unwind" fn resolve(_: usize, _: NativeFrameExit) {}
    extern "C-unwind" fn move_completion(_: usize, _: usize) {}
}
