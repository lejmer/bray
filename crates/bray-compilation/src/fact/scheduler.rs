use std::cell::RefCell;
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock};

use rayon::{ThreadPool, ThreadPoolBuilder};

use super::{FactQueryError, QueryPriority};
use crate::WorkerBudget;

const MAX_INTERACTIVE_STREAK: usize = 8;

thread_local! {
    static ACTIVE_SCHEDULERS: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug)]
pub(crate) struct FactScheduler {
    worker_count: usize,
    ordinary_pool: OnceLock<Result<ThreadPool, ()>>,
    interactive_pool: OnceLock<Result<ThreadPool, ()>>,
    slots: ExecutionSlots,
}

impl FactScheduler {
    pub(crate) fn new(worker_budget: WorkerBudget) -> Self {
        let worker_count = worker_budget.get();

        Self {
            worker_count,
            ordinary_pool: OnceLock::new(),
            interactive_pool: OnceLock::new(),
            slots: ExecutionSlots::new(worker_count),
        }
    }

    pub(crate) fn run<T>(
        &self,
        priority: QueryPriority,
        operation: impl FnOnce() -> T + Send,
    ) -> Result<T, FactQueryError>
    where
        T: Send,
    {
        if self.is_active()? {
            return Ok(operation());
        }

        let pool = self.pool(priority)?;

        Ok(pool.install(|| self.execute(priority, operation)))
    }

    pub(crate) fn map_indexed<T>(
        &self,
        len: usize,
        operation: impl Fn(usize) -> T + Send + Sync,
    ) -> Result<Vec<T>, FactQueryError>
    where
        T: Send,
    {
        let pool = self.pool(QueryPriority::Normal)?;

        let slots = (0..len)
            .map(|_| Mutex::new(None))
            .collect::<Vec<Mutex<Option<T>>>>();

        pool.scope(|scope| {
            for (index, slot) in slots.iter().enumerate() {
                let operation = &operation;

                scope.spawn(move |_| {
                    let value = self.execute(QueryPriority::Normal, || operation(index));

                    let mut slot = slot
                        .lock()
                        .unwrap_or_else(|_| panic!("scheduled result slot must remain available"));

                    *slot = Some(value);
                });
            }
        });

        let results = slots
            .into_iter()
            .map(|slot| {
                slot.into_inner()
                    .unwrap_or_else(|_| panic!("scheduled result slot must remain available"))
                    .unwrap_or_else(|| panic!("scheduled work must publish one result"))
            })
            .collect();

        Ok(results)
    }

    #[cfg(test)]
    pub(crate) fn worker_count(&self) -> usize {
        self.worker_count
    }

    fn execute<T>(&self, priority: QueryPriority, operation: impl FnOnce() -> T) -> T {
        if self
            .is_active()
            .unwrap_or_else(|_| panic!("scheduler-local state must remain available"))
        {
            return operation();
        }

        let _slot = self
            .slots
            .acquire(priority)
            .unwrap_or_else(|_| panic!("scheduler slots must remain available"));

        let _active = ActiveSchedulerGuard::enter(self.identity())
            .unwrap_or_else(|_| panic!("scheduler-local state must remain available"));

        operation()
    }

    fn pool(&self, priority: QueryPriority) -> Result<&ThreadPool, FactQueryError> {
        if priority == QueryPriority::Interactive && self.worker_count > 1 {
            return build_pool(&self.interactive_pool, 1, "bray-query-interactive");
        }

        build_pool(&self.ordinary_pool, self.worker_count, "bray-query")
    }

    fn is_active(&self) -> Result<bool, FactQueryError> {
        ACTIVE_SCHEDULERS
            .try_with(|active| {
                let active = active
                    .try_borrow()
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                Ok(active.contains(&self.identity()))
            })
            .map_err(|_| FactQueryError::InfrastructureFailure)?
    }

    fn identity(&self) -> usize {
        std::ptr::from_ref(self).addr()
    }
}

