use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use triomphe::{Arc, UniqueArc};

use bray_runtime_abi::{NativeRuntimeStatus, NativeTaskHandle};
use bray_runtime_model::{ProtectedFrameDescriptor, ProtectedFrameStateId};

use crate::{CancellationContext, ExecutionLanePlacement, TaskControlBlock};

use super::super::frame::NativeFrameTransfer;
use super::super::run::{NativeActivation, NativeActivationReservation, NativeRun};
use super::core::{NativeRuntime, NativeTaskSlot, StartedTask};

struct TaskStart<'runtime> {
    runtime: &'runtime NativeRuntime,
    handle: NativeTaskHandle,
    committed: bool,
    admission: crate::task::TaskAdmissionKind,
}

impl<'runtime> TaskStart<'runtime> {
    fn publish(
        &mut self,
        install: impl FnOnce() -> Result<Arc<StartedTask>, NativeRuntimeStatus>,
    ) -> NativeRuntimeStatus {
        let mut tasks = self
            .runtime
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if !matches!(tasks.get(&self.handle), Some(NativeTaskSlot::Starting(_))) {
            return NativeRuntimeStatus::UNKNOWN_TASK;
        }

        // Binding may wake workers, which must observe the fully installed task through this table.
        let task = match install() {
            Ok(task) => task,
            Err(status) => return status,
        };

        tasks.insert(self.handle, NativeTaskSlot::Started(task));
        self.committed = true;

        NativeRuntimeStatus::SUCCESS
    }

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
    pub(in crate::native) fn start_run(
        &self,
        handle: NativeTaskHandle,
        mut reservation: NativeRunReservation,
    ) -> NativeRuntimeStatus {
        let Some(mut start) = TaskStart::claim(self, handle) else {
            return NativeRuntimeStatus::UNKNOWN_TASK;
        };

        reservation.task.admission = start.admission;

        // Keep failed ownership outside the task-table lock while binding may reject admission.
        let mut pending = Some(reservation);

        let status = start.publish(|| {
            let reservation = pending
                .as_mut()
                .unwrap_or_else(|| unreachable!("run installs once"));

            reservation.bind(self, true)?;

            Ok(pending
                .take()
                .unwrap_or_else(|| unreachable!("bound run remains owned"))
                .install())
        });

        if !status.is_success() {
            let mut tasks = self
                .tasks
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            if let Some(slot @ NativeTaskSlot::Starting(_)) = tasks.get_mut(&handle) {
                *slot = NativeTaskSlot::Allocated(start.admission);
            }

            start.committed = true;
        }

        status
    }

    pub(in crate::native) fn start(
        &self,
        handle: NativeTaskHandle,
        frame: &mut NativeFrameTransfer,
    ) -> NativeRuntimeStatus {
        let Some(mut start) = TaskStart::claim(self, handle) else {
            return NativeRuntimeStatus::UNKNOWN_TASK;
        };

        let mut claim = match super::super::frames::claim(
            frame.frame().context(),
            frame.entry(),
            frame.frame().metadata(),
        ) {
            Ok(Some(claim)) => claim,
            Ok(None) => {
                match NativeTaskReservation::prepare(frame.frame().metadata(), start.admission) {
                    Ok(reservation) => super::super::frames::FrameTaskClaim::fresh(reservation),
                    Err(status) => return status,
                }
            }
            Err(status) => return status,
        };

        let admission = start.admission;

        start.publish(|| {
            let reservation = claim.reservation();
            reservation.run.task.admission = admission;
            reservation.run.bind(self, true)?;

            Ok(claim.install(frame.take()))
        })
    }
}

// Generated frame admission retains its first activation separately from run-only capacity.
// Composition consumes the activation and releases the unused independent-run resources.
pub(in crate::native) struct NativeTaskReservation {
    run: NativeRunReservation,
    activation: NativeActivationReservation,
}

impl NativeTaskReservation {
    pub(in crate::native) fn descriptor(&self) -> &ProtectedFrameDescriptor {
        self.activation.descriptor()
    }

    pub(in crate::native) fn admit_cleanup(&mut self) {
        self.run.registration_storage.cleanup_admitted = true;
    }

    pub(in crate::native) fn prepare(
        metadata: &bray_runtime_abi::NativeFrameMetadata,
        admission: crate::task::TaskAdmissionKind,
    ) -> Result<Self, NativeRuntimeStatus> {
        let activation = NativeActivationReservation::prepare(metadata)?;

        // The first activation and run share their immutable contract and terminal destination.
        let run = NativeRunReservation::prepare(
            activation.descriptor().clone(),
            Arc::clone(activation.terminal()),
            admission,
        )?;

        Ok(Self { run, activation })
    }

