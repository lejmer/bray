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
        let mut active = self.active()?;

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
                .map_err(|_| ExternalToolFailure::ProcessBudgetUnavailable)?;

            active = waited.0;
        }
    }

    fn active(&self) -> Result<MutexGuard<'_, usize>, ExternalToolFailure> {
        self.state
            .active
            .lock()
            .map_err(|_| ExternalToolFailure::ProcessBudgetUnavailable)
    }
}

pub(super) struct ProcessPermit<'a> {
    budget: &'a ExternalToolProcessBudget,
}

impl Drop for ProcessPermit<'_> {
    fn drop(&mut self) {
        let Ok(mut active) = self.budget.state.active.lock() else {
            return;
        };

        *active = active.saturating_sub(1);
        self.budget.state.available.notify_one();
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier};

    use super::ExternalToolProcessBudget;
    use crate::ExternalToolFailure;

    #[test]
    fn process_budget_serializes_external_tool_ownership() {
        let budget = ExternalToolProcessBudget::new(NonZeroUsize::MIN);
        let rendezvous = Arc::new(Barrier::new(2));
        let first_has_permit = Arc::new(AtomicBool::new(false));
        let second_has_permit = Arc::new(AtomicBool::new(false));

        std::thread::scope(|scope| {
            let first_budget = budget.clone();
            let first_rendezvous = Arc::clone(&rendezvous);
            let first_acquired = Arc::clone(&first_has_permit);

            let first = scope.spawn(move || {
                let _permit = first_budget
                    .acquire(&|| false)
                    .unwrap_or_else(|error| panic!("first permit should be available: {error:?}"));

                first_acquired.store(true, Ordering::Release);
                first_rendezvous.wait();
                first_rendezvous.wait();
            });

            while !first_has_permit.load(Ordering::Acquire) {
                std::thread::yield_now();
            }

            let second_budget = budget.clone();
            let second_acquired = Arc::clone(&second_has_permit);

            let second = scope.spawn(move || {
                let _permit = second_budget
                    .acquire(&|| false)
                    .unwrap_or_else(|error| panic!("second permit should become available: {error:?}"));

                second_acquired.store(true, Ordering::Release);
            });

            rendezvous.wait();

            assert!(!second_has_permit.load(Ordering::Acquire));

            rendezvous.wait();

            first
                .join()
                .unwrap_or_else(|_| panic!("first budget worker should finish"));

            second
                .join()
                .unwrap_or_else(|_| panic!("second budget worker should finish"));
        });

        assert!(second_has_permit.load(Ordering::Acquire));
    }

    #[test]
    fn process_budget_waiting_observes_cancellation() {
        let budget = ExternalToolProcessBudget::new(NonZeroUsize::MIN);

        let _permit = budget
            .acquire(&|| false)
            .unwrap_or_else(|error| panic!("test permit should be available: {error:?}"));

        assert_eq!(
            budget.acquire(&|| true).err(),
            Some(ExternalToolFailure::Cancelled)
        );
    }
}
