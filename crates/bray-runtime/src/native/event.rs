use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, RwLock, Weak};

use bray_runtime_abi::NativeRuntimeStatus;

use crate::RuntimeEvent;

use super::state::NativeRuntimeCore;

// Native threads signal events without a runtime thread attachment. This registry resolves
// opaque integer handles without exposing Rust addresses across the native ABI boundary.
static NEXT_EVENT: AtomicUsize = AtomicUsize::new(1);
static EVENTS: LazyLock<RwLock<HashMap<usize, EventEntry>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

struct EventEntry {
    owner: Weak<NativeRuntimeCore>,
    event: RuntimeEvent,
}

pub(super) fn create(owner: &Arc<NativeRuntimeCore>) -> usize {
    let identity = NEXT_EVENT.try_update(Ordering::Relaxed, Ordering::Relaxed, |identity| {
        identity.checked_add(1)
    });

    let Ok(identity) = identity else {
        return 0;
    };

    EVENTS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            identity,
            EventEntry {
                owner: Arc::downgrade(owner),
                event: RuntimeEvent::new(),
            },
        );

    identity
}

pub(super) fn event(owner: &Arc<NativeRuntimeCore>, identity: usize) -> Option<RuntimeEvent> {
    let events = EVENTS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let entry = events.get(&identity)?;

    Weak::ptr_eq(&entry.owner, &Arc::downgrade(owner)).then(|| entry.event.clone())
}

pub(super) fn signal(identity: usize) -> NativeRuntimeStatus {
    let event = EVENTS
        .read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&identity)
        .map(|entry| entry.event.clone());

    let Some(event) = event else {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    };

    event
        .close()
        .map_or(NativeRuntimeStatus::RUNTIME_FAILURE, |_| {
            NativeRuntimeStatus::SUCCESS
        })
}

pub(super) fn destroy(owner: &Arc<NativeRuntimeCore>, identity: usize) -> NativeRuntimeStatus {
    let mut events = EVENTS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let Some(entry) = events.get(&identity) else {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    };

    if !Weak::ptr_eq(&entry.owner, &Arc::downgrade(owner)) {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    }

    events.remove(&identity);

    NativeRuntimeStatus::SUCCESS
}

pub(super) fn clear(owner: &NativeRuntimeCore) {
    EVENTS
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .retain(|_, entry| {
            entry
                .owner
                .upgrade()
                .is_some_and(|entry_owner| !std::ptr::eq(Arc::as_ptr(&entry_owner), owner))
        });
}
