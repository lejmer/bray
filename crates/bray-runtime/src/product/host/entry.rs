use bray_runtime_abi::{NativeProductHostStatus, NativeRuntimeStatus};

use super::model::{product_hosts, runtime_status};

/// Retains one already formed product through an accepted ownership obligation.
pub(crate) struct ProductEntry(Option<usize>);

impl ProductEntry {
    pub(crate) fn acquire(product: usize) -> Result<Self, NativeRuntimeStatus> {
        let mut hosts = product_hosts()
            .lock()
            .map_err(|_| NativeRuntimeStatus::RUNTIME_FAILURE)?;

        let host = hosts
            .get_mut(&product)
            .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

        let status = super::operations::acquire(&mut host.active_entries, host.state);

        if status != NativeProductHostStatus::SUCCESS {
            return Err(runtime_status(status));
        }

        Ok(Self(Some(product)))
    }

    /// Disarms release after the caller transfers this entry into another host obligation.
    pub(super) fn transfer(&mut self) {
        self.0 = None;
    }
}

impl Drop for ProductEntry {
    fn drop(&mut self) {
        if let Some(product) = self.0 {
            super::operations::release_admission_entry(product);
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::{
        NativeProductHostDescriptor, NativeProductHostOperation, NativeProductHostState,
        NativeProductIdentity, NativeStaticHostEntry,
    };

    #[test]
    fn entry_retains_formed_product_and_last_release_completes_closure() {
        extern "C" fn empty(_: usize) -> NativeStaticHostEntry {
            panic!("empty product has no statics");
        }

        static DESCRIPTOR: NativeProductHostDescriptor =
            NativeProductHostDescriptor::new(NativeProductIdentity::new([174; 32]), empty, 0);

        let product = super::super::model::product_key(&DESCRIPTOR);
        assert!(super::ProductEntry::acquire(product).is_err());
        let control = |operation| super::super::operations::control(&DESCRIPTOR, operation);

        assert_eq!(
            control(NativeProductHostOperation::FORM).state(),
            NativeProductHostState::OPEN
        );

        let first =
            crate::test_support::with_allocation_failure(|| super::ProductEntry::acquire(product))
                .unwrap();

        let second = super::ProductEntry::acquire(product).unwrap();

        assert_eq!(
            control(NativeProductHostOperation::CLOSE).active_entries(),
            2
        );

        assert!(super::ProductEntry::acquire(product).is_err());
        drop(first);

        assert_eq!(
            control(NativeProductHostOperation::OBSERVE).state(),
            NativeProductHostState::CLOSING
        );

        crate::test_support::with_allocation_failure(|| drop(second));

        assert_eq!(
            control(NativeProductHostOperation::OBSERVE).state(),
            NativeProductHostState::CLOSED
        );
    }
}
