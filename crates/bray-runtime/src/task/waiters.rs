use std::sync::Mutex;

use super::{JoinNotification, JoinWake, TaskId, TaskObservationError};

/// Wait registrations retain only notification state, never the executable frame or its result.
pub(super) struct JoinWaitState {
    data: Mutex<JoinWaitData>,
}

struct JoinWaitData {
    next_identity: u64,
    pending: Option<JoinWaiters>,
}

impl JoinWaitState {
    pub(super) fn new() -> Self {
        Self {
            data: Mutex::new(JoinWaitData {
                next_identity: 0,
                pending: Some(JoinWaiters::default()),
            }),
        }
    }

    pub(super) fn register(
        &self,
        task: TaskId,
        waiter: JoinNotification,
        kind: JoinWaitKind,
    ) -> Result<Option<u64>, TaskObservationError> {
        let mut data = self
            .data
            .lock()
            .map_err(|_| TaskObservationError::SynchronizationPoisoned)?;

        let identity = data.next_identity;

        let Some(pending) = data.pending.as_mut() else {
            drop(data);
            waiter.wake(task);

            return Ok(None);
        };

        let next = identity
            .checked_add(1)
            .ok_or(TaskObservationError::WaiterIdentityExhausted)?;

        if let Err((error, waiter)) = pending.insert(identity, waiter, kind) {
            drop(data);
            drop(waiter);

            return Err(error);
        }

        data.next_identity = next;

        Ok(Some(identity))
    }

    pub(super) fn close(&self) -> JoinWaiters {
        self.data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pending
            .take()
            .unwrap_or_default()
    }

    pub(super) fn remove(&self, identity: u64) {
        let removed = self
            .data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pending
            .as_mut()
            .and_then(|pending| pending.remove(&identity));

        // Notification destruction can reenter task APIs, so release it outside the registry lock.
        drop(removed);
    }

    pub(super) fn contains(&self, identity: u64) -> bool {
        self.data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pending
            .as_ref()
            .is_some_and(|pending| pending.contains_key(&identity))
    }

    pub(super) fn len(&self) -> usize {
        self.data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pending
            .as_ref()
            .map_or(0, JoinWaiters::len)
    }
}

#[derive(Clone, Copy)]
pub(super) enum JoinWaitKind {
    Owner,
    Observer,
}

/// The owning continuation has admission-time storage independent of optional observers.
#[derive(Default)]
pub(super) struct JoinWaiters {
    owner: Option<(u64, JoinNotification)>,
    observers: Vec<(u64, JoinNotification)>,
}

impl JoinWaiters {
    pub(super) fn insert(
        &mut self,
        identity: u64,
        waiter: impl Into<JoinNotification>,
        kind: JoinWaitKind,
    ) -> Result<(), (TaskObservationError, JoinNotification)> {
        let waiter = waiter.into();

        match kind {
            JoinWaitKind::Owner => {
                if self.owner.is_some() {
                    return Err((TaskObservationError::OwnerAlreadyWaiting, waiter));
                }

                self.owner = Some((identity, waiter));
            }
            JoinWaitKind::Observer => {
                if crate::allocation::reserve_vec_entries(&mut self.observers, 1).is_err() {
                    return Err((TaskObservationError::WaiterAllocationFailed, waiter));
                }

                // Registration identities increase monotonically, retaining notification order.
                self.observers.push((identity, waiter));
            }
        }

        Ok(())
    }

    pub(super) fn remove(&mut self, identity: &u64) -> Option<JoinNotification> {
        if self
            .owner
            .as_ref()
            .is_some_and(|(owner, _)| owner == identity)
        {
            self.owner.take().map(|(_, waiter)| waiter)
        } else {
            self.observers
                .binary_search_by_key(identity, |(identity, _)| *identity)
                .ok()
                .map(|index| self.observers.remove(index).1)
        }
    }

    pub(super) fn contains_key(&self, identity: &u64) -> bool {
        self.owner
            .as_ref()
            .is_some_and(|(owner, _)| owner == identity)
            || self
                .observers
                .binary_search_by_key(identity, |(identity, _)| *identity)
                .is_ok()
    }

    pub(super) fn len(&self) -> usize {
        self.observers.len() + usize::from(self.owner.is_some())
    }

