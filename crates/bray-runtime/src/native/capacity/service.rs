use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use bray_runtime_abi::{
    NativeCleanupCapacityMetadata, NativeCleanupCapacityMetadataProvider, NativeCleanupStorage,
    NativeExecutionServices, NativeProductServiceCallbacks, NativeProductServices,
    NativeProviderRetention, NativeProviderRetentionCallbacks, NativeRuntimeStatus,
};

use super::domain::CleanupCapacityDomain;
use crate::native::export::contain_status;

// Handles route calls to explicitly formed services. This registry selects no default domain.
static NEXT_SERVICE: AtomicUsize = AtomicUsize::new(1);
static SERVICES: OnceLock<Mutex<HashMap<usize, Service>>> = OnceLock::new();
static OPERATIONS: NativeProductServiceCallbacks = NativeProductServiceCallbacks::new(
    &super::super::services::HOST_SERVICES,
    execution,
    admit,
    activate,
    discharge,
    register_provider,
);
static RETENTION: NativeProviderRetentionCallbacks =
    NativeProviderRetentionCallbacks::new(retain, release);

struct Service {
    domain: triomphe::Arc<CleanupCapacityDomain>,
    references: usize,
    execution: Option<&'static NativeExecutionServices>,
}

fn services() -> &'static Mutex<HashMap<usize, Service>> {
    SERVICES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Creates one host-owned service before providers exchange ownership through its binding.
///
/// The host must keep this runtime image loaded until every binding, provider registration,
/// retained provider reference, and active service call has ended. Product closure does not
/// release that independent image obligation. Once a provider descriptor is bound, the image
/// remains resident until that provider unloads, including when later product formation fails.
pub extern "C" fn bray_runtime_product_services_formation(
    destination: &mut NativeProductServices,
    execution: Option<&'static NativeExecutionServices>,
) -> NativeRuntimeStatus {
    contain_status(|| {
        form(destination, execution)
            .err()
            .unwrap_or(NativeRuntimeStatus::SUCCESS)
    })
}

/// Releases one service binding and restores its initialized empty state.
pub extern "C" fn bray_runtime_product_services_release(binding: &mut NativeProductServices) {
    drop(std::mem::replace(binding, NativeProductServices::empty()));
}

fn form(
    destination: &mut NativeProductServices,
    execution: Option<&'static NativeExecutionServices>,
) -> Result<(), NativeRuntimeStatus> {
    if !destination.is_empty()
        || execution
            .is_some_and(|table| !table.is_compatible(&super::super::services::HOST_SERVICES))
    {
        return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
    }

    let domain = crate::allocation::allocate_shared(CleanupCapacityDomain::new())
        .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    let context = NEXT_SERVICE
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |identity| identity.checked_add(1))
        .ok()
        .and_then(NonZeroUsize::new)
        .ok_or(NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    let mut entries = services()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    crate::allocation::reserve_map_entries(&mut entries, 1)
        .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    assert!(
        !entries.contains_key(&context.get()),
        "monotonic service identities are never reused"
    );

    entries.insert(
        context.get(),
        Service {
            domain,
            references: 1,
            execution,
        },
    );

    drop(entries);

    *destination = NativeProductServices::new(
        context,
        &OPERATIONS,
        NativeProviderRetention::new(context, &RETENTION),
    );

    Ok(())
}

extern "C" fn execution(context: usize) -> Option<&'static NativeExecutionServices> {
    let entries = services()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    entries.get(&context).and_then(|service| service.execution)
}

fn domain(context: usize) -> Result<triomphe::Arc<CleanupCapacityDomain>, NativeRuntimeStatus> {
    let entries = services()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    // Retain the shared service while invoking provider callbacks outside its registry lock.
    entries
        .get(&context)
        .map(|service| service.domain.clone())
        .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)
}

extern "C" fn admit(
    context: usize,
    count: usize,
    metadata: Option<NativeCleanupCapacityMetadataProvider>,
) -> NativeRuntimeStatus {
    contain_status(|| {
        admit_bundle(context, count, metadata)
            .err()
            .unwrap_or(NativeRuntimeStatus::SUCCESS)
    })
}

