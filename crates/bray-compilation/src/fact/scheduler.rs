use std::cell::RefCell;
use std::error::Error as _;
use std::io;
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock};

use rayon::{ThreadPool, ThreadPoolBuilder};

use super::{
    CapacityResource, FactQueryError, FactRuntimeError, FactRuntimeFailure, HostIoFailure,
    LocalStateFailure, QueryPriority, QueryPriorityDemand, SchedulerCounter,
    SchedulerLocalOperation, SynchronizationComponent, WorkerPoolKind,
};
use crate::WorkerBudget;
use crate::profile::{CompilationProfileOutcome, ProfileOperation, ProfileSession};

const MAX_INTERACTIVE_STREAK: usize = 8;

thread_local! {
    static ACTIVE_SCHEDULERS: RefCell<Vec<ActiveScheduler>> = const { RefCell::new(Vec::new()) };
}

#[derive(Debug)]
pub(crate) struct FactScheduler {
    worker_count: usize,
    ordinary_pool: OnceLock<Result<ThreadPool, FactRuntimeError>>,
    interactive_pool: OnceLock<Result<ThreadPool, FactRuntimeError>>,
    slots: ExecutionSlots,
    profile: Option<Arc<ProfileSession>>,
}

impl FactScheduler {
    #[cfg(test)]
    pub(crate) fn new(worker_budget: WorkerBudget) -> Self {
        Self::with_profile(worker_budget, None)
    }

