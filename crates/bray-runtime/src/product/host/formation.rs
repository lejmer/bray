use std::collections::{BTreeMap, BTreeSet};

use bray_runtime_abi::{
    NativeProductHostDescriptor, NativeProductHostObservation, NativeProductHostState,
    NativeProductHostStatus, NativeRuntimeStatus, NativeStaticDuration, NativeStaticIdentity,
    PRODUCT_HOST_ABI_VERSION,
};

use super::super::cleanup::admit_finalizer;
use super::model::{
    MAXIMUM_STATIC_ENTRIES, ProductCleanup, ProductHost, ProductStatic, product_hosts, product_key,
    runtime_status,
};
use super::operations::host_failure;

extern "C" fn retain_resident_provider(product: usize) {
    let mut hosts = product_hosts()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let host = hosts
        .get_mut(&product)
        .expect("retained resident code must have its load binding");

    host.provider_roots = host
        .provider_roots
        .checked_add(1)
        .expect("provider reference counts must not overflow");
}

extern "C" fn release_resident_provider(product: usize) {
    let mut hosts = product_hosts()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    // Retirement removes the registry before releasing its last formation reference.
    if let Some(host) = hosts.get_mut(&product) {
        host.provider_roots = host
            .provider_roots
            .checked_sub(1)
            .expect("provider references release exactly once");
    }
}

extern "C" fn resident_provider_references(product: usize) -> usize {
    product_hosts()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&product)
        .expect("a residency observer must keep its load alive")
        .provider_roots
}

pub(crate) fn retain_provider(
    descriptor: &NativeProductHostDescriptor,
    destination: &mut bray_runtime_abi::NativeProviderOwner,
) -> NativeRuntimeStatus {
    if let Err(failure) = ensure_formed(descriptor) {
        return runtime_status(failure.status());
    }

    let product = product_key(descriptor);

    let (reference, closed) = {
        let mut hosts = match product_hosts().lock().map_err(|_| ()) {
            Ok(hosts) => hosts,
            Err(_) => return NativeRuntimeStatus::RUNTIME_FAILURE,
        };

        let Some(host) = hosts.get_mut(&product) else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        if !matches!(
            host.state,
            NativeProductHostState::OPEN
                | NativeProductHostState::CLOSING
                | NativeProductHostState::CLOSED
        ) {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        host.provider_calls = host
            .provider_calls
            .checked_add(1)
            .expect("simultaneous residency calls must not overflow");

        // Invalidate an off-lock retirement observation even if this loan finishes first.
        host.provider_revision = host
            .provider_revision
            .checked_add(1)
            .expect("provider acquisitions must not exhaust their revision range");

        (
            host.provider.reference(),
            host.state == NativeProductHostState::CLOSED,
        )
    };

    // A closed load may extend an existing callback owner's retention, but accept no new root.
    let owner = if closed && reference.can_retire() {
        Err(NativeRuntimeStatus::INVALID_ARGUMENT)
    } else {
        reference.retain()
    };

    {
        let mut hosts = product_hosts()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let host = hosts
            .get_mut(&product)
            .expect("a provider callback loan must prevent retirement");

        host.provider_calls -= 1;
    }

    match owner {
        Ok(owner) => {
            *destination = owner;

            NativeRuntimeStatus::SUCCESS
        }
        Err(status) => status,
    }
}

pub(super) fn ensure_formed(
    descriptor: &NativeProductHostDescriptor,
) -> Result<(), NativeProductHostObservation> {
    let product = product_key(descriptor);

    if product == usize::MAX {
        return Err(NativeProductHostObservation::invalid());
    }

    let binding = descriptor.binding();

    let compatible = descriptor.abi_version() == PRODUCT_HOST_ABI_VERSION
        && descriptor.required_services()
            & !(bray_runtime_abi::PRODUCT_HOST_SERVICES
                | bray_runtime_abi::PRODUCT_EXECUTION_SERVICES
                | bray_runtime_abi::PRODUCT_UNLOADABLE)
            == 0
        && binding.is_compatible(descriptor.required_services(), descriptor.domain())
        && (binding.load_identity() == 0 || binding.load_identity() == product);

    let hosts = product_hosts().lock().map_err(|_| host_failure())?;

    if let Some(host) = hosts.get(&product) {
        return existing_formation(host, descriptor, binding, compatible);
    }

    // Provider callbacks and runtime creation execute outside the registry lock.
    drop(hosts);

    let mut host = if compatible {
        let provider = binding
            .provider()
            .retain()
            .map_err(|_| NativeProductHostObservation::invalid())?;

        let mut host = read_descriptor(descriptor)
            .unwrap_or_else(|failure| ProductHost::failed(descriptor.identity(), failure.status()));

        host.binding = Some(binding);
        host.provider = provider;

        host
    } else {
        ProductHost::failed(
            descriptor.identity(),
            NativeProductHostStatus::INVALID_ARGUMENT,
        )
    };

    host.descriptor_address = std::ptr::from_ref(descriptor) as usize;

    let runtime = host.runtime.clone();

    let result = {
        let mut hosts = match product_hosts().lock().map_err(|_| ()) {
            Ok(hosts) => hosts,
            Err(_) => {
                if let Some(runtime) = runtime {
                    runtime.release();
                }

                return Err(host_failure());
            }
        };

        if descriptor.load_identity() != product {
            Err(NativeProductHostObservation::invalid())
        } else {
            match hosts.entry(product) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    let result = host
                        .formation_failure
                        .map_or(Ok(()), |status| Err(host.observation(status)));

                    if host.binding.is_some() && !binding.provider().is_retained() {
                        host.provider_roots = 1;

                        host.provider = bray_runtime_abi::NativeProviderOwner::adopt(
                            bray_runtime_abi::NativeProviderReference::new(
                                product,
                                retain_resident_provider,
                                release_resident_provider,
                                resident_provider_references,
                            ),
                        );
                    }

                    entry.insert(host);

                    return result;
                }
                std::collections::btree_map::Entry::Occupied(entry) =>
                    existing_formation(entry.get(), descriptor, binding, compatible),
            }
        }
    };

    if let Some(runtime) = runtime {
        runtime.release();
    }

    // Attempt-owned admission and provider references are released after unlocking.

    host.cleanups.clear();

    result
}

