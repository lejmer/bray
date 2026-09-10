use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use bray_platform::NativeThread;
use bray_runtime_abi::NativeRuntimeStatus;

use super::super::state::NativeRuntimeCore;
use super::control::WorkerControl;
use crate::ExecutionWorkload;

pub(in crate::native) struct WorkerPool {
    stopping: AtomicBool,
    idle_blocking_workers: AtomicUsize,
    workers: Mutex<WorkerRegistry>,
}

struct WorkerRegistry {
    workers: Vec<Worker>,
    controls: Option<triomphe::Arc<Vec<Arc<WorkerControl>>>>,
}

struct Worker {
    thread: NativeThread<()>,
    control: Arc<WorkerControl>,
}

impl WorkerRegistry {
    fn prepare_control(
        &mut self,
        control: &Arc<WorkerControl>,
    ) -> Result<triomphe::Arc<Vec<Arc<WorkerControl>>>, NativeRuntimeStatus> {
        crate::allocation::reserve_vec_entries(&mut self.workers, 1)
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        let mut controls = Vec::new();

        let count = self
            .workers
            .len()
            .checked_add(1)
            .ok_or(NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        crate::allocation::reserve_vec_entries(&mut controls, count)
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        // The immutable snapshot lets closures release the worker registry lock before callbacks.
        controls.extend(
            self.workers
                .iter()
                .map(|worker| Arc::clone(&worker.control)),
        );

        controls.push(Arc::clone(control));

        triomphe::Arc::try_new(controls).map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)
    }
}

impl WorkerPool {
    pub(in crate::native) const fn new() -> Self {
        Self {
            stopping: AtomicBool::new(false),
            idle_blocking_workers: AtomicUsize::new(0),
            workers: Mutex::new(WorkerRegistry {
                workers: Vec::new(),
                controls: None,
            }),
        }
    }

    pub(in crate::native) fn start(&self, runtime: &Arc<NativeRuntimeCore>) -> bool {
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

        let mut registry = self
            .workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if self.is_stopping() {
            return false;
        }

        let mut index = 0;

        while index < registry.workers.len() {
            if registry.workers[index].thread.is_finished() {
                let finished = registry.workers.swap_remove(index);
                let _ = finished.thread.join();
            } else {
                index += 1;
            }
        }

        let Ok(controls) = registry.prepare_control(&control) else {
            return false;
        };

        if workload == ExecutionWorkload::Blocking {
            self.idle_blocking_workers.fetch_add(1, Ordering::AcqRel);
        }

        let thread = NativeThread::spawn(None, move |thread| {
            super::super::state::run_worker(worker_runtime, thread, workload, worker_control);
        });

        let Ok(thread) = thread else {
            if workload == ExecutionWorkload::Blocking {
                self.idle_blocking_workers.fetch_sub(1, Ordering::AcqRel);
            }

            return false;
        };

        registry.workers.push(Worker { thread, control });
        registry.controls = Some(controls);

        true
    }

    pub(in crate::native) fn begin_blocking_work(&self, runtime: &Arc<NativeRuntimeCore>) {
        let idle = self.idle_blocking_workers.fetch_sub(1, Ordering::AcqRel);

        debug_assert!(
            idle > 0,
            "a blocking worker must account for its idle state"
        );

        if idle == 1 && !self.is_stopping() {
            let _ = self.spawn(runtime, ExecutionWorkload::Blocking);
        }
    }

    pub(in crate::native) fn finish_blocking_work(
        &self,
        retain_thread: impl FnOnce() -> bool,
    ) -> bool {
        if self.is_stopping() {
            return false;
        }

        self.idle_blocking_workers.fetch_add(1, Ordering::AcqRel);

        !self.try_retire_idle_blocking_worker(retain_thread)
    }