fn build_pool<'a>(
    storage: &'a OnceLock<Result<ThreadPool, ()>>,
    worker_count: usize,
    thread_name: &'static str,
) -> Result<&'a ThreadPool, FactQueryError> {
    storage
        .get_or_init(|| {
            ThreadPoolBuilder::new()
                .num_threads(worker_count)
                .thread_name(move |index| format!("{thread_name}-{index}"))
                .build()
                .map_err(|_| ())
        })
        .as_ref()
        .map_err(|_| FactQueryError::InfrastructureFailure)
}

#[derive(Debug)]
struct ExecutionSlots {
    limit: usize,
    state: Mutex<SlotState>,
    available: Condvar,
}

#[derive(Debug, Default)]
struct SlotState {
    active: usize,
    interactive_waiters: usize,
    ordinary_waiters: usize,
    interactive_streak: usize,
}

impl ExecutionSlots {
    fn new(limit: usize) -> Self {
        Self {
            limit,
            state: Mutex::new(SlotState::default()),
            available: Condvar::new(),
        }
    }

    fn acquire(&self, priority: QueryPriority) -> Result<ExecutionSlot<'_>, FactQueryError> {
        let interactive = priority == QueryPriority::Interactive;
        let mut state = self.state()?;

        if interactive {
            state.interactive_waiters += 1;
        } else {
            state.ordinary_waiters += 1;
        }

        while !self.can_acquire(&state, interactive) {
            state = self
                .available
                .wait(state)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;
        }

        if interactive {
            state.interactive_waiters -= 1;
            state.interactive_streak += 1;
        } else {
            state.ordinary_waiters -= 1;
            state.interactive_streak = 0;
        }

        state.active += 1;

        Ok(ExecutionSlot { slots: self })
    }

    fn can_acquire(&self, state: &SlotState, interactive: bool) -> bool {
        if state.active >= self.limit {
            return false;
        }

        if interactive {
            return state.interactive_streak < MAX_INTERACTIVE_STREAK
                || state.ordinary_waiters == 0;
        }

        state.interactive_waiters == 0 || state.interactive_streak >= MAX_INTERACTIVE_STREAK
    }

    fn state(&self) -> Result<MutexGuard<'_, SlotState>, FactQueryError> {
        self.state
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)
    }
}

struct ExecutionSlot<'a> {
    slots: &'a ExecutionSlots,
}

impl Drop for ExecutionSlot<'_> {
    fn drop(&mut self) {
        let Ok(mut state) = self.slots.state.lock() else {
            return;
        };

        state.active = state.active.saturating_sub(1);
        self.slots.available.notify_all();
    }
}

struct ActiveSchedulerGuard {
    identity: usize,
}

impl ActiveSchedulerGuard {
    fn enter(identity: usize) -> Result<Self, FactQueryError> {
        ACTIVE_SCHEDULERS
            .try_with(|active| {
                let mut active = active
                    .try_borrow_mut()
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                active.push(identity);

                Ok(())
            })
            .map_err(|_| FactQueryError::InfrastructureFailure)??;

        Ok(Self { identity })
    }
}