    pub(crate) fn with_profile(
        worker_budget: WorkerBudget,
        profile: Option<Arc<ProfileSession>>,
    ) -> Self {
        let worker_count = worker_budget.get();

        Self {
            worker_count,
            ordinary_pool: OnceLock::new(),
            interactive_pool: OnceLock::new(),
            slots: ExecutionSlots::new(worker_count),
            profile,
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

        pool.install(|| self.execute(priority, operation))
    }

    pub(crate) fn map_indexed<T>(
        &self,
        priority: QueryPriority,
        len: usize,
        operation: impl Fn(usize) -> T + Send + Sync,
    ) -> Result<Vec<T>, FactQueryError>
    where
        T: Send,
    {
        let Some(profile) = self.profile.as_deref() else {
            return self.map_indexed_core(priority, len, operation);
        };

        let wave = profile.start_scheduling_wave(len);

        self.map_indexed_core(priority, len, |index| {
            let _worker = wave.start_ready_item();

            operation(index)
        })
    }

    fn map_indexed_core<T>(
        &self,
        priority: QueryPriority,
        len: usize,
        operation: impl Fn(usize) -> T + Send + Sync,
    ) -> Result<Vec<T>, FactQueryError>
    where
        T: Send,
    {
        let slots = (0..len)
            .map(|_| Mutex::new(None))
            .collect::<Vec<Mutex<Option<T>>>>();

        let evaluate = |index: usize| -> Result<(), FactQueryError> {
            let value = operation(index);

            publish_scheduled_result(&slots[index], index, value)
        };

        if self.is_active()? {
            self.map_indexed_nested(priority, len, &evaluate)?;
        } else {
            let pool = self.pool(priority)?;
            let scheduler_error = Mutex::new(None);

            pool.scope(|scope| {
                for index in 0..len {
                    let evaluate = &evaluate;
                    let scheduler_error = &scheduler_error;

                    scope.spawn(move |_| {
                        let priority = QueryPriorityDemand::new(priority);

                        if let Err(error) = self
                            .execute(&priority, || evaluate(index))
                            .and_then(std::convert::identity)
                        {
                            record_scheduler_error(scheduler_error, index, error);
                        }
                    });
                }
            });

            if let Some(error) = take_scheduler_error(&scheduler_error)? {
                return Err(error);
            }
        }

        collect_scheduled_results(slots)
    }

    fn map_indexed_nested(
        &self,
        priority: QueryPriority,
        len: usize,
        evaluate: &(impl Fn(usize) -> Result<(), FactQueryError> + Send + Sync),
    ) -> Result<(), FactQueryError> {
        let priority_demand = QueryPriorityDemand::new(priority);
        let mut reserved = Vec::new();

        while reserved.len() < len.saturating_sub(1) {
            let Some(slot) = self.slots.try_acquire(&priority_demand)? else {
                break;
            };

            reserved.push(slot);
        }

        let lane_count = reserved.len() + 1;

        if lane_count == 1 {
            for index in 0..len {
                evaluate(index)?;
            }

            return Ok(());
        }

        let pool = self.pool(priority)?;
        let scheduler_error = Mutex::new(None);

        pool.scope(|scope| {
            for (lane, slot) in reserved.into_iter().enumerate() {
                let scheduler_error = &scheduler_error;

                scope.spawn(move |_| {
                    let _slot = slot;

                    let _worker_activity = self
                        .profile
                        .as_deref()
                        .map(ProfileSession::start_worker_activity);

                    let _active = match ActiveSchedulerGuard::enter(self.identity(), priority) {
                        Ok(active) => active,
                        Err(error) => {
                            record_scheduler_error(scheduler_error, lane + 1, error);

                            return;
                        }
                    };

                    for index in ((lane + 1)..len).step_by(lane_count) {
                        if let Err(error) = evaluate(index) {
                            record_scheduler_error(scheduler_error, index, error);

                            break;
                        }
                    }
                });
            }

            for index in (0..len).step_by(lane_count) {
                if let Err(error) = evaluate(index) {
                    record_scheduler_error(&scheduler_error, index, error);

                    break;
                }
            }
        });

        match take_scheduler_error(&scheduler_error)? {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    #[cfg(test)]
    pub(crate) fn worker_count(&self) -> usize {
        self.worker_count
    }

    pub(crate) fn current_priority(&self) -> Result<Option<QueryPriority>, FactQueryError> {
        ACTIVE_SCHEDULERS
            .try_with(|active| {
                let active = active.try_borrow().map_err(|_| {
                    FactRuntimeFailure::SchedulerLocalStateUnavailable {
                        operation: SchedulerLocalOperation::CurrentPriority,
                        cause: LocalStateFailure::BorrowConflict,
                    }
                })?;

                Ok(active
                    .iter()
                    .rev()
                    .find(|active| active.identity == self.identity())
                    .map(|active| active.priority))
            })
            .map_err(|_| FactRuntimeFailure::SchedulerLocalStateUnavailable {
                operation: SchedulerLocalOperation::CurrentPriority,
                cause: LocalStateFailure::Unavailable,
            })?
    }

    pub(crate) fn promote(&self, priority: &QueryPriorityDemand, requested: QueryPriority) {
        priority.promote(requested);
        self.slots.priority_changed();
    }

    fn execute<T>(
        &self,
        priority: &QueryPriorityDemand,
        operation: impl FnOnce() -> T,
    ) -> Result<T, FactQueryError> {
        if self.is_active()? {
            return Ok(operation());
        }

        let queue_span = self
            .profile
            .as_deref()
            .map(|profile| profile.start(ProfileOperation::SchedulerQueue, None));

        let _slot = self.slots.acquire(priority)?;

        if let Some(span) = queue_span {
            span.finish(CompilationProfileOutcome::Completed);
        }

        let _worker_activity = self
            .profile
            .as_deref()
            .map(ProfileSession::start_worker_activity);

        let _active = ActiveSchedulerGuard::enter(self.identity(), priority.current())?;

        Ok(operation())
    }

    fn pool(&self, priority: QueryPriority) -> Result<&ThreadPool, FactQueryError> {
        if priority == QueryPriority::Interactive && self.worker_count > 1 {
            return build_pool(
                &self.interactive_pool,
                WorkerPoolKind::Interactive,
                1,
                "bray-query-interactive",
            );
        }

        build_pool(
            &self.ordinary_pool,
            WorkerPoolKind::Ordinary,
            self.worker_count,
            "bray-query",
        )
    }

    fn is_active(&self) -> Result<bool, FactQueryError> {
        ACTIVE_SCHEDULERS
            .try_with(|active| {
                let active = active.try_borrow().map_err(|_| {
                    FactRuntimeFailure::SchedulerLocalStateUnavailable {
                        operation: SchedulerLocalOperation::Inspect,
                        cause: LocalStateFailure::BorrowConflict,
                    }
                })?;

                Ok(active
                    .iter()
                    .any(|active| active.identity == self.identity()))
            })
            .map_err(|_| FactRuntimeFailure::SchedulerLocalStateUnavailable {
                operation: SchedulerLocalOperation::Inspect,
                cause: LocalStateFailure::Unavailable,
            })?
    }

    fn identity(&self) -> usize {
        std::ptr::from_ref(self).addr()
    }
}

fn record_scheduler_error(
    storage: &Mutex<Option<(usize, FactQueryError)>>,
    item: usize,
    error: FactQueryError,
) {
    // A scoped worker cannot return through Rayon's callback. If publication is poisoned, the
    // coordinating worker reports that exact result-storage poison through `take_scheduler_error`.
    let Ok(mut current) = storage.lock() else {
        return;
    };

    if current
        .as_ref()
        .is_none_or(|(current_item, _)| item < *current_item)
    {
        *current = Some((item, error));
    }
}

fn publish_scheduled_result<T>(
    slot: &Mutex<Option<T>>,
    item: usize,
    value: T,
) -> Result<(), FactQueryError> {
    let mut slot = slot
        .lock()
        .map_err(|_| FactRuntimeFailure::SchedulerResultStatePoisoned { item: Some(item) })?;

    *slot = Some(value);

    Ok(())
}

fn collect_scheduled_results<T>(slots: Vec<Mutex<Option<T>>>) -> Result<Vec<T>, FactQueryError> {
    slots
        .into_iter()
        .enumerate()
        .map(|(index, slot)| {
            let result = slot.into_inner().map_err(|_| {
                FactRuntimeFailure::SchedulerResultStatePoisoned { item: Some(index) }
            })?;

            result.ok_or_else(|| {
                FactRuntimeFailure::WorkerTerminated {
                    worker: None,
                    item: Some(index),
                }
                .into()
            })
        })
        .collect()
}

fn take_scheduler_error(
    storage: &Mutex<Option<(usize, FactQueryError)>>,
) -> Result<Option<FactQueryError>, FactQueryError> {
    let mut current = storage
        .lock()
        .map_err(|_| FactRuntimeFailure::SchedulerResultStatePoisoned { item: None })?;

    Ok(current.take().map(|(_, error)| error))
}

fn build_pool<'a>(
    storage: &'a OnceLock<Result<ThreadPool, FactRuntimeError>>,
    pool: WorkerPoolKind,
    worker_count: usize,
    thread_name: &'static str,
) -> Result<&'a ThreadPool, FactQueryError> {
    storage
        .get_or_init(|| {
            ThreadPoolBuilder::new()
                .num_threads(worker_count)
                .thread_name(move |index| format!("{thread_name}-{index}"))
                .build()
                .map_err(|error| {
                    FactRuntimeError::from(FactRuntimeFailure::WorkerPoolCreation {
                        pool,
                        workers: worker_count,
                        host: error
                            .source()
                            .and_then(|source| source.downcast_ref::<io::Error>())
                            .map(HostIoFailure::from),
                    })
                })
        })
        .as_ref()
        .map_err(|error| FactQueryError::Runtime(error.clone()))
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

        register_waiter(&mut state, interactive)?;
        self.available.notify_all();

        loop {
            let promoted = priority.current() == QueryPriority::Interactive;

            if promoted != interactive {
                unregister_waiter(&mut state, interactive)?;
                interactive = promoted;
                register_waiter(&mut state, interactive)?;
                self.available.notify_all();
            }

            if self.can_acquire(&state, interactive) {
                break;
            }

            state = self.available.wait(state).map_err(|_| {
                FactRuntimeFailure::SynchronizationPoisoned {
                    component: SynchronizationComponent::SchedulerSlots,
                    fact: None,
                    task: None,
                }
            })?;
        }

        unregister_waiter(&mut state, interactive)?;
        grant_slot(&mut state, interactive)?;

        Ok(ExecutionSlot { slots: self })
    }

