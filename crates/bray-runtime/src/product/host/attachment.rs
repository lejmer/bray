use bray_runtime_abi::{NativeProductIdentity, NativeRuntimeStatus, NativeStaticDuration};

use super::model::{ThreadStaticEntry, product_hosts};

pub(super) struct ThreadProductAttachment {
    pub(super) product: usize,
    pub(super) product_identity: NativeProductIdentity,
    pub(super) identity: u64,
    pub(super) acquired: bool,
    pub(super) worker: bool,
    pub(super) entries: Vec<ThreadStaticEntry>,
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

    pub(super) fn attachment(
        &mut self,
        product: usize,
        worker: bool,
    ) -> Result<&mut ThreadProductAttachment, NativeRuntimeStatus> {
        if let Some(index) = self
            .products
            .iter()
            .position(|attachment| attachment.product == product)
        {
            return Ok(&mut self.products[index]);
        }

        let identity = self.next_identity;

        let next_identity = identity
            .checked_add(1)
            .filter(|_| identity != 0)
            .ok_or(NativeRuntimeStatus::RUNTIME_FAILURE)?;

        let (product_identity, count) = {
            let hosts = product_hosts()
                .lock()
                .map_err(|_| NativeRuntimeStatus::RUNTIME_FAILURE)?;

            let host = hosts
                .get(&product)
                .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

            (
                host.identity,
                host.statics
                    .iter()
                    .filter(|entry| entry.duration == NativeStaticDuration::EXACT_THREAD)
                    .count(),
            )
        };

        let mut entries = Vec::new();

        crate::allocation::reserve_vec_entries(&mut entries, count)
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        crate::allocation::reserve_vec_entries(&mut self.products, 1)
            .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

        self.ensure_exit_callback()?;

        self.products.push(ThreadProductAttachment {
            product,
            product_identity,
            identity,
            acquired: false,
            worker,
            entries,
        });

        self.next_identity = next_identity;

        self.products
            .last_mut()
            .ok_or(NativeRuntimeStatus::RUNTIME_FAILURE)
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
