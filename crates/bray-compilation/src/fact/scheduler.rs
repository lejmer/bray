use std::cell::RefCell;
use std::sync::{Condvar, Mutex, MutexGuard, OnceLock};

use rayon::{ThreadPool, ThreadPoolBuilder};

use super::{FactQueryError, QueryPriority, QueryPriorityDemand};
use crate::WorkerBudget;

const MAX_INTERACTIVE_STREAK: usize = 8;

thread_local! {
    static ACTIVE_SCHEDULERS: RefCell<Vec<ActiveScheduler>> = const { RefCell::new(Vec::new()) };
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

    #[cfg(test)]
    pub(crate) fn run<T>(
        &self,
        priority: QueryPriority,
        operation: impl FnOnce() -> T + Send,
    ) -> Result<T, FactQueryError>
    where
        T: Send,
    {
        self.run_demand(&QueryPriorityDemand::new(priority), operation)
    }

    pub(crate) fn run_demand<T>(
        &self,
        priority: &QueryPriorityDemand,
        operation: impl FnOnce() -> T + Send,
    ) -> Result<T, FactQueryError>
    where
        T: Send,
    {
        if self.is_active()? {
            return Ok(operation());
        }

        let pool = self.pool(priority.current())?;

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
                    let priority = QueryPriorityDemand::new(QueryPriority::Normal);
                    let value = self.execute(&priority, || operation(index));

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

    pub(crate) fn current_priority(&self) -> Result<Option<QueryPriority>, FactQueryError> {
        ACTIVE_SCHEDULERS
            .try_with(|active| {
                let active = active
                    .try_borrow()
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                Ok(active
                    .iter()
                    .rev()
                    .find(|active| active.identity == self.identity())
                    .map(|active| active.priority))
            })
            .map_err(|_| FactQueryError::InfrastructureFailure)?
    }

    pub(crate) fn promote(&self, priority: &QueryPriorityDemand, requested: QueryPriority) {
        priority.promote(requested);
        self.slots.priority_changed();
    }

    fn execute<T>(&self, priority: &QueryPriorityDemand, operation: impl FnOnce() -> T) -> T {
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

        let _active = ActiveSchedulerGuard::enter(self.identity(), priority.current())
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

                Ok(active
                    .iter()
                    .any(|active| active.identity == self.identity()))
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

    fn acquire(&self, priority: &QueryPriorityDemand) -> Result<ExecutionSlot<'_>, FactQueryError> {
        let mut interactive = priority.current() == QueryPriority::Interactive;
        let mut state = self.state()?;

        register_waiter(&mut state, interactive);
        self.available.notify_all();

        loop {
            let promoted = priority.current() == QueryPriority::Interactive;

            if promoted != interactive {
                unregister_waiter(&mut state, interactive);
                interactive = promoted;
                register_waiter(&mut state, interactive);
                self.available.notify_all();
            }

            if self.can_acquire(&state, interactive) {
                break;
            }

            state = self
                .available
                .wait(state)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;
        }

        unregister_waiter(&mut state, interactive);

        if interactive {
            state.interactive_streak += 1;
        } else {
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

    fn priority_changed(&self) {
        self.available.notify_all();
    }

    #[cfg(test)]
    fn wait_until_queued(&self, interactive: usize, ordinary: usize) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        let mut state = self
            .state()
            .unwrap_or_else(|_| panic!("scheduler slots must remain available"));

        while state.interactive_waiters < interactive || state.ordinary_waiters < ordinary {
            let now = std::time::Instant::now();

            assert!(
                now < deadline,
                "scheduled requests did not reach the slot queue"
            );

            let waited = self
                .available
                .wait_timeout(state, deadline.saturating_duration_since(now))
                .unwrap_or_else(|_| panic!("scheduler slots must remain available"));

            state = waited.0;
        }
    }
}

fn register_waiter(state: &mut SlotState, interactive: bool) {
    if interactive {
        state.interactive_waiters += 1;
    } else {
        state.ordinary_waiters += 1;
    }
}

fn unregister_waiter(state: &mut SlotState, interactive: bool) {
    if interactive {
        assert!(
            state.interactive_waiters > 0,
            "interactive waiter registration must remain balanced"
        );

        state.interactive_waiters -= 1;
    } else {
        assert!(
            state.ordinary_waiters > 0,
            "ordinary waiter registration must remain balanced"
        );

        state.ordinary_waiters -= 1;
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
    fn enter(identity: usize, priority: QueryPriority) -> Result<Self, FactQueryError> {
        ACTIVE_SCHEDULERS
            .try_with(|active| {
                let mut active = active
                    .try_borrow_mut()
                    .map_err(|_| FactQueryError::InfrastructureFailure)?;

                active.push(ActiveScheduler { identity, priority });

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

            if active
                .last()
                .is_some_and(|active| active.identity == self.identity)
            {
                active.pop();
            }
        });
    }
}

#[derive(Clone, Copy)]
struct ActiveScheduler {
    identity: usize,
    priority: QueryPriority,
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, mpsc};
    use std::time::Duration;

    use super::{ExecutionSlots, FactScheduler};
    use crate::fact::QueryPriorityDemand;
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
        let slots = Arc::new(ExecutionSlots::new(2));
        let occupied = QueryPriorityDemand::new(QueryPriority::Normal);
        let first = slots
            .acquire(&occupied)
            .unwrap_or_else(|error| panic!("test must occupy one slot: {error:?}"));

        let second = slots
            .acquire(&occupied)
            .unwrap_or_else(|error| panic!("test must occupy another slot: {error:?}"));

        let (order_sender, order_receiver) = mpsc::channel();

        std::thread::scope(|scope| {
            let background_slots = Arc::clone(&slots);
            let background_sender = order_sender.clone();

            scope.spawn(move || {
                let priority = QueryPriorityDemand::new(QueryPriority::Background);
                let _slot = background_slots
                    .acquire(&priority)
                    .unwrap_or_else(|error| panic!("background work must run: {error:?}"));

                background_sender
                    .send(QueryPriority::Background)
                    .unwrap_or_else(|_| panic!("test must observe background work"));
            });

            let interactive_slots = Arc::clone(&slots);
            let interactive_sender = order_sender.clone();

            scope.spawn(move || {
                let priority = QueryPriorityDemand::new(QueryPriority::Interactive);
                let _slot = interactive_slots
                    .acquire(&priority)
                    .unwrap_or_else(|error| panic!("interactive work must run: {error:?}"));

                interactive_sender
                    .send(QueryPriority::Interactive)
                    .unwrap_or_else(|_| panic!("test must observe interactive work"));
            });

            slots.wait_until_queued(1, 1);

            drop(first);

            assert_eq!(
                order_receiver
                    .recv_timeout(Duration::from_secs(1))
                    .unwrap_or_else(|_| panic!("scheduled work must complete")),
                QueryPriority::Interactive
            );

            drop(second);
        });
    }

    #[test]
    fn queued_shared_work_observes_later_priority_promotion() {
        let slots = Arc::new(ExecutionSlots::new(1));
        let occupied_priority = QueryPriorityDemand::new(QueryPriority::Normal);
        let occupied = slots
            .acquire(&occupied_priority)
            .unwrap_or_else(|error| panic!("test must occupy the execution slot: {error:?}"));

        let shared_priority = QueryPriorityDemand::new(QueryPriority::Background);
        let (order_sender, order_receiver) = mpsc::channel();

        std::thread::scope(|scope| {
            let shared_slots = Arc::clone(&slots);
            let shared_demand = shared_priority.clone();
            let shared_sender = order_sender.clone();

            scope.spawn(move || {
                let _slot = shared_slots
                    .acquire(&shared_demand)
                    .unwrap_or_else(|error| panic!("shared work must run: {error:?}"));

                shared_sender
                    .send(shared_demand.current())
                    .unwrap_or_else(|_| panic!("test must observe shared work"));
            });

            let background_slots = Arc::clone(&slots);
            let background_sender = order_sender.clone();

            scope.spawn(move || {
                let priority = QueryPriorityDemand::new(QueryPriority::Background);
                let _slot = background_slots
                    .acquire(&priority)
                    .unwrap_or_else(|error| panic!("background work must run: {error:?}"));

                background_sender
                    .send(QueryPriority::Background)
                    .unwrap_or_else(|_| panic!("test must observe background work"));
            });

            slots.wait_until_queued(0, 2);
            shared_priority.promote(QueryPriority::Interactive);
            slots.priority_changed();
            slots.wait_until_queued(1, 1);

            drop(occupied);

            assert_eq!(
                order_receiver
                    .recv_timeout(Duration::from_secs(1))
                    .unwrap_or_else(|_| panic!("promoted work must complete")),
                QueryPriority::Interactive
            );
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
