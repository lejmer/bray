use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, Weak};

#[derive(Debug)]
struct CancellationState {
    requested: AtomicUsize,
    shields: AtomicUsize,
    children: Mutex<Vec<Weak<CancellationState>>>,
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
                requested: AtomicUsize::new(0),
                shields: AtomicUsize::new(0),
                children: Mutex::new(Vec::new()),
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

        if self.state.requested.load(Ordering::Acquire) != 0 {
            child.request();
        }

        child
    }

    /// Requests cancellation for this context and every current descendant.
    ///
    /// Returns whether this call changed this context's request state.
    pub fn request(&self) -> bool {
        let changed = self
            .state
            .requested
            .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok();

        let mut pending = vec![Arc::clone(&self.state)];

        while let Some(state) = pending.pop() {
            state.requested.store(1, Ordering::Release);

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

        changed
    }

    /// Returns whether cancellation is currently observable by this context.
    pub fn is_requested(&self) -> bool {
        self.state.requested.load(Ordering::Acquire) != 0
            && self.state.shields.load(Ordering::Acquire) == 0
    }

    /// Temporarily shields cleanup from observing a pending cancellation request.
    pub fn shield(&self) -> CancellationShield {
        self.state.shields.fetch_add(1, Ordering::AcqRel);

        CancellationShield {
            context: self.clone(),
        }
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
    }
}

#[cfg(test)]
mod tests {
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
}