impl Drop for ActiveSchedulerGuard {
    fn drop(&mut self) {
        let _ = ACTIVE_SCHEDULERS.try_with(|active| {
            let Ok(mut active) = active.try_borrow_mut() else {
                return;
            };

            if active.last().copied() == Some(self.identity) {
                active.pop();
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    use super::FactScheduler;
    use crate::{QueryPriority, WorkerBudget};

    #[test]
    fn indexed_work_is_bounded_and_retains_input_order() {
        let scheduler = FactScheduler::new(worker_budget(2));
        let active = AtomicUsize::new(0);
        let maximum = AtomicUsize::new(0);

        let results = scheduler
            .map_indexed(8, |index| {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;

                maximum.fetch_max(current, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));
                active.fetch_sub(1, Ordering::SeqCst);

                index
            })
            .unwrap_or_else(|error| panic!("scheduled work must complete: {error:?}"));

        assert_eq!(results, (0..8).collect::<Vec<_>>());
        assert_eq!(scheduler.worker_count(), 2);
        assert_eq!(maximum.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn interactive_work_precedes_queued_background_work() {
        let scheduler = Arc::new(FactScheduler::new(worker_budget(2)));
        let (started_sender, started_receiver) = mpsc::channel();
        let (first_release_sender, first_release_receiver) = mpsc::channel();
        let (second_release_sender, second_release_receiver) = mpsc::channel();
        let (order_sender, order_receiver) = mpsc::channel();

        std::thread::scope(|scope| {
            let first_scheduler = Arc::clone(&scheduler);
            let first_started = started_sender.clone();

            scope.spawn(move || {
                first_scheduler
                    .run(QueryPriority::Background, move || {
                        first_started
                            .send(())
                            .unwrap_or_else(|_| panic!("test must observe occupied work"));

                        first_release_receiver
                            .recv()
                            .unwrap_or_else(|_| panic!("test must release occupied work"));
                    })
                    .unwrap_or_else(|error| panic!("background work must run: {error:?}"));
            });

            let second_scheduler = Arc::clone(&scheduler);

            scope.spawn(move || {
                second_scheduler
                    .run(QueryPriority::Background, move || {
                        started_sender
                            .send(())
                            .unwrap_or_else(|_| panic!("test must observe occupied work"));

                        second_release_receiver
                            .recv()
                            .unwrap_or_else(|_| panic!("test must release occupied work"));
                    })
                    .unwrap_or_else(|error| panic!("background work must run: {error:?}"));
            });

            for _ in 0..2 {
                started_receiver
                    .recv()
                    .unwrap_or_else(|_| panic!("test must occupy every execution slot"));
            }

            let background_scheduler = Arc::clone(&scheduler);
            let background_sender = order_sender.clone();

            scope.spawn(move || {
                background_scheduler
                    .run(QueryPriority::Background, || {
                        background_sender
                            .send(QueryPriority::Background)
                            .unwrap_or_else(|_| panic!("test must observe background work"));
                    })
                    .unwrap_or_else(|error| panic!("queued background work must run: {error:?}"));
            });

            let interactive_scheduler = Arc::clone(&scheduler);
            let interactive_sender = order_sender.clone();

            scope.spawn(move || {
                interactive_scheduler
                    .run(QueryPriority::Interactive, || {
                        interactive_sender
                            .send(QueryPriority::Interactive)
                            .unwrap_or_else(|_| panic!("test must observe interactive work"));
                    })
                    .unwrap_or_else(|error| panic!("interactive work must run: {error:?}"));
            });

            std::thread::sleep(Duration::from_millis(10));
            first_release_sender
                .send(())
                .unwrap_or_else(|_| panic!("test must release one execution slot"));

            assert_eq!(
                order_receiver
                    .recv_timeout(Duration::from_secs(1))
                    .unwrap_or_else(|_| panic!("scheduled work must complete")),
                QueryPriority::Interactive
            );

            second_release_sender
                .send(())
                .unwrap_or_else(|_| panic!("test must release the other execution slot"));
        });
    }

    #[test]
    fn serial_nested_work_reuses_the_current_worker() {
        let scheduler = FactScheduler::new(WorkerBudget::serial());

        let (outer, inner) = scheduler
            .run(QueryPriority::Normal, || {
                let outer = std::thread::current().id();

                let inner = scheduler
                    .run(QueryPriority::Interactive, || std::thread::current().id())
                    .unwrap_or_else(|error| panic!("nested work must complete: {error:?}"));

                (outer, inner)
            })
            .unwrap_or_else(|error| panic!("outer work must complete: {error:?}"));

        assert_eq!(outer, inner);
    }

    fn worker_budget(workers: usize) -> WorkerBudget {
        WorkerBudget::new(workers)
            .unwrap_or_else(|error| panic!("test worker budget must be valid: {error:?}"))
    }
}