fn admit_bundle(
    context: usize,
    count: usize,
    metadata: Option<NativeCleanupCapacityMetadataProvider>,
) -> Result<(), NativeRuntimeStatus> {
    let domain = domain(context)?;

    if count == 0 {
        return Ok(());
    }

    let metadata = metadata.ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;
    let mut prepared = Vec::new();

    crate::allocation::reserve_vec_entries(&mut prepared, count)
        .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    for index in 0..count {
        let metadata = metadata(index).ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;
        let mut storage = NativeCleanupStorage::empty();
        let status = metadata.prepare(&mut storage);

        if status != NativeRuntimeStatus::SUCCESS {
            return Err(status);
        }

        if storage.is_empty() {
            return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
        }

        prepared.push((*metadata.identity(), storage));
    }

    domain.admit(prepared)
}

extern "C" fn register_provider(
    context: usize,
    provider: usize,
    teardown: extern "C" fn(usize) -> NativeRuntimeStatus,
    destination: &mut bray_runtime_abi::NativeProviderRetirement,
) -> NativeRuntimeStatus {
    contain_status(|| {
        domain(context)
            .and_then(|domain| super::retirement::register(domain, provider, teardown, destination))
            .err()
            .unwrap_or(NativeRuntimeStatus::SUCCESS)
    })
}

