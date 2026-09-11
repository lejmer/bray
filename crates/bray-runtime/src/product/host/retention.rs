use std::num::NonZeroUsize;

use bray_runtime_abi::{
    NativeProductHostDescriptor, NativeProductHostObservation, NativeProductHostState,
    NativeProviderRetention, NativeProviderRetentionCallbacks, NativeRuntimeStatus,
};

use super::model::{product_hosts, product_key};

static CALLBACKS: NativeProviderRetentionCallbacks =
    NativeProviderRetentionCallbacks::new(retain, release);

pub(crate) fn retain_provider(
    descriptor: &NativeProductHostDescriptor,
    destination: &mut NativeProviderRetention,
) -> NativeRuntimeStatus {
    if !destination.is_empty() {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    }

    let product = product_key(descriptor);

    let Ok(mut hosts) = product_hosts().lock() else {
        return NativeRuntimeStatus::RUNTIME_FAILURE;
    };

    let Some(host) = hosts.get_mut(&product) else {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    };

    if !matches!(
        host.state,
        NativeProductHostState::OPEN | NativeProductHostState::CLOSING
    ) {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    }

    let held = host.active_entries != 0
        || host.external_roots != 0
        || host.thread_attachments != 0
        || (host.state == NativeProductHostState::CLOSING && host.cleanup_running);

    if !held {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    }

    let Some(context) = NonZeroUsize::new(product) else {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    };

    increment(&mut host.retirement_roots);
    *destination = NativeProviderRetention::new(context, &CALLBACKS);

    NativeRuntimeStatus::SUCCESS
}

extern "C" fn retain(product: usize) {
    let mut hosts = product_hosts()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    // The source reference keeps this host registered throughout cloning.
    let host = hosts
        .get_mut(&product)
        .expect("live retention owns its host");

    assert!(matches!(
        host.state,
        NativeProductHostState::OPEN
            | NativeProductHostState::CLOSING
            | NativeProductHostState::RETIRING
    ));

    assert_ne!(host.retirement_roots, 0);
    increment(&mut host.retirement_roots);
}

extern "C" fn release(product: usize) {
    {
        let mut hosts = product_hosts()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // This owner keeps the host registered and contributes one unreleased count.
        let host = hosts
            .get_mut(&product)
            .expect("live retention owns its host");

        host.retirement_roots = host
            .retirement_roots
            .checked_sub(1)
            .expect("each retention releases once");
    }

    finish_retirement(product);
}

fn increment(count: &mut usize) {
    *count = count
        .checked_add(1)
        .unwrap_or_else(|| std::process::abort());
}

/// Releases execution resources before publishing permission to unload the provider.
pub(super) fn finish_retirement(product: usize) -> NativeProductHostObservation {
    let execution = {
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

        // The shared execution owner keeps shutdown outside the host registry lock.
        host.execution.clone()
    };

    if let Some(execution) = execution {
        execution.release();
    }

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