    pub(super) fn wake_all(mut self, task: TaskId) {
        for (identity, waiter) in self.observers {
            if self
                .owner
                .as_ref()
                .is_some_and(|(owner, _)| *owner < identity)
                && let Some((_, owner)) = self.owner.take()
            {
                owner.wake(task);
            }

            waiter.wake(task);
        }

        if let Some((_, owner)) = self.owner {
            owner.wake(task);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{JoinWaitKind, JoinWaitState, JoinWaiters};
    use crate::{JoinWake, TaskObservationError};

    #[test]
    fn failed_observer_growth_preserves_the_owner_and_next_registration_identity() {
        let state = JoinWaitState::new();

        let task =
            crate::TaskControlBlock::start(crate::test_support::TestFrame::completing(0)).unwrap();

        let wake: Arc<dyn JoinWake> = Arc::new(|| {});

        assert_eq!(
            state.register(task.id(), Arc::clone(&wake).into(), JoinWaitKind::Owner),
            Ok(Some(0))
        );

        assert_eq!(
            crate::test_support::with_allocation_failure(|| state.register(
                task.id(),
                Arc::clone(&wake).into(),
                JoinWaitKind::Observer
            )),
            Err(TaskObservationError::WaiterAllocationFailed),
        );

        assert_eq!(state.len(), 1);
        assert!(state.contains(0));

        assert_eq!(
            state.register(task.id(), wake.into(), JoinWaitKind::Observer),
            Ok(Some(1))
        );
    }

    #[test]
    fn rejected_notification_destruction_can_withdraw_an_existing_wait() {
        struct WithdrawOnDrop {
            state: triomphe::Arc<JoinWaitState>,
            dropped: Arc<std::sync::atomic::AtomicBool>,
        }

        impl JoinWake for WithdrawOnDrop {
            fn wake(&self, _: crate::TaskId) {}
        }

        impl Drop for WithdrawOnDrop {
            fn drop(&mut self) {
                assert!(
                    self.state.data.try_lock().is_ok(),
                    "notification destroyed under registry lock"
                );

                self.state.remove(0);

                self.dropped
                    .store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }

        let state = triomphe::Arc::try_new(JoinWaitState::new()).unwrap();

        let task =
            crate::TaskControlBlock::start(crate::test_support::TestFrame::completing(0)).unwrap();

        let dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));

        state
            .register(task.id(), Arc::new(|| {}).into(), JoinWaitKind::Owner)
            .unwrap();

        assert_eq!(
            state.register(
                task.id(),
                Arc::new(WithdrawOnDrop {
                    state: triomphe::Arc::clone(&state),
                    dropped: Arc::clone(&dropped),
                })
                .into(),
                JoinWaitKind::Owner
            ),
            Err(TaskObservationError::OwnerAlreadyWaiting)
        );

        assert!(dropped.load(std::sync::atomic::Ordering::Relaxed));
        assert!(!state.contains(0));
    }

    #[test]
    fn owner_slot_is_independent_of_observers_and_reusable_after_withdrawal() {
        let mut waiters = JoinWaiters::default();
        let wake: Arc<dyn JoinWake> = Arc::new(|| {});

        waiters
            .insert(0, Arc::clone(&wake), JoinWaitKind::Observer)
            .unwrap();

        waiters
            .insert(1, Arc::clone(&wake), JoinWaitKind::Owner)
            .unwrap();

        assert_eq!(waiters.observers.len(), 1);
        assert_eq!(waiters.len(), 2);
        assert!(waiters.contains_key(&1));

        assert_eq!(
            waiters
                .insert(2, Arc::clone(&wake), JoinWaitKind::Owner)
                .map_err(|(error, _)| error),
            Err(TaskObservationError::OwnerAlreadyWaiting),
        );

        assert!(waiters.contains_key(&1));
        assert!(!waiters.contains_key(&2));

        waiters.remove(&1);
        waiters.insert(2, wake, JoinWaitKind::Owner).unwrap();
        waiters.remove(&0);

        assert!(waiters.observers.is_empty());
        assert!(!waiters.contains_key(&1));
        assert!(waiters.contains_key(&2));
        assert_eq!(waiters.len(), 1);
    }

    #[test]
    fn terminal_wakes_preserve_registration_order_for_every_owner_position() {
        for owner in 0..3 {
            let mut waiters = JoinWaiters::default();
            let awakened = Arc::new(Mutex::new(Vec::new()));

            for identity in 0..3 {
                let awakened = Arc::clone(&awakened);

                let kind = if owner == identity {
                    JoinWaitKind::Owner
                } else {
                    JoinWaitKind::Observer
                };

                waiters
                    .insert(
                        identity,
                        Arc::new(move || {
                            awakened.lock().unwrap().push(identity);
                        }),
                        kind,
                    )
                    .unwrap();
            }

            let task =
                crate::TaskControlBlock::start(crate::test_support::TestFrame::completing(0))
                    .unwrap();

            waiters.wake_all(task.id());

            assert_eq!(*awakened.lock().unwrap(), [0, 1, 2]);
        }
    }
}
