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
}

#[derive(Default)]
struct WorkerControlState {
    requests: VecDeque<Arc<WorkerRequest>>,
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

        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .requests
            .push_back(Arc::clone(&request));

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
    use std::sync::mpsc;
    use std::time::Duration;

    use super::WorkerControl;

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

        waiter
            .join()
            .unwrap_or_else(|_| panic!("waiter must join"));
    }
}
