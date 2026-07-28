use std::collections::BTreeMap;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

struct CancellationState {
    requested: AtomicBool,
    shields: AtomicUsize,
    children: Mutex<Vec<Weak<CancellationState>>>,
    wakes: Mutex<CancellationWakes>,
}

#[derive(Default)]
struct CancellationWakes {
    next_id: u64,
    registered: BTreeMap<u64, Arc<dyn CancellationWake>>,
}

impl fmt::Debug for CancellationState {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CancellationState")
            .field("requested", &self.requested)
            .field("shields", &self.shields)
            .finish_non_exhaustive()
    }
}

/// Shared cancellation state for one structured run or child task.
#[derive(Clone, Debug)]
pub struct CancellationContext {
    state: Arc<CancellationState>,
}

impl CancellationContext {
    /// Creates an uncancelled root context.
    pub fn root() -> Self {
        Self {
            state: Arc::new(CancellationState {
                requested: AtomicBool::new(false),
                shields: AtomicUsize::new(0),
                children: Mutex::new(Vec::new()),
                wakes: Mutex::new(CancellationWakes::default()),
            }),
        }
    }

    /// Creates a child that observes cancellation requested by this context.
    pub fn child(&self) -> Self {
        let child = Self::root();

        let mut children = self
            .state
            .children
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        children.retain(|child| child.strong_count() != 0);
        children.push(Arc::downgrade(&child.state));

        if self.state.requested.load(Ordering::Acquire) {
            child.request();
        }

        child
    }

    /// Requests cancellation for this context and every current descendant.
    ///
    /// Returns whether this call changed this context's request state.
    pub fn request(&self) -> bool {
        let mut changed = false;
        let mut pending = vec![Arc::clone(&self.state)];

        while let Some(state) = pending.pop() {
            let state_changed = state
                .requested
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok();

            if Arc::ptr_eq(&state, &self.state) {
                changed = state_changed;
            }

            collect_children(&state, &mut pending);

            if state_changed {
                notify_if_observable(&state);
            }
        }

        changed
    }

    /// Returns whether cancellation is currently observable by this context.
    pub fn is_requested(&self) -> bool {
        self.state.requested.load(Ordering::Acquire)
            && self.state.shields.load(Ordering::Acquire) == 0
    }

    /// Temporarily shields cleanup from observing a pending cancellation request.
    pub fn shield(&self) -> CancellationShield {
        self.state.shields.fetch_add(1, Ordering::AcqRel);

        CancellationShield {
            context: self.clone(),
        }
    }

    /// Registers a notification that makes suspended work observe cancellation.
    pub(crate) fn register_wake(
        &self,
        wake: Arc<dyn CancellationWake>,
    ) -> Result<CancellationWakeRegistration, CancellationWakeRegistrationError> {
        let id = {
            let mut wakes = self
                .state
                .wakes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            let Some(next_id) = wakes.next_id.checked_add(1) else {
                return Err(CancellationWakeRegistrationError::IdentityExhausted);
            };

            let id = wakes.next_id;

            wakes.next_id = next_id;
            wakes.registered.insert(id, Arc::clone(&wake));

            id
        };

        if self.is_requested() {
            wake.wake();
        }

        Ok(CancellationWakeRegistration {
            id,
            state: Arc::downgrade(&self.state),
        })
    }
}

impl Default for CancellationContext {
    fn default() -> Self {
        Self::root()
    }
}

/// Scoped suppression of cancellation observation during cleanup.
#[derive(Debug)]
pub struct CancellationShield {
    context: CancellationContext,
}

impl Drop for CancellationShield {
    fn drop(&mut self) {
        let previous = self.context.state.shields.fetch_sub(1, Ordering::AcqRel);

        debug_assert!(
            previous != 0,
            "cancellation shield count must not underflow"
        );

        if previous == 1 {
            notify_if_observable(&self.context.state);
        }
    }
}

/// Infallible notification used when cancellation becomes observable.
pub(crate) trait CancellationWake: Send + Sync + 'static {
    /// Makes suspended work runnable so it can observe cancellation.
    fn wake(&self);
}

impl<F> CancellationWake for F
where
    F: Fn() + Send + Sync + 'static,
{
    fn wake(&self) {
        self();
    }
}

/// Failure to register a cancellation wake.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CancellationWakeRegistrationError {
    /// Cancellation-wake identities were exhausted.
    IdentityExhausted,
}

/// Registration whose lifetime keeps one cancellation wake active.
#[derive(Debug)]
pub(crate) struct CancellationWakeRegistration {
    id: u64,
    state: Weak<CancellationState>,
}

impl Drop for CancellationWakeRegistration {
    fn drop(&mut self) {
        let Some(state) = self.state.upgrade() else {
            return;
        };

        let mut wakes = state
            .wakes
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        wakes.registered.remove(&self.id);
    }
}

fn collect_children(state: &CancellationState, pending: &mut Vec<Arc<CancellationState>>) {
    let mut children = state
        .children
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    children.retain(|child| {
        if let Some(child) = child.upgrade() {
            pending.push(child);

            true
        } else {
            false
        }
    });
}

fn notify_if_observable(state: &CancellationState) {
    if !state.requested.load(Ordering::Acquire) || state.shields.load(Ordering::Acquire) != 0 {
        return;
    }

    let wakes: Vec<_> = state
        .wakes
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .registered
        .values()
        .cloned()
        .collect();

    for wake in wakes {
        wake.wake();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::CancellationContext;

    #[test]
    fn cancellation_propagates_to_existing_and_later_children() {
        let root = CancellationContext::root();
        let child = root.child();
        let grandchild = child.child();

        assert!(root.request());
        assert!(!root.request());

        assert!(child.is_requested());
        assert!(grandchild.is_requested());
        assert!(root.child().is_requested());
    }

    #[test]
    fn cleanup_shields_delay_observation_without_losing_the_request() {
        let context = CancellationContext::root();
        let shield = context.shield();

        context.request();

        assert!(!context.is_requested());

        drop(shield);

        assert!(context.is_requested());
    }

    #[test]
    fn cancellation_wakes_are_delayed_by_shields_and_removed_with_registration() {
        let context = CancellationContext::root();
        let shield = context.shield();
        let wake_count = Arc::new(AtomicUsize::new(0));
        let observed_count = Arc::clone(&wake_count);

        let registration = context
            .register_wake(Arc::new(move || {
                observed_count.fetch_add(1, Ordering::Relaxed);
            }))
            .unwrap_or_else(|error| panic!("cancellation wake must register: {error:?}"));

        context.request();

        assert_eq!(wake_count.load(Ordering::Relaxed), 0);

        drop(shield);

        assert_eq!(wake_count.load(Ordering::Relaxed), 1);

        drop(registration);
        context.request();

        assert_eq!(wake_count.load(Ordering::Relaxed), 1);
    }
}
