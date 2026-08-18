use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use bray_platform::NativeThread;

use crate::ExecutionWorkload;

use super::state::NativeRuntimeCore;

pub(super) struct WorkerPool {
    stopping: AtomicBool,
    threads: Mutex<Vec<NativeThread<()>>>,
}

impl WorkerPool {
    pub(super) const fn new() -> Self {
        Self {
            stopping: AtomicBool::new(false),
            threads: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn start(&self, runtime: &Arc<NativeRuntimeCore>) -> bool {
        for workload in [
            ExecutionWorkload::Cooperative,
            ExecutionWorkload::Blocking,
            ExecutionWorkload::Compute,
        ] {
            let worker_runtime = Arc::clone(runtime);

            let thread = NativeThread::spawn(None, move |thread| {
                super::state::run_worker(worker_runtime, thread, workload);
            });

            let Ok(thread) = thread else {
                self.stop(runtime.scheduler());

                return false;
            };

            self.threads
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(thread);
        }

        true
    }

    pub(super) fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::Acquire)
    }

    pub(super) fn stop(&self, scheduler: &crate::Scheduler) {
        self.stopping.store(true, Ordering::Release);
        scheduler.wake_waiters();

        let threads = std::mem::take(
            &mut *self
                .threads
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );

        for thread in threads {
            let _ = thread.join();
        }
    }
}
