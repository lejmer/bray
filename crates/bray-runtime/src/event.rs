use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, Weak};

/// Monotonic change generation published by one runtime event.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct RuntimeEventGeneration(u64);

impl RuntimeEventGeneration {
    /// Returns the process-local numeric generation.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Failure to observe or mutate one runtime event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeEventError {
    /// Event generations cannot advance without losing ordering.
    GenerationExhausted,
    /// Wait-registration identities cannot be represented.
    RegistrationIdentityExhausted,
    /// Event state was poisoned by an unexpected runtime panic.
    SynchronizationPoisoned,
}

/// Infallible notification used when a runtime event changes or closes.
pub trait RuntimeEventWake: Send + Sync + 'static {
    /// Makes the registered observer runnable.
    fn wake(&self);
}

impl<F> RuntimeEventWake for F
where
    F: Fn() + Send + Sync + 'static,
{
    fn wake(&self) {
        self();
    }
}

#[derive(Default)]
struct RuntimeEventData {
    generation: u64,
    next_registration: u64,
    closed: bool,
    waiters: BTreeMap<u64, Arc<dyn RuntimeEventWake>>,
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
    pub fn observation(&self) -> Result<(RuntimeEventGeneration, bool), RuntimeEventError> {
        let data = self.lock_data()?;

        Ok((RuntimeEventGeneration(data.generation), data.closed))
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
        let (identity, wake_now) = {
            let mut data = self.lock_data()?;

            if data.closed || data.generation != observed.0 {
                (None, Some(wake))
            } else {
                let identity = data.next_registration;

                let Some(next) = identity.checked_add(1) else {
                    return Err(RuntimeEventError::RegistrationIdentityExhausted);
                };

                data.next_registration = next;
                data.waiters.insert(identity, wake);

                (Some(identity), None)
            }
        };

        if let Some(wake) = wake_now {
            wake.wake();
        }

        Ok(RuntimeEventRegistration {
            identity,
            event: Arc::downgrade(&self.data),
        })
    }

    /// Advances the event generation and wakes every current waiter once.
    pub fn signal(&self) -> Result<RuntimeEventGeneration, RuntimeEventError> {
        let (generation, waiters) = {
            let mut data = self.lock_data()?;

            if data.closed {
                return Ok(RuntimeEventGeneration(data.generation));
            }

            let Some(generation) = data.generation.checked_add(1) else {
                return Err(RuntimeEventError::GenerationExhausted);
            };

            data.generation = generation;

            (generation, std::mem::take(&mut data.waiters))
        };

        wake_all(waiters);

        Ok(RuntimeEventGeneration(generation))
    }

    /// Closes the event and wakes every current waiter once.
    ///
    /// Returns whether this call performed the open-to-closed transition.
    pub fn close(&self) -> Result<bool, RuntimeEventError> {
        let waiters = {
            let mut data = self.lock_data()?;

            if data.closed {
                return Ok(false);
            }

            data.closed = true;

            std::mem::take(&mut data.waiters)
        };

        wake_all(waiters);

        Ok(true)
    }

    fn lock_data(&self) -> Result<std::sync::MutexGuard<'_, RuntimeEventData>, RuntimeEventError> {
        self.data
            .lock()
            .map_err(|_| RuntimeEventError::SynchronizationPoisoned)
    }
}

/// Cancellation-safe ownership of one pending runtime-event wait.
pub struct RuntimeEventRegistration {
    identity: Option<u64>,
    event: Weak<Mutex<RuntimeEventData>>,
}

impl Drop for RuntimeEventRegistration {
    fn drop(&mut self) {
        let Some(identity) = self.identity.take() else {
            return;
        };

        let Some(event) = self.event.upgrade() else {
            return;
        };

        let mut data = event
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        data.waiters.remove(&identity);
    }
}

fn wake_all(waiters: BTreeMap<u64, Arc<dyn RuntimeEventWake>>) {
    for waiter in waiters.into_values() {
        waiter.wake();
    }
}

#[cfg(test)]
mod tests {
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
}
