use bray_runtime_abi::{
    NativeProductHostDescriptor, NativeProductHostObservation, NativeProductHostState,
    NativeProductHostStatus,
};

use super::model::{product_hosts, product_key};

/// Keeps provider code and backing alive independently of its static cleanup.
pub(crate) struct ProviderRetention {
    product: usize,
}

impl ProviderRetention {
    /// The caller holds a live entry, external owner, attachment, or cleanup invocation.
    pub(crate) fn acquire(
        descriptor: &NativeProductHostDescriptor,
    ) -> Result<Self, NativeProductHostStatus> {
        let product = product_key(descriptor);

        let mut hosts = product_hosts()
            .lock()
            .map_err(|_| NativeProductHostStatus::RUNTIME_FAILURE)?;

        let host = hosts
            .get_mut(&product)
            .ok_or(NativeProductHostStatus::INVALID_ARGUMENT)?;

        if !matches!(
            host.state,
            NativeProductHostState::OPEN | NativeProductHostState::CLOSING
        ) {
            return Err(NativeProductHostStatus::CLOSED);
        }

        let held = host.active_entries != 0
            || host.external_roots != 0
            || host.thread_attachments != 0
            || (host.state == NativeProductHostState::CLOSING && host.cleanup_running);

        if !held {
            return Err(NativeProductHostStatus::INVALID_ARGUMENT);
        }

        increment(&mut host.retirement_roots);

        Ok(Self { product })
    }
}

impl Clone for ProviderRetention {
    fn clone(&self) -> Self {
        let mut hosts = product_hosts()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let host = hosts
            .get_mut(&self.product)
            .expect("live retention owns its host");

        assert!(matches!(
            host.state,
            NativeProductHostState::OPEN
                | NativeProductHostState::CLOSING
                | NativeProductHostState::RETIRING
        ));

        assert_ne!(host.retirement_roots, 0);
        increment(&mut host.retirement_roots);

        Self {
            product: self.product,
        }
    }
}

impl Drop for ProviderRetention {
    fn drop(&mut self) {
        {
            let mut hosts = product_hosts()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            let host = hosts
                .get_mut(&self.product)
                .expect("live retention owns its host");

            host.retirement_roots = host
                .retirement_roots
                .checked_sub(1)
                .expect("each retention releases once");
        }

        finish_retirement(self.product);
    }
}

fn increment(count: &mut usize) {
    *count = count
        .checked_add(1)
        .unwrap_or_else(|| std::process::abort());
}

/// Releases execution resources before publishing permission to unload the provider.
pub(super) fn finish_retirement(product: usize) -> NativeProductHostObservation {
    let runtime = {
        let mut hosts = product_hosts()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(host) = hosts.get_mut(&product) else {
            return NativeProductHostObservation::invalid();
        };

        if host.state != NativeProductHostState::RETIRING
            || host.retirement_roots != 0
            || host.cleanup_running
        {
            return host.observation(super::operations::status_for_state(host));
        }

        host.cleanup_running = true;

        // The shared runtime token keeps shutdown outside the host registry lock.
        host.runtime.clone()
    };

    runtime.release();

    let mut hosts = product_hosts()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let host = hosts
        .get_mut(&product)
        .expect("retiring host remains registered");

    assert_eq!(host.retirement_roots, 0);
    host.cleanup_running = false;
    host.state = NativeProductHostState::CLOSED;

    host.observation(super::operations::status_for_state(host))
}
