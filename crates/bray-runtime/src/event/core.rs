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
            let mut data = self.lock_data()?;

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
    pub fn signal(&self) -> Result<RuntimeEventGeneration, RuntimeEventError> {
        let generation = {
            let mut data = self.lock_data()?;

            if data.closed {
                return Ok(RuntimeEventGeneration(data.generation));
            }

            let generation = data
                .generation
                .checked_add(1)
                .ok_or(RuntimeEventError::GenerationExhausted)?;

            data.generation = generation;

            RuntimeEventGeneration(generation)
        };

        self.wake_before(Some(generation))?;

        Ok(generation)
    }

    /// Closes the event and wakes every current waiter once.
    ///
    /// Returns whether this call performed the open-to-closed transition.
    pub fn close(&self) -> Result<bool, RuntimeEventError> {
        {
            let mut data = self.lock_data()?;

            if data.closed {
                return Ok(false);
            }

            data.closed = true;
        }

        self.wake_before(None)?;

        Ok(true)
    }

    fn wake_before(
        &self,
        generation: Option<RuntimeEventGeneration>,
    ) -> Result<(), RuntimeEventError> {
        loop {
            let wake = {
                let mut data = self.lock_data()?;

                let Some(first) = data.first.as_ref().map(triomphe::Arc::clone) else {
                    return Ok(());
                };

                let observed = first
                    .pending
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .as_ref()
                    .map(|pending| pending.observed);

                if generation.is_some_and(|generation| observed >= Some(generation)) {
                    return Ok(());
                }

                data.remove_waiter(&first)
            };

            if let Some(wake) = wake {
                wake.wake();
            }
        }
    }

    fn lock_data(&self) -> Result<std::sync::MutexGuard<'_, RuntimeEventData>, RuntimeEventError> {
        self.data
            .lock()
            .map_err(|_| RuntimeEventError::SynchronizationPoisoned)
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
