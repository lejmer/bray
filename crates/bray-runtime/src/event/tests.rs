use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::{RuntimeEvent, RuntimeEventGeneration};

#[test]
fn signal_advances_generation_and_coalesces_each_registration() {
    let event = RuntimeEvent::new();
    let wakes = Arc::new(AtomicUsize::new(0));

    let observed = event
        .observation()
        .unwrap_or_else(|error| panic!("event must be observable: {error:?}"))
        .0;

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

    let observed = event
        .observation()
        .unwrap_or_else(|error| panic!("event must be observable: {error:?}"))
        .0;

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

    let observed = event
        .observation()
        .unwrap_or_else(|error| panic!("event must be observable: {error:?}"))
        .0;

    let retained_wakes = Arc::clone(&wakes);

    let _registration = event
        .register(
            observed,
            Arc::new(move || {
                retained_wakes.fetch_add(1, Ordering::Relaxed);
            }),
        )
        .unwrap_or_else(|error| panic!("event wait must register: {error:?}"));

    assert_eq!(event.close(), Ok(true));
    assert_eq!(event.close(), Ok(false));
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
            let observed = event.observation().unwrap().0;

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
        let observed = event.observation().unwrap().0;

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
            event.observation().unwrap().0,
            Arc::new(move || {
                let wakes = Arc::clone(&next_wakes);

                let registration = next_event
                    .register(
                        next_event.observation().unwrap().0,
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
        .register(
            event.observation().unwrap().0,
            Arc::new(Callback(event.clone())),
        )
        .unwrap();

    drop(registration);
    assert_eq!(event.observation().unwrap().0.raw(), 1);
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
    let observed = event.observation().unwrap().0;
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
            event.observation().unwrap().0,
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
