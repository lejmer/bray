use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};

use bray_runtime_abi::{
    NativeCleanupExecution, NativeProductHostDescriptor, NativeProductHostObservation,
    NativeProductHostState, NativeProductHostStatus, NativeStaticIdentity,
};

use super::descriptor::read_statics;
use super::model::{ProductHost, host_status, product_hosts, product_key};

// IDs belong to this resident registry and never reuse an unloaded image's address.
static NEXT_PRODUCT: AtomicUsize = AtomicUsize::new(2);

pub(super) fn ensure_formed(
    descriptor: &NativeProductHostDescriptor,
    capacity: Option<&bray_runtime_abi::NativeProductServices>,
    retain: impl FnOnce() -> Result<
        Option<crate::product::RetainedProductExecution>,
        bray_runtime_abi::NativeRuntimeStatus,
    >,
) -> Result<(), NativeProductHostObservation> {
    let product = bind_product(descriptor, capacity).map_err(unformed)?;

    let hosts = product_hosts()
        .lock()
        .map_err(|_| unformed(NativeProductHostStatus::RUNTIME_FAILURE))?;

    if let Some(host) = hosts.get(&product) {
        if host.capacity.is_none() && host.state != NativeProductHostState::RETIRING {
            return Err(host.observation(super::operations::status_for_state(host)));
        }

        return match capacity {
            Some(capacity)
                if host.capacity.as_ref().is_some_and(|existing| {
                    !capacity.is_valid() || !existing.same_domain(capacity)
                }) =>
            {
                Err(host.observation(NativeProductHostStatus::INVALID_ARGUMENT))
            }
            _ => Ok(()),
        };
    }

    // Descriptor callbacks and execution retention must not hold the shared host registry.
    drop(hosts);

    let capacity = capacity
        .filter(|capacity| capacity.is_valid())
        .ok_or_else(|| unformed(NativeProductHostStatus::INVALID_ARGUMENT))?;

    // The host owns its service reference independently of the caller.
    let capacity = crate::allocation::allocate_shared(capacity.clone())
        .map_err(|_| unformed(NativeProductHostStatus::ALLOCATION_FAILURE))?;

    let (statics, thread_statics) = read_statics(descriptor).map_err(unformed)?;

    let mut registration = bray_runtime_abi::NativeProviderRetirement::empty();

    let status = capacity.register_provider(
        product,
        super::retention::teardown_product,
        &mut registration,
    );

    if status != bray_runtime_abi::NativeRuntimeStatus::SUCCESS {
        return Err(unformed(host_status(status)));
    }

    let retirement = crate::allocation::allocate_shared(registration)
        .map_err(|_| unformed(NativeProductHostStatus::ALLOCATION_FAILURE))?;

    let cleanup_thread = bray_platform::RuntimeThreadReservation::reserve()
        .map_err(|error| unformed(host_status(crate::native::thread_attachment_status(error))))?;

    let execution = retain().map_err(|status| unformed(host_status(status)))?;

    if execution.is_none()
        && statics
            .iter()
            .chain(&thread_statics)
            .any(|entry| entry.finalizer.execution() == NativeCleanupExecution::ASYNCHRONOUS)
    {
        return Err(unformed(NativeProductHostStatus::RUNTIME_FAILURE));
    }

    let cleanup_driver = execution
        .as_ref()
        .map(|owner| owner.admit_cleanup(&statics))
        .transpose()
        .map_err(|status| unformed(host_status(status)))?
        .flatten();

    let initialized_statics = statics.len();

    let host = ProductHost {
        identity: descriptor.identity(),
        capacity: Some(capacity),
        // Keep a losing insertion releasable after the registry lock is dropped.
        execution: execution.clone(),
        cleanup_driver,
        state: NativeProductHostState::OPEN,
        active_entries: 0,
        external_roots: 0,
        retirement: Some(retirement),
        thread_attachments: 0,
        worker_attachments: 0,
        initialized_statics,
        cleaned_statics: 0,
        cleanup_incidents: 0,
        last_incident: NativeStaticIdentity::new([0; 32]),
        cleanup_running: false,
        cleanup_blocked: false,
        statics,
        thread_statics,
        cleanup_thread: Some(cleanup_thread),
    };

    // Release a losing or failed owner only after its registry guard has been dropped.
    let inserted = insert_host(product, host);

    if !matches!(inserted, Ok(true)) {
        if let Some(execution) = execution {
            execution.release();
        }
    }

    inserted.map(|_| ()).map_err(unformed)
}

fn bind_product(
    descriptor: &NativeProductHostDescriptor,
    services: Option<&bray_runtime_abi::NativeProductServices>,
) -> Result<usize, NativeProductHostStatus> {
    let Some(services) = services else {
        return NonZeroUsize::new(product_key(descriptor))
            .map(NonZeroUsize::get)
            .ok_or(NativeProductHostStatus::INVALID_ARGUMENT);
    };

    if !services
        .host()
        .is_some_and(|host| std::ptr::eq(host, &crate::native::services::HOST_SERVICES))
    {
        return Err(NativeProductHostStatus::INVALID_ARGUMENT);
    }

    let candidate = match NonZeroUsize::new(product_key(descriptor)) {
        Some(existing) => existing,
        None => NEXT_PRODUCT
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |identity| {
                identity.checked_add(1)
            })
            .ok()
            .and_then(NonZeroUsize::new)
            .ok_or(NativeProductHostStatus::ALLOCATION_FAILURE)?,
    };

    descriptor
        .bind_services(services, candidate)
        .map_err(host_status)
}

