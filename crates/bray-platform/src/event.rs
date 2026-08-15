use std::collections::VecDeque;
use std::sync::{Arc, Condvar, Mutex};

#[cfg(feature = "network")]
use mio::Interest;
#[cfg(feature = "network")]
use mio::event::Source;
use mio::{Events, Poll, Token, Waker};

use crate::{MonotonicDeadline, PlatformError, PlatformErrorKind, PlatformOperation};

const HOST_WAKE_TOKEN: Token = Token(usize::MAX);

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
            .map_err(|_| poisoned_event(PlatformOperation::Event))?;

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
            .map_err(|_| poisoned_event(PlatformOperation::Event))?;

        loop {
            if *generation != observed.0 {
                return Ok(NativeWaitOutcome::Woken(WakeObservation(*generation)));
            }

            let Some(deadline) = deadline else {
                generation = self
                    .state
                    .changed
                    .wait(generation)
                    .map_err(|_| poisoned_event(PlatformOperation::Event))?;

                continue;
            };

            if deadline.has_elapsed() {
                return Ok(NativeWaitOutcome::TimedOut(WakeObservation(*generation)));
            }

            let (next_generation, timeout) = self
                .state
                .changed
                .wait_timeout(generation, deadline.remaining())
                .map_err(|_| poisoned_event(PlatformOperation::Event))?;

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
            .map_err(|_| poisoned_event(PlatformOperation::Event))?;

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

/// Caller-owned identity of one event source registered with a native poller.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativePollEvent(usize);

impl NativePollEvent {
    /// Creates a caller-owned stable event identity.
    pub const fn new(value: usize) -> Self {
        Self(value)
    }

    /// Returns the caller-owned event identity.
    pub const fn raw(self) -> usize {
        self.0
    }
}

/// Native readiness requested for one registered I/O source.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NativePollInterest {
    /// Notify when input can be read.
    Readable,
    /// Notify when output can be written.
    Writable,
    /// Notify for either readable or writable readiness.
    ReadableOrWritable,
}

impl NativePollInterest {
    #[cfg(feature = "network")]
    const fn into_mio(self) -> Interest {
        match self {
            Self::Readable => Interest::READABLE,
            Self::Writable => Interest::WRITABLE,
            Self::ReadableOrWritable => Interest::READABLE.add(Interest::WRITABLE),
        }
    }
}

/// One readiness notification returned by a native event poller.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum NativePollReady {
    /// An explicit cross-thread wake was published.
    Woken(NativePollEvent),
    /// A registered operating-system I/O source became ready.
    Io {
        /// Caller-owned source identity.
        event: NativePollEvent,
        /// Whether the source may be read without blocking.
        readable: bool,
        /// Whether the source may be written without blocking.
        writable: bool,
        /// Whether the source reported an error condition.
        error: bool,
        /// Whether the readable side closed.
        read_closed: bool,
        /// Whether the writable side closed.
        write_closed: bool,
    },
}

impl NativePollReady {
    /// Returns the caller-owned source identity.
    pub const fn event(self) -> NativePollEvent {
        match self {
            Self::Woken(event) | Self::Io { event, .. } => event,
        }
    }
}

#[derive(Debug)]
struct PollWakeState {
    pending: Mutex<VecDeque<NativePollEvent>>,
    waker: Waker,
}

/// Transferable registration that wakes one poller with a stable event identity.
#[derive(Clone, Debug)]
pub struct NativeEventRegistration {
    event: NativePollEvent,
    state: Arc<PollWakeState>,
}

impl NativeEventRegistration {
    /// Publishes this registration's event to the poller.
    pub fn wake(&self) -> Result<(), PlatformError> {
        {
            let mut pending = self
                .state
                .pending
                .lock()
                .map_err(|_| poisoned_event(PlatformOperation::EventPoll))?;

            pending.push_back(self.event);
        }

        self.state
            .waker
            .wake()
            .map_err(|error| PlatformError::from_io(PlatformOperation::EventPoll, &error))
    }
}

/// Operating-system event poller integrating I/O readiness, deadlines, and host wakes.
#[derive(Debug)]
pub struct NativeEventPoller {
    poll: Poll,
    events: Events,
    wake_state: Arc<PollWakeState>,
    ready: VecDeque<NativePollReady>,
}

impl NativeEventPoller {
    /// Creates an empty native poller.
    pub fn new() -> Result<Self, PlatformError> {
        let poll = Poll::new()
            .map_err(|error| PlatformError::from_io(PlatformOperation::EventPoll, &error))?;

        let waker = Waker::new(poll.registry(), HOST_WAKE_TOKEN)
            .map_err(|error| PlatformError::from_io(PlatformOperation::EventPoll, &error))?;

        Ok(Self {
            poll,
            events: Events::with_capacity(128),
            wake_state: Arc::new(PollWakeState {
                pending: Mutex::new(VecDeque::new()),
                waker,
            }),
            ready: VecDeque::new(),
        })
    }