fn existing_formation(
    host: &ProductHost,
    descriptor: &NativeProductHostDescriptor,
    binding: bray_runtime_abi::NativeProductBinding,
    compatible: bool,
) -> Result<(), NativeProductHostObservation> {
    if host.descriptor_address != std::ptr::from_ref(descriptor) as usize
        || !compatible
        || host.binding.is_some_and(|formed| !formed.same_binding(binding))
    {
        return Err(host.observation(NativeProductHostStatus::INVALID_ARGUMENT));
    }

    host.formation_failure
        .map_or(Ok(()), |status| Err(host.observation(status)))
}

pub(super) fn retire_product(
    descriptor: &NativeProductHostDescriptor,
) -> NativeProductHostObservation {
    let product = descriptor.load_identity();

    let (provider, revision) = {
        let mut hosts = match product_hosts().lock().map_err(|_| ()) {
            Ok(hosts) => hosts,
            Err(_) => return host_failure(),
        };

        let Some(host) = hosts.get_mut(&product) else {
            return NativeProductHostObservation::invalid();
        };

        if host.descriptor_address != std::ptr::from_ref(descriptor) as usize {
            return NativeProductHostObservation::invalid();
        }

        if !matches!(
            host.state,
            NativeProductHostState::CLOSED | NativeProductHostState::FAILED
        ) || !host.is_quiescent()
            || host.cleanup_running
            || host.cleanup_blocked
            || host.provider_calls != 0
        {
            return host.observation(NativeProductHostStatus::PENDING);
        }

        // Prevent reentrant retirement from disposing the observer's provider context.
        host.cleanup_blocked = true;

        (host.provider.reference(), host.provider_revision)
    };

    let ready = provider.can_retire();

    let mut hosts = match product_hosts().lock().map_err(|_| ()) {
        Ok(hosts) => hosts,
        Err(_) => return host_failure(),
    };

    let host = hosts
        .get_mut(&product)
        .expect("a retirement observer must keep its load registered");

    host.cleanup_blocked = false;

    if !ready || host.provider_calls != 0 || host.provider_revision != revision {
        return host.observation(NativeProductHostStatus::PENDING);
    }

    let observation = host.observation(NativeProductHostStatus::SUCCESS);

    if !descriptor.retire_load_identity(product) {
        return NativeProductHostObservation::invalid();
    }

    let mut removed = hosts
        .remove(&product)
        .expect("retiring hosts remain registered until removal");

    let provider = std::mem::replace(
        &mut removed.provider,
        bray_runtime_abi::NativeProviderOwner::resident(),
    );

    drop(hosts);
    drop(removed);

    observation.with_retired_provider(provider)
}

