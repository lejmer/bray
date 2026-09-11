use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use bray_runtime_abi::{
    NativeProductHostDescriptor, NativeProductHostObservation, NativeProductHostState,
    NativeProductHostStatus, NativeProductIdentity, NativeRuntimeStatus,
    NativeStaticCleanupCallback, NativeStaticDuration, NativeStaticFinalizer, NativeStaticIdentity,
    NativeStaticTransitionCallback,
};

use super::attachment::ThreadStaticRegistry;

// Loaded products share one process registry so archive and shared-library hosts coordinate with
// exact-thread attachments owned by the same runtime.
pub(super) static PRODUCT_HOSTS: OnceLock<Mutex<HashMap<usize, ProductHost>>> = OnceLock::new();

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
    pub(super) execution: Option<crate::product::RetainedProductExecution>,
    pub(super) state: NativeProductHostState,
    pub(super) active_entries: usize,
    pub(super) external_roots: usize,
    pub(super) retirement_roots: usize,
    pub(super) thread_attachments: usize,
    pub(super) worker_attachments: usize,
    pub(super) initialized_statics: usize,
    pub(super) cleaned_statics: usize,
    pub(super) cleanup_incidents: usize,
    pub(super) last_incident: NativeStaticIdentity,
    pub(super) cleanup_running: bool,
    pub(super) cleanup_blocked: bool,
    pub(super) statics: Vec<ProductStatic>,
    pub(super) cleanup_thread: Option<bray_platform::RuntimeThreadReservation>,
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
            self.retirement_roots,
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

    pub(super) fn take_product_cleanups(&mut self) -> Vec<ProductStatic> {
        let mut statics = std::mem::take(&mut self.statics);
        statics.retain(|entry| entry.duration == NativeStaticDuration::PRODUCT);

        statics
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
    pub(super) execution: Option<crate::product::RetainedProductExecution>,
    pub(super) statics: Vec<ProductStatic>,
    pub(super) thread: bray_platform::RuntimeThreadReservation,
}

#[derive(Clone, Copy)]
pub(super) struct ThreadStaticEntry {
    pub(super) static_identity: NativeStaticIdentity,
    pub(super) order: u64,
    pub(super) prepare: NativeStaticTransitionCallback,
    pub(super) finalizer: NativeStaticFinalizer,
    pub(super) destroy: NativeStaticCleanupCallback,
    pub(super) detach: NativeStaticTransitionCallback,
}

pub(super) fn product_hosts() -> &'static Mutex<HashMap<usize, ProductHost>> {
    PRODUCT_HOSTS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(in crate::product) fn initialize_thread_static_registry() {
    THREAD_STATICS.with(|_| {});
}

pub(super) fn product_key(descriptor: &NativeProductHostDescriptor) -> usize {
    std::ptr::from_ref(descriptor) as usize
}

pub(super) fn runtime_status(status: NativeProductHostStatus) -> NativeRuntimeStatus {
    match status {
        NativeProductHostStatus::SUCCESS => NativeRuntimeStatus::SUCCESS,
        NativeProductHostStatus::ALLOCATION_FAILURE => NativeRuntimeStatus::ALLOCATION_FAILURE,
        NativeProductHostStatus::CLOSED | NativeProductHostStatus::INVALID_ARGUMENT => {
            NativeRuntimeStatus::INVALID_ARGUMENT
        }
        _ => NativeRuntimeStatus::RUNTIME_FAILURE,
    }
}

pub(in crate::product) fn host_status(status: NativeRuntimeStatus) -> NativeProductHostStatus {
    match status {
        NativeRuntimeStatus::SUCCESS => NativeProductHostStatus::SUCCESS,
        NativeRuntimeStatus::ALLOCATION_FAILURE => NativeProductHostStatus::ALLOCATION_FAILURE,
        NativeRuntimeStatus::INVALID_ARGUMENT => NativeProductHostStatus::INVALID_ARGUMENT,
        _ => NativeProductHostStatus::RUNTIME_FAILURE,
    }
}
