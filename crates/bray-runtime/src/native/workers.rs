use std::sync::{Arc, Mutex, OnceLock};

use bray_runtime_abi::{NativeWorkerCallback, NativeWorkerFailure, NativeWorkerReference};

use crate::event::NativeCallback;
use crate::{ExecutionLanePlacement, ExecutionWorkload, Scheduler};

use super::state::NativeRuntimeCore;

#[expect(
    unsafe_code,
    reason = "the selected Bray provider owns worker handles and lifecycle"
)]
unsafe extern "C" {
    safe fn bray_runtime_worker_admission_state(
        reference: &bray_runtime_abi::NativeSchedulerReference,
        close: u32,
    ) -> u32;
    #[link_name = "bray_runtime_worker_failure_reporting"]
    pub(crate) safe fn failure_reporting(
        failure: &NativeWorkerFailure,
        outcome: &mut bray_runtime_abi::NativeRunOutcome,
    );
    safe fn bray_runtime_worker_creation(
        scheduler: &bray_runtime_abi::NativeSchedulerReference,
        callback: &NativeWorkerCallback,
        output: &mut NativeWorkerReference,
    ) -> u32;
    safe fn bray_runtime_worker_destruction(reference: &NativeWorkerReference);
    safe fn bray_runtime_worker_demand(
        reference: &NativeWorkerReference,
        workload: u32,
        failure: &mut NativeWorkerFailure,
    ) -> u32;
    safe fn bray_runtime_worker_stop(reference: &NativeWorkerReference);
    safe fn bray_runtime_worker_notification(reference: &NativeWorkerReference);
    #[link_name = "bray_runtime_worker_acquisition"]
    pub(crate) safe fn acquisition(
        scope: &bray_runtime_abi::NativeWorkerScope,
        lanes: u32,
        output: &mut bray_runtime_abi::NativeSchedulerObservation,
    ) -> u32;
    #[link_name = "bray_runtime_worker_compatibility"]
    pub(crate) safe fn compatibility(
        scope: &bray_runtime_abi::NativeWorkerScope,
        required: &bray_runtime_abi::NativeSchedulerRequirements,
    ) -> u32;
}

// This owns only the native callback binding and the Source allocation. OS
// handles, publication, acknowledgement, stop and join belong to that allocation.
#[derive(Default)]
pub(super) struct WorkerPool {
    source: OnceLock<BrayWorkers>,
    formation: Mutex<()>,
}

struct BrayWorkers {
    reference: NativeWorkerReference,
    _callback: NativeCallback,
}

impl WorkerPool {
    pub(super) const fn new() -> Self {
        Self {
            source: OnceLock::new(),
            formation: Mutex::new(()),
        }
    }

    pub(super) fn demand(
        &self,
        runtime: &Arc<NativeRuntimeCore>,
        lane: crate::ExecutionLane,
    ) -> Result<(), NativeWorkerFailure> {
        if !matches!(lane.placement(), ExecutionLanePlacement::Migratable) {
            return Ok(());
        }

        self.demand_workload(runtime, lane.workload())
    }

    pub(super) fn demand_workload(
        &self,
        runtime: &Arc<NativeRuntimeCore>,
        workload: ExecutionWorkload,
    ) -> Result<(), NativeWorkerFailure> {
        if self.source.get().is_none() {
            // Serialize native binding creation only. OS admission and callbacks
            // run after this guard has been released.
            let _formation = self
                .formation
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            if self.source.get().is_none() {
                let callback = NativeCallback::worker(runtime).map_err(|_| allocation_failure())?;
                let descriptor = callback.worker_descriptor();
                let mut reference = NativeWorkerReference::default();

                let status = bray_runtime_worker_creation(
                    runtime.scheduler.source_reference(),
                    &descriptor,
                    &mut reference,
                );

                match status {
                    0 => {}
                    1 => return Err(allocation_failure()),
                    4 => {
                        return Err(NativeWorkerFailure {
                            stage: 4,
                            category: 0,
                            native_code: 0,
                        });
                    }
                    _ => panic!("Source worker creation returned an undefined status: {status}"),
                }

                assert!(
                    self.source
                        .set(BrayWorkers {
                            reference,
                            _callback: callback
                        })
                        .is_ok(),
                    "serialized formation installs one native worker binding"
                );
            }
        }

        let workers = self
            .source
            .get()
            .expect("worker formation installs its actual Source owner");

        let workload = match workload {
            ExecutionWorkload::Cooperative => 0,
            ExecutionWorkload::Blocking | ExecutionWorkload::BlockingCompute => 1,
            ExecutionWorkload::Compute => 2,
        };

        let mut failure = NativeWorkerFailure::default();
        let status = bray_runtime_worker_demand(&workers.reference, workload, &mut failure);

        if status == 0 { Ok(()) } else { Err(failure) }
    }

