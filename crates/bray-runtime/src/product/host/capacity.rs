use bray_runtime_abi::{
    NativeCleanupCapacityMetadata, NativeCleanupCapacityMetadataProvider,
    NativeProductHostDescriptor, NativeProductServices, NativeProviderRetention,
    NativeRuntimeStatus,
};

use super::model::{product_hosts, product_key};

pub(crate) fn cleanup_capacity_admission(
    descriptor: &NativeProductHostDescriptor,
    count: usize,
    metadata: Option<NativeCleanupCapacityMetadataProvider>,
) -> NativeRuntimeStatus {
    with_capacity(descriptor, |capacity| capacity.admit(count, metadata))
}

pub(crate) fn cleanup_capacity_discharge(
    descriptor: &NativeProductHostDescriptor,
    metadata: &NativeCleanupCapacityMetadata,
) -> NativeRuntimeStatus {
    with_capacity(descriptor, |capacity| capacity.discharge(metadata))
}

fn with_capacity(
    descriptor: &NativeProductHostDescriptor,
    operation: impl FnOnce(&NativeProductServices) -> NativeRuntimeStatus,
) -> NativeRuntimeStatus {
    // Keep provider teardown behind this call, including callbacks during retirement.
    let mut retention = NativeProviderRetention::empty();
    let status = super::retention::retain_provider(descriptor, &mut retention);

    if status != NativeRuntimeStatus::SUCCESS {
        return status;
    }

    let capacity = {
        let Ok(hosts) = product_hosts().lock() else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        // Share the already retained binding without calling its foreign retain callback.
        hosts
            .get(&product_key(descriptor))
            .and_then(|host| host.capacity.clone())
    };

    capacity
        .as_ref()
        .map_or(NativeRuntimeStatus::INVALID_ARGUMENT, |capacity| {
            operation(capacity)
        })
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroUsize;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{
        NativeCleanupCapacityMetadata, NativeCleanupStorage, NativeProductHostDescriptor,
        NativeProductHostOperation, NativeProductHostState, NativeProductHostStatus,
        NativeProductIdentity, NativeProviderRetention, NativeRuntimeStatus, NativeStaticHostEntry,
    };

    use super::{cleanup_capacity_admission, cleanup_capacity_discharge};
    use crate::product::host::operations::control;
    use crate::test_support::{cleanup_capacity_binding, with_allocation_failure};

    extern "C" fn empty(_: usize) -> NativeStaticHostEntry {
        panic!("empty product has no static entries");
    }

    static OWNER: NativeProductHostDescriptor =
        NativeProductHostDescriptor::new(NativeProductIdentity::new([201; 32]), empty, 0);
    static PEER: NativeProductHostDescriptor =
        NativeProductHostDescriptor::new(NativeProductIdentity::new([202; 32]), empty, 0);
    static OTHER: NativeProductHostDescriptor =
        NativeProductHostDescriptor::new(NativeProductIdentity::new([203; 32]), empty, 0);
    static METADATA: NativeCleanupCapacityMetadata =
        NativeCleanupCapacityMetadata::new([204; 32], prepare);
    static RELEASED: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn release(_: usize, _: usize) {
        drop(super::product_hosts().lock().unwrap());
        RELEASED.fetch_add(1, Ordering::Relaxed);
    }

    extern "C" fn prepare(destination: &mut NativeCleanupStorage) -> NativeRuntimeStatus {
        drop(super::product_hosts().lock().unwrap());
        let mut retention = NativeProviderRetention::empty();
        let status = crate::product::retain_provider(&OWNER, &mut retention);

        if status != NativeRuntimeStatus::SUCCESS {
            return status;
        }

        *destination =
            NativeCleanupStorage::owned(NonZeroUsize::new(1).unwrap(), 0, release, retention);

        NativeRuntimeStatus::SUCCESS
    }

    extern "C" fn metadata(index: usize) -> Option<&'static NativeCleanupCapacityMetadata> {
        (index == 0).then_some(&METADATA)
    }

    #[test]
    fn product_capacity_routes_shared_ownership_through_retirement_without_scheduler() {
        let shared = cleanup_capacity_binding();
        let other = cleanup_capacity_binding();

        assert_eq!(
            cleanup_capacity_admission(&OWNER, 0, None),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        for (descriptor, capacity) in [(&OWNER, &shared), (&PEER, &shared), (&OTHER, &other)] {
            assert_eq!(
                control(
                    descriptor,
                    NativeProductHostOperation::ACQUIRE_ENTRY,
                    Some(capacity)
                )
                .status(),
                NativeProductHostStatus::SUCCESS
            );
        }

        assert_eq!(
            with_allocation_failure(|| cleanup_capacity_admission(&OWNER, 0, None)),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            cleanup_capacity_admission(&OWNER, 1, None),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            cleanup_capacity_admission(&OWNER, 1, Some(metadata)),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            cleanup_capacity_discharge(&OTHER, &METADATA),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            control(&OWNER, NativeProductHostOperation::CLOSE, None).state(),
            NativeProductHostState::CLOSING
        );

        assert_eq!(
            control(&OWNER, NativeProductHostOperation::RELEASE_ENTRY, None).state(),
            NativeProductHostState::RETIRING
        );

        // An existing retained owner's cleanup may construct another local while retiring.
        assert_eq!(
            cleanup_capacity_admission(&OWNER, 1, Some(metadata)),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            control(&OWNER, NativeProductHostOperation::ACQUIRE_ENTRY, None).state(),
            NativeProductHostState::RETIRING
        );

        with_allocation_failure(|| {
            assert_eq!(
                cleanup_capacity_discharge(&PEER, &METADATA),
                NativeRuntimeStatus::SUCCESS
            );

            assert_eq!(
                control(&OWNER, NativeProductHostOperation::OBSERVE, None).state(),
                NativeProductHostState::RETIRING
            );

            assert_eq!(
                cleanup_capacity_discharge(&PEER, &METADATA),
                NativeRuntimeStatus::SUCCESS
            );

            assert_eq!(
                control(&OWNER, NativeProductHostOperation::OBSERVE, None).state(),
                NativeProductHostState::CLOSED
            );

            assert_eq!(
                cleanup_capacity_admission(&OWNER, 0, None),
                NativeRuntimeStatus::INVALID_ARGUMENT
            );
        });

        assert_eq!(RELEASED.load(Ordering::Relaxed), 2);

        for descriptor in [&PEER, &OTHER] {
            control(descriptor, NativeProductHostOperation::CLOSE, None);
            control(descriptor, NativeProductHostOperation::RELEASE_ENTRY, None);

            assert_eq!(
                control(descriptor, NativeProductHostOperation::OBSERVE, None).state(),
                NativeProductHostState::CLOSED
            );
        }
    }
}
