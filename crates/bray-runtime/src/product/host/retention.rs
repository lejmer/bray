use bray_runtime_abi::{
    NativeProductHostDescriptor, NativeProductHostObservation, NativeProductHostState,
    NativeProviderRetention, NativeRuntimeStatus,
};

use super::model::{host_status, product_hosts, product_key};

pub(crate) fn retain_provider(
    descriptor: &NativeProductHostDescriptor,
    destination: &mut NativeProviderRetention,
) -> NativeRuntimeStatus {
    if !destination.is_empty() {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    }

    let Ok(hosts) = product_hosts().lock() else {
        return NativeRuntimeStatus::RUNTIME_FAILURE;
    };

    let Some(host) = hosts.get(&product_key(descriptor)) else {
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

    // Resident acquisition cannot invoke providers. Holding this lock interlocks with closure.
    host.retirement
        .as_ref()
        .map_or(NativeRuntimeStatus::INVALID_ARGUMENT, |registration| {
            registration.acquire(destination)
        })
}

/// Teardown runs from resident code, which publishes completion only after this returns.
pub(super) extern "C" fn teardown_product(product: usize) -> NativeRuntimeStatus {
    crate::native::contain_status(|| {
        let (execution, capacity) = {
            let mut hosts = product_hosts()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            let Some(host) = hosts.get_mut(&product) else {
                return NativeRuntimeStatus::INVALID_ARGUMENT;
            };

            if host.state != NativeProductHostState::RETIRING || host.cleanup_running {
                return NativeRuntimeStatus::INVALID_ARGUMENT;
            }

            host.cleanup_running = true;

            let capacity = std::mem::replace(
                &mut host.capacity,
                bray_runtime_abi::NativeCleanupCapacityBinding::empty(),
            );

            (host.execution.take(), capacity)
        };

        if let Some(execution) = execution {
            execution.release();
        }

        drop(capacity);

        let mut hosts = product_hosts()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // The outstanding registration keeps the retiring host present throughout teardown.
        let host = hosts
            .get_mut(&product)
            .expect("retiring host remains registered");

        host.cleanup_running = false;

        NativeRuntimeStatus::SUCCESS
    })
}

/// Requests teardown without publishing permission to unload the callback's own image.
pub(super) fn begin_retirement(product: usize) -> NativeProductHostObservation {
    let registration = {
        let hosts = product_hosts()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(host) = hosts.get(&product) else {
            return NativeProductHostObservation::invalid();
        };

        if host.state != NativeProductHostState::RETIRING {
            return host.observation(super::operations::status_for_state(host));
        }

        // Share registration ownership across the unlocked resident service call.
        host.retirement.clone()
    };

    let status = registration
        .as_ref()
        .map_or(NativeRuntimeStatus::INVALID_ARGUMENT, |registration| {
            registration.begin()
        });

    let mut hosts = product_hosts()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let Some(host) = hosts.get_mut(&product) else {
        return NativeProductHostObservation::invalid();
    };

    if status != NativeRuntimeStatus::SUCCESS {
        host.state = NativeProductHostState::FAILED;

        return host.observation(host_status(status));
    }

    host.observation(super::operations::status_for_state(host))
}

/// Only the explicit host-control boundary acknowledges resident completion and permits unload.
pub(super) fn observe_retirement(product: usize) -> Option<NativeProductHostObservation> {
    let registration = {
        let hosts = product_hosts().lock().ok()?;
        let host = hosts.get(&product)?;

        if host.state != NativeProductHostState::RETIRING {
            return None;
        }

        // Keep the registration live while observing outside the local host registry.
        host.retirement.clone()?
    };

    let progress = registration.observe();

    let (observation, released) = {
        let mut hosts = product_hosts().lock().ok()?;
        let host = hosts.get_mut(&product)?;

        if host.state != NativeProductHostState::RETIRING
            || !host
                .retirement
                .as_ref()
                .is_some_and(|current| triomphe::Arc::ptr_eq(current, &registration))
        {
            return Some(host.observation(super::operations::status_for_state(host)));
        }

        let released = if progress.status() != NativeRuntimeStatus::SUCCESS {
            host.state = NativeProductHostState::FAILED;

            None
        } else if progress.is_complete() {
            host.state = NativeProductHostState::CLOSED;

            host.retirement.take()
        } else {
            None
        };

        let status = if progress.status() == NativeRuntimeStatus::SUCCESS {
            super::operations::status_for_state(host)
        } else {
            host_status(progress.status())
        };

        (host.observation(status), released)
    };

    drop(released);
    drop(registration);

    Some(observation)
}
