use std::collections::BTreeMap;
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::thread::JoinHandle;
use std::time::Duration;

use bray_platform::{MonotonicClock, MonotonicDeadline};

use crate::RootCancellationHandle;

/// Failure to create a host timeout service or register one invocation deadline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunTimeoutError {
    /// The host could not start its shared timeout worker.
    WorkerUnavailable,
    /// The requested relative deadline cannot be represented.
    DeadlineOverflow,
    /// Registration identities cannot be represented.
    IdentityExhausted,
    /// The timeout service is shutting down.
    ShuttingDown,
}

/// Host-owned service enforcing every active invocation deadline with one worker.
pub struct RunTimeoutScheduler {
    shared: Arc<RunTimeoutShared>,
    worker: Option<JoinHandle<()>>,
}

impl RunTimeoutScheduler {
    /// Starts an empty timeout service for one native test host.
    pub fn start() -> Result<Self, RunTimeoutError> {
        let shared = Arc::new(RunTimeoutShared {
            state: Mutex::new(RunTimeoutState::default()),
            wake: Condvar::new(),
        });

        let worker_shared = Arc::clone(&shared);

        let worker = std::thread::Builder::new()
            .name(String::from("bray-run-timeouts"))
            .spawn(move || run_timeout_worker(&worker_shared))
            .map_err(|_| RunTimeoutError::WorkerUnavailable)?;

        Ok(Self {
            shared,
            worker: Some(worker),
        })
    }

    /// Registers one invocation for cooperative cancellation at a relative deadline.
    pub fn register(
        &self,
        limit: Duration,
        cancellation: RootCancellationHandle,
    ) -> Result<RunCancellationTimer, RunTimeoutError> {
        let deadline = MonotonicClock
            .deadline_after(limit)
            .ok_or(RunTimeoutError::DeadlineOverflow)?;

        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if state.stopping {
            return Err(RunTimeoutError::ShuttingDown);
        }

        let identity = state.next_identity;

        state.next_identity = identity
            .checked_add(1)
            .ok_or(RunTimeoutError::IdentityExhausted)?;

        state
            .deadlines
            .entry(deadline)
            .or_default()
            .insert(identity, cancellation);

        drop(state);

        self.shared.wake.notify_all();

        Ok(RunCancellationTimer {
            deadline,
            identity,
            shared: Arc::downgrade(&self.shared),
        })
    }
}

