use std::fmt;
#[cfg(test)]
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use triomphe::Arc as StateArc;

use crate::CancellationObservation;

struct CancellationState {
    parent: Option<CancellationParent>,
    contexts: AtomicUsize,
    requested: AtomicBool,
    root_source: AtomicU8,
    shields: AtomicUsize,
    children: Mutex<Vec<Option<StateArc<CancellationState>>>>,
    wakes: Mutex<CancellationWakes>,
}

struct CancellationParent {
    context: CancellationContext,
    slot: usize,
}

impl CancellationState {
    fn new(parent: Option<CancellationParent>, requested: bool) -> Self {
        Self {
            parent,
            contexts: AtomicUsize::new(1),
            requested: AtomicBool::new(requested),
            root_source: AtomicU8::new(0),
            shields: AtomicUsize::new(0),
            children: Mutex::new(Vec::new()),
            wakes: Mutex::new(CancellationWakes::default()),
        }
    }
}

impl Drop for CancellationState {
    fn drop(&mut self) {
        let mut parent = self.parent.take();

        // A descendant retains the traversal path. Release a last-owned path iteratively too,
        // so both cancellation and destruction have bounded call-stack use for deep run trees.
        while let Some(link) = parent {
            let retained = StateArc::clone(&link.context.state);
            drop(link);
            parent = StateArc::into_unique(retained).and_then(|mut state| state.parent.take());
        }
    }
}

#[derive(Default)]
struct CancellationWakes {
    next_id: u64,
    registered: Vec<(u64, Option<CancellationWake>)>,
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
#[derive(Debug)]
pub struct CancellationContext {
    state: StateArc<CancellationState>,
}

/// Failure to admit a root or child cancellation context.
#[derive(Debug)]
pub enum CancellationAdmissionError {
    /// Shared cancellation state could not be allocated.
    StateAllocation(triomphe::AllocError),
    /// The parent's child registry could not grow.
    ChildStorage(std::collections::TryReserveError),
}

impl Clone for CancellationContext {
    fn clone(&self) -> Self {
        // Each logical owner needs addressable storage, so its count cannot reach usize::MAX.
        self.state
            .contexts
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |count| {
                count.checked_add(1)
            })
            .expect("cancellation context count must not overflow");

        Self {
            state: StateArc::clone(&self.state),
        }
    }
}

impl Drop for CancellationContext {
    fn drop(&mut self) {
        if self.state.contexts.fetch_sub(1, Ordering::AcqRel) != 1 {
            return;
        }

        // Tree slots retain storage only. Descendants retain logical parent contexts, so the
        // last context can unlink itself without a parent/child ownership cycle.
        if let Some(parent) = &self.state.parent {
            let removed = parent
                .context
                .state
                .children
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .get_mut(parent.slot)
                .and_then(Option::take);

            drop(removed);
        }

        let removed = std::mem::take(
            &mut self
                .state
                .wakes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .registered,
        );

        drop(removed);
    }
}

impl CancellationContext {
    /// Creates an uncancelled root context, reporting allocation failure before admission.
    pub fn root() -> Result<Self, CancellationAdmissionError> {
        Ok(Self {
            state: reserve_state(CancellationState::new(None, false))?,
        })
    }

    /// Creates a child that observes cancellation requested by this context.
    pub fn child(&self) -> Result<Self, CancellationAdmissionError> {
        let mut children = self
            .state
            .children
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let slot = children
            .iter()
            .position(Option::is_none)
            .unwrap_or(children.len());

        if slot == children.len() {
            crate::allocation::reserve_vec_entries(&mut children, 1)
                .map_err(CancellationAdmissionError::ChildStorage)?;
        }

        let child = Self {
            state: reserve_state(CancellationState::new(
                Some(CancellationParent {
                    context: self.clone(),
                    slot,
                }),
                self.state.requested.load(Ordering::Acquire),
            ))?,
        };

        let retained = Some(StateArc::clone(&child.state));

        if let Some(vacant) = children.get_mut(slot) {
            *vacant = retained;
        } else {
            children.push(retained);
        }

        Ok(child)
    }