    pub(in crate::native) fn install(
        self,
        abi: bray_runtime_abi::NativeProtectedFrame,
    ) -> Arc<StartedTask> {
        self.run.task.run.install_root(self.activation.install(abi));

        self.run.install()
    }

    pub(in crate::native) fn install_activation(
        self,
        abi: bray_runtime_abi::NativeProtectedFrame,
    ) -> Box<NativeActivation> {
        assert!(
            self.run.task.registration.is_none(),
            "composed activation must not be registered"
        );

        self.activation.install(abi)
    }
}

// A host sequence admits this capacity before any generated finalizer activation exists.
pub(in crate::native) struct NativeRunReservation {
    task: UniqueArc<StartedTask>,
    registration_storage: crate::scheduler::TaskRegistrationStorage,
    run_storage: Box<std::mem::MaybeUninit<Arc<NativeRun>>>,
}

impl NativeRunReservation {
    pub(in crate::native) fn reserve(
        &mut self,
        scheduler: &crate::Scheduler,
    ) -> Result<(), NativeRuntimeStatus> {
        self.registration_storage
            .reserve(scheduler)
            .map_err(super::binding::scheduler_status)
    }

    pub(in crate::native) fn run(&self) -> &Arc<NativeRun> {
        &self.task.run
    }

    pub(in crate::native) fn prepare(
        descriptor: ProtectedFrameDescriptor,
        terminal: Arc<super::super::frame::NativeTerminalState>,
        admission: crate::task::TaskAdmissionKind,
    ) -> Result<Self, NativeRuntimeStatus> {
        let cancellation =
            CancellationContext::root().map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        let control = match TaskControlBlock::prepare(cancellation, descriptor) {
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

        let run_storage = crate::allocation::reserve_storage::<Arc<NativeRun>>()
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        // The run and its task share the admitted terminal destination and immutable root contract.
        let run = crate::allocation::allocate_shared(NativeRun::new(
            control.descriptor().clone(),
            Arc::clone(&terminal),
        ))
        .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        let continuation = super::continuation::ContinuationWait::reserve()
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        let event_wait = super::event_wait::EventWait::reserve()
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        let registration_storage = crate::scheduler::TaskRegistrationStorage::prepare(
            control.descriptor().clone(),
            control.cancellation_context(),
        )
        .map_err(super::binding::scheduler_status)?;

        #[cfg(test)]
        if crate::test_support::allocation_should_fail() {
            return Err(NativeRuntimeStatus::ALLOCATION_FAILURE);
        }

        let task = UniqueArc::try_new(StartedTask {
            admission,
            task: control,
            registration: None,
            waits: Mutex::new(Vec::new()),
            continuation,
            event_wait,
            run,
            observation_claimed: AtomicBool::new(false),
            terminal,
        })
        .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        Ok(Self {
            task,
            registration_storage,
            run_storage,
        })
    }

    fn bind(&mut self, runtime: &NativeRuntime, ready: bool) -> Result<(), NativeRuntimeStatus> {
        if self.task.registration.is_some() {
            return Err(NativeRuntimeStatus::ALREADY_INITIALIZED);
        }

        let control = &self.task.task;
        let state = ProtectedFrameStateId::new(0);

        let lane = self
            .registration_storage
            .lane(&runtime.scheduler, runtime.thread.runtime().id(), state)
            .map_err(|_| NativeRuntimeStatus::RUNTIME_FAILURE)?;

        if runtime.cleanup_workloads.get()
            && !runtime.main_thread_lane
            && matches!(lane.placement(), ExecutionLanePlacement::MainThread(_))
        {
            return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
        }

        let register = if ready {
            crate::Scheduler::register_ready_prepared_task
        } else {
            crate::Scheduler::register_prepared_task
        };

        let registration = register(
            &runtime.scheduler,
            control.id(),
            runtime.thread.runtime().id(),
            state,
            self.task.admission,
            &mut self.registration_storage,
        )
        .map_err(super::binding::scheduler_status)?;

        self.task.continuation.bind(registration.wake_handle());
        self.task.event_wait.bind(registration.wake_handle());
        self.task.registration = Some(registration);

        Ok(())
    }

    fn install(mut self) -> Arc<StartedTask> {
        assert!(
            self.task.registration.is_some(),
            "native run storage must bind before installation"
        );

        // Native exports and the task's protected frame share one run driver.
        let run = Arc::clone(&self.task.run);

        self.task
            .task
            .install_frame(Box::into_pin(Box::write(self.run_storage, run)));

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
    fn unbound_storage_survives_runtime_shutdown_and_failed_binding_preserves_it() {
        let metadata = NativeFrameMetadata::new([29; 32], 1, 8, 8, 0, 1, native_origin_frame_state);

        assert_eq!(
            initialize(NativeRuntimeConfiguration::new(1, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        let mut reservation =
            NativeTaskReservation::prepare(&metadata, TaskAdmissionKind::Independent).unwrap();

        let identity = reservation.run.task.task.id();
        assert!(reservation.run.task.registration.is_none());
        assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);

        assert_eq!(
            initialize(NativeRuntimeConfiguration::new(1, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        with_runtime(|runtime| {
            assert_eq!(runtime.scheduler.task_count().unwrap(), 0);

            assert_eq!(
                with_allocation_failure(|| reservation.run.bind(runtime, false)),
                Err(NativeRuntimeStatus::ALLOCATION_FAILURE),
            );

            assert!(reservation.run.task.registration.is_none());
            assert_eq!(reservation.run.task.task.id(), identity);
            assert_eq!(runtime.scheduler.task_count().unwrap(), 0);

            // Reuse admitted scheduler slots. Per-task lane and cancellation storage already
            // belongs to the reservation, so binding itself needs no further allocation.
            let mut warm =
                NativeTaskReservation::prepare(&metadata, TaskAdmissionKind::Independent).unwrap();

            warm.run.bind(runtime, false).unwrap();
            drop(warm);
            with_allocation_failure(|| reservation.run.bind(runtime, false)).unwrap();
            let state = ProtectedFrameStateId::new(0);

            assert_eq!(
                reservation
                    .run
                    .task
                    .registration()
                    .lane(state)
                    .unwrap()
                    .placement(),
                crate::ExecutionLanePlacement::OriginThread(runtime.thread.runtime().id()),
            );

            assert_eq!(
                with_allocation_failure(|| reservation.run.bind(runtime, false)),
                Err(NativeRuntimeStatus::ALREADY_INITIALIZED),
            );

            assert_eq!(runtime.scheduler.task_count().unwrap(), 1);
            with_allocation_failure(|| drop(reservation));
            assert_eq!(runtime.scheduler.task_count().unwrap(), 0);
        })
        .unwrap();

        assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);
    }

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
                    NativeTaskReservation::prepare(&metadata, TaskAdmissionKind::Independent)
                        .and_then(|mut reservation| {
                            reservation.run.bind(runtime, false)?;

                            Ok(reservation)
                        })
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
                let lane = reservation.run.task.registration().lane(state).unwrap();
                assert!(runtime.scheduler.take_ready(lane).unwrap().is_none());

                // Discarding unused capacity unregisters it without invoking frame code.
                with_allocation_failure(|| drop(reservation));
                assert_eq!(runtime.scheduler.task_count().unwrap(), 0);
                assert_eq!(RESUMES.load(Ordering::Relaxed), 0);
                assert_eq!(RELEASES.load(Ordering::Relaxed), 0);

                let mut reservation =
                    NativeTaskReservation::prepare(&metadata, TaskAdmissionKind::Independent)
                        .unwrap();

                reservation.run.bind(runtime, false).unwrap();

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

                let handle = runtime.allocate().task().unwrap();

                with_allocation_failure(|| {
                    let task = reservation.install(abi);

                    runtime.tasks.lock().unwrap().insert(
                        handle,
                        super::NativeTaskSlot::Started(triomphe::Arc::clone(&task)),
                    );

                    let wake = task.registration().wake_handle();
                    wake.wake().unwrap();
                    let ready = runtime.scheduler.take_ready(lane).unwrap().unwrap();
                    assert_eq!(runtime.drive_ready(ready), NativeRuntimeStatus::SUCCESS);

                    assert_eq!(
                        runtime.observe(handle).state(),
                        bray_runtime_abi::NativeRunState::COMPLETED
                    );

                    assert_eq!(runtime.destroy_task(handle), NativeRuntimeStatus::SUCCESS);
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

    #[test]
    fn admitted_activation_installs_without_task_registration_or_allocation() {
        let metadata = NativeFrameMetadata::new([39; 32], 1, 8, 8, 0, 1, native_origin_frame_state);

        assert_eq!(
            initialize(NativeRuntimeConfiguration::new(1, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        RESUMES.store(0, Ordering::Relaxed);
        RELEASES.store(0, Ordering::Relaxed);

        with_runtime(|runtime| {
            let reservation =
                NativeTaskReservation::prepare(&metadata, TaskAdmissionKind::Continuation).unwrap();

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
                let activation = reservation.install_activation(abi);
                assert_eq!(runtime.scheduler.task_count().unwrap(), 0);
                assert_eq!(RESUMES.load(Ordering::Relaxed), 0);
                drop(activation);
            });

            assert_eq!(runtime.scheduler.task_count().unwrap(), 0);
            assert_eq!(RELEASES.load(Ordering::Relaxed), 1);
        })
        .unwrap();

        assert_eq!(shutdown(), NativeRuntimeStatus::SUCCESS);
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