fn read_descriptor(
    descriptor: &NativeProductHostDescriptor,
) -> Result<ProductHost, NativeProductHostObservation> {
    if descriptor.abi_version() != PRODUCT_HOST_ABI_VERSION
        || descriptor.static_count() > MAXIMUM_STATIC_ENTRIES
    {
        return Err(NativeProductHostObservation::invalid());
    }

    let mut identities = BTreeSet::new();
    let mut orders = BTreeSet::new();
    let mut statics = Vec::new();

    statics
        .try_reserve_exact(descriptor.static_count())
        .map_err(|_| host_failure())?;

    let mut dependency_tables = Vec::with_capacity(descriptor.static_count());

    for index in 0..descriptor.static_count() {
        let entry = descriptor.static_entry()(index);
        let finalizer = entry.finalizer();
        let execution = finalizer.execution();

        let valid_result_layout = finalizer.result_alignment().is_power_of_two()
            && (execution != bray_runtime_abi::NativeStaticFinalizerExecution::NONE
                || (finalizer.result_size() == 0 && finalizer.result_alignment() == 1));

        if entry.abi_version() != PRODUCT_HOST_ABI_VERSION
            || !entry.duration().is_known()
            || !execution.is_known()
            || !valid_result_layout
            || !identities.insert(entry.identity())
            || !orders.insert(entry.order())
            || entry.dependency_count() > MAXIMUM_STATIC_ENTRIES
        {
            return Err(NativeProductHostObservation::invalid());
        }

        let dependency = entry.dependency();

        let dependencies = (0..entry.dependency_count())
            .map(|index| dependency(index))
            .collect::<BTreeSet<_>>();

        if dependencies.contains(&entry.identity()) {
            return Err(NativeProductHostObservation::invalid());
        }

        if dependencies.len() != entry.dependency_count() {
            return Err(NativeProductHostObservation::invalid());
        }

        statics.push(ProductStatic {
            identity: entry.identity(),
            duration: entry.duration(),
            order: entry.order(),
            prepare: entry.prepare(),
            finalizer: entry.finalizer(),
            destroy: entry.destroy(),
            detach: entry.detach(),
        });

        dependency_tables.push((entry.identity(), dependencies));
    }

    statics.sort_unstable_by_key(|entry| entry.order);

    let order_by_identity = statics
        .iter()
        .map(|entry| (entry.identity, entry.order))
        .collect::<BTreeMap<_, _>>();

    if dependency_tables.iter().any(|(identity, dependencies)| {
        let Some(order) = order_by_identity.get(identity) else {
            return true;
        };

        dependencies.iter().any(|dependency| {
            order_by_identity
                .get(dependency)
                .is_none_or(|dependency_order| dependency_order <= order)
        })
    }) {
        return Err(NativeProductHostObservation::invalid());
    }

    let initialized_statics = statics
        .iter()
        .filter(|entry| entry.duration == NativeStaticDuration::PRODUCT)
        .count();

    let mut cleanups = Vec::new();

    cleanups
        .try_reserve_exact(initialized_statics)
        .map_err(|_| host_failure())?;

    for entry in statics
        .iter()
        .copied()
        .filter(|entry| entry.duration == NativeStaticDuration::PRODUCT)
    {
        let admission = admit_finalizer(entry.finalizer).map_err(|_| host_failure())?;

        cleanups.push(ProductCleanup {
            entry,
            admission,
            incidents: std::array::from_fn(|_| None),
        });
    }

    let runtime = (descriptor.required_services() & bray_runtime_abi::PRODUCT_EXECUTION_SERVICES
        != 0
        || statics.iter().any(|entry| {
            entry.finalizer.execution()
                == bray_runtime_abi::NativeStaticFinalizerExecution::ASYNCHRONOUS
        }))
    .then(crate::native::retain_runtime)
    .transpose()
    .map_err(|_| host_failure())?;

    Ok(ProductHost {
        identity: descriptor.identity(),
        descriptor_address: std::ptr::from_ref(descriptor) as usize,
        runtime,
        state: NativeProductHostState::OPEN,
        formation_failure: None,
        binding: None,
        provider: bray_runtime_abi::NativeProviderOwner::resident(),
        provider_roots: 0,
        provider_calls: 0,
        provider_revision: 0,
        active_entries: 0,
        external_roots: 0,
        thread_attachments: 0,
        worker_attachments: 0,
        initialized_statics,
        cleaned_statics: 0,
        cleanup_incidents: 0,
        last_incident: NativeStaticIdentity::new([0; 32]),
        cleanup_running: false,
        cleanup_blocked: false,
        statics,
        cleanups,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use bray_runtime_abi::{
        NativeProductBinding, NativeProductHostDescriptor, NativeProductHostOperation,
        NativeProductHostState, NativeProductHostStatus, NativeProductIdentity,
        NativeProviderOwner, NativeProviderReference, NativeStaticHostEntry,
        PRODUCT_EXECUTION_SERVICES, PRODUCT_HOST_SERVICES, PRODUCT_UNLOADABLE,
    };

    use super::super::operations::control;
    use super::{product_hosts, product_key, retain_provider};

    thread_local! {
        static REFERENCES: Cell<usize> = const { Cell::new(0) };
    }

    extern "C" fn retain(_: usize) {
        REFERENCES.with(|count| count.set(count.get() + 1));
    }
    extern "C" fn release(_: usize) {
        REFERENCES.with(|count| count.set(count.get() - 1));
    }
    extern "C" fn references(_: usize) -> usize {
        REFERENCES.get()
    }
    extern "C" fn no_entry(_: usize) -> NativeStaticHostEntry {
        panic!("empty hosts never request a static entry");
    }

    fn descriptor() -> NativeProductHostDescriptor {
        NativeProductHostDescriptor::new(NativeProductIdentity::new([81; 32]), no_entry, 0)
    }

    #[test]
    fn synchronous_formation_closes_statics_without_waiting_for_code_owners() {
        let descriptor = descriptor();

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::SUCCESS
        );

        let mut owner = NativeProviderOwner::resident();

        assert!(retain_provider(&descriptor, &mut owner).is_success());

        assert!(
            product_hosts()
                .lock()
                .unwrap()
                .get(&product_key(&descriptor))
                .unwrap()
                .runtime
                .is_none()
        );

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::CLOSE).state(),
            NativeProductHostState::CLOSED
        );

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::RETIRE).status(),
            NativeProductHostStatus::PENDING
        );

        let mut callback_report = NativeProviderOwner::resident();

        assert!(retain_provider(&descriptor, &mut callback_report).is_success());

        drop(owner);

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::RETIRE).status(),
            NativeProductHostStatus::PENDING
        );

        drop(callback_report);

        assert_eq!(
            retain_provider(&descriptor, &mut NativeProviderOwner::resident()),
            bray_runtime_abi::NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::RETIRE).status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(descriptor.load_identity(), usize::MAX);

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::INVALID_ARGUMENT
        );
    }

    #[test]
    fn reload_at_the_same_descriptor_address_gets_a_new_load() {
        let mut descriptor = descriptor();
        let address = std::ptr::from_ref(&descriptor).addr();

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::SUCCESS
        );

        let first = descriptor.load_identity();

        control(&descriptor, NativeProductHostOperation::CLOSE);

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::RETIRE).status(),
            NativeProductHostStatus::SUCCESS
        );

        descriptor =
            NativeProductHostDescriptor::new(NativeProductIdentity::new([81; 32]), no_entry, 0);

        assert_eq!(std::ptr::from_ref(&descriptor).addr(), address);

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_ne!(descriptor.load_identity(), first);
        assert!(!product_hosts().lock().unwrap().contains_key(&first));

        control(&descriptor, NativeProductHostOperation::CLOSE);

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::RETIRE).status(),
            NativeProductHostStatus::SUCCESS
        );
    }

    #[test]
    fn invalid_supplied_services_never_fall_back_and_failed_load_is_terminal() {
        let binding = NativeProductBinding::new(
            isize::MAX as usize + 80,
            PRODUCT_HOST_SERVICES,
            9,
            NativeProviderReference::new(80, retain, release, references),
        );

        let descriptor = descriptor().with_binding(
            PRODUCT_HOST_SERVICES | PRODUCT_EXECUTION_SERVICES | PRODUCT_UNLOADABLE,
            9,
            binding,
        );

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::INVALID_ARGUMENT
        );

        let identity = descriptor.load_identity();

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::INVALID_ARGUMENT
        );

        assert_eq!(descriptor.load_identity(), identity);
        assert_eq!(REFERENCES.get(), 0);

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::RETIRE).status(),
            NativeProductHostStatus::SUCCESS
        );
    }

    #[test]
    fn failed_static_validation_keeps_one_binding_until_retirement() {
        let binding = NativeProductBinding::new(
            isize::MAX as usize + 81,
            PRODUCT_HOST_SERVICES,
            9,
            NativeProviderReference::new(81, retain, release, references),
        );

        let descriptor = NativeProductHostDescriptor::new(
            NativeProductIdentity::new([81; 32]),
            no_entry,
            super::MAXIMUM_STATIC_ENTRIES + 1,
        )
        .with_binding(PRODUCT_HOST_SERVICES | PRODUCT_UNLOADABLE, 9, binding);

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::INVALID_ARGUMENT
        );

        assert_eq!(REFERENCES.get(), 1);

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::INVALID_ARGUMENT
        );

        assert_eq!(REFERENCES.get(), 1);

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::RETIRE).status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(REFERENCES.get(), 0);
    }

    #[test]
    fn competing_formation_rejects_a_reused_supplied_load_identity() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Barrier, OnceLock};

        static REFERENCES: AtomicUsize = AtomicUsize::new(0);
        static GATES: OnceLock<[Barrier; 2]> = OnceLock::new();

        extern "C" fn retain(context: usize) {
            REFERENCES.fetch_add(1, Ordering::SeqCst);

            if context == 0 {
                let gates = GATES.get().unwrap();

                gates[0].wait();
                gates[1].wait();
            }
        }

        extern "C" fn release(_: usize) {
            REFERENCES.fetch_sub(1, Ordering::SeqCst);
        }

        extern "C" fn references(_: usize) -> usize {
            REFERENCES.load(Ordering::SeqCst)
        }

        let gates = GATES.get_or_init(|| [Barrier::new(2), Barrier::new(2)]);
        let load = isize::MAX as usize + 83;

        let make = |context| {
            descriptor().with_binding(
                PRODUCT_HOST_SERVICES | PRODUCT_UNLOADABLE,
                9,
                NativeProductBinding::new(
                    load,
                    PRODUCT_HOST_SERVICES,
                    9,
                    NativeProviderReference::new(context, retain, release, references),
                ),
            )
        };

        let first = Arc::new(make(0));
        let competitor = make(1);

        let worker = std::thread::spawn({
            let first = Arc::clone(&first);

            move || control(&first, NativeProductHostOperation::FORM).status()
        });

        gates[0].wait();

        assert_eq!(
            control(&competitor, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::SUCCESS
        );

        gates[1].wait();

        assert_eq!(worker.join().unwrap(), NativeProductHostStatus::INVALID_ARGUMENT);
        assert_eq!(REFERENCES.load(Ordering::SeqCst), 1);

        assert_eq!(
            control(&first, NativeProductHostOperation::RETIRE).status(),
            NativeProductHostStatus::INVALID_ARGUMENT
        );

        control(&competitor, NativeProductHostOperation::CLOSE);

        assert_eq!(
            control(&competitor, NativeProductHostOperation::RETIRE).status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(REFERENCES.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn retirement_rechecks_acquisitions_completed_during_the_reference_observation() {
        use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
        use std::sync::{Arc, Barrier, OnceLock};

        static REFERENCES: AtomicUsize = AtomicUsize::new(0);
        static OBSERVING: AtomicBool = AtomicBool::new(false);
        static GATES: OnceLock<[Barrier; 6]> = OnceLock::new();

        thread_local! {
            static RETIRING: Cell<bool> = const { Cell::new(false) };
        }

        extern "C" fn retain(_: usize) {
            REFERENCES.fetch_add(1, Ordering::SeqCst);
        }

        extern "C" fn release(_: usize) {
            REFERENCES.fetch_sub(1, Ordering::SeqCst);
        }

        extern "C" fn references(_: usize) -> usize {
            if !OBSERVING.load(Ordering::SeqCst) {
                return REFERENCES.load(Ordering::SeqCst);
            }

            let gates = GATES.get().unwrap();

            if RETIRING.get() {
                gates[0].wait();
                gates[2].wait();

                let observed = REFERENCES.load(Ordering::SeqCst);

                gates[3].wait();
                gates[5].wait();

                observed
            } else {
                let observed = REFERENCES.load(Ordering::SeqCst);

                gates[1].wait();
                gates[4].wait();

                observed
            }
        }

        let gates = GATES.get_or_init(|| std::array::from_fn(|_| Barrier::new(2)));

        let binding = NativeProductBinding::new(
            isize::MAX as usize + 82,
            PRODUCT_HOST_SERVICES,
            9,
            NativeProviderReference::new(82, retain, release, references),
        );

        let descriptor = Arc::new(descriptor().with_binding(
            PRODUCT_HOST_SERVICES | PRODUCT_UNLOADABLE,
            9,
            binding,
        ));

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::SUCCESS
        );

        let mut original = NativeProviderOwner::resident();

        assert!(retain_provider(&descriptor, &mut original).is_success());

        control(&descriptor, NativeProductHostOperation::CLOSE);

        OBSERVING.store(true, Ordering::SeqCst);

        let retiring = Arc::clone(&descriptor);

        let retirement = std::thread::spawn(move || {
            RETIRING.set(true);

            control(&retiring, NativeProductHostOperation::RETIRE)
        });

        gates[0].wait();

        let retaining = Arc::clone(&descriptor);

        let acquisition = std::thread::spawn(move || {
            let mut owner = NativeProviderOwner::resident();

            assert!(retain_provider(&retaining, &mut owner).is_success());

            owner
        });

        gates[1].wait();

        drop(original);

        gates[2].wait();
        gates[3].wait();
        gates[4].wait();

        let owner = acquisition.join().unwrap();

        assert_eq!(REFERENCES.load(Ordering::SeqCst), 2);

        gates[5].wait();

        assert_eq!(retirement.join().unwrap().status(), NativeProductHostStatus::PENDING);

        OBSERVING.store(false, Ordering::SeqCst);

        drop(owner);

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::RETIRE).status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(REFERENCES.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn simultaneous_resident_formation_publishes_one_binding() {
        let descriptor = descriptor();

        std::thread::scope(|scope| {
            for _ in 0..8 {
                let descriptor = &descriptor;

                scope.spawn(move || {
                    assert_eq!(
                        control(descriptor, NativeProductHostOperation::FORM).status(),
                        NativeProductHostStatus::SUCCESS
                    )
                });
            }
        });

        control(&descriptor, NativeProductHostOperation::CLOSE);

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::RETIRE).status(),
            NativeProductHostStatus::SUCCESS
        );
    }
}