extern "C" fn activate(
    context: usize,
    metadata: &NativeCleanupCapacityMetadata,
    destination: &mut NativeCleanupStorage,
) -> NativeRuntimeStatus {
    contain_status(|| {
        if !destination.is_empty() {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        match domain(context).and_then(|domain| domain.activate(*metadata.identity())) {
            Ok(storage) => {
                *destination = storage;

                NativeRuntimeStatus::SUCCESS
            }
            Err(status) => status,
        }
    })
}

extern "C" fn discharge(
    context: usize,
    metadata: &NativeCleanupCapacityMetadata,
) -> NativeRuntimeStatus {
    contain_status(|| {
        domain(context)
            .and_then(|domain| domain.discharge(*metadata.identity()))
            .err()
            .unwrap_or(NativeRuntimeStatus::SUCCESS)
    })
}

extern "C" fn retain(context: usize) {
    let mut entries = services()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    // A cloned binding already owns a service reference for this exact context.
    let service = entries
        .get_mut(&context)
        .expect("live binding owns its service");

    service.references = service
        .references
        .checked_add(1)
        .unwrap_or_else(|| std::process::abort());
}

extern "C" fn release(context: usize) {
    let removed = {
        let mut entries = services()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        // Each binding releases exactly one previously acquired reference.
        let service = entries
            .get_mut(&context)
            .expect("live binding owns its service");

        service.references = service
            .references
            .checked_sub(1)
            .expect("binding reference is owned");

        if service.references == 0 {
            entries.remove(&context)
        } else {
            None
        }
    };

    // The final service release may drop unused backing with provider callbacks.
    drop(removed);
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{
        NativeCleanupCapacityMetadata, NativeCleanupStorage, NativeProductServices,
        NativeProviderRetention, NativeRuntimeStatus,
    };

    use super::{bray_runtime_product_services_formation as form, services};
    use crate::test_support::with_allocation_failure;

    static NEXT: AtomicUsize = AtomicUsize::new(1);
    static RELEASED: AtomicUsize = AtomicUsize::new(0);
    static METADATA: NativeCleanupCapacityMetadata =
        NativeCleanupCapacityMetadata::new([7; 32], prepare);

    #[test]
    fn retired_service_identity_cannot_rebind_a_published_descriptor() {
        extern "C" fn empty(_: usize) -> bray_runtime_abi::NativeStaticHostEntry {
            panic!("descriptor has no statics");
        }

        let descriptor = bray_runtime_abi::NativeProductHostDescriptor::new(
            bray_runtime_abi::NativeProductIdentity::new([217; 32]),
            empty,
            0,
        );

        let mut original = NativeProductServices::empty();
        assert_eq!(form(&mut original, None), NativeRuntimeStatus::SUCCESS);
        let identity = descriptor.bind_services(&original, NonZeroUsize::new(2).unwrap()).unwrap();
        drop(original);

        let mut replacement = NativeProductServices::empty();
        assert_eq!(form(&mut replacement, None), NativeRuntimeStatus::SUCCESS);

        assert_eq!(
            descriptor.bind_services(&replacement, NonZeroUsize::new(identity).unwrap()),
            Err(NativeRuntimeStatus::INVALID_ARGUMENT)
        );
    }

    extern "C" fn prepare(destination: &mut NativeCleanupStorage) -> NativeRuntimeStatus {
        drop(services().lock().unwrap());
        let address = NonZeroUsize::new(NEXT.fetch_add(1, Ordering::Relaxed)).unwrap();

        *destination =
            NativeCleanupStorage::owned(address, 0, release, NativeProviderRetention::empty());

        NativeRuntimeStatus::SUCCESS
    }

    extern "C" fn release(_: usize, _: usize) {
        drop(services().lock().unwrap());
        RELEASED.fetch_add(1, Ordering::Relaxed);
    }

    extern "C" fn metadata(index: usize) -> Option<&'static NativeCleanupCapacityMetadata> {
        (index < 2).then_some(&METADATA)
    }

    #[test]
    fn optional_execution_services_are_bound_to_the_same_resident_host() {
        let mut binding = NativeProductServices::empty();
        assert_eq!(form(&mut binding, None), NativeRuntimeStatus::SUCCESS);
        assert!(binding.host().is_some());
        assert!(binding.execution().is_none());
        drop(binding);

        let execution = crate::native::services::bray_runtime_execution_services();
        let mut binding = NativeProductServices::empty();

        assert_eq!(
            form(&mut binding, Some(execution)),
            NativeRuntimeStatus::SUCCESS
        );

        with_allocation_failure(|| {
            let table = binding.execution().unwrap();
            assert!(std::ptr::eq(table, execution));
            assert!(table.is_compatible(binding.host().unwrap()));
        });

        let wrong_host = Box::leak(Box::new(bray_runtime_abi::NativeHostServices {
            version: 2,
            ..crate::native::services::HOST_SERVICES
        }));

        assert!(!execution.is_compatible(wrong_host));

        let invalid_execution = Box::leak(Box::new(bray_runtime_abi::NativeExecutionServices {
            host: wrong_host,
            ..crate::native::services::EXECUTION_SERVICES
        }));

        let mut rejected = NativeProductServices::empty();

        assert_eq!(
            form(&mut rejected, Some(invalid_execution)),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert!(rejected.is_empty());
    }

    #[test]
    fn explicit_bindings_share_only_the_selected_domain_and_retain_it_without_allocation() {
        let mut binding = NativeProductServices::empty();

        assert_eq!(
            with_allocation_failure(|| form(&mut binding, None)),
            NativeRuntimeStatus::ALLOCATION_FAILURE
        );

        assert!(binding.is_empty());
        assert_eq!(form(&mut binding, None), NativeRuntimeStatus::SUCCESS);
        assert!(binding.is_valid());

        assert_eq!(
            form(&mut binding, None),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        let mut other = NativeProductServices::empty();
        assert_eq!(form(&mut other, None), NativeRuntimeStatus::SUCCESS);
        assert!(!binding.same_domain(&other));
        let retained = with_allocation_failure(|| binding.clone());
        assert!(retained.same_domain(&binding));

        assert_eq!(
            binding.admit(3, Some(metadata)),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(RELEASED.load(Ordering::Relaxed), 2);

        assert_eq!(
            binding.admit(2, Some(metadata)),
            NativeRuntimeStatus::SUCCESS
        );

        let mut transferred = NativeCleanupStorage::empty();

        with_allocation_failure(|| {
            assert_eq!(
                other.activate(&METADATA, &mut transferred),
                NativeRuntimeStatus::INVALID_ARGUMENT
            );

            assert!(transferred.is_empty());

            assert_eq!(
                retained.activate(&METADATA, &mut transferred),
                NativeRuntimeStatus::SUCCESS
            );

            assert_eq!(
                binding.activate(&METADATA, &mut transferred),
                NativeRuntimeStatus::INVALID_ARGUMENT
            );

            assert_eq!(binding.discharge(&METADATA), NativeRuntimeStatus::SUCCESS);
            assert_eq!(RELEASED.load(Ordering::Relaxed), 2);
            assert_eq!(binding.discharge(&METADATA), NativeRuntimeStatus::SUCCESS);
            assert_eq!(RELEASED.load(Ordering::Relaxed), 3);
        });

        drop(binding);

        assert_eq!(
            with_allocation_failure(|| retained.admit(0, None)),
            NativeRuntimeStatus::SUCCESS
        );

        drop(retained);
        assert_eq!(other.admit(1, Some(metadata)), NativeRuntimeStatus::SUCCESS);
        super::bray_runtime_product_services_release(&mut other);
        assert!(other.is_empty());
        assert_eq!(RELEASED.load(Ordering::Relaxed), 4);
        drop(transferred);
        assert_eq!(RELEASED.load(Ordering::Relaxed), 5);
    }
}