impl Drop for RunTimeoutScheduler {
    fn drop(&mut self) {
        {
            let mut state = self
                .shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            state.stopping = true;
            state.deadlines.clear();
        }

        self.shared.wake.notify_all();

        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

/// Cancellation-safe ownership of one pending invocation timeout.
pub struct RunCancellationTimer {
    deadline: MonotonicDeadline,
    identity: u64,
    shared: Weak<RunTimeoutShared>,
}

impl Drop for RunCancellationTimer {
    fn drop(&mut self) {
        let Some(shared) = self.shared.upgrade() else {
            return;
        };

        let mut state = shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(registrations) = state.deadlines.get_mut(&self.deadline) else {
            return;
        };

        registrations.remove(&self.identity);

        if registrations.is_empty() {
            state.deadlines.remove(&self.deadline);
        }

        drop(state);

        shared.wake.notify_all();
    }
}

struct RunTimeoutShared {
    state: Mutex<RunTimeoutState>,
    wake: Condvar,
}

#[derive(Default)]
struct RunTimeoutState {
    stopping: bool,
    next_identity: u64,
    deadlines: BTreeMap<MonotonicDeadline, BTreeMap<u64, RootCancellationHandle>>,
}

fn run_timeout_worker(shared: &RunTimeoutShared) {
    let mut state = shared
        .state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    loop {
        if state.stopping {
            return;
        }

        let Some(deadline) = state.deadlines.keys().next().copied() else {
            state = shared
                .wake
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            continue;
        };

        if !deadline.has_elapsed() {
            let (next, _) = shared
                .wake
                .wait_timeout(state, deadline.remaining())
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            state = next;

            continue;
        }

        let expired = state.deadlines.remove(&deadline).unwrap_or_default();

        drop(state);

        for cancellation in expired.into_values() {
            cancellation.request_timeout();
        }

        state = shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::RunTimeoutScheduler;
    use crate::{
        RootCancellationSource, RunOutcome, current_run_cancellation_requested,
        execute_synchronous_root,
    };

    #[test]
    fn elapsed_deadlines_request_timeout_cancellation() {
        let timeouts = RunTimeoutScheduler::start()
            .unwrap_or_else(|error| panic!("test timeout service must start: {error:?}"));

        let mut timer = None;
        let mut cancellation = None;
        let mut admitted = crate::outgoing::OutgoingRecords::admit(1).unwrap();

        let outcome = execute_synchronous_root(
            || {
                while !current_run_cancellation_requested() {
                    std::thread::yield_now();
                }
            },
            |root| {
                cancellation = Some(root.clone());

                timer = Some(
                    timeouts
                        .register(Duration::ZERO, root)
                        .unwrap_or_else(|error| panic!("test timeout must register: {error:?}")),
                );
            },
            &mut admitted,
        );

        drop(timer);
        drop(timeouts);

        assert!(matches!(outcome, RunOutcome::Completed(())));

        assert_eq!(
            cancellation.and_then(|cancellation| cancellation.source()),
            Some(RootCancellationSource::Timeout)
        );
    }

    #[test]
    fn dropping_a_registration_before_its_deadline_prevents_cancellation() {
        let timeouts = RunTimeoutScheduler::start()
            .unwrap_or_else(|error| panic!("test timeout service must start: {error:?}"));

        let mut timer = None;
        let mut cancellation = None;
        let mut admitted = crate::outgoing::OutgoingRecords::admit(1).unwrap();

        let outcome = execute_synchronous_root(
            || (),
            |root| {
                cancellation = Some(root.clone());

                timer = Some(
                    timeouts
                        .register(Duration::from_secs(60), root)
                        .unwrap_or_else(|error| panic!("test timeout must register: {error:?}")),
                );
            },
            &mut admitted,
        );

        drop(timer);
        drop(timeouts);

        assert!(matches!(outcome, RunOutcome::Completed(())));

        assert_eq!(
            cancellation.and_then(|cancellation| cancellation.source()),
            None
        );
    }

    #[test]
    fn one_worker_enforces_multiple_independent_deadlines() {
        let timeouts = RunTimeoutScheduler::start()
            .unwrap_or_else(|error| panic!("test timeout service must start: {error:?}"));

        let mut first = None;
        let mut second = None;
        let mut registrations = Vec::new();
        let mut first_admitted = crate::outgoing::OutgoingRecords::admit(1).unwrap();

        let _ = execute_synchronous_root(
            || (),
            |root| {
                first = Some(root.clone());

                registrations.push(
                    timeouts
                        .register(Duration::ZERO, root)
                        .unwrap_or_else(|error| panic!("first timeout must register: {error:?}")),
                );
            },
            &mut first_admitted,
        );

        let mut second_admitted = crate::outgoing::OutgoingRecords::admit(1).unwrap();

        let _ = execute_synchronous_root(
            || (),
            |root| {
                second = Some(root.clone());

                registrations.push(
                    timeouts
                        .register(Duration::ZERO, root)
                        .unwrap_or_else(|error| panic!("second timeout must register: {error:?}")),
                );
            },
            &mut second_admitted,
        );

        wait_for_timeout(
            first
                .as_ref()
                .unwrap_or_else(|| panic!("first cancellation handle must exist")),
        );

        wait_for_timeout(
            second
                .as_ref()
                .unwrap_or_else(|| panic!("second cancellation handle must exist")),
        );

        assert_eq!(registrations.len(), 2);
    }

    fn wait_for_timeout(cancellation: &crate::RootCancellationHandle) {
        let deadline = Instant::now() + Duration::from_secs(1);

        while cancellation.source().is_none() {
            assert!(
                Instant::now() < deadline,
                "timeout worker must make progress"
            );

            std::thread::yield_now();
        }
    }
}
