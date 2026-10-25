use std::cell::RefCell;
use std::collections::BTreeMap;

use bray_runtime_abi::{
    NativeProductHostDescriptor, NativeProductHostStatus, NativeProductIdentity,
    NativeRuntimeStatus, NativeStaticCleanupCallback, NativeStaticFinalizer, NativeStaticIdentity,
    NativeStaticTransitionCallback,
};

thread_local! {
    pub(super) static THREAD_STATICS: RefCell<ThreadStaticRegistry> =
        const { RefCell::new(ThreadStaticRegistry::new()) };
}

pub(super) struct ThreadStaticEntry {
    pub(super) admission: super::super::cleanup::StaticAdmission,
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

        if !crate::bootstrap::register_thread_cleanup(super::thread::thread_exit_cleanup) {
            return false;
        }

        self.callback_registered = true;

        true
    }
}

pub(crate) fn initialize_thread_static_registry() {
    THREAD_STATICS.with(|_| {});
}

pub(super) fn product_key(descriptor: &NativeProductHostDescriptor) -> usize {
    std::ptr::from_ref(descriptor) as usize
}

pub(super) fn runtime_status(status: NativeProductHostStatus) -> NativeRuntimeStatus {
    match status {
        NativeProductHostStatus::SUCCESS => NativeRuntimeStatus::SUCCESS,
        NativeProductHostStatus::IDENTITY_EXHAUSTED => NativeRuntimeStatus::IDENTITY_EXHAUSTED,
        NativeProductHostStatus::OUTGOING_STORAGE_UNAVAILABLE => {
            NativeRuntimeStatus::OUTGOING_STORAGE_UNAVAILABLE
        }
        NativeProductHostStatus::CLOSED | NativeProductHostStatus::INVALID_ARGUMENT => {
            NativeRuntimeStatus::INVALID_ARGUMENT
        }
        _ => NativeRuntimeStatus::RUNTIME_FAILURE,
    }
}
