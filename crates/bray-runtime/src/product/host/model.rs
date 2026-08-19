use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use bray_runtime_abi::{
    NativeProductHostDescriptor, NativeProductHostObservation, NativeProductHostState,
    NativeProductHostStatus, NativeProductIdentity, NativeRuntimeStatus,
    NativeStaticCleanupCallback, NativeStaticDuration, NativeStaticFinalizer, NativeStaticIdentity,
    NativeStaticTransitionCallback,
};

pub(super) const MAXIMUM_STATIC_ENTRIES: usize = 1_000_000;

// Loaded products share one process registry so archive and shared-library hosts coordinate with
// exact-thread attachments owned by the same runtime.
pub(super) static PRODUCT_HOSTS: OnceLock<Mutex<BTreeMap<usize, ProductHost>>> = OnceLock::new();

thread_local! {
    pub(super) static THREAD_STATICS: RefCell<ThreadStaticRegistry> =
        const { RefCell::new(ThreadStaticRegistry::new()) };
}

#[derive(Clone, Copy)]
pub(super) struct ProductStatic {
    pub(super) identity: NativeStaticIdentity,
    pub(super) duration: NativeStaticDuration,
    pub(super) order: u64,
    pub(super) prepare: NativeStaticTransitionCallback,
    pub(super) finalizer: NativeStaticFinalizer,
    pub(super) destroy: NativeStaticCleanupCallback,
    pub(super) detach: NativeStaticTransitionCallback,
}

pub(super) struct ProductHost {
    pub(super) identity: NativeProductIdentity,
    pub(super) runtime: crate::native::RetainedRuntime,
    pub(super) state: NativeProductHostState,
    pub(super) active_entries: usize,
    pub(super) external_roots: usize,
    pub(super) thread_attachments: usize,
    pub(super) worker_attachments: usize,
    pub(super) initialized_statics: usize,
    pub(super) cleaned_statics: usize,
    pub(super) cleanup_incidents: usize,
    pub(super) last_incident: NativeStaticIdentity,
    pub(super) cleanup_running: bool,
    pub(super) cleanup_blocked: bool,
    pub(super) statics: Vec<ProductStatic>,
}

impl ProductHost {
    pub(super) fn observation(
        &self,
        status: NativeProductHostStatus,
    ) -> NativeProductHostObservation {
        NativeProductHostObservation::new(
            status,
            self.state,
            self.active_entries,
            self.external_roots,
            self.thread_attachments,
            self.initialized_statics,
            self.cleaned_statics,
            self.cleanup_incidents,
            self.last_incident,
        )
    }

    pub(super) const fn is_quiescent(&self) -> bool {
        self.active_entries == 0 && self.external_roots == 0 && self.thread_attachments == 0
    }

    pub(super) fn product_cleanups(&self) -> Vec<ProductStatic> {
        self.statics
            .iter()
            .copied()
            .filter(|entry| entry.duration == NativeStaticDuration::PRODUCT)
            .collect()
    }

    pub(super) fn static_entry(&self, identity: NativeStaticIdentity) -> Option<ProductStatic> {
        self.statics
            .iter()
            .copied()
            .find(|entry| entry.identity == identity)
    }
}

pub(super) struct PendingCleanup {
    pub(super) product: usize,
    pub(super) runtime: crate::native::RetainedRuntime,
    pub(super) statics: Vec<ProductStatic>,
}

#[derive(Clone, Copy)]
pub(super) struct ThreadStaticEntry {
    pub(super) product: usize,
    pub(super) product_identity: NativeProductIdentity,
    pub(super) static_identity: NativeStaticIdentity,
    pub(super) order: u64,
    pub(super) prepare: NativeStaticTransitionCallback,
    pub(super) finalizer: NativeStaticFinalizer,
    pub(super) destroy: NativeStaticCleanupCallback,
    pub(super) detach: NativeStaticTransitionCallback,
}

#[derive(Clone, Copy)]
pub(super) struct ThreadProductAttachment {
    pub(super) identity: u64,
    pub(super) acquired: bool,
    pub(super) worker: bool,
}

pub(super) struct ThreadStaticRegistry {
    pub(super) entries: Vec<ThreadStaticEntry>,
    pub(super) products: BTreeMap<usize, ThreadProductAttachment>,
    pub(super) callback_registered: bool,
    pub(super) next_identity: u64,
}

impl ThreadStaticRegistry {
    pub(super) const fn new() -> Self {
        Self {
            entries: Vec::new(),
            products: BTreeMap::new(),
            callback_registered: false,
            next_identity: 1,
        }
    }

    pub(super) fn attachment(
        &mut self,
        product: usize,
        worker: bool,
    ) -> Option<ThreadProductAttachment> {
        if let Some(attachment) = self.products.get(&product) {
            return Some(*attachment);
        }

        let identity = self.next_identity;

        if identity == 0 || identity == u64::MAX {
            return None;
        }

        self.next_identity = identity.checked_add(1).unwrap_or(0);

        let attachment = ThreadProductAttachment {
            identity,
            acquired: false,
            worker,
        };

        self.products.insert(product, attachment);

        Some(attachment)
    }

    pub(super) fn ensure_exit_callback(&mut self) -> bool {
        if self.callback_registered {
            return true;
        }

        if !bray_platform::register_runtime_thread_exit_callback(
            super::thread::drain_thread_statics,
        ) {
            return false;
        }

        self.callback_registered = true;

        true
    }
}

pub(super) fn product_hosts() -> &'static Mutex<BTreeMap<usize, ProductHost>> {
    PRODUCT_HOSTS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(super) fn product_key(descriptor: &NativeProductHostDescriptor) -> usize {
    std::ptr::from_ref(descriptor) as usize
}

pub(super) fn runtime_status(status: NativeProductHostStatus) -> NativeRuntimeStatus {
    match status {
        NativeProductHostStatus::SUCCESS => NativeRuntimeStatus::SUCCESS,
        NativeProductHostStatus::CLOSED | NativeProductHostStatus::INVALID_ARGUMENT => {
            NativeRuntimeStatus::INVALID_ARGUMENT
        }
        _ => NativeRuntimeStatus::RUNTIME_FAILURE,
    }
}