    /// Requests cancellation for this context and every current descendant.
    ///
    /// Returns whether this call changed this context's request state.
    pub fn request(&self) -> bool {
        let mut changed = false;
        let mut state = StateArc::clone(&self.state);

        loop {
            let state_changed = state
                .requested
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok();

            if StateArc::ptr_eq(&state, &self.state) {
                changed = state_changed;
            }

            if state_changed {
                notify_if_observable(&state);
            }

            let Some(next) = following_descendant(state, &self.state) else {
                break;
            };

            state = next;
        }

        changed
    }

    /// Returns whether cancellation is currently observable by this context.
    pub fn is_requested(&self) -> bool {
        self.state.requested.load(Ordering::Acquire)
            && self.state.shields.load(Ordering::Acquire) == 0
    }

    pub(crate) fn root_source(&self) -> Option<crate::RootCancellationSource> {
        crate::RootCancellationSource::from_code(self.state.root_source.load(Ordering::Acquire))
    }

    pub(crate) fn request_root(&self, source: crate::RootCancellationSource) -> bool {
        let _ = self.state.root_source.compare_exchange(
            0,
            source.code(),
            Ordering::AcqRel,
            Ordering::Acquire,
        );

        self.request()
    }

    /// Captures pending and currently observable cancellation state.
    pub fn observation(&self) -> CancellationObservation {
        let requested = self.state.requested.load(Ordering::Acquire);

        let observable = requested && self.state.shields.load(Ordering::Acquire) == 0;

        CancellationObservation::new(requested, observable)
    }

    /// Temporarily shields cleanup from observing a pending cancellation request.
    pub fn shield(&self) -> CancellationShield {
        self.enter_shield();

        CancellationShield {
            context: self.clone(),
        }
    }

    pub(crate) fn enter_shield(&self) {
        // Each shield needs a live call frame or owned guard, so the count cannot reach usize::MAX.
        self.state
            .shields
            .try_update(Ordering::AcqRel, Ordering::Acquire, |depth| {
                depth.checked_add(1)
            })
            .expect("cancellation shield count must not overflow");
    }

    pub(crate) fn leave_shield(&self) {
        // Only a paired generated leave or an owned CancellationShield can release a shield.
        let previous = self
            .state
            .shields
            .try_update(Ordering::AcqRel, Ordering::Acquire, |depth| {
                depth.checked_sub(1)
            })
            .expect("cancellation shield count must not underflow");

        if previous == 1 {
            notify_if_observable(&self.state);
        }
    }

    /// Registers a notification that makes suspended work observe cancellation.
    #[cfg(test)]
    pub(crate) fn register_wake(
        &self,
        wake: impl Into<CancellationWake>,
    ) -> Result<CancellationWakeRegistration, CancellationWakeRegistrationError> {
        let mut registration = self.reserve_wake()?;
        registration.bind(wake);

        Ok(registration)
    }

    /// Retains a notification slot and identity before its execution target is known.
    pub(crate) fn reserve_wake(
        &self,
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

            crate::allocation::reserve_vec_entries(&mut wakes.registered, 1)
                .map_err(CancellationWakeRegistrationError::Allocation)?;

            wakes.next_id = next_id;
            wakes.registered.push((id, None));

            id
        };

        Ok(CancellationWakeRegistration {
            id,
            state: StateArc::clone(&self.state),
        })
    }
}

fn reserve_state(
    state: CancellationState,
) -> Result<StateArc<CancellationState>, CancellationAdmissionError> {
    crate::allocation::allocate_shared(state).map_err(CancellationAdmissionError::StateAllocation)
}

/// Scoped suppression of cancellation observation during cleanup.
#[derive(Debug)]
pub struct CancellationShield {
    context: CancellationContext,
}

impl Drop for CancellationShield {
    fn drop(&mut self) {
        self.context.leave_shield();
    }
}

/// Retained notification used when cancellation becomes observable.
#[derive(Clone)]
pub(crate) enum CancellationWake {
    Task(crate::TaskWakeHandle),
    #[cfg(test)]
    Callback(Arc<dyn Fn() + Send + Sync>),
}