    fn try_acquire(
        &self,
        priority: &QueryPriorityDemand,
    ) -> Result<Option<ExecutionSlot<'_>>, FactQueryError> {
        let interactive = priority.current() == QueryPriority::Interactive;
        let mut state = self.state()?;

        if !self.can_acquire(&state, interactive) {
            return Ok(None);
        }

        grant_slot(&mut state, interactive)?;

        drop(state);

        Ok(Some(ExecutionSlot { slots: self }))
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
        self.state.lock().map_err(|_| {
            FactRuntimeFailure::SynchronizationPoisoned {
                component: SynchronizationComponent::SchedulerSlots,
                fact: None,
                task: None,
            }
            .into()
        })
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

fn grant_slot(state: &mut SlotState, interactive: bool) -> Result<(), FactQueryError> {
    let interactive_streak = if interactive {
        state
            .interactive_streak
            .checked_add(1)
            .ok_or(FactRuntimeFailure::CapacityExhausted {
                resource: CapacityResource::SchedulerInteractiveStreak,
                fact: None,
                task: None,
            })?
    } else {
        0
    };

    let active = state
        .active
        .checked_add(1)
        .ok_or(FactRuntimeFailure::CapacityExhausted {
            resource: CapacityResource::SchedulerActiveSlots,
            fact: None,
            task: None,
        })?;

    state.interactive_streak = interactive_streak;
    state.active = active;

    Ok(())
}

fn register_waiter(state: &mut SlotState, interactive: bool) -> Result<(), FactQueryError> {
    if interactive {
        state.interactive_waiters = state.interactive_waiters.checked_add(1).ok_or(
            FactRuntimeFailure::CapacityExhausted {
                resource: CapacityResource::SchedulerInteractiveWaiters,
                fact: None,
                task: None,
            },
        )?;
    } else {
        state.ordinary_waiters =
            state
                .ordinary_waiters
                .checked_add(1)
                .ok_or(FactRuntimeFailure::CapacityExhausted {
                    resource: CapacityResource::SchedulerOrdinaryWaiters,
                    fact: None,
                    task: None,
                })?;
    }

    Ok(())
}

fn unregister_waiter(state: &mut SlotState, interactive: bool) -> Result<(), FactQueryError> {
    if interactive {
        if state.interactive_waiters == 0 {
            return Err(FactRuntimeFailure::InvalidSchedulerState {
                counter: SchedulerCounter::InteractiveWaiters,
                expected_minimum: 1,
                actual: 0,
            }
            .into());
        }

        state.interactive_waiters -= 1;
    } else {
        if state.ordinary_waiters == 0 {
            return Err(FactRuntimeFailure::InvalidSchedulerState {
                counter: SchedulerCounter::OrdinaryWaiters,
                expected_minimum: 1,
                actual: 0,
            }
            .into());
        }

        state.ordinary_waiters -= 1;
    }

    Ok(())
}

struct ExecutionSlot<'a> {
    slots: &'a ExecutionSlots,
}