fn insert_host(product: usize, host: ProductHost) -> Result<bool, NativeProductHostStatus> {
    let mut hosts = product_hosts()
        .lock()
        .map_err(|_| NativeProductHostStatus::RUNTIME_FAILURE)?;

    if let Some(existing) = hosts.get(&product) {
        return if existing
            .capacity
            .as_ref()
            .zip(host.capacity.as_ref())
            .is_some_and(|(existing, candidate)| existing.same_domain(candidate))
        {
            Ok(false)
        } else {
            Err(NativeProductHostStatus::INVALID_ARGUMENT)
        };
    }

    crate::allocation::reserve_map_entries(&mut hosts, 1)
        .map_err(|_| NativeProductHostStatus::ALLOCATION_FAILURE)?;

    hosts.insert(product, host);

    Ok(true)
}

fn unformed(status: NativeProductHostStatus) -> NativeProductHostObservation {
    NativeProductHostObservation::new(
        status,
        NativeProductHostState::UNFORMED,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        NativeStaticIdentity::new([0; 32]),
    )
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::{
        NativeHostServices, NativeProductHostDescriptor, NativeProductHostOperation,
        NativeProductHostState, NativeProductHostStatus, NativeProductIdentity, NativeStaticHostEntry,
    };

    extern "C" fn empty(_: usize) -> NativeStaticHostEntry {
        panic!("empty product has no static entries");
    }

    fn descriptor() -> NativeProductHostDescriptor {
        NativeProductHostDescriptor::new(NativeProductIdentity::new([197; 32]), empty, 0)
    }

    #[test]
    fn concurrent_publication_selects_one_resident_identity_and_rejects_rebinding() {
        let descriptor = descriptor();
        let services = crate::test_support::cleanup_capacity_binding();
        let other = crate::test_support::cleanup_capacity_binding();
        assert_eq!(super::product_key(&descriptor), 0);
        assert!(descriptor.runtime_host().is_null());
        assert!(descriptor.runtime_execution().is_null());

        let identities = std::thread::scope(|scope| {
            let handles = (0..8)
                .map(|_| scope.spawn(|| super::bind_product(&descriptor, Some(&services)).unwrap()))
                .collect::<Vec<_>>();

            handles.into_iter().map(|handle| handle.join().unwrap()).collect::<Vec<_>>()
        });

        let identity = identities[0];
        assert!(identity >= 2);
        assert!(identities.iter().all(|candidate| *candidate == identity));
        assert_eq!(super::product_key(&descriptor), identity);
        assert_eq!(descriptor.runtime_host(), std::ptr::from_ref(services.host().unwrap()));
        assert!(descriptor.runtime_execution().is_null());

        crate::test_support::with_allocation_failure(|| {
            assert_eq!(super::bind_product(&descriptor, Some(&services)), Ok(identity));

            assert_eq!(
                super::bind_product(&descriptor, Some(&other)),
                Err(NativeProductHostStatus::INVALID_ARGUMENT)
            );
        });

        let foreign = NativeHostServices { ..crate::native::services::HOST_SERVICES };
        assert!(descriptor.runtime_instance(&foreign).is_none());
        assert_eq!(super::product_key(&descriptor), identity);
    }

    #[test]
    fn a_reloaded_descriptor_at_the_same_address_gets_a_new_host() {
        let services = crate::test_support::cleanup_capacity_binding();
        let mut loaded = Box::new(descriptor());
        let address = std::ptr::from_ref(loaded.as_ref()).addr();

        let control = |loaded: &NativeProductHostDescriptor, operation| {
            super::super::operations::control(loaded, operation, Some(&services))
        };

        assert_eq!(control(&loaded, NativeProductHostOperation::FORM).state(), NativeProductHostState::OPEN);
        let first = super::product_key(&loaded);
        assert_eq!(control(&loaded, NativeProductHostOperation::CLOSE).state(), NativeProductHostState::CLOSED);

        *loaded = descriptor();
        assert_eq!(std::ptr::from_ref(loaded.as_ref()).addr(), address);
        assert_eq!(super::product_key(&loaded), 0);
        assert_eq!(control(&loaded, NativeProductHostOperation::FORM).state(), NativeProductHostState::OPEN);
        let second = super::product_key(&loaded);
        assert_ne!(first, second);
        assert_eq!(super::product_hosts().lock().unwrap().get(&first).unwrap().state, NativeProductHostState::CLOSED);
        assert_eq!(control(&loaded, NativeProductHostOperation::CLOSE).state(), NativeProductHostState::CLOSED);
    }
}
