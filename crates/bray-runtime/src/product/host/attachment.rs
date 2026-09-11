use bray_runtime_abi::{NativeProductHostState, NativeProductIdentity, NativeRuntimeStatus};

use super::model::{THREAD_STATICS, product_hosts};
use crate::product::cleanup::StaticCleanup;

pub(super) struct ThreadProductAttachment {
    pub(super) product: usize,
    pub(super) product_identity: NativeProductIdentity,
    pub(super) identity: u64,
    pub(super) worker: bool,
    pub(super) entries: Vec<StaticCleanup>,
    pub(super) cleanup_driver: Option<Box<dyn crate::product::ProductCleanup>>,
}

pub(super) struct ThreadStaticRegistry {
    pub(super) products: Vec<ThreadProductAttachment>,
    pub(super) callback_registered: bool,
    next_identity: u64,
}

impl ThreadStaticRegistry {
    pub(super) const fn new() -> Self {
        Self {
            products: Vec::new(),
            callback_registered: false,
            next_identity: 1,
        }
    }

    pub(super) fn attachment(&mut self, product: usize) -> Option<&mut ThreadProductAttachment> {
        self.products
            .iter_mut()
            .find(|attachment| attachment.product == product)
    }

    pub(super) fn remove_product(&mut self, product: usize) -> Option<ThreadProductAttachment> {
        let index = self
            .products
            .iter()
            .position(|attachment| attachment.product == product)?;

        Some(self.products.swap_remove(index))
    }

    fn ensure_exit_callback(&mut self) -> Result<(), NativeRuntimeStatus> {
        if self.callback_registered {
            return Ok(());
        }

        bray_platform::register_runtime_thread_exit_callback(super::thread::drain_thread_statics)
            .map_err(|error| match error {
            bray_platform::RuntimeThreadExitRegistrationError::AllocationFailed => {
                NativeRuntimeStatus::ALLOCATION_FAILURE
            }
            bray_platform::RuntimeThreadExitRegistrationError::NotAttached => {
                NativeRuntimeStatus::NOT_INITIALIZED
            }
        })?;

        self.callback_registered = true;

        Ok(())
    }
}

/// Prepares ownership outside the thread registry, then publishes it with a fresh identity.
pub(super) fn ensure_attachment(product: usize, worker: bool) -> Result<(), NativeRuntimeStatus> {
    if THREAD_STATICS.with(|registry| registry.borrow_mut().attachment(product).is_some()) {
        return Ok(());
    }

    let mut admission = super::entry::ProductEntry::acquire(product)?;

    let (product_identity, mut entries, execution) = {
        let hosts = product_hosts()
            .lock()
            .map_err(|_| NativeRuntimeStatus::RUNTIME_FAILURE)?;

        let host = hosts
            .get(&product)
            .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

        let mut entries = Vec::new();

        crate::allocation::reserve_vec_entries(&mut entries, host.thread_statics.len())
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        entries.extend_from_slice(&host.thread_statics);

        // The temporary entry prevents retirement while callbacks run without the host lock.
        (host.identity, entries, host.execution.clone())
    };

    let mut cleanup_driver = execution
        .as_ref()
        .map(|owner| owner.admit_cleanup(&entries))
        .transpose()?
        .flatten();

    entries.clear();

    // Losing resources drop after the TLS borrow and before the temporary entry releases.
    THREAD_STATICS.with(|registry| {
        let mut registry = registry.borrow_mut();

        if registry.attachment(product).is_some() {
            return Ok(());
        }

        let identity = registry.next_identity;

        let next_identity = identity
            .checked_add(1)
            .filter(|_| identity != 0)
            .ok_or(NativeRuntimeStatus::RUNTIME_FAILURE)?;

        crate::allocation::reserve_vec_entries(&mut registry.products, 1)
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        registry.ensure_exit_callback()?;

        let mut hosts = product_hosts()
            .lock()
            .map_err(|_| NativeRuntimeStatus::RUNTIME_FAILURE)?;

        let host = hosts
            .get_mut(&product)
            .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

        // A close during metadata preparation wins before any static ownership is accepted.
        if host.state != NativeProductHostState::OPEN {
            return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
        }

        let attachments = host
            .thread_attachments
            .checked_add(1)
            .ok_or(NativeRuntimeStatus::RUNTIME_FAILURE)?;

        let workers = host
            .worker_attachments
            .checked_add(usize::from(worker))
            .ok_or(NativeRuntimeStatus::RUNTIME_FAILURE)?;

        if worker {
            // Admission is serialized with closure before it broadcasts worker requests.
            host.execution
                .as_ref()
                .ok_or(NativeRuntimeStatus::NOT_INITIALIZED)?
                .admit_worker_cleanup(product)?;
        }

        registry.products.push(ThreadProductAttachment {
            product,
            product_identity,
            identity,
            worker,
            entries: std::mem::take(&mut entries),
            cleanup_driver: cleanup_driver.take(),
        });

        registry.next_identity = next_identity;
        host.thread_attachments = attachments;
        host.worker_attachments = workers;
        host.active_entries -= 1;
        admission.transfer();

        Ok(())
    })
}