impl Drop for ExecutionSlot<'_> {
    fn drop(&mut self) {
        // Slot release cannot report errors from Drop. Fallible acquisition validates every
        // counter transition before a slot guard is constructed.
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
                let mut active = active.try_borrow_mut().map_err(|_| {
                    FactRuntimeFailure::SchedulerLocalStateUnavailable {
                        operation: SchedulerLocalOperation::Enter,
                        cause: LocalStateFailure::BorrowConflict,
                    }
                })?;

                active.push(ActiveScheduler { identity, priority });

                Ok::<_, FactQueryError>(())
            })
            .map_err(|_| FactRuntimeFailure::SchedulerLocalStateUnavailable {
                operation: SchedulerLocalOperation::Enter,
                cause: LocalStateFailure::Unavailable,
            })??;

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
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Barrier, Mutex, mpsc};
    use std::time::Duration;

    use super::{
        ExecutionSlots, FactScheduler, SlotState, collect_scheduled_results, grant_slot,
        publish_scheduled_result, register_waiter, unregister_waiter,
    };
    use crate::fact::{
        CapacityResource, FactQueryError, FactRuntimeFailure, QueryPriorityDemand, SchedulerCounter,
    };
    use crate::{QueryPriority, WorkerBudget};

    #[test]
    fn indexed_work_is_bounded_and_retains_input_order() {
        let scheduler = FactScheduler::new(worker_budget(2));
        let active = AtomicUsize::new(0);
        let maximum = AtomicUsize::new(0);
        let rendezvous = Barrier::new(2);

        let results = scheduler
            .map_indexed(QueryPriority::Normal, 8, |index| {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;

                maximum.fetch_max(current, Ordering::SeqCst);
                rendezvous.wait();
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
                acquire_and_report(
                    background_slots,
                    QueryPriorityDemand::new(QueryPriority::Background),
                    background_sender,
                );
            });

            let interactive_slots = Arc::clone(&slots);
            let interactive_sender = order_sender.clone();

            scope.spawn(move || {
                acquire_and_report(
                    interactive_slots,
                    QueryPriorityDemand::new(QueryPriority::Interactive),
                    interactive_sender,
                );
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
                acquire_and_report(shared_slots, shared_demand, shared_sender);
            });

            let background_slots = Arc::clone(&slots);
            let background_sender = order_sender.clone();

            scope.spawn(move || {
                acquire_and_report(
                    background_slots,
                    QueryPriorityDemand::new(QueryPriority::Background),
                    background_sender,
                );
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

    #[test]
    fn nested_indexed_work_reuses_the_owned_worker_without_waiting_for_another_slot() {
        let scheduler = FactScheduler::new(WorkerBudget::serial());

        let results = scheduler
            .run(QueryPriority::Normal, || {
                scheduler.map_indexed(QueryPriority::Normal, 8, |index| index)
            })
            .unwrap_or_else(|error| panic!("outer work must complete: {error:?}"))
            .unwrap_or_else(|error| panic!("nested indexed work must complete: {error:?}"));

        assert_eq!(results, (0..8).collect::<Vec<_>>());
    }

    #[test]
    fn nested_indexed_work_uses_available_workers_without_exceeding_the_budget() {
        let scheduler = FactScheduler::new(worker_budget(3));
        let active = AtomicUsize::new(0);
        let maximum = AtomicUsize::new(0);
        let rendezvous = Barrier::new(3);

        let results = scheduler
            .run(QueryPriority::Normal, || {
                scheduler.map_indexed(QueryPriority::Normal, 6, |index| {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;

                    maximum.fetch_max(current, Ordering::SeqCst);

                    if index < 3 {
                        rendezvous.wait();
                    }

                    active.fetch_sub(1, Ordering::SeqCst);

                    index
                })
            })
            .unwrap_or_else(|error| panic!("outer work must complete: {error:?}"))
            .unwrap_or_else(|error| panic!("nested indexed work must complete: {error:?}"));

        assert_eq!(results, (0..6).collect::<Vec<_>>());
        assert_eq!(maximum.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn scheduler_capacity_failure_does_not_partially_grant_a_slot() {
        let mut state = SlotState {
            active: 2,
            interactive_streak: usize::MAX,
            ..SlotState::default()
        };

        let error = match grant_slot(&mut state, true) {
            Ok(()) => panic!("an exhausted interactive streak must reject a slot"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            FactQueryError::Runtime(error)
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::CapacityExhausted {
                        resource: CapacityResource::SchedulerInteractiveStreak,
                        ..
                    }
                )
        ));

        assert_eq!(state.active, 2);
        assert_eq!(state.interactive_streak, usize::MAX);
    }

    #[test]
    fn scheduled_result_publication_reports_result_slot_poison() {
        let slot = Mutex::new(None);

        let _ = std::panic::catch_unwind(|| {
            let _guard = slot
                .lock()
                .unwrap_or_else(|_| panic!("test result slot must initially be available"));

            panic!("poison test result slot");
        });

        let error = match publish_scheduled_result(&slot, 7, 42) {
            Ok(()) => panic!("a poisoned result slot must reject publication"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            FactQueryError::Runtime(error)
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::SchedulerResultStatePoisoned {
                        item: Some(7),
                    }
                )
        ));
    }

    #[test]
    fn panicking_indexed_work_remains_a_compiler_domain_panic() {
        let scheduler = FactScheduler::new(worker_budget(2));

        let result = catch_unwind(AssertUnwindSafe(|| {
            let _ = scheduler.map_indexed(QueryPriority::Normal, 4, |index| {
                if index == 2 {
                    panic!("test compiler invariant failed");
                }

                index
            });
        }));

        assert!(result.is_err());
    }

    #[test]
    fn missing_scheduled_result_reports_worker_termination() {
        let error = match collect_scheduled_results::<u32>(vec![Mutex::new(None)]) {
            Ok(values) => panic!("missing worker publication must fail: {values:?}"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            FactQueryError::Runtime(error)
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::WorkerTerminated {
                        worker: None,
                        item: Some(0),
                    }
                )
        ));
    }

    #[test]
    fn scheduler_waiter_state_reports_the_exact_counter() {
        let mut state = SlotState::default();

        let error = match unregister_waiter(&mut state, false) {
            Ok(()) => panic!("an absent ordinary waiter must not be unregistered"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            FactQueryError::Runtime(error)
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::InvalidSchedulerState {
                        counter: SchedulerCounter::OrdinaryWaiters,
                        expected_minimum: 1,
                        actual: 0,
                    }
                )
        ));

        state.interactive_waiters = usize::MAX;

        let error = match register_waiter(&mut state, true) {
            Ok(()) => panic!("an exhausted waiter counter must reject registration"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            FactQueryError::Runtime(error)
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::CapacityExhausted {
                        resource: CapacityResource::SchedulerInteractiveWaiters,
                        ..
                    }
                )
        ));
    }

    fn worker_budget(workers: usize) -> WorkerBudget {
        WorkerBudget::new(workers)
            .unwrap_or_else(|error| panic!("test worker budget must be valid: {error:?}"))
    }

    fn acquire_and_report(
        slots: Arc<ExecutionSlots>,
        priority: QueryPriorityDemand,
        sender: mpsc::Sender<QueryPriority>,
    ) {
        let _slot = slots
            .acquire(&priority)
            .unwrap_or_else(|error| panic!("scheduled priority work must run: {error:?}"));

        sender
            .send(priority.current())
            .unwrap_or_else(|_| panic!("test must observe scheduled priority work"));
    }
}