    /// Creates a transferable explicit-wake registration.
    pub fn register(&self, event: NativePollEvent) -> NativeEventRegistration {
        NativeEventRegistration {
            event,
            state: Arc::clone(&self.wake_state),
        }
    }

    /// Waits for I/O readiness, an explicit wake, or an optional deadline.
    pub fn wait(
        &mut self,
        deadline: Option<MonotonicDeadline>,
    ) -> Result<Option<NativePollReady>, PlatformError> {
        if let Some(ready) = self.ready.pop_front() {
            return Ok(Some(ready));
        }

        self.events.clear();

        self.poll
            .poll(&mut self.events, deadline.map(MonotonicDeadline::remaining))
            .map_err(|error| PlatformError::from_io(PlatformOperation::EventPoll, &error))?;

        self.collect_ready()?;

        Ok(self.ready.pop_front())
    }

    #[cfg(feature = "network")]
    pub(crate) fn register_source(
        &self,
        source: &mut impl Source,
        event: NativePollEvent,
        interest: NativePollInterest,
    ) -> Result<(), PlatformError> {
        let token = source_token(event)?;

        self.poll
            .registry()
            .register(source, token, interest.into_mio())
            .map_err(|error| PlatformError::from_io(PlatformOperation::EventRegistration, &error))
    }

    #[cfg(feature = "network")]
    pub(crate) fn reregister_source(
        &self,
        source: &mut impl Source,
        event: NativePollEvent,
        interest: NativePollInterest,
    ) -> Result<(), PlatformError> {
        let token = source_token(event)?;

        self.poll
            .registry()
            .reregister(source, token, interest.into_mio())
            .map_err(|error| PlatformError::from_io(PlatformOperation::EventRegistration, &error))
    }

    #[cfg(feature = "network")]
    pub(crate) fn deregister_source(&self, source: &mut impl Source) -> Result<(), PlatformError> {
        self.poll
            .registry()
            .deregister(source)
            .map_err(|error| PlatformError::from_io(PlatformOperation::EventRegistration, &error))
    }

    fn collect_ready(&mut self) -> Result<(), PlatformError> {
        for event in &self.events {
            if event.token() == HOST_WAKE_TOKEN {
                continue;
            }

            self.ready.push_back(NativePollReady::Io {
                event: NativePollEvent::new(event.token().0),
                readable: event.is_readable(),
                writable: event.is_writable(),
                error: event.is_error(),
                read_closed: event.is_read_closed(),
                write_closed: event.is_write_closed(),
            });
        }

        while let Some(event) = self.pop_pending_wake()? {
            self.ready.push_back(NativePollReady::Woken(event));
        }

        Ok(())
    }

    fn pop_pending_wake(&self) -> Result<Option<NativePollEvent>, PlatformError> {
        self.wake_state
            .pending
            .lock()
            .map_err(|_| poisoned_event(PlatformOperation::EventPoll))
            .map(|mut pending| pending.pop_front())
    }
}

#[cfg(feature = "network")]
fn source_token(event: NativePollEvent) -> Result<Token, PlatformError> {
    let token = Token(event.raw());

    if token == HOST_WAKE_TOKEN {
        return Err(PlatformError::new(
            PlatformOperation::EventRegistration,
            PlatformErrorKind::InvalidEventIdentity,
        ));
    }

    Ok(token)
}

fn poisoned_event(operation: PlatformOperation) -> PlatformError {
    PlatformError::new(operation, PlatformErrorKind::SynchronizationPoisoned)
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::Duration;

    use crate::MonotonicClock;

    use super::{
        NativeEvent, NativeEventPoller, NativePollEvent, NativePollReady, NativeWaitOutcome,
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
        let mut poller = NativeEventPoller::new()
            .unwrap_or_else(|error| panic!("poller must initialize: {error:?}"));

        let registration = poller.register(NativePollEvent::new(7));

        let worker = thread::spawn(move || registration.wake());

        let event = poller
            .wait(MonotonicClock.deadline_after(Duration::from_secs(1)))
            .unwrap_or_else(|error| panic!("poll wait must succeed: {error:?}"));

        let wake = worker
            .join()
            .unwrap_or_else(|_| panic!("wake worker must not panic"));

        wake.unwrap_or_else(|error| panic!("poll wake must succeed: {error:?}"));

        assert_eq!(event, Some(NativePollReady::Woken(NativePollEvent::new(7))));
    }

    #[test]
    fn elapsed_deadlines_do_not_hide_queued_wakes() {
        let mut poller = NativeEventPoller::new()
            .unwrap_or_else(|error| panic!("poller must initialize: {error:?}"));

        let registration = poller.register(NativePollEvent::new(11));

        registration
            .wake()
            .unwrap_or_else(|error| panic!("poll wake must succeed: {error:?}"));

        let ready = poller
            .wait(MonotonicClock.deadline_after(Duration::ZERO))
            .unwrap_or_else(|error| panic!("poll wait must succeed: {error:?}"));

        assert_eq!(
            ready,
            Some(NativePollReady::Woken(NativePollEvent::new(11)))
        );
    }
}