impl CancellationWake {
    fn wake(&self) {
        match self {
            Self::Task(task) => task.wake_cancellation(),
            #[cfg(test)]
            Self::Callback(callback) => callback(),
        }
    }
}

impl From<crate::TaskWakeHandle> for CancellationWake {
    fn from(task: crate::TaskWakeHandle) -> Self {
        Self::Task(task)
    }
}

#[cfg(test)]
impl<F: Fn() + Send + Sync + 'static> From<Arc<F>> for CancellationWake {
    fn from(callback: Arc<F>) -> Self {
        Self::Callback(callback)
    }
}

/// Failure to register a cancellation wake.
#[derive(Debug)]
pub(crate) enum CancellationWakeRegistrationError {
    /// Cancellation-wake identities were exhausted.
    IdentityExhausted,
    /// The notification registry could not grow.
    Allocation(std::collections::TryReserveError),
}

/// Registration whose lifetime keeps one cancellation wake active.
#[derive(Debug)]
pub(crate) struct CancellationWakeRegistration {
    id: u64,
    state: StateArc<CancellationState>,
}

impl CancellationWakeRegistration {
    pub(crate) fn bind(&mut self, wake: impl Into<CancellationWake>) {
        let wake = wake.into();

        {
            let mut wakes = self
                .state
                .wakes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            let Ok(index) = wakes
                .registered
                .binary_search_by_key(&self.id, |(id, _)| *id)
            else {
                unreachable!("reserved cancellation wake must retain its slot");
            };

            let retained = &mut wakes.registered[index].1;

            assert!(
                retained.is_none(),
                "cancellation wake must bind exactly once"
            );

            *retained = Some(wake.clone());
        }

        // A request may precede binding or race with it. Publication precedes this observation,
        // so either the requester or this branch delivers the wake, and duplicate wakes are safe.
        if self.state.requested.load(Ordering::Acquire)
            && self.state.shields.load(Ordering::Acquire) == 0
        {
            wake.wake();
        }
    }
}

impl Drop for CancellationWakeRegistration {
    fn drop(&mut self) {
        let removed = {
            let mut wakes = self
                .state
                .wakes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            wakes
                .registered
                .binary_search_by_key(&self.id, |(id, _)| *id)
                .ok()
                .map(|index| wakes.registered.remove(index))
        };

        drop(removed);
    }
}

fn next_child(state: &CancellationState, first: usize) -> Option<StateArc<CancellationState>> {
    state
        .children
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .skip(first)
        .find_map(|child| child.as_ref().map(StateArc::clone))
}

fn following_descendant(
    mut state: StateArc<CancellationState>,
    root: &StateArc<CancellationState>,
) -> Option<StateArc<CancellationState>> {
    if let Some(child) = next_child(&state, 0) {
        return Some(child);
    }

    while !StateArc::ptr_eq(&state, root) {
        let parent = state.parent.as_ref()?;
        let next = next_child(&parent.context.state, parent.slot + 1);

        if next.is_some() {
            return next;
        }

        state = StateArc::clone(&parent.context.state);
    }

    None
}

