use std::sync::{Condvar, Mutex};

use bray_runtime_abi::NativeRuntimeStatus;

#[derive(Default)]
pub(in crate::native) struct WorkerControl {
    state: Mutex<WorkerControlState>,
    completed: Condvar,
}

#[derive(Default)]
struct WorkerControlState {
    requests: Vec<WorkerRequest>,
    retired: bool,
}

struct WorkerRequest {
    product: usize,
    status: RequestStatus,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum RequestStatus {
    Admitted,
    Requested,
    Draining,
    Complete,
}

impl WorkerControl {
    pub(in crate::native) fn admit(&self, product: usize) -> Result<(), NativeRuntimeStatus> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if state.retired {
            return Err(NativeRuntimeStatus::NOT_INITIALIZED);
        }

        if state
            .requests
            .iter()
            .any(|request| request.product == product)
        {
            return Ok(());
        }

        crate::allocation::reserve_vec_entries(&mut state.requests, 1)
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        state.requests.push(WorkerRequest {
            product,
            status: RequestStatus::Admitted,
        });

        Ok(())
    }

    pub(super) fn request(&self, product: usize) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some(request) = state
            .requests
            .iter_mut()
            .find(|request| request.product == product)
            && request.status == RequestStatus::Admitted
        {
            request.status = RequestStatus::Requested;
        }
    }

    fn take_requested(&self) -> Option<usize> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let request = state
            .requests
            .iter_mut()
            .find(|request| request.status == RequestStatus::Requested)?;

        request.status = RequestStatus::Draining;

        Some(request.product)
    }

    pub(in crate::native) fn drain(&self) {
        while let Some(product) = self.take_requested() {
            let _ = crate::product::drain_product_thread_statics(product);
            self.complete(product);
        }
    }

    pub(super) fn retire(&self) {
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            state.retired = true;

            // Unrequested static cleanup remains owned by exact-thread exit and its host counters.
            // A request already dequeued must still wait for its callback to finish.
            for request in &mut state.requests {
                if request.status == RequestStatus::Admitted {
                    request.status = RequestStatus::Complete;
                }
            }
        }

        self.completed.notify_all();
        self.drain();
    }

    fn complete(&self, product: usize) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some(request) = state
            .requests
            .iter_mut()
            .find(|request| request.product == product)
        {
            request.status = RequestStatus::Complete;
        }

        drop(state);
        self.completed.notify_all();
    }

    pub(super) fn wait(&self, product: usize) {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let mut state = self
            .completed
            .wait_while(state, |state| {
                state.requests.iter().any(|request| {
                    request.product == product && request.status != RequestStatus::Complete
                })
            })
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some(index) = state
            .requests
            .iter()
            .position(|request| request.product == product)
        {
            state.requests.swap_remove(index);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WorkerControl;
    use crate::test_support::with_allocation_failure;
    use bray_runtime_abi::NativeRuntimeStatus;
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    #[test]
    fn worker_admission_preserves_existing_requests_on_failure() {
        let control = WorkerControl::default();

        assert_eq!(
            with_allocation_failure(|| control.admit(1)),
            Err(NativeRuntimeStatus::ALLOCATION_FAILURE)
        );

        control.admit(1).unwrap();
        let capacity = control.state.lock().unwrap().requests.capacity();

        with_allocation_failure(|| {
            for product in 1..=capacity {
                control.admit(product).unwrap();
            }

            assert_eq!(
                control.admit(capacity + 1),
                Err(NativeRuntimeStatus::ALLOCATION_FAILURE)
            );

            for product in 1..=capacity {
                control.request(product);
            }

            control.drain();

            for product in 1..=capacity {
                control.wait(product);
            }

            control.admit(capacity + 1).unwrap();
        });

        assert_eq!(control.state.lock().unwrap().requests.len(), 1);
        control.request(capacity + 1);
        control.drain();
        control.wait(capacity + 1);
    }

    #[test]
    fn dequeued_request_waits_for_cleanup_completion_even_after_retirement() {
        let control = Arc::new(WorkerControl::default());
        control.admit(1).unwrap();
        control.request(1);
        assert_eq!(control.take_requested(), Some(1));
        control.retire();

        let (started_sender, started_receiver) = mpsc::channel();

        let (done_sender, done_receiver) = mpsc::channel();

        let waiter_control = Arc::clone(&control);

        let waiter = std::thread::spawn(move || {
            started_sender.send(()).unwrap();
            waiter_control.wait(1);
            done_sender.send(()).unwrap();
        });

        started_receiver
            .recv_timeout(Duration::from_secs(1))
            .unwrap();

        assert!(
            done_receiver
                .recv_timeout(Duration::from_millis(20))
                .is_err()
        );

        with_allocation_failure(|| control.complete(1));
        done_receiver.recv_timeout(Duration::from_secs(1)).unwrap();
        waiter.join().unwrap();
    }

    #[test]
    fn retired_controls_leave_unrequested_cleanup_to_thread_exit() {
        let control = WorkerControl::default();
        control.admit(1).unwrap();
        control.retire();

        with_allocation_failure(|| {
            control.request(1);
            control.wait(1);
            control.request(2);
            control.wait(2);
        });

        assert_eq!(control.admit(2), Err(NativeRuntimeStatus::NOT_INITIALIZED));
    }
}
