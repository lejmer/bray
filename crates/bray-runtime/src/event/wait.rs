use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};

use crate::task::TaskWaitWake;

use super::contract::{RuntimeEventError, RuntimeEventGeneration, RuntimeEventWake};
use super::core::RuntimeEventData;

pub(crate) enum EventNotification {
    Callback(Arc<dyn RuntimeEventWake>),
    Task {
        wake: triomphe::Arc<TaskWaitWake<(usize, RuntimeEventGeneration)>>,
        source: (usize, RuntimeEventGeneration),
    },
}

impl EventNotification {
    pub(super) fn wake(self) {
        match self {
            Self::Callback(wake) => wake.wake(),
            Self::Task { wake, source } => wake.notify(source),
        }
    }
}

/// One admission-owned node, reusable after its registration is withdrawn.
pub(crate) struct ReservedEventWait {
    pub(super) node: triomphe::Arc<EventWaitNode>,
}

pub(super) struct EventWaitNode {
    pub(super) registered: AtomicBool,
    pub(super) pending: Mutex<Option<PendingEventWait>>,
}

pub(super) struct PendingEventWait {
    pub(super) previous: Option<triomphe::Arc<EventWaitNode>>,
    pub(super) next: Option<triomphe::Arc<EventWaitNode>>,
    pub(super) observed: RuntimeEventGeneration,
    pub(super) wake: EventNotification,
}

impl ReservedEventWait {
    pub(crate) fn reserve() -> Result<Self, RuntimeEventError> {
        let node = crate::allocation::allocate_shared(EventWaitNode {
            registered: AtomicBool::new(false),
            pending: Mutex::new(None),
        })
        .map_err(|_| RuntimeEventError::AllocationFailed)?;

        Ok(Self { node })
    }
}

/// Cancellation-safe ownership of one pending runtime-event wait.
pub struct RuntimeEventRegistration {
    pub(super) node: triomphe::Arc<EventWaitNode>,
    pub(super) event: Weak<Mutex<RuntimeEventData>>,
}

impl Drop for RuntimeEventRegistration {
    fn drop(&mut self) {
        let withdrawn = self.event.upgrade().and_then(|event| {
            event
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove_waiter(&self.node)
        });

        self.node.registered.store(false, Ordering::Release);

        // Callback destruction may reenter an event, so release its lock first.
        drop(withdrawn);
    }
}