fn notify_if_observable(state: &CancellationState) {
    if !state.requested.load(Ordering::Acquire) || state.shields.load(Ordering::Acquire) != 0 {
        return;
    }

    let limit = state
        .wakes
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .next_id;

    let mut first = 0;

    loop {
        let next = {
            let wakes = state
                .wakes
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            let index = wakes.registered.partition_point(|(id, _)| *id < first);

            wakes
                .registered
                .get(index)
                .filter(|(id, _)| *id < limit)
                .cloned()
        };

        let Some((id, wake)) = next else {
            break;
        };

        // Admission reserves the successor identity before publishing each registration.
        first = id + 1;

        if let Some(wake) = wake {
            wake.wake();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::CancellationContext;

    #[test]
    fn reserved_notification_binds_after_cancellation_without_allocating() {
        for requested_before_binding in [false, true] {
            let context = CancellationContext::root().unwrap();
            let count = Arc::new(AtomicUsize::new(0));
            let observed = Arc::clone(&count);

            let callback = Arc::new(move || {
                observed.fetch_add(1, Ordering::SeqCst);
            });

            let mut registration = context.reserve_wake().unwrap();

            crate::test_support::with_allocation_failure(|| {
                if requested_before_binding {
                    assert!(context.request());
                    assert_eq!(count.load(Ordering::SeqCst), 0);
                }

                registration.bind(callback);

                if !requested_before_binding {
                    assert_eq!(count.load(Ordering::SeqCst), 0);
                    assert!(context.request());
                }

                assert_eq!(count.load(Ordering::SeqCst), 1);
                drop(registration);
            });

            assert!(context.state.wakes.lock().unwrap().registered.is_empty());
        }
    }

    #[test]
    fn reserved_notification_observes_shield_and_unbound_slots_do_not_stop_delivery() {
        let context = CancellationContext::root().unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let observed = Arc::clone(&count);

        let callback = Arc::new(move || {
            observed.fetch_add(1, Ordering::SeqCst);
        });

        let unbound = context.reserve_wake().unwrap();
        let mut registration = context.reserve_wake().unwrap();
        let shield = context.shield();
        context.request();

        crate::test_support::with_allocation_failure(|| {
            registration.bind(callback);
            assert_eq!(count.load(Ordering::SeqCst), 0);
            drop(shield);
            assert_eq!(count.load(Ordering::SeqCst), 1);
            drop(unbound);
            drop(registration);
        });
    }

    #[test]
    fn failed_root_and_child_admission_preserve_existing_contexts_and_allow_retry() {
        use super::CancellationAdmissionError;
        use crate::test_support::{with_allocation_failure, with_allocation_failure_after};

        assert!(matches!(
            with_allocation_failure(CancellationContext::root),
            Err(CancellationAdmissionError::StateAllocation(_))
        ));

        for successful in 0..2 {
            let root = CancellationContext::root().unwrap();
            let result = with_allocation_failure_after(successful, || root.child());

            match successful {
                0 => assert!(matches!(
                    result,
                    Err(CancellationAdmissionError::ChildStorage(_))
                )),
                1 => assert!(matches!(
                    result,
                    Err(CancellationAdmissionError::StateAllocation(_))
                )),
                _ => unreachable!(),
            }

            assert_eq!(root.state.contexts.load(Ordering::Acquire), 1);
            assert!(root.state.children.lock().unwrap().is_empty());

            let retained = root.child().unwrap();
            let vacant = root.child().unwrap();
            drop(vacant);
            let result = with_allocation_failure(|| root.child());

            assert!(matches!(
                result,
                Err(CancellationAdmissionError::StateAllocation(_))
            ));

            assert_eq!(root.state.contexts.load(Ordering::Acquire), 2);

            assert!(
                root.state
                    .children
                    .lock()
                    .unwrap()
                    .get(1)
                    .unwrap()
                    .is_none()
            );

            root.request();
            assert!(retained.is_requested());
            let retried = root.child().unwrap();
            assert!(retried.is_requested());
            assert_eq!(retried.state.parent.as_ref().unwrap().slot, 1);
        }
    }

    #[test]
    fn notification_storage_does_not_keep_a_dead_child_registered_or_remove_its_replacement() {
        let root = CancellationContext::root().unwrap();
        let child = root.child().unwrap();
        let notification = child.register_wake(Arc::new(|| {})).unwrap();
        drop(child);

        assert!(
            root.state
                .children
                .lock()
                .unwrap()
                .get(0)
                .unwrap()
                .is_none()
        );

        assert_eq!(notification.state.contexts.load(Ordering::Acquire), 0);

        assert!(
            notification
                .state
                .wakes
                .lock()
                .unwrap()
                .registered
                .is_empty()
        );

        let replacement = root.child().unwrap();
        assert_eq!(replacement.state.parent.as_ref().unwrap().slot, 0);
        drop(notification);
        root.request();
        assert!(replacement.is_requested());
        drop(replacement);
        assert_eq!(root.state.contexts.load(Ordering::Acquire), 1);
        assert_eq!(triomphe::Arc::strong_count(&root.state), 1);
    }

    #[test]
    fn callback_destruction_can_release_the_last_context_and_reenter_cancellation() {
        struct ReenterOnDrop {
            context: CancellationContext,
            dropped: Arc<AtomicUsize>,
        }

        impl Drop for ReenterOnDrop {
            fn drop(&mut self) {
                self.context.request();
                self.dropped.fetch_add(1, Ordering::Relaxed);
            }
        }

        for rejected in [false, true] {
            let context = CancellationContext::root().unwrap();
            let dropped = Arc::new(AtomicUsize::new(0));

            let guard = ReenterOnDrop {
                context: context.clone(),
                dropped: Arc::clone(&dropped),
            };

            let callback = Arc::new(move || {
                let _ = &guard;
            });

            if rejected {
                let result = crate::test_support::with_allocation_failure(|| {
                    context.register_wake(callback)
                });

                assert!(matches!(
                    result,
                    Err(super::CancellationWakeRegistrationError::Allocation(_))
                ));

                assert_eq!(context.state.wakes.lock().unwrap().next_id, 0);
            } else {
                let registration = context.register_wake(callback).unwrap();
                let retained = triomphe::Arc::clone(&context.state);
                drop(context);
                drop(registration);
                assert_eq!(retained.contexts.load(Ordering::Acquire), 0);
                assert_eq!(triomphe::Arc::strong_count(&retained), 1);
            }

            assert_eq!(dropped.load(Ordering::Relaxed), 1);
        }
    }

    #[test]
    fn concurrent_cancellation_child_admission_and_release_preserve_the_tree() {
        let root = CancellationContext::root().unwrap();
        let start = std::sync::Barrier::new(4);

        std::thread::scope(|scope| {
            for _ in 0..2 {
                scope.spawn(|| {
                    start.wait();

                    for _ in 0..200 {
                        let child = root.child().unwrap();
                        let grandchild = child.child().unwrap();
                        drop(child);
                        root.request();
                        assert!(grandchild.is_requested());
                    }
                });

                scope.spawn(|| {
                    start.wait();

                    for _ in 0..200 {
                        root.request();
                    }
                });
            }
        });

        assert!(root.is_requested());
        assert_eq!(root.state.contexts.load(Ordering::Acquire), 1);
        assert_eq!(triomphe::Arc::strong_count(&root.state), 1);

        assert!(
            root.state
                .children
                .lock()
                .unwrap()
                .iter()
                .all(Option::is_none)
        );
    }

    #[test]
    fn wake_admission_failure_preserves_existing_notifications_and_reuses_capacity() {
        let context = CancellationContext::root().unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let mut registrations = Vec::new();

        loop {
            let count = Arc::clone(&count);

            registrations.push(
                context
                    .register_wake(Arc::new(move || {
                        count.fetch_add(1, Ordering::Relaxed);
                    }))
                    .unwrap(),
            );

            let wakes = context.state.wakes.lock().unwrap();

            if wakes.registered.len() == wakes.registered.capacity() {
                break;
            }
        }

        let next_id = context.state.wakes.lock().unwrap().next_id;

        assert!(
            crate::test_support::with_allocation_failure(|| context.register_wake(Arc::new(|| {})))
                .is_err()
        );

        assert_eq!(context.state.wakes.lock().unwrap().next_id, next_id);
        let expected = registrations.len() - 1;
        drop(registrations.pop());

        crate::test_support::with_allocation_failure(|| {
            let replacement = context.register_wake(Arc::new(|| {})).unwrap();
            context.request();
            assert_eq!(count.load(Ordering::Relaxed), expected);
            drop(replacement);
        });
    }

    #[test]
    fn cancellation_propagates_to_existing_and_later_children() {
        let root = CancellationContext::root().unwrap();
        let child = root.child().unwrap();
        let grandchild = child.child().unwrap();

        assert!(root.request());
        assert!(!root.request());

        assert!(child.is_requested());
        assert!(grandchild.is_requested());
        assert!(root.child().unwrap().is_requested());
    }

    #[test]
    fn deep_cancellation_and_ancestor_release_use_bounded_call_stack() {
        std::thread::Builder::new()
            .stack_size(128 * 1024)
            .spawn(|| {
                let root = CancellationContext::root().unwrap();
                let mut leaf = root.clone();

                for _ in 0..20_000 {
                    leaf = leaf.child().unwrap();
                }

                crate::test_support::with_allocation_failure(|| {
                    assert!(root.request());
                    assert!(leaf.is_requested());
                });

                drop(root);
                drop(leaf);
            })
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn descendants_preserve_ancestry_and_vacant_sibling_slots_are_reused() {
        let root = CancellationContext::root().unwrap();
        let retained_root = triomphe::Arc::clone(&root.state);
        let first = root.child().unwrap();
        let grandchild = first.child().unwrap();
        let removed = root.child().unwrap();
        let last = root.child().unwrap();

        drop(first);
        drop(removed);

        let replacement = root.child().unwrap();

        assert_eq!(replacement.state.parent.as_ref().unwrap().slot, 1);
        assert_eq!(root.state.children.lock().unwrap().len(), 3);
        assert!(root.request());
        assert!(grandchild.is_requested());
        assert!(replacement.is_requested());
        assert!(last.is_requested());

        drop(root);

        assert!(retained_root.contexts.load(Ordering::Acquire) > 0);

        drop(grandchild);
        drop(replacement);
        drop(last);

        assert_eq!(retained_root.contexts.load(Ordering::Acquire), 0);
        assert_eq!(triomphe::Arc::strong_count(&retained_root), 1);

        assert!(
            retained_root
                .children
                .lock()
                .unwrap()
                .iter()
                .all(Option::is_none)
        );
    }

    #[test]
    fn cancellation_callbacks_can_withdraw_and_register_notifications_without_snapshot_allocation()
    {
        let context = CancellationContext::root().unwrap();
        let first_count = Arc::new(AtomicUsize::new(0));
        let removed_count = Arc::new(AtomicUsize::new(0));
        let added_count = Arc::new(AtomicUsize::new(0));
        let removed = Arc::new(std::sync::Mutex::new(None));
        let added = Arc::new(std::sync::Mutex::new(None));

        let first = context
            .register_wake(Arc::new({
                let context = context.clone();
                let first_count = Arc::clone(&first_count);
                let added_count = Arc::clone(&added_count);
                let removed = Arc::clone(&removed);
                let added = Arc::clone(&added);

                move || {
                    first_count.fetch_add(1, Ordering::Relaxed);
                    removed.lock().unwrap().take();

                    let count = Arc::clone(&added_count);

                    let registration = context
                        .register_wake(Arc::new(move || {
                            count.fetch_add(1, Ordering::Relaxed);
                        }))
                        .unwrap();

                    *added.lock().unwrap() = Some(registration);
                    assert!(!context.request());
                }
            }))
            .unwrap();

        *removed.lock().unwrap() = Some(
            context
                .register_wake(Arc::new({
                    let count = Arc::clone(&removed_count);

                    move || {
                        count.fetch_add(1, Ordering::Relaxed);
                    }
                }))
                .unwrap(),
        );

        assert!(context.request());
        assert_eq!(first_count.load(Ordering::Relaxed), 1);
        assert_eq!(removed_count.load(Ordering::Relaxed), 0);
        assert_eq!(added_count.load(Ordering::Relaxed), 1);

        drop(first);
    }

    #[test]
    fn cleanup_shields_delay_observation_without_losing_the_request() {
        let context = CancellationContext::root().unwrap();
        let shield = context.shield();

        context.request();

        assert!(!context.is_requested());

        drop(shield);

        assert!(context.is_requested());
    }

    #[test]
    fn cancellation_wakes_are_delayed_by_shields_and_removed_with_registration() {
        let context = CancellationContext::root().unwrap();
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