    pub(in crate::native) fn try_retire_idle_blocking_worker(
        &self,
        retain_thread: impl FnOnce() -> bool,
    ) -> bool {
        let mut idle = self.idle_blocking_workers.load(Ordering::Acquire);

        if idle <= 2 || retain_thread() {
            return false;
        }

        while idle > 2 {
            match self.idle_blocking_workers.compare_exchange_weak(
                idle,
                idle - 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return true,
                Err(observed) => idle = observed,
            }
        }

        false
    }

    pub(in crate::native) fn retire(
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

    pub(in crate::native) fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::Acquire)
    }

    pub(in crate::native) fn stop(
        &self,
        scheduler: &crate::Scheduler,
        current: Option<&Arc<WorkerControl>>,
    ) {
        self.stopping.store(true, Ordering::Release);
        scheduler.wake_waiters();

        let workers = {
            let mut registry = self
                .workers
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            registry.controls = None;

            std::mem::take(&mut registry.workers)
        };

        for worker in workers {
            if current.is_some_and(|current| Arc::ptr_eq(current, &worker.control)) {
                continue;
            }

            let _ = worker.thread.join();
        }
    }

    pub(in crate::native) fn detach_product(
        &self,
        product: usize,
        current: Option<&Arc<WorkerControl>>,
        scheduler: &crate::Scheduler,
    ) {
        // Keep the published worker set alive without holding its lock through cleanup.
        let controls = self
            .workers
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .controls
            .clone();

        let Some(controls) = controls else {
            return;
        };

        for control in controls.iter() {
            control.request(product);
        }

        if let Some(current) = current {
            current.drain();
        }

        scheduler.wake_waiters();

        for control in controls.iter() {
            control.wait(product);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{WorkerControl, WorkerPool, WorkerRegistry};
    use crate::test_support::with_allocation_failure;
    use std::sync::Arc;

    #[test]
    fn failed_worker_snapshot_admission_preserves_published_controls() {
        let first = Arc::new(WorkerControl::default());
        let second = Arc::new(WorkerControl::default());
        let published = triomphe::Arc::new(vec![Arc::clone(&first)]);

        let mut registry = WorkerRegistry {
            workers: Vec::new(),
            controls: Some(published.clone()),
        };

        assert!(with_allocation_failure(|| registry.prepare_control(&second)).is_err());

        assert!(triomphe::Arc::ptr_eq(
            registry.controls.as_ref().unwrap(),
            &published
        ));

        assert!(Arc::ptr_eq(&registry.controls.as_ref().unwrap()[0], &first));
        let prepared = registry.prepare_control(&second).unwrap();
        assert_eq!(prepared.len(), 1);
        assert!(Arc::ptr_eq(&prepared[0], &second));

        assert!(triomphe::Arc::ptr_eq(
            registry.controls.as_ref().unwrap(),
            &published
        ));
    }

    use std::sync::atomic::Ordering;
    #[test]
    fn completed_blocking_work_keeps_a_bounded_idle_reserve() {
        let workers = WorkerPool::new();

        assert!(workers.finish_blocking_work(|| panic!("idle reserve must retain the worker")));
        assert!(workers.finish_blocking_work(|| panic!("idle reserve must retain the worker")));
        assert!(!workers.finish_blocking_work(|| false));

        assert_eq!(workers.idle_blocking_workers.load(Ordering::Acquire), 2);
    }

    #[test]
    fn affined_work_retains_a_worker_beyond_the_idle_reserve() {
        let workers = WorkerPool::new();
        workers.idle_blocking_workers.store(2, Ordering::Relaxed);

        assert!(workers.finish_blocking_work(|| true));
        assert_eq!(workers.idle_blocking_workers.load(Ordering::Acquire), 3);
        assert!(!workers.try_retire_idle_blocking_worker(|| true));
        assert!(workers.try_retire_idle_blocking_worker(|| false));

        assert!(
            !workers.try_retire_idle_blocking_worker(|| panic!("idle reserve must be retained"))
        );

        assert_eq!(workers.idle_blocking_workers.load(Ordering::Acquire), 2);
    }
}
