use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

use super::contract::{RuntimeEventError, RuntimeEventGeneration, RuntimeEventWake};
use super::wait::{
    EventNotification, EventWaitNode, PendingEventWait, ReservedEventWait, RuntimeEventRegistration,
};

#[derive(Default)]
pub(super) struct RuntimeEventData {
    generation: u64,
    closed: bool,
    first: Option<triomphe::Arc<EventWaitNode>>,
    last: Option<triomphe::Arc<EventWaitNode>>,
}

/// Shareable generation-based event used to connect external completion to task wakes.
#[derive(Clone, Default)]
pub struct RuntimeEvent {
    data: Arc<Mutex<RuntimeEventData>>,
}

impl RuntimeEvent {
    /// Creates an open event at generation zero.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the current generation and whether the event is closed.
    pub fn observation(&self) -> (RuntimeEventGeneration, bool) {
        let data = self.lock_data();

        (RuntimeEventGeneration(data.generation), data.closed)
    }

    /// Registers a wake for the next change after an observed generation.
    ///
    /// Registration after a concurrent signal or close wakes immediately. Dropping
    /// the returned registration withdraws a waiter that has not already fired.
    pub fn register(
        &self,
        observed: RuntimeEventGeneration,
        wake: Arc<dyn RuntimeEventWake>,
    ) -> Result<RuntimeEventRegistration, RuntimeEventError> {
        let reserved = ReservedEventWait::reserve()?;

        self.register_reserved(observed, &reserved, EventNotification::Callback(wake))
    }

    pub(crate) fn register_reserved(
        &self,
        observed: RuntimeEventGeneration,
        reserved: &ReservedEventWait,
        wake: EventNotification,
    ) -> Result<RuntimeEventRegistration, RuntimeEventError> {
        reserved
            .node
            .registered
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| RuntimeEventError::AlreadyRegistered)?;

        let registration = RuntimeEventRegistration {
            node: triomphe::Arc::clone(&reserved.node),
            event: Arc::downgrade(&self.data),
        };

        let wake_now = {
            let mut data = self.lock_data();

            if data.closed || data.generation != observed.0 {
                Some(wake)
            } else {
                data.push_waiter(triomphe::Arc::clone(&reserved.node), observed, wake);

                None
            }
        };

        if let Some(wake) = wake_now {
            wake.wake();
        }

        Ok(registration)
    }

    /// Advances the event generation and wakes every current waiter once.
    /// Exhaustion closes the event and drains its waiters before returning the error.
    pub fn signal(&self) -> Result<RuntimeEventGeneration, RuntimeEventError> {
        let generation = {
            let mut data = self.lock_data();

            if data.closed {
                return Ok(RuntimeEventGeneration(data.generation));
            }

            match data.generation.checked_add(1) {
                Some(generation) => {
                    data.generation = generation;

                    Some(RuntimeEventGeneration(generation))
                }
                None => {
                    data.closed = true;

                    None
                }
            }
        };

        self.wake_before(generation);

        generation.ok_or(RuntimeEventError::GenerationExhausted)
    }

    /// Closes the event and wakes every current waiter once.
    ///
    /// Returns whether this call performed the open-to-closed transition.
    pub fn close(&self) -> bool {
        {
            let mut data = self.lock_data();

            if data.closed {
                return false;
            }

            data.closed = true;
        }

        self.wake_before(None);

        true
    }

    fn wake_before(&self, generation: Option<RuntimeEventGeneration>) {
        loop {
            let wake = {
                let mut data = self.lock_data();

                let Some(first) = data.first.as_ref().map(triomphe::Arc::clone) else {
                    return;
                };

                let observed = first
                    .pending
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .as_ref()
                    .map(|pending| pending.observed);

                if generation.is_some_and(|generation| observed >= Some(generation)) {
                    return;
                }

                data.remove_waiter(&first)
            };

            if let Some(wake) = wake {
                wake.wake();
            }
        }
    }

    fn lock_data(&self) -> std::sync::MutexGuard<'_, RuntimeEventData> {
        // Critical sections only relink admitted nodes. Callbacks and their destruction run outside.
        self.data
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl RuntimeEventData {
    fn push_waiter(
        &mut self,
        node: triomphe::Arc<EventWaitNode>,
        observed: RuntimeEventGeneration,
        wake: EventNotification,
    ) {
        *node
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(PendingEventWait {
            previous: self.last.as_ref().map(triomphe::Arc::clone),
            next: None,
            observed,
            wake,
        });

        if let Some(last) = &self.last {
            if let Some(pending) = last
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_mut()
            {
                pending.next = Some(triomphe::Arc::clone(&node));
            }
        } else {
            self.first = Some(triomphe::Arc::clone(&node));
        }

        self.last = Some(node);
    }

    pub(super) fn remove_waiter(&mut self, node: &EventWaitNode) -> Option<EventNotification> {
        let pending = node
            .pending
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()?;

        if let Some(previous) = &pending.previous {
            if let Some(link) = previous
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_mut()
            {
                link.next = pending.next.as_ref().map(triomphe::Arc::clone);
            }
        } else {
            self.first = pending.next.as_ref().map(triomphe::Arc::clone);
        }

        if let Some(next) = &pending.next {
            if let Some(link) = next
                .pending
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_mut()
            {
                link.previous = pending.previous.as_ref().map(triomphe::Arc::clone);
            }
        } else {
            self.last = pending.previous.as_ref().map(triomphe::Arc::clone);
        }

        Some(pending.wake)
    }
}

