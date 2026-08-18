use std::sync::atomic::{AtomicBool, Ordering};
use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};

use bray_platform::NativeThread;

use crate::ExecutionWorkload;

use super::state::NativeRuntimeCore;

pub(super) struct WorkerPool {
    stopping: AtomicBool,
    threads: Mutex<Vec<NativeThread<()>>>,
    controls: Mutex<Vec<Arc<WorkerControl>>>,
}

#[derive(Default)]
pub(super) struct WorkerControl {
    state: Mutex<WorkerControlState>,
    completed: Condvar,
}

#[derive(Default)]
struct WorkerControlState {
    requests: VecDeque<usize>,
    completed: usize,
}

impl WorkerPool {
    pub(super) const fn new() -> Self {
        Self {
            stopping: AtomicBool::new(false),
            threads: Mutex::new(Vec::new()),
            controls: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn start(&self, runtime: &Arc<NativeRuntimeCore>) -> bool {
        for workload in [
            ExecutionWorkload::Cooperative,
            ExecutionWorkload::Blocking,
            ExecutionWorkload::Compute,
        ] {
            let worker_runtime = Arc::clone(runtime);
            let control = Arc::new(WorkerControl::default());
            let worker_control = Arc::clone(&control);

            let thread = NativeThread::spawn(None, move |thread| {
                super::state::run_worker(worker_runtime, thread, workload, worker_control);
            });

            let Ok(thread) = thread else {
                self.stop(runtime.scheduler(), None);

                return false;
            };

            self.threads
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(thread);

            self.controls
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(control);
        }

        true
    }

    pub(super) fn is_stopping(&self) -> bool {
        self.stopping.load(Ordering::Acquire)
    }

    pub(super) fn stop(
        &self,
        scheduler: &crate::Scheduler,
        current: Option<&Arc<WorkerControl>>,
    ) {
        self.stopping.store(true, Ordering::Release);
        scheduler.wake_waiters();

        let controls = self
            .controls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();

        let current = current.and_then(|current| {
            controls
                .iter()
                .position(|control| Arc::ptr_eq(current, control))
        });

        let threads = std::mem::take(
            &mut *self
                .threads
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );

        for (index, thread) in threads.into_iter().enumerate() {
            if current == Some(index) {
                continue;
            }

            let _ = thread.join();
        }
    }

    pub(super) fn detach_product(
        &self,
        product: usize,
        current: Option<&Arc<WorkerControl>>,
        scheduler: &crate::Scheduler,
    ) {
        let controls = self
            .controls
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();

        for control in &controls {
            control.request(product);

            if current.is_some_and(|current| Arc::ptr_eq(current, control)) {
                control.drain();
            }
        }

        scheduler.wake_waiters();

        for control in controls {
            control.wait();
        }
    }
}

impl WorkerControl {
    fn request(&self, product: usize) {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .requests
            .push_back(product);
    }

    pub(super) fn drain(&self) {
        let requests = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            std::mem::take(&mut state.requests)
        };

        for product in requests {
            let _ = crate::product::drain_product_thread_statics(product);

            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            state.completed = state.completed.saturating_add(1);

            self.completed.notify_all();
        }
    }

    fn wait(&self) {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let target = state.completed.saturating_add(state.requests.len());

        drop(
            self.completed
                .wait_while(state, |state| state.completed < target)
                .unwrap_or_else(std::sync::PoisonError::into_inner),
        );
    }
}