    #[cfg(test)]
    pub(super) fn is_stopping(&self, scheduler: &Scheduler) -> bool {
        bray_runtime_worker_admission_state(scheduler.source_reference(), 0) != 0
    }

    // Source completion follows every admitted worker's exact-thread cleanup.
    // With no Source pool, the retiring native owner completes its own services.
    pub(super) fn stop(&self, scheduler: &Scheduler) -> bool {
        bray_runtime_worker_admission_state(scheduler.source_reference(), 1);

        if let Some(workers) = self.source.get() {
            bray_runtime_worker_stop(&workers.reference);

            true
        } else {
            false
        }
    }

    pub(super) fn detach_product(&self) {
        if let Some(workers) = self.source.get() {
            bray_runtime_worker_notification(&workers.reference);
        }
    }
}

impl Drop for BrayWorkers {
    fn drop(&mut self) {
        bray_runtime_worker_destruction(&self.reference);
    }
}

fn allocation_failure() -> NativeWorkerFailure {
    NativeWorkerFailure {
        stage: 1,
        category: 0,
        native_code: 0,
    }
}

pub(super) struct Context {
    pub(super) scope: bray_runtime_abi::NativeWorkerScope,
    pub(super) workload: ExecutionWorkload,
    pub(super) ready: std::cell::RefCell<Option<crate::ReadyTask>>,
}

