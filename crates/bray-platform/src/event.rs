use std::sync::{
    Arc, Condvar, Mutex,
    mpsc::{self, Receiver, RecvTimeoutError, Sender},
};

use crate::{
    MonotonicDeadline, PlatformError, PlatformErrorKind, PlatformOperation,
};

#[derive(Debug)]
struct EventState {
    generation: Mutex<u64>,
    changed: Condvar,
}

/// Stable observation used to avoid losing a wake between checking and waiting.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WakeObservation(u64);

/// Result of waiting for one native event.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NativeWaitOutcome {
    /// The event generation changed.
    Woken(WakeObservation),
    /// The supplied deadline elapsed first.
    TimedOut(WakeObservation),
}

/// Waitable process-local native event.
#[derive(Debug)]
pub struct NativeEvent {
    state: Arc<EventState>,
}

impl NativeEvent {
    /// Creates an unsignalled event.
    pub fn new() -> Self {
        Self {
            state: Arc::new(EventState {
                generation: Mutex::new(0),
                changed: Condvar::new(),
            }),
        }
    }

    /// Returns a transferable wake handle for this event.
    pub fn wake_handle(&self) -> NativeEventWakeHandle {
        NativeEventWakeHandle {
            state: Arc::clone(&self.state),
        }
    }

    /// Observes the current wake generation.
    pub fn observe(&self) -> Result<WakeObservation, PlatformError> {
        let generation = self
            .state
            .generation
            .lock()
            .map_err(|_| poisoned_event())?;

        Ok(WakeObservation(*generation))
    }

    /// Waits until the generation changes or an optional deadline elapses.
    pub fn wait(
        &self,
        observed: WakeObservation,
        deadline: Option<MonotonicDeadline>,
    ) -> Result<NativeWaitOutcome, PlatformError> {
        let mut generation = self
            .state
            .generation
            .lock()
            .map_err(|_| poisoned_event())?;

        loop {
            if *generation != observed.0 {
                return Ok(NativeWaitOutcome::Woken(WakeObservation(*generation)));
            }

            let Some(deadline) = deadline else {
                generation = self
                    .state
                    .changed
                    .wait(generation)
                    .map_err(|_| poisoned_event())?;

                continue;
            };

            if deadline.has_elapsed() {
                return Ok(NativeWaitOutcome::TimedOut(WakeObservation(*generation)));
            }

            let (next_generation, timeout) = self
                .state
                .changed
                .wait_timeout(generation, deadline.remaining())
                .map_err(|_| poisoned_event())?;

            generation = next_generation;

            if timeout.timed_out() && *generation == observed.0 {
                return Ok(NativeWaitOutcome::TimedOut(WakeObservation(*generation)));
            }
        }
    }
}

impl Default for NativeEvent {
    fn default() -> Self {
        Self::new()
    }
}

/// Transferable authority to wake one native event.
#[derive(Clone, Debug)]
pub struct NativeEventWakeHandle {
    state: Arc<EventState>,
}

impl NativeEventWakeHandle {
    /// Advances the event generation and wakes all current waiters.
    pub fn wake(&self) -> Result<WakeObservation, PlatformError> {
        let mut generation = self
            .state
            .generation
            .lock()
            .map_err(|_| poisoned_event())?;

        *generation = generation.checked_add(1).ok_or_else(|| {
            PlatformError::new(
                PlatformOperation::Event,
                PlatformErrorKind::EventGenerationExhausted,
            )
        })?;

        let observed = WakeObservation(*generation);

        self.state.changed.notify_all();

        Ok(observed)
    }
}

/// Event delivered by a native event poller.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativePollEvent(u64);

impl NativePollEvent {
    /// Creates a caller-owned stable event identity.
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the caller-owned event identity.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

/// Transferable registration that wakes one poller with a stable event identity.
#[derive(Clone, Debug)]
pub struct NativeEventRegistration {
    event: NativePollEvent,
    sender: Sender<NativePollEvent>,
}

impl NativeEventRegistration {
    /// Publishes this registration's event to the poller.
    pub fn wake(&self) -> Result<(), PlatformError> {
        self.sender.send(self.event).map_err(|_| {
            PlatformError::new(
                PlatformOperation::EventPoll,
                PlatformErrorKind::Io(std::io::ErrorKind::BrokenPipe),
            )
        })
    }
}

/// Process-local event poller used to integrate host wakeups and timers.
#[derive(Debug)]
pub struct NativeEventPoller {
    sender: Sender<NativePollEvent>,
    receiver: Receiver<NativePollEvent>,
}

impl NativeEventPoller {
    /// Creates an empty poller.
    pub fn new() -> Self {
        let (sender, receiver) = mpsc::channel();

        Self { sender, receiver }
    }

    /// Creates a transferable registration for one caller-owned identity.
    pub fn register(&self, event: NativePollEvent) -> NativeEventRegistration {
        // Each registration needs independent authority to publish into this poller.
        let sender = self.sender.clone();

        NativeEventRegistration {
            event,
            sender,
        }
    }

    /// Waits for the next event or optional deadline.
    pub fn wait(
        &self,
        deadline: Option<MonotonicDeadline>,
    ) -> Result<Option<NativePollEvent>, PlatformError> {
        match deadline {
            Some(deadline) if deadline.has_elapsed() => Ok(None),
            Some(deadline) => match self.receiver.recv_timeout(deadline.remaining()) {
                Ok(event) => Ok(Some(event)),
                Err(RecvTimeoutError::Timeout) => Ok(None),
                Err(RecvTimeoutError::Disconnected) => Err(disconnected_poller()),
            },
            None => self.receiver.recv().map(Some).map_err(|_| disconnected_poller()),
        }
    }
}

impl Default for NativeEventPoller {
    fn default() -> Self {
        Self::new()
    }
}

fn disconnected_poller() -> PlatformError {
    PlatformError::new(
        PlatformOperation::EventPoll,
        PlatformErrorKind::Io(std::io::ErrorKind::BrokenPipe),
    )
}

fn poisoned_event() -> PlatformError {
    PlatformError::new(
        PlatformOperation::Event,
        PlatformErrorKind::SynchronizationPoisoned,
    )
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;

    use crate::MonotonicClock;

    use super::{
        NativeEvent, NativeEventPoller, NativePollEvent, NativeWaitOutcome,
    };

    #[test]
    fn observations_prevent_lost_wakes() {
        let event = NativeEvent::new();

        let observed = event
            .observe()
            .unwrap_or_else(|error| panic!("event observation must succeed: {error:?}"));

        event
            .wake_handle()
            .wake()
            .unwrap_or_else(|error| panic!("event wake must succeed: {error:?}"));

        let outcome = event
            .wait(observed, None)
            .unwrap_or_else(|error| panic!("observed wake must be available: {error:?}"));

        assert!(matches!(outcome, NativeWaitOutcome::Woken(_)));
    }

    #[test]
    fn poller_delivers_transferable_registrations_and_deadlines() {
        let poller = NativeEventPoller::new();
        let registration = poller.register(NativePollEvent::new(7));

        let worker = thread::spawn(move || registration.wake());

        let event = poller
            .wait(MonotonicClock.deadline_after(Duration::from_secs(1)))
            .unwrap_or_else(|error| panic!("poll wait must succeed: {error:?}"));

        let wake = worker
            .join()
            .unwrap_or_else(|_| panic!("wake worker must not panic"));

        wake.unwrap_or_else(|error| panic!("poll wake must succeed: {error:?}"));

        assert_eq!(event, Some(NativePollEvent::new(7)));
    }
}
