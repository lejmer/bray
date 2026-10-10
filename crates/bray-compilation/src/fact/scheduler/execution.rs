use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use super::super::{
    CapacityResource, FactQueryError, FactRuntimeFailure, QueryPriority, QueryPriorityDemand,
    SchedulerCounter, SynchronizationComponent,
};
use crate::profile::{ProfileSession, ProfileWorkerActivity};

const MAX_INTERACTIVE_STREAK: usize = 8;

#[derive(Debug)]
pub(super) struct ExecutionSlots {
    limit: usize,
    state: Mutex<SlotState>,
    available: Condvar,
}

#[derive(Debug, Default)]
pub(super) struct SlotState {
    pub(super) active: usize,
    pub(super) interactive_waiters: usize,
    pub(super) ordinary_waiters: usize,
    pub(super) interactive_streak: usize,
}

impl ExecutionSlots {
    pub(super) fn new(limit: usize) -> Self {
        Self {
            limit,
            state: Mutex::new(SlotState::default()),
            available: Condvar::new(),
        }
    }

    pub(super) fn acquire(
        &self,
        priority: &QueryPriorityDemand,
    ) -> Result<ExecutionSlot<'_>, FactQueryError> {
        self.acquire_capacity(priority)?;

        Ok(ExecutionSlot {
            slots: self,
            lease: Arc::new(ExecutionLease::new(priority.clone())),
        })
    }

    fn acquire_capacity(&self, priority: &QueryPriorityDemand) -> Result<(), FactQueryError> {
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

        Ok(())
    }

    pub(super) fn try_acquire(
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

        Ok(Some(ExecutionSlot {
            slots: self,
            lease: Arc::new(ExecutionLease::new(priority.clone())),
        }))
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

    pub(super) fn state(&self) -> Result<MutexGuard<'_, SlotState>, FactQueryError> {
        self.state.lock().map_err(|_| {
            FactRuntimeFailure::SynchronizationPoisoned {
                component: SynchronizationComponent::SchedulerSlots,
                fact: None,
                task: None,
            }
            .into()
        })
    }

    pub(super) fn priority_changed(&self) {
        self.available.notify_all();
    }

    #[cfg(test)]
    pub(super) fn wait_until_queued(&self, interactive: usize, ordinary: usize) {
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

pub(super) fn grant_slot(state: &mut SlotState, interactive: bool) -> Result<(), FactQueryError> {
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

pub(super) fn register_waiter(
    state: &mut SlotState,
    interactive: bool,
) -> Result<(), FactQueryError> {
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

pub(super) fn unregister_waiter(
    state: &mut SlotState,
    interactive: bool,
) -> Result<(), FactQueryError> {
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

#[derive(Debug)]
pub(super) struct ExecutionLease {
    pub(super) priority: QueryPriorityDemand,
    pub(super) held: AtomicBool,
    activity: Mutex<Option<ProfileWorkerActivity>>,
}

impl ExecutionLease {
    fn new(priority: QueryPriorityDemand) -> Self {
        Self {
            priority,
            held: AtomicBool::new(true),
            activity: Mutex::new(None),
        }
    }

    fn activity(&self) -> Result<MutexGuard<'_, Option<ProfileWorkerActivity>>, FactQueryError> {
        self.activity.lock().map_err(|_| {
            FactRuntimeFailure::SynchronizationPoisoned {
                component: SynchronizationComponent::SchedulerSlots,
                fact: None,
                task: None,
            }
            .into()
        })
    }

    pub(super) fn suspend(&self, slots: &ExecutionSlots) -> Result<(), FactQueryError> {
        let mut activity = self.activity()?;
        let mut state = slots.state()?;

        if state.active == 0 {
            return Err(FactRuntimeFailure::InvalidSchedulerState {
                counter: SchedulerCounter::ActiveSlots,
                expected_minimum: 1,
                actual: 0,
            }
            .into());
        }

        drop(activity.take());
        self.held.store(false, Ordering::Relaxed);
        state.active -= 1;
        slots.available.notify_all();

        Ok(())
    }

    pub(super) fn resume(
        &self,
        slots: &ExecutionSlots,
        profile: Option<&Arc<ProfileSession>>,
    ) -> Result<(), FactQueryError> {
        let mut activity = self.activity()?;

        slots.acquire_capacity(&self.priority)?;
        self.held.store(true, Ordering::Relaxed);
        *activity = profile.map(ProfileSession::start_worker_activity);

        Ok(())
    }
}

pub(super) struct ExecutionSlot<'a> {
    slots: &'a ExecutionSlots,
    pub(super) lease: Arc<ExecutionLease>,
}

impl ExecutionSlot<'_> {
    pub(super) fn start_activity(
        &self,
        profile: Option<&Arc<ProfileSession>>,
    ) -> Result<(), FactQueryError> {
        *self.lease.activity()? = profile.map(ProfileSession::start_worker_activity);

        Ok(())
    }
}

impl Drop for ExecutionSlot<'_> {
    fn drop(&mut self) {
        // Slot release cannot report errors from Drop. Fallible acquisition validates every
        // counter transition before a slot guard is constructed.
        if let Ok(mut activity) = self.lease.activity.lock() {
            drop(activity.take());
        }

        if !self.lease.held.swap(false, Ordering::Relaxed) {
            return;
        }

        let Ok(mut state) = self.slots.state.lock() else {
            return;
        };

        state.active = state.active.saturating_sub(1);
        self.slots.available.notify_all();
    }
}