impl Drop for RuntimeEventData {
    fn drop(&mut self) {
        // Break both directions iteratively, including when registrations outlive the event.
        while let Some(first) = self.first.as_ref().map(triomphe::Arc::clone) {
            self.remove_waiter(&first);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{RuntimeEvent, RuntimeEventGeneration};

    #[test]
    fn exhaustion_closes_and_drains_without_reusing_a_generation() {
        let event = RuntimeEvent::new();
        event.data.lock().unwrap().generation = u64::MAX;
        let wakes = Arc::new(AtomicUsize::new(0));

        let registrations: [_; 2] = std::array::from_fn(|_| {
            let wakes = Arc::clone(&wakes);

            event
                .register(
                    event.observation().0,
                    Arc::new(move || {
                        wakes.fetch_add(1, Ordering::Relaxed);
                    }),
                )
                .unwrap()
        });

        assert_eq!(
            event.signal(),
            Err(super::RuntimeEventError::GenerationExhausted)
        );

        assert_eq!(
            event.observation(),
            (RuntimeEventGeneration(u64::MAX), true)
        );

        assert_eq!(wakes.load(Ordering::Relaxed), 2);
        assert!(!event.close());
        assert_eq!(event.signal(), Ok(RuntimeEventGeneration(u64::MAX)));
        let wakes_again = Arc::clone(&wakes);

        let late = event
            .register(
                event.observation().0,
                Arc::new(move || {
                    wakes_again.fetch_add(1, Ordering::Relaxed);
                }),
            )
            .unwrap();

        assert_eq!(wakes.load(Ordering::Relaxed), 3);
        drop((registrations, late));
    }

    #[test]
    fn poisoned_intact_event_preserves_waiters_and_generation() {
        let event = RuntimeEvent::new();
        let wakes = Arc::new(AtomicUsize::new(0));
        let retained = Arc::clone(&wakes);

        let registration = event
            .register(
                event.observation().0,
                Arc::new(move || {
                    retained.fetch_add(1, Ordering::Relaxed);
                }),
            )
            .unwrap();

        let _ = std::panic::catch_unwind(|| {
            let _guard = event.data.lock().unwrap();
            panic!("poison intact event state");
        });

        assert_eq!(event.observation(), (RuntimeEventGeneration(0), false));
        assert_eq!(event.signal(), Ok(RuntimeEventGeneration(1)));
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
        assert!(event.close());
        drop(registration);
    }

    #[test]
    fn signal_advances_generation_and_coalesces_each_registration() {
        let event = RuntimeEvent::new();
        let wakes = Arc::new(AtomicUsize::new(0));

        let observed = event.observation().0;

        let retained_wakes = Arc::clone(&wakes);

        let _registration = event
            .register(
                observed,
                Arc::new(move || {
                    retained_wakes.fetch_add(1, Ordering::Relaxed);
                }),
            )
            .unwrap_or_else(|error| panic!("event wait must register: {error:?}"));

        assert_eq!(
            event
                .signal()
                .unwrap_or_else(|error| panic!("event must signal: {error:?}")),
            RuntimeEventGeneration(1)
        );

        event
            .signal()
            .unwrap_or_else(|error| panic!("event must signal again: {error:?}"));

        assert_eq!(wakes.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn stale_registration_wakes_immediately() {
        let event = RuntimeEvent::new();
        let wakes = Arc::new(AtomicUsize::new(0));

        event
            .signal()
            .unwrap_or_else(|error| panic!("event must signal: {error:?}"));

        let retained_wakes = Arc::clone(&wakes);

        let _registration = event
            .register(
                RuntimeEventGeneration(0),
                Arc::new(move || {
                    retained_wakes.fetch_add(1, Ordering::Relaxed);
                }),
            )
            .unwrap_or_else(|error| panic!("stale wait must resolve: {error:?}"));

        assert_eq!(wakes.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn dropping_registration_withdraws_the_waiter() {
        let event = RuntimeEvent::new();
        let wakes = Arc::new(AtomicUsize::new(0));

        let observed = event.observation().0;

        let retained_wakes = Arc::clone(&wakes);

        let registration = event
            .register(
                observed,
                Arc::new(move || {
                    retained_wakes.fetch_add(1, Ordering::Relaxed);
                }),
            )
            .unwrap_or_else(|error| panic!("event wait must register: {error:?}"));

        drop(registration);

        event
            .signal()
            .unwrap_or_else(|error| panic!("event must signal: {error:?}"));

        assert_eq!(wakes.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn close_is_idempotent_and_resolves_waiters() {
        let event = RuntimeEvent::new();
        let wakes = Arc::new(AtomicUsize::new(0));

        let observed = event.observation().0;

        let retained_wakes = Arc::clone(&wakes);

        let _registration = event
            .register(
                observed,
                Arc::new(move || {
                    retained_wakes.fetch_add(1, Ordering::Relaxed);
                }),
            )
            .unwrap_or_else(|error| panic!("event wait must register: {error:?}"));

        assert!(event.close());
        assert!(!event.close());
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn reserved_waits_reuse_storage_and_unlink_head_middle_and_tail() {
        use super::{EventNotification, ReservedEventWait, RuntimeEventError, RuntimeEventWake};
        use crate::test_support::with_allocation_failure;

        assert!(matches!(
            with_allocation_failure(ReservedEventWait::reserve),
            Err(RuntimeEventError::AllocationFailed)
        ));

        let event = RuntimeEvent::new();

        let waits: Vec<_> = (0..4)
            .map(|_| ReservedEventWait::reserve().unwrap())
            .collect();

        let wakes = Arc::new(AtomicUsize::new(0));

        let callbacks: Vec<Arc<dyn RuntimeEventWake>> = (0..4)
            .map(|index| {
                let wakes = Arc::clone(&wakes);

                Arc::new(move || {
                    wakes.fetch_or(1 << index, Ordering::Relaxed);
                }) as Arc<dyn RuntimeEventWake>
            })
            .collect();

        with_allocation_failure(|| {
            for removed in [[1, 3], [0, 2], [3, 0]] {
                wakes.store(0, Ordering::Relaxed);
                let observed = event.observation().0;

                let mut registrations: [_; 4] = std::array::from_fn(|index| {
                    Some(
                        event
                            .register_reserved(
                                observed,
                                &waits[index],
                                EventNotification::Callback(Arc::clone(&callbacks[index])),
                            )
                            .unwrap(),
                    )
                });

                assert!(matches!(
                    event.register_reserved(
                        observed,
                        &waits[0],
                        EventNotification::Callback(Arc::clone(&callbacks[0])),
                    ),
                    Err(RuntimeEventError::AlreadyRegistered)
                ));

                for index in removed {
                    registrations[index].take();
                }

                event.signal().unwrap();

                assert_eq!(
                    wakes.load(Ordering::Relaxed),
                    15 ^ (1 << removed[0]) ^ (1 << removed[1])
                );

                drop(registrations);
            }

            // Event destruction releases the links even when registrations and reservations survive.
            let observed = event.observation().0;

            let registrations: [_; 4] = std::array::from_fn(|index| {
                event
                    .register_reserved(
                        observed,
                        &waits[index],
                        EventNotification::Callback(Arc::clone(&callbacks[index])),
                    )
                    .unwrap()
            });

            drop(event);

            assert!(
                waits
                    .iter()
                    .all(|wait| wait.node.pending.lock().unwrap().is_none())
            );

            drop(registrations);
        });
    }

    #[test]
    fn a_reentrant_registration_waits_for_its_own_generation() {
        use std::sync::Mutex;

        let event = RuntimeEvent::new();
        let retained = Arc::new(Mutex::new(None));
        let wakes = Arc::new(AtomicUsize::new(0));
        let next_event = event.clone();
        let next_retained = Arc::clone(&retained);
        let next_wakes = Arc::clone(&wakes);

        let first = event
            .register(
                event.observation().0,
                Arc::new(move || {
                    let wakes = Arc::clone(&next_wakes);

                    let registration = next_event
                        .register(
                            next_event.observation().0,
                            Arc::new(move || {
                                wakes.fetch_add(1, Ordering::Relaxed);
                            }),
                        )
                        .unwrap();

                    *next_retained.lock().unwrap() = Some(registration);
                }),
            )
            .unwrap();

        event.signal().unwrap();
        assert_eq!(wakes.load(Ordering::Relaxed), 0);
        event.signal().unwrap();
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
        drop(first);
        retained.lock().unwrap().take();
    }

    #[test]
    fn withdrawal_can_reenter_the_event_from_callback_destruction() {
        use super::RuntimeEventWake;

        struct Callback(RuntimeEvent);

        impl RuntimeEventWake for Callback {
            fn wake(&self) {}
        }

        impl Drop for Callback {
            fn drop(&mut self) {
                self.0.signal().unwrap();
            }
        }

        let event = RuntimeEvent::new();

        let registration = event
            .register(event.observation().0, Arc::new(Callback(event.clone())))
            .unwrap();

        drop(registration);
        assert_eq!(event.observation().0.raw(), 1);
    }

    #[test]
    fn concurrent_signal_withdrawal_and_reuse_preserve_the_generation_boundary() {
        use std::sync::{Mutex, mpsc};
        use std::time::Duration;

        use super::{EventNotification, ReservedEventWait};

        let event = RuntimeEvent::new();

        let waits: Vec<_> = (0..3)
            .map(|_| ReservedEventWait::reserve().unwrap())
            .collect();

        let (entered, started) = mpsc::channel();

        let (release, resume) = mpsc::channel();

        let resume = Mutex::new(resume);
        let observed = event.observation().0;
        let wakes = Arc::new(AtomicUsize::new(0));

        let first = event
            .register_reserved(
                observed,
                &waits[0],
                EventNotification::Callback(Arc::new(move || {
                    entered.send(()).unwrap();

                    resume
                        .lock()
                        .unwrap()
                        .recv_timeout(Duration::from_secs(5))
                        .unwrap();
                })),
            )
            .unwrap();

        let second_wakes = Arc::clone(&wakes);

        let second = event
            .register_reserved(
                observed,
                &waits[1],
                EventNotification::Callback(Arc::new(move || {
                    second_wakes.fetch_or(1, Ordering::Relaxed);
                })),
            )
            .unwrap();

        let third_wakes = Arc::clone(&wakes);

        let third = event
            .register_reserved(
                observed,
                &waits[2],
                EventNotification::Callback(Arc::new(move || {
                    third_wakes.fetch_or(2, Ordering::Relaxed);
                })),
            )
            .unwrap();

        let signalling = event.clone();
        let worker = std::thread::spawn(move || signalling.signal().unwrap());
        started.recv_timeout(Duration::from_secs(5)).unwrap();

        drop(second);
        let replacement_wakes = Arc::clone(&wakes);

        let replacement = event
            .register_reserved(
                event.observation().0,
                &waits[1],
                EventNotification::Callback(Arc::new(move || {
                    replacement_wakes.fetch_or(4, Ordering::Relaxed);
                })),
            )
            .unwrap();

        release.send(()).unwrap();
        assert_eq!(worker.join().unwrap().raw(), 1);
        assert_eq!(wakes.load(Ordering::Relaxed), 2);
        event.signal().unwrap();
        assert_eq!(wakes.load(Ordering::Relaxed), 6);
        drop((first, third, replacement));
    }
}
