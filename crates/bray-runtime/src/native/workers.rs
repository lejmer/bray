use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use bray_platform::NativeThread;

use crate::ExecutionWorkload;

use super::state::NativeRuntimeCore;

pub(super) struct WorkerPool {
    stopping: AtomicBool,
    idle_blocking_workers: AtomicUsize,
    workers: Mutex<Vec<Worker>>,
}

struct Worker {
    thread: NativeThread<()>,
    control: Arc<WorkerControl>,
}

#[derive(Default)]
pub(super) struct WorkerControl {
    state: Mutex<WorkerControlState>,
}

#[derive(Default)]
struct WorkerControlState {
    requests: VecDeque<Arc<WorkerRequest>>,
    retired: bool,
}

struct WorkerRequest {
    product: usize,
    complete: Mutex<bool>,
    completed: Condvar,
}

impl WorkerPool {
    pub(super) const fn new() -> Self {
        Self {
            stopping: AtomicBool::new(false),
            idle_blocking_workers: AtomicUsize::new(0),
            workers: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn start(&self, runtime: &Arc<NativeRuntimeCore>) -> bool {
        for workload in [
            ExecutionWorkload::Cooperative,
            ExecutionWorkload::Blocking,
            ExecutionWorkload::Compute,
        ] {
            if !self.spawn(runtime, workload) {
                self.stop(runtime.scheduler(), None);

                return false;
            }
        }

        true
    }

    fn spawn(&self, runtime: &Arc<NativeRuntimeCore>, workload: ExecutionWorkload) -> bool {
        let worker_runtime = Arc::clone(runtime);
        let control = Arc::new(WorkerControl::default());
        let worker_control = Arc::clone(&control);

        let (attached_sender, attached_receiver) = std::sync::mpsc::sync_channel(1);

        let mut workers = self
            .workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if self.is_stopping() {
            return false;
        }

        if workload == ExecutionWorkload::Blocking {
            self.idle_blocking_workers.fetch_add(1, Ordering::AcqRel);
        }

        let thread = NativeThread::spawn(None, move || {
            let attachment = crate::bootstrap::RuntimeThreadScope::enter_or_reuse();

            if attached_sender.send(attachment.is_ok()).is_err() {
                return;
            }

            if let Ok(attachment) = attachment {
                super::state::run_worker(worker_runtime, attachment, workload, worker_control);
            }
        });

        let thread = thread.ok().and_then(|thread| {
            if attached_receiver.recv().unwrap_or(false) {
                Some(thread)
            } else {
                let _ = thread.join();

                None
            }
        });

        let Some(thread) = thread else {
            if workload == ExecutionWorkload::Blocking {
                self.idle_blocking_workers.fetch_sub(1, Ordering::AcqRel);
            }

            return false;
        };

        let mut index = 0;

        while index < workers.len() {
            if workers[index].thread.is_finished() {
                let finished = workers.swap_remove(index);
                let _ = finished.thread.join();
            } else {
                index += 1;
            }
        }

        workers.push(Worker { thread, control });

        true
    }

    pub(super) fn begin_blocking_work(&self, runtime: &Arc<NativeRuntimeCore>) {
        let idle = self.idle_blocking_workers.fetch_sub(1, Ordering::AcqRel);

        debug_assert!(
            idle > 0,
            "a blocking worker must account for its idle state"
        );

        if idle == 1 && !self.is_stopping() {
            let _ = self.spawn(runtime, ExecutionWorkload::Blocking);
        }
    }

    pub(super) fn finish_blocking_work(&self) -> bool {
        if self.is_stopping() {
            return false;
        }

        let mut idle = self.idle_blocking_workers.load(Ordering::Acquire);

        while idle < 2 {
            match self.idle_blocking_workers.compare_exchange_weak(
                idle,
                idle + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(observed) => idle = observed,
            }
        }

        false
    }

    pub(super) fn retire(
        &self,
        workload: ExecutionWorkload,
        control: &Arc<WorkerControl>,
        accounted_as_idle: bool,
    ) {
        if workload == ExecutionWorkload::Blocking && accounted_as_idle {
            self.idle_blocking_workers.fetch_sub(1, Ordering::AcqRel);
        }

        control.retire();
    }

    pub(super) fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::Acquire)
    }

    pub(super) fn stop(&self, scheduler: &crate::Scheduler, current: Option<&Arc<WorkerControl>>) {
        self.stopping.store(true, Ordering::Release);
        scheduler.wake_waiters();

        let workers = std::mem::take(
            &mut *self
                .workers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );

        for worker in workers {
            if current.is_some_and(|current| Arc::ptr_eq(current, &worker.control)) {
                continue;
            }

            let _ = worker.thread.join();
        }
    }

