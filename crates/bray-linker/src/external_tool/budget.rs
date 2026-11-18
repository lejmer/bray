use std::num::NonZeroUsize;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use bray_base::Cancellation;

use super::ExternalToolFailure;

const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Shared compiler-host limit for concurrently running external tools.
#[derive(Clone, Debug)]
pub struct ExternalToolProcessBudget {
    state: Arc<ProcessBudgetState>,
}

#[derive(Debug)]
struct ProcessBudgetState {
    limit: NonZeroUsize,
    active: Mutex<usize>,
    available: Condvar,
}

impl ExternalToolProcessBudget {
    /// Creates a process budget with an explicit nonzero concurrency limit.
    pub fn new(limit: NonZeroUsize) -> Self {
        Self {
            state: Arc::new(ProcessBudgetState {
                limit,
                active: Mutex::new(0),
                available: Condvar::new(),
            }),
        }
    }

    /// Returns the maximum number of simultaneous external tools.
    pub fn limit(&self) -> NonZeroUsize {
        self.state.limit
    }

    pub(super) fn acquire(
        &self,
        cancellation: &dyn Cancellation,
    ) -> Result<ProcessPermit<'_>, ExternalToolFailure> {
        let mut active = self.active();

        loop {
            if cancellation.is_cancelled() {
                return Err(ExternalToolFailure::Cancelled);
            }

            if *active < self.state.limit.get() {
                *active += 1;

                return Ok(ProcessPermit { budget: self });
            }

            let waited = self
                .state
                .available
                .wait_timeout(active, CANCELLATION_POLL_INTERVAL)
                .expect("native process budget wait state must remain unpoisoned");

            active = waited.0;
        }
    }

    fn active(&self) -> MutexGuard<'_, usize> {
        self.state
            .active
            .lock()
            .expect("native process budget state must remain unpoisoned")
    }
}

pub(super) struct ProcessPermit<'a> {
    budget: &'a ExternalToolProcessBudget,
}

impl Drop for ProcessPermit<'_> {
    fn drop(&mut self) {
        let mut active = self.budget.active();

        *active = active
            .checked_sub(1)
            .expect("a native process permit must own an active slot");

        self.budget.state.available.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::mpsc;
    use std::time::Duration;

    use super::ExternalToolProcessBudget;
    use crate::ExternalToolFailure;

    #[test]
    #[should_panic(expected = "native process budget state must remain unpoisoned")]
    fn poisoned_budget_fails_at_acquisition() {
        let budget = ExternalToolProcessBudget::new(NonZeroUsize::MIN);

        let _ = std::panic::catch_unwind(|| {
            let _active = budget
                .state
                .active
                .lock()
                .expect("fixture mutex must not be poisoned");

            panic!("poison the test budget");
        });

        let _permit = budget.acquire(&|| false);
    }

    #[test]
    #[should_panic(expected = "a native process permit must own an active slot")]
    fn permit_retirement_requires_its_owned_slot() {
        let budget = ExternalToolProcessBudget::new(NonZeroUsize::MIN);

        let permit = budget
            .acquire(&|| false)
            .expect("fixture `acquire` call must succeed");

        *budget.active() = 0;
        drop(permit);
    }

    #[test]
    fn a_waiting_process_observes_permit_release_or_cancellation() {
        for cancel_waiter in [false, true] {
            let budget = ExternalToolProcessBudget::new(NonZeroUsize::MIN);
            let cancelled = AtomicBool::new(false);
            let polls = AtomicUsize::new(0);

            let (waiting, wait_started) = mpsc::channel();

            let (finished, wait_finished) = mpsc::channel();

            std::thread::scope(|scope| {
                let first = budget
                    .acquire(&|| false)
                    .expect("the first process must acquire the only permit");

                scope.spawn(|| {
                    let acquired = budget.acquire(&|| {
                        if polls.fetch_add(1, Ordering::Relaxed) == 1 {
                            waiting.send(()).expect("the test must observe the wait");
                        }

                        cancelled.load(Ordering::Acquire)
                    });

                    finished.send(acquired.map(drop)).ok();
                });

                let observed_wait = wait_started.recv_timeout(Duration::from_secs(5));

                if cancel_waiter && observed_wait.is_ok() {
                    cancelled.store(true, Ordering::Release);

                    let outcome = wait_finished.recv_timeout(Duration::from_secs(5));

                    drop(first);
                    assert_eq!(outcome, Ok(Err(ExternalToolFailure::Cancelled)));
                } else {
                    drop(first);
                    observed_wait.expect("the second process must wait for the held permit");
                    assert_eq!(wait_finished.recv_timeout(Duration::from_secs(5)), Ok(Ok(())));
                }
            });
        }
    }
}
