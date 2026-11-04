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

pub(in crate::native) fn take_failure() -> Option<bray_runtime_abi::NativePanicReport> {
    super::state::current_services().and_then(|task| task.take_worker_failure())
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
            cancel: resume,
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