    pub(super) fn detach_product(
        &self,
        product: usize,
        current: Option<&Arc<WorkerControl>>,
        scheduler: &crate::Scheduler,
    ) {
        let controls = self
            .workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .map(|worker| Arc::clone(&worker.control))
            .collect::<Vec<_>>();

        let requests = controls
            .iter()
            .map(|control| (control, control.request(product)))
            .collect::<Vec<_>>();

        for (control, _) in &requests {
            if current.is_some_and(|current| Arc::ptr_eq(current, control)) {
                control.drain();
            }
        }

        scheduler.wake_waiters();

        for (_, request) in requests {
            request.wait();
        }
    }
}

impl WorkerControl {
    fn request(&self, product: usize) -> Arc<WorkerRequest> {
        let request = Arc::new(WorkerRequest {
            product,
            complete: Mutex::new(false),
            completed: Condvar::new(),
        });

        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if state.retired {
            request.complete();
        } else {
            state.requests.push_back(Arc::clone(&request));
        }

        request
    }

    pub(super) fn drain(&self) {
        let requests = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            std::mem::take(&mut state.requests)
        };

        for request in requests {
            let _ = crate::product::drain_product_thread_statics(request.product);

            request.complete();
        }
    }

    fn retire(&self) {
        let requests = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            state.retired = true;

            std::mem::take(&mut state.requests)
        };

        for request in requests {
            let _ = crate::product::drain_product_thread_statics(request.product);

            request.complete();
        }
    }
}

impl WorkerRequest {
    fn complete(&self) {
        *self
            .complete
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = true;

        self.completed.notify_all();
    }

    fn wait(&self) {
        let complete = self
            .complete
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        drop(
            self.completed
                .wait_while(complete, |complete| !*complete)
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;
    use std::time::Duration;

    use super::{WorkerControl, WorkerPool};

    #[test]
    fn dequeued_request_waits_for_cleanup_completion() {
        let control = WorkerControl::default();
        let request = control.request(1);

        let dequeued = control
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .requests
            .pop_front()
            .unwrap_or_else(|| panic!("worker request must be queued"));

        let (started_sender, started_receiver) = mpsc::channel();

        let (done_sender, done_receiver) = mpsc::channel();

        let waiter = std::thread::spawn(move || {
            started_sender
                .send(())
                .unwrap_or_else(|error| panic!("waiter must start: {error}"));

            request.wait();

            done_sender
                .send(())
                .unwrap_or_else(|error| panic!("waiter must finish: {error}"));
        });

        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap_or_else(|error| panic!("waiter must report startup: {error}"));

        assert!(done_receiver.try_recv().is_err());

        dequeued.complete();

        done_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap_or_else(|error| panic!("completed request must release waiter: {error}"));

        waiter.join().unwrap_or_else(|_| panic!("waiter must join"));
    }

    #[test]
    fn retired_controls_complete_cleanup_requests_immediately() {
        let control = WorkerControl::default();

        control.retire();

        let request = control.request(1);

        request.wait();
    }

    #[test]
    fn exact_thread_tasks_keep_an_excess_blocking_worker_until_resolution() {
        use crate::native::{export, fixtures, state};

        use bray_runtime_abi::{
            NativeActivationDescriptor, NativeCleanupStorageShape, NativeFrameAffinity,
            NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind, NativeFrameState,
            NativeInactiveFrame, NativeLaneRequirements, NativeRunState,
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
            fixtures::admit(
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
                    .start(task, &frame(requirements, child))
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

        let core = state::current_runtime()
            .expect("the host owns the runtime")
            .core
            .clone();

        // With three idle workers, the parent consumes one and leaves the full
        // reserve. Its worker would retire immediately without its child owner.
        assert!(
            core.workers
                .spawn(&core, crate::ExecutionWorkload::Blocking)
        );

        assert!(
            core.workers
                .spawn(&core, crate::ExecutionWorkload::Blocking)
        );

        let parent_task = export::bray_runtime_task_allocation()
            .task()
            .expect("the parent is admitted");

        assert!(export::bray_runtime_task_start(parent_task, frame(blocking, parent)).is_success());

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
            core.workers.idle_blocking_workers.load(Ordering::Acquire),
            2
        );

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
    fn completed_blocking_work_keeps_a_bounded_idle_reserve() {
        let workers = WorkerPool::new();

        assert!(workers.finish_blocking_work());
        assert!(workers.finish_blocking_work());
        assert!(!workers.finish_blocking_work());

        assert_eq!(workers.idle_blocking_workers.load(Ordering::Acquire), 2);
    }
}