pub(crate) extern "C" fn invoke(
    context: usize,
    operation: u32,
    scope: &bray_runtime_abi::NativeWorkerScope,
    argument: &mut u64,
    outcome: &mut bray_runtime_abi::NativeRunOutcome,
) -> u32 {
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        match operation {
            0 => NativeCallback::retain_worker(context),
            1 => NativeCallback::release_worker(context),
            2 => {
                let workload = match *argument {
                    0 => ExecutionWorkload::Cooperative,
                    1 => ExecutionWorkload::Blocking,
                    2 => ExecutionWorkload::Compute,
                    _ => panic!("Source publishes a selected worker workload"),
                };

                super::state::bind_worker(
                    NativeCallback::worker_runtime(context),
                    *scope,
                    workload,
                );
            }
            3 => {
                let runtime =
                    super::state::current_runtime().expect("Source retains the worker context");

                let worker = runtime
                    .worker
                    .as_ref()
                    .expect("the OS entry installed its scope");

                let lanes = super::state::worker_lanes(&runtime, worker.workload);

                let ready = runtime
                    .scheduler
                    .take_worker_ready(runtime.thread.runtime(), lanes, &worker.scope)
                    .expect("the retained worker scheduler remains available");

                let available = ready.is_some();

                assert!(
                    worker.ready.replace(ready).is_none(),
                    "one pending dispatch belongs to its worker"
                );

                return u32::from(available);
            }
            4 => {
                let runtime =
                    super::state::current_runtime().expect("Source retains the worker context");

                let ready = runtime
                    .worker
                    .as_ref()
                    .expect("the OS entry installed its scope")
                    .ready
                    .borrow_mut()
                    .take()
                    .expect("Source drives only an acquired task");

                let _ = runtime.drive_ready(ready);

            }
            5 => super::state::unbind_worker(),
            6 => {
                let runtime =
                    super::state::current_runtime().expect("Source retains the worker context");

                *argument = runtime.scheduler.worker_wait_milliseconds();
            }
            7 => {
                let runtime = super::state::current_runtime()
                    .expect("the failed formation retains its worker context");

                let worker = runtime
                    .worker
                    .as_ref()
                    .expect("the selected callback belongs to its OS-entry scope");

                // Source transfers an inline worker cause without record or message
                // backing. Retain that typed value until its task can enter cleanup.
                let (primary, head, tail, count, reserved) = outcome.take_report().take_parts();

                assert_eq!((head, tail, count, reserved), (0, 0, 0, 0), "worker formation uses only its inline primary");

                let failure = primary.expect("worker formation preserves its primary")
                    .worker_failure().expect("worker formation preserves its typed cause");

                worker.ready.borrow().as_ref().expect("the failed spare admission retains its dispatch")
                    .record_worker_failure(failure, scope);
            }
            8 => NativeCallback::worker_owner(context).finish_services(),
            _ => panic!("Source invokes one defined worker operation"),
        }

        0
    }));

    result.unwrap_or(bray_runtime_abi::NativeRuntimeStatus::PANICKED.code())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    use bray_runtime_abi::{
        NativeActivationDescriptor, NativeCleanupStorageShape, NativeFrameExit,
        NativeFrameProgress, NativeFrameProgressKind, NativeInactiveFrame, NativeRunOutcome,
    };

    extern "C-unwind" fn ignore(_: usize) {}
    extern "C-unwind" fn broadcast(_: &mut NativeFrameProgress, _: usize) {}
    extern "C-unwind" fn completion(_: usize, _: usize) {}

    extern "C-unwind" fn resolve(
        progress: &mut NativeFrameProgress,
        _: usize,
        _: NativeFrameExit,
    ) {
        *progress = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
    }

    fn frame(
        state: bray_runtime_abi::NativeFrameStateCallback,
        resume: bray_runtime_abi::NativeFrameResumeCallback,
    ) -> NativeInactiveFrame {
        super::super::fixtures::admit(
            0,
            NativeActivationDescriptor {
                storage: NativeCleanupStorageShape::new([92; 32], 8, 8),
                header: 0,
                completion: 0,
                completion_bytes: 8,
                completion_align: 8,
                states: 2,
                state,
                resume,
                cancel: resume,
                broadcast,
                resolve,
                move_completion: completion,
                destroy: ignore,
            },
        )
    }

    #[test]
    fn closed_worker_admission_preserves_suspended_cleanup_on_its_source_root() {
        use crate::native::{export, state};

        use bray_runtime_abi::{
            NativeFrameAffinity, NativeFrameState, NativeLaneRequirements,
            NativeRunState, NativeRuntimeConfiguration,
        };

        use std::sync::atomic::AtomicUsize;

        static BODIES: AtomicUsize = AtomicUsize::new(0);
        static CLEANUP_STEPS: AtomicUsize = AtomicUsize::new(0);
        static DESTROYED: AtomicUsize = AtomicUsize::new(0);

        extern "C" fn requirements(_: usize, state: u32, operation: u32, _: usize) -> NativeFrameState {
            NativeFrameState::new(
                NativeFrameAffinity::MOVABLE,
                if state == 0 && operation == 0 {
                    NativeLaneRequirements::NONE
                } else {
                    NativeLaneRequirements::BLOCKING
                },
            )
        }

        extern "C-unwind" fn resume(progress: &mut NativeFrameProgress, _: usize) {
            let runtime = state::current_runtime().expect("the admitted source root remains bound");
            let state = crate::current_task_execution_context().expect("the callback retains its state").state().raw();

            if state == 2 {
                // A yielded finalizer resumes through the frame's resume callback.
                cleanup_step(progress);

                return;
            }

            assert_eq!(state, 0, "failed later admission must not enter ordinary work");
            BODIES.fetch_add(1, Ordering::Relaxed);
            super::bray_runtime_worker_admission_state(runtime.scheduler.source_reference(), 1);
            *progress = NativeFrameProgress::new(NativeFrameProgressKind::YIELDED, 1, 0);
        }

        extern "C-unwind" fn cancel(progress: &mut NativeFrameProgress, _: usize) {
            *progress = NativeFrameProgress::new(NativeFrameProgressKind::CANCELLED, 1, 0);
        }

        extern "C-unwind" fn cleanup(progress: &mut NativeFrameProgress, _: usize, _: NativeFrameExit) {
            cleanup_step(progress);
        }

        fn cleanup_step(progress: &mut NativeFrameProgress) {
            assert_eq!(crate::context::current_task_execution_lane().expect("cleanup retains its actual lane").workload(), crate::ExecutionWorkload::Blocking);

            let step = CLEANUP_STEPS.fetch_add(1, Ordering::Relaxed);

            *progress = NativeFrameProgress::new(
                if step == 0 { NativeFrameProgressKind::YIELDED } else { NativeFrameProgressKind::COMPLETED },
                2,
                0,
            );
        }

        extern "C-unwind" fn destroy(_: usize) {
            DESTROYED.fetch_add(1, Ordering::Relaxed);
        }

        let _isolation = crate::native::test_runtime_isolation();

        BODIES.store(0, Ordering::Relaxed);
        CLEANUP_STEPS.store(0, Ordering::Relaxed);
        DESTROYED.store(0, Ordering::Relaxed);

        assert!(export::bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(4, 1)).is_success());

        let frame = super::super::fixtures::admit(0, NativeActivationDescriptor {
            storage: NativeCleanupStorageShape::new([93; 32], 8, 8),
            header: 0,
            completion: 0,
            completion_bytes: 8,
            completion_align: 8,
            states: 3,
            state: requirements,
            resume,
            cancel,
            broadcast,
            resolve: cleanup,
            move_completion: completion,
            destroy,
        });

        let task = export::bray_runtime_task_allocation().task().expect("the task is admitted");

        let mut admitted = NativeRunOutcome::new(NativeRunState::PENDING, 0);

        assert!(export::bray_runtime_task_start(task, frame, &mut admitted).is_success());

        let mut outcome = state::with_runtime(|runtime| runtime.resolve_task(task)).expect("the root remains live");

        assert_eq!(outcome.state(), NativeRunState::PANICKED);
        assert_eq!(BODIES.load(Ordering::Relaxed), 1);
        assert_eq!(CLEANUP_STEPS.load(Ordering::Relaxed), 2);
        assert!(outcome.take_report().consume(false).is_success());
        assert!(state::with_runtime(|runtime| runtime.resolve_task_retirement(task)).expect("the owner remains live").is_success());
        assert!(export::bray_runtime_task_destruction(task).is_success());
        assert_eq!(DESTROYED.load(Ordering::Relaxed), 1);
        assert!(export::bray_runtime_execution_shutdown().is_success());
    }

    #[test]
    fn late_worker_failure_preserves_cleanup_waits_results_and_original_panics() {
        use crate::native::{export, fixtures, state};

        use bray_runtime_abi::{
            NativeFrameAffinity, NativeFrameState, NativeLaneRequirements,
            NativePanicReport, NativeRunState, NativeRuntimeConfiguration, NativeRuntimeStatus,
        };

        use std::sync::atomic::AtomicUsize;

        static STEPS: AtomicUsize = AtomicUsize::new(0);
        static RESOLVERS: AtomicUsize = AtomicUsize::new(0);
        static ABANDONED: AtomicUsize = AtomicUsize::new(0);
        static MOVED: AtomicUsize = AtomicUsize::new(0);
        static CHILD_CONSUMED: AtomicUsize = AtomicUsize::new(0);
        static EVENTS: [AtomicUsize; 2] = [const { AtomicUsize::new(0) }; 2];

        extern "C" fn requirements(mode: usize, state: u32, operation: u32, _: usize) -> NativeFrameState {
            NativeFrameState::new(
                NativeFrameAffinity::MOVABLE,
                if (mode == 6 && operation == 5) || (state != 0 && mode != 5) {
                    NativeLaneRequirements::COMPUTE
                } else {
                    NativeLaneRequirements::NONE
                },
            )
        }

        fn close_workers() {
            let runtime = state::current_runtime().expect("cleanup retains its Source runtime");

            super::bray_runtime_worker_admission_state(runtime.scheduler.source_reference(), 1);
        }

        extern "C-unwind" fn resume(progress: &mut NativeFrameProgress, mode: usize) {
            let current = crate::current_task_execution_context().expect("the callback retains its continuation").state().raw();

            if mode == 6 {
                *progress = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 17);
            } else if mode == 10 {
                assert_ne!(current, 0, "the canceled parent abandons the child before body entry");
                cleanup_step(progress, mode);
            } else if mode == 8 {
                if current == 0 {
                    crate::context::enter_current_run_cleanup_shield();
                }

                cleanup_step(progress, mode);

                if progress.kind() == NativeFrameProgressKind::COMPLETED {
                    crate::context::leave_current_run_cleanup_shield();
                }
            } else if current == 0 {
                if mode == 2 {
                    panic!("original body panic");
                }

                if mode == 9 {
                    fixtures::compose(child(10), 1);
                    *progress = NativeFrameProgress::new(NativeFrameProgressKind::CANCELLED, 1, 0);
                } else if mode == 5 {
                    fixtures::compose(child(6), 1);
                    *progress = NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0);
                } else {
                    *progress = NativeFrameProgress::new(
                        if mode == 1 { NativeFrameProgressKind::CANCELLED } else { NativeFrameProgressKind::COMPLETED },
                        0,
                        0,
                    );
                }
            } else {
                if mode == 4 && STEPS.load(Ordering::Relaxed) == 0 {
                    consume_child();
                }

                cleanup_step(progress, mode);
            }
        }

        fn consume_child() {
            let mut outcome = NativeRunOutcome::new(NativeRunState::PENDING, 0);

            fixtures::take_completion(&mut outcome);
            assert_eq!(outcome.state(), NativeRunState::COMPLETED);
            CHILD_CONSUMED.fetch_add(1, Ordering::Relaxed);
        }

        extern "C-unwind" fn cancel(progress: &mut NativeFrameProgress, mode: usize) {
            assert_eq!(mode, 5, "only an initialized incoming result needs cancellation entry");
            consume_child();
            *progress = NativeFrameProgress::new(NativeFrameProgressKind::CANCELLED, 1, 0);
        }

        extern "C-unwind" fn cleanup(progress: &mut NativeFrameProgress, mode: usize, exit: NativeFrameExit) {
            if mode == 10 {
                assert_eq!(exit, NativeFrameExit::RUNTIME_FAILURE);
                cleanup_step(progress, mode);
                return;
            }

            if mode == 9 {
                assert_eq!(exit, NativeFrameExit::CANCELLED);
                assert_eq!(STEPS.load(Ordering::Relaxed), 3);
                *progress = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
                return;
            }

            if mode == 8 {
                if exit == NativeFrameExit::COMPLETED {
                    assert_eq!(STEPS.load(Ordering::Relaxed), 3);
                    *progress = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
                    return;
                }

                assert_eq!(exit, NativeFrameExit::RUNTIME_FAILURE);
                assert_eq!(STEPS.load(Ordering::Relaxed), 3);
                ABANDONED.fetch_add(1, Ordering::Relaxed);
                *progress = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
                return;
            }

            if mode == 6 || mode == 5 {
                *progress = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
                return;
            }

            if RESOLVERS.fetch_add(1, Ordering::Relaxed) != 0 {
                assert_eq!(exit, NativeFrameExit::RUNTIME_FAILURE);
                assert_eq!(STEPS.load(Ordering::Relaxed), 3);
                ABANDONED.fetch_add(1, Ordering::Relaxed);
                *progress = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
                return;
            }

            assert_eq!(exit, match mode {
                1 => NativeFrameExit::CANCELLED,
                2 => NativeFrameExit::PANICKED,
                _ => NativeFrameExit::COMPLETED,
            });

            if mode == 4 {
                fixtures::compose(child(7), 1);
                *progress = NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0);
            } else {
                cleanup_step(progress, mode);
            }
        }

        extern "C-unwind" fn child_resume(progress: &mut NativeFrameProgress, _: usize) {
            let current = crate::current_task_execution_context().expect("child continuation is live").state().raw();

            *progress = if current == 0 {
                close_workers();

                NativeFrameProgress::new(NativeFrameProgressKind::TASK_EVENT, 1, EVENTS[0].load(Ordering::Relaxed))
            } else {
                NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 1, 17)
            };
        }

        fn cleanup_step(progress: &mut NativeFrameProgress, mode: usize) {
            let step = STEPS.fetch_add(1, Ordering::Relaxed);

            if step == 0 && mode != 4 {
                close_workers();
            }

            *progress = NativeFrameProgress::new(
                if step == 2 { NativeFrameProgressKind::COMPLETED }
                else if matches!(mode, 3 | 8 | 10) { NativeFrameProgressKind::TASK_EVENT }
                else { NativeFrameProgressKind::YIELDED },
                step as u32 + 1,
                if matches!(mode, 3 | 8 | 10) && step < 2 { EVENTS[step].load(Ordering::Relaxed) } else { 0 },
            );
        }

        extern "C-unwind" fn moved(mode: usize, _: usize) {
            if mode == 6 {
                close_workers();
            } else if mode != 7 && mode != 10 {
                MOVED.fetch_add(1, Ordering::Relaxed);
            }
        }

        fn descriptor(resume: bray_runtime_abi::NativeFrameResumeCallback) -> NativeActivationDescriptor {
            NativeActivationDescriptor {
                storage: NativeCleanupStorageShape::new([94; 32], 8, 8),
                header: 0, completion: 0, completion_bytes: 8, completion_align: 8,
                states: 4, state: requirements, resume, cancel, broadcast,
                resolve: cleanup, move_completion: moved, destroy: ignore,
            }
        }

        fn child(mode: usize) -> NativeInactiveFrame {
            let mut callbacks = descriptor(if mode == 7 { child_resume } else { resume });

            callbacks.resolve = if mode == 10 { cleanup } else { resolve };

            fixtures::admit(mode, callbacks)
        }

        fn drive_cleanup() {
            let status = state::with_runtime(|runtime| runtime.with_cleanup_driving(|| runtime.drive_main_thread()))
                .expect("cleanup retains its established runtime");

            assert!(status.is_success() || status == NativeRuntimeStatus::PENDING, "cleanup drive: {status:?}");
        }

        fn wait_for_failure(task: bray_runtime_abi::NativeTaskHandle) {
            let limit = std::time::Instant::now() + std::time::Duration::from_secs(5);

            loop {
                let failed = state::with_runtime(|runtime| {
                    let task = runtime.retained_task(task).expect("the task remains live");

                    task.has_worker_failure() || task.snapshot().phase >= 3
                }).expect("the worker retains its runtime");

                if failed {
                    return;
                }

                assert!(std::time::Instant::now() < limit, "closed worker admission must settle");
                std::thread::yield_now();
            }
        }

        fn wait_for_registration(task: bray_runtime_abi::NativeTaskHandle, mode: usize, steps: usize) {
            let limit = std::time::Instant::now() + std::time::Duration::from_secs(5);

            loop {
                drive_cleanup();

                let waiting = state::with_runtime(|runtime| {
                    let task = runtime.retained_task(task).expect("the suspended task remains live");
                    let wait = task.suspended_wait.lock().expect("wait transport remains available");

                    wait.as_ref().is_some_and(|wait| !wait.is_ready())
                }).expect("the runtime remains live");

                if waiting && STEPS.load(Ordering::Relaxed) == steps {
                    return;
                }

                assert!(
                    std::time::Instant::now() < limit,
                    "mode {mode}: expected cleanup step {steps} at an unready event, found step {} and waiting={waiting}",
                    STEPS.load(Ordering::Relaxed),
                );

                std::thread::yield_now();
            }
        }

        let _isolation = crate::native::test_runtime_isolation();

        for mode in [0, 1, 2, 3, 4, 5, 8, 9] {
            for count in [&STEPS, &RESOLVERS, &ABANDONED, &MOVED, &CHILD_CONSUMED] {
                count.store(0, Ordering::Relaxed);
            }

            assert!(export::bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(4, 1)).is_success());

            for event in &EVENTS {
                event.store(export::bray_runtime_task_event_creation(), Ordering::Relaxed);
            }

            let records = crate::outgoing::OutgoingRecords::admit(1).expect("one frame report record is sufficient");
            let frame = fixtures::admit_with_records(mode, descriptor(resume), records);
            let task = export::bray_runtime_task_allocation().task().expect("the task is admitted");
            let mut admitted = NativeRunOutcome::new(NativeRunState::PENDING, 0);

            assert!(export::bray_runtime_task_start(task, frame, &mut admitted).is_success());

            wait_for_failure(task);

            if matches!(mode, 3 | 4 | 8 | 9) {
                wait_for_registration(task, mode, usize::from(mode != 4));

                // A fault and stale wake cannot resume a finalizer's unready wait.
                for _ in 0..4 {
                    assert!(export::bray_runtime_wake(task).is_success());
                    drive_cleanup();
                }

                assert_eq!(STEPS.load(Ordering::Relaxed), usize::from(mode != 4));
                assert!(export::bray_runtime_task_event_signal(EVENTS[0].load(Ordering::Relaxed)).is_success());

                if matches!(mode, 3 | 8 | 9) {
                    wait_for_registration(task, mode, 2);
                    assert_eq!(STEPS.load(Ordering::Relaxed), 2);
                    assert!(export::bray_runtime_task_event_signal(EVENTS[1].load(Ordering::Relaxed)).is_success());
                }
            }

            let mut outcome = state::with_runtime(|runtime| runtime.with_cleanup_driving(|| runtime.resolve_task(task)))
                .expect("the task remains live");

            assert_eq!(outcome.state(), NativeRunState::PANICKED, "mode {mode}");
            assert_eq!(STEPS.load(Ordering::Relaxed), if mode == 5 { 0 } else { 3 }, "mode {mode}");
            assert_eq!(ABANDONED.load(Ordering::Relaxed), usize::from(matches!(mode, 0 | 3 | 4 | 8)), "mode {mode}");
            assert_eq!(MOVED.load(Ordering::Relaxed), 0);
            assert_eq!(CHILD_CONSUMED.load(Ordering::Relaxed), usize::from(matches!(mode, 4 | 5)));

            let (primary, head, tail, count, reserved) = outcome.take_report().take_parts();

            let primary = primary.expect("the operational or original panic stays primary");

            assert_eq!(primary.worker_failure().is_none(), mode == 2);
            assert_eq!(count, usize::from(mode == 2));

            let mut report = NativePanicReport::new(primary, crate::report_provider::bray_runtime_report_consumer);

            report.set_outgoing(head, tail, count);
            report.set_reserved(reserved);
            assert!(report.consume(false).is_success());
            assert!(state::with_runtime(|runtime| runtime.resolve_task_retirement(task)).expect("retirement retains its owner").is_success());
            assert!(export::bray_runtime_task_destruction(task).is_success());

            for event in &EVENTS {
                assert!(export::bray_runtime_task_event_destruction(event.load(Ordering::Relaxed)).is_success());
            }

            assert!(export::bray_runtime_execution_shutdown().is_success());
        }
    }

    #[test]
    fn concurrent_first_demands_share_one_worker_owner() {
        use crate::ExecutionWorkload;
        use crate::native::{export, state};
        use bray_runtime_abi::NativeRuntimeConfiguration;
        use std::sync::{Arc, Barrier};

        let _isolation = crate::native::test_runtime_isolation();

        assert!(
            export::bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(1, 1),)
                .is_success()
        );

        let core = Arc::clone(
            &state::current_runtime()
                .expect("the host is initialized")
                .core,
        );

        let barrier = Barrier::new(8);

        std::thread::scope(|scope| {
            for _ in 0..8 {
                let core = &core;
                let barrier = &barrier;

                scope.spawn(move || {
                    barrier.wait();

                    core.workers
                        .demand_workload(core, ExecutionWorkload::Cooperative)
                        .expect("concurrent demands retain the same admitted owner");
                });
            }
        });

        assert!(!core.workers.is_stopping(&core.scheduler));
        assert!(export::bray_runtime_execution_shutdown().is_success());
    }

    #[test]
    fn exact_thread_tasks_keep_their_worker_until_resolution() {
        use crate::native::{export, state};

        use bray_runtime_abi::{
            NativeFrameAffinity, NativeFrameState,
            NativeLaneRequirements, NativeRunState,
            NativeRuntimeConfiguration, NativeTaskHandle,
        };

        use std::sync::atomic::AtomicU64;

        static CHILD: AtomicU64 = AtomicU64::new(0);
        static ORIGIN: AtomicU64 = AtomicU64::new(0);
        static RESUMES: AtomicU64 = AtomicU64::new(0);

        extern "C" fn requirements(_: usize, _: u32, _: u32, _: usize) -> NativeFrameState {
            NativeFrameState::new(
                NativeFrameAffinity::ORIGIN_THREAD,
                NativeLaneRequirements::NONE,
            )
        }

        extern "C" fn blocking(_: usize, _: u32, _: u32, _: usize) -> NativeFrameState {
            NativeFrameState::new(
                NativeFrameAffinity::MOVABLE,
                NativeLaneRequirements::BLOCKING,
            )
        }

        extern "C-unwind" fn child(progress: &mut NativeFrameProgress, _: usize) {
            let thread = state::current_runtime()
                .expect("the worker remains attached")
                .thread
                .runtime()
                .id()
                .raw();

            assert_eq!(thread, ORIGIN.load(Ordering::Acquire));

            *progress = if RESUMES.fetch_add(1, Ordering::AcqRel) == 0 {
                NativeFrameProgress::new(NativeFrameProgressKind::YIELDED, 1, 0)
            } else {
                NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 1, 0)
            };
        }

        extern "C-unwind" fn parent(progress: &mut NativeFrameProgress, _: usize) {
            let runtime = state::current_runtime().expect("the parent executes on its worker");

            ORIGIN.store(runtime.thread.runtime().id().raw(), Ordering::Release);

            let task = runtime
                .allocate()
                .task()
                .expect("the independent exact-thread task is admitted");

            assert!(
                runtime
                    .start(task, &frame(requirements, child), &mut bray_runtime_abi::NativeRunOutcome::new(NativeRunState::PENDING, 0))
                    .is_success()
            );

            CHILD.store(task.raw(), Ordering::Release);

            *progress = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
        }

        let _isolation = crate::native::test_runtime_isolation();

        CHILD.store(0, Ordering::Relaxed);
        ORIGIN.store(0, Ordering::Relaxed);
        RESUMES.store(0, Ordering::Relaxed);

        assert!(
            export::bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(1, 1))
                .is_success()
        );

        let parent_task = export::bray_runtime_task_allocation()
            .task()
            .expect("the parent is admitted");

        let mut admitted = NativeRunOutcome::new(NativeRunState::PENDING, 0);

        assert!(export::bray_runtime_task_start(parent_task, frame(blocking, parent), &mut admitted).is_success());

        let deadline = std::time::Instant::now() + Duration::from_secs(5);

        let child = loop {
            if let Some(child) = NativeTaskHandle::new(CHILD.load(Ordering::Acquire)) {
                if state::with_runtime(|runtime| runtime.observe(child))
                    .expect("the host remains active")
                    .state()
                    == NativeRunState::COMPLETED
                {
                    break child;
                }
            }

            assert!(
                std::time::Instant::now() < deadline,
                "the exact-thread child must resume on its original worker"
            );

            std::thread::yield_now();
        };

        assert_eq!(RESUMES.load(Ordering::Acquire), 2);

        assert_eq!(
            state::with_runtime(|runtime| runtime.observe(parent_task))
                .expect("the parent remains owned")
                .state(),
            NativeRunState::COMPLETED
        );

        for task in [child, parent_task] {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);

            loop {
                let status = state::with_runtime(|runtime| runtime.retire_task(task))
                    .expect("the run remains owned");

                if status.is_success() {
                    break;
                }

                assert!(
                    std::time::Instant::now() < deadline,
                    "retirement must progress: {:?}",
                    state::with_runtime(|runtime| (
                        runtime
                            .retained_task(task)
                            .expect("the run remains retained")
                            .snapshot(),
                        runtime.scheduler.snapshot()
                    ))
                    .expect("the runtime remains active")
                );

                std::thread::yield_now();
            }

            assert!(export::bray_runtime_task_destruction(task).is_success());
        }

        assert!(export::bray_runtime_execution_shutdown().is_success());
    }

    #[test]
    fn blocked_work_keeps_other_lanes_and_blocking_children_running() {
        use crate::native::{export, state};

        use bray_runtime_abi::{
            NativeFrameAffinity, NativeFrameState, NativeLaneRequirements,
            NativeRunState, NativeRuntimeConfiguration, NativeRuntimeStatus,
        };

        use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64};
        use std::time::Instant;

        static ENTERED: AtomicBool = AtomicBool::new(false);
        static RELEASE: AtomicBool = AtomicBool::new(false);
        static FINISHED: AtomicU32 = AtomicU32::new(0);
        static RESUMES: AtomicU32 = AtomicU32::new(0);
        static MAIN: AtomicU64 = AtomicU64::new(0);
        const COMPUTE_RESUMES: u32 = 256;

        struct ReleaseBlockedWorker;

        impl Drop for ReleaseBlockedWorker {
            fn drop(&mut self) {
                RELEASE.store(true, Ordering::Release);
            }
        }

        extern "C" fn blocking(_: usize, _: u32, _: u32, _: usize) -> NativeFrameState {
            NativeFrameState::new(NativeFrameAffinity::MOVABLE, NativeLaneRequirements::BLOCKING)
        }

        extern "C" fn compute(_: usize, _: u32, _: u32, _: usize) -> NativeFrameState {
            NativeFrameState::new(NativeFrameAffinity::MOVABLE, NativeLaneRequirements::COMPUTE)
        }

        extern "C" fn main(_: usize, _: u32, _: u32, _: usize) -> NativeFrameState {
            NativeFrameState::new(NativeFrameAffinity::MAIN_THREAD, NativeLaneRequirements::MAIN_THREAD)
        }

        extern "C-unwind" fn held(progress: &mut NativeFrameProgress, _: usize) {
            ENTERED.store(true, Ordering::Release);

            while !RELEASE.load(Ordering::Acquire) {
                std::thread::park_timeout(Duration::from_millis(1));
            }

            *progress = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
        }

        extern "C-unwind" fn child(progress: &mut NativeFrameProgress, _: usize) {
            FINISHED.fetch_or(1, Ordering::Release);
            *progress = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
        }

        extern "C-unwind" fn compute_work(progress: &mut NativeFrameProgress, _: usize) {
            let complete = RESUMES.fetch_add(1, Ordering::Relaxed) + 1 == COMPUTE_RESUMES;

            if complete {
                FINISHED.fetch_or(2, Ordering::Release);
            }

            *progress = NativeFrameProgress::new(
                if complete { NativeFrameProgressKind::COMPLETED } else { NativeFrameProgressKind::YIELDED }, 0, 0,
            );
        }

        extern "C-unwind" fn main_work(progress: &mut NativeFrameProgress, _: usize) {
            let runtime = state::current_runtime().expect("main work retains its attachment");

            assert_eq!(runtime.thread.runtime().id().raw(), MAIN.load(Ordering::Acquire));
            FINISHED.fetch_or(4, Ordering::Release);
            *progress = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
        }

        let _isolation = crate::native::test_runtime_isolation();

        ENTERED.store(false, Ordering::Relaxed);
        RELEASE.store(false, Ordering::Relaxed);
        FINISHED.store(0, Ordering::Relaxed);
        RESUMES.store(0, Ordering::Relaxed);

        assert!(export::bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(4, 1)).is_success());

        MAIN.store(state::current_runtime().expect("the host is active").thread.runtime().id().raw(), Ordering::Release);

        let _release = ReleaseBlockedWorker;
        let started = Instant::now();
        let mut tasks = Vec::new();

        for (requirements, resume) in [
            (blocking as bray_runtime_abi::NativeFrameStateCallback, held as bray_runtime_abi::NativeFrameResumeCallback),
            (blocking, child), (compute, compute_work), (main, main_work),
        ] {
            let task = export::bray_runtime_task_allocation().task().expect("the lane task is admitted");

            let mut admitted = NativeRunOutcome::new(NativeRunState::PENDING, 0);

            assert!(export::bray_runtime_task_start(task, frame(requirements, resume), &mut admitted).is_success());
            tasks.push(task);

            if tasks.len() == 1 {
                let deadline = Instant::now() + Duration::from_secs(5);

                while !ENTERED.load(Ordering::Acquire) && Instant::now() < deadline {
                    std::thread::yield_now();
                }

                if !ENTERED.load(Ordering::Acquire) {
                    RELEASE.store(true, Ordering::Release);
                    panic!("the first blocking callback must enter");
                }
            }
        }

        let deadline = Instant::now() + Duration::from_secs(5);

        while FINISHED.load(Ordering::Acquire) != 7 && Instant::now() < deadline {
            let status = export::bray_runtime_main_thread_lane_drive();

            assert!(status.is_success() || status == NativeRuntimeStatus::PENDING);
            std::thread::yield_now();
        }

        let finished_while_blocked = FINISHED.load(Ordering::Acquire);

        RELEASE.store(true, Ordering::Release);
        assert_eq!(finished_while_blocked, 7, "independent lanes and a blocking child must progress");
        println!("compute_resumes={COMPUTE_RESUMES} elapsed_ns={}", started.elapsed().as_nanos());

        for task in tasks {
            let runtime = state::current_runtime().expect("the host stays active");

            assert_eq!(runtime.resolve_task(task).state(), NativeRunState::COMPLETED);
            assert!(runtime.resolve_task_retirement(task).is_success());

            assert!(export::bray_runtime_task_destruction(task).is_success());
        }

        assert!(export::bray_runtime_execution_shutdown().is_success());
    }
}
