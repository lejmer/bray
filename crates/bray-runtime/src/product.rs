use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Mutex, OnceLock};

use bray_runtime_abi::{
    NativeProductHostDescriptor, NativeProductHostObservation, NativeProductHostOperation,
    NativeProductHostState, NativeProductHostStatus, NativeProductIdentity, NativeRuntimeStatus,
    NativeStaticCleanupCallback, NativeStaticDuration, NativeStaticIdentity,
    NativeThreadStaticCleanupRegistration, PRODUCT_HOST_ABI_VERSION,
};

const MAXIMUM_STATIC_ENTRIES: usize = 1_000_000;

// Loaded products share one process registry so archive and shared-library hosts coordinate with
// exact-thread attachments owned by the same runtime.
static PRODUCT_HOSTS: OnceLock<Mutex<BTreeMap<usize, ProductHost>>> = OnceLock::new();

thread_local! {
    static THREAD_STATICS: RefCell<ThreadStaticRegistry> =
        const { RefCell::new(ThreadStaticRegistry::new()) };
    static FOREIGN_THREAD_ATTACHMENT: RefCell<Option<bray_platform::RuntimeThreadScope>> =
        const { RefCell::new(None) };
}

#[derive(Clone, Copy)]
struct ProductStatic {
    identity: NativeStaticIdentity,
    duration: NativeStaticDuration,
    order: u64,
    cleanup: NativeStaticCleanupCallback,
}

struct ProductHost {
    identity: NativeProductIdentity,
    state: NativeProductHostState,
    active_entries: usize,
    external_roots: usize,
    thread_attachments: usize,
    initialized_statics: usize,
    cleaned_statics: usize,
    cleanup_incidents: usize,
    last_incident: NativeStaticIdentity,
    cleanup_running: bool,
    statics: Vec<ProductStatic>,
}

impl ProductHost {
    fn observation(&self, status: NativeProductHostStatus) -> NativeProductHostObservation {
        NativeProductHostObservation::new(
            status,
            self.state,
            self.active_entries,
            self.external_roots,
            self.thread_attachments,
            self.initialized_statics,
            self.cleaned_statics,
            self.cleanup_incidents,
            self.last_incident,
        )
    }

    const fn is_quiescent(&self) -> bool {
        self.active_entries == 0 && self.external_roots == 0 && self.thread_attachments == 0
    }

    fn product_cleanups(&self) -> Vec<ProductStatic> {
        self.statics
            .iter()
            .copied()
            .filter(|entry| entry.duration == NativeStaticDuration::PRODUCT)
            .collect()
    }

    fn static_entry(&self, identity: NativeStaticIdentity) -> Option<ProductStatic> {
        self.statics
            .iter()
            .copied()
            .find(|entry| entry.identity == identity)
    }
}

struct PendingCleanup {
    product: usize,
    statics: Vec<ProductStatic>,
}

#[derive(Clone, Copy)]
struct ThreadStaticEntry {
    product: usize,
    product_identity: NativeProductIdentity,
    static_identity: NativeStaticIdentity,
    order: u64,
    callback: NativeStaticCleanupCallback,
}

struct ThreadStaticRegistry {
    entries: Vec<ThreadStaticEntry>,
    products: BTreeMap<usize, &'static NativeProductHostDescriptor>,
    callback_registered: bool,
}

impl ThreadStaticRegistry {
    const fn new() -> Self {
        Self {
            entries: Vec::new(),
            products: BTreeMap::new(),
            callback_registered: false,
        }
    }
}

pub(crate) fn control(
    descriptor: &NativeProductHostDescriptor,
    operation: NativeProductHostOperation,
) -> NativeProductHostObservation {
    if !operation.is_known() {
        return NativeProductHostObservation::invalid();
    }

    let product = product_key(descriptor);

    if operation == NativeProductHostOperation::FORM {
        let _ = reform_closed(descriptor, product);
    }

    if let Err(status) = ensure_formed(descriptor) {
        return status;
    }

    if operation == NativeProductHostOperation::ATTACH_CURRENT_THREAD {
        return attach_current_thread(product);
    }

    if operation == NativeProductHostOperation::DETACH_CURRENT_THREAD {
        return detach_current_thread(product);
    }

    let result = mutate_host(product, operation);

    match result {
        Ok((_observation, Some(cleanup))) => finish_cleanup(cleanup),
        Ok((observation, None)) => observation,
        Err(()) => NativeProductHostObservation::new(
            NativeProductHostStatus::RUNTIME_FAILURE,
            NativeProductHostState::FAILED,
            0,
            0,
            0,
            0,
            0,
            0,
            NativeStaticIdentity::new([0; 32]),
        ),
    }
}

pub(crate) fn register_thread_static(
    registration: &NativeThreadStaticCleanupRegistration,
) -> NativeRuntimeStatus {
    if bray_platform::current_runtime_thread().is_none() {
        return NativeRuntimeStatus::NOT_INITIALIZED;
    }

    if let Err(observation) = ensure_formed(registration.product()) {
        return runtime_status(observation.status());
    }

    let product = product_key(registration.product());

    let Some((product_identity, entry)) = static_entry(product, registration.static_identity())
    else {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    };

    if entry.duration != NativeStaticDuration::EXACT_THREAD {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    }

    THREAD_STATICS.with(|registry| {
        let mut registry = registry.borrow_mut();

        if registry.entries.iter().any(|registered| {
            registered.product == product
                && registered.static_identity == registration.static_identity()
        }) {
            return NativeRuntimeStatus::SUCCESS;
        }

        if !registry.callback_registered {
            if !bray_platform::register_runtime_thread_exit_callback(drain_thread_statics) {
                return NativeRuntimeStatus::NOT_INITIALIZED;
            }

            registry.callback_registered = true;
        }

        if registry
            .products
            .insert(product, registration.product())
            .is_none()
        {
            let observation = control(
                registration.product(),
                NativeProductHostOperation::ACQUIRE_ATTACHMENT,
            );

            if observation.status() != NativeProductHostStatus::SUCCESS {
                registry.products.remove(&product);

                return runtime_status(observation.status());
            }
        }

        registry.entries.push(ThreadStaticEntry {
            product,
            product_identity,
            static_identity: registration.static_identity(),
            order: entry.order,
            callback: registration.callback(),
        });

        NativeRuntimeStatus::SUCCESS
    })
}

fn mutate_host(
    product: usize,
    operation: NativeProductHostOperation,
) -> Result<(NativeProductHostObservation, Option<PendingCleanup>), ()> {
    let mut hosts = product_hosts().lock().map_err(|_| ())?;
    let host = hosts.get_mut(&product).ok_or(())?;

    let status = match operation.code() {
        0 | 6 => status_for_state(host),
        1 => acquire(&mut host.active_entries, host.state),
        2 => release(&mut host.active_entries),
        3 => acquire(&mut host.external_roots, host.state),
        4 => release(&mut host.external_roots),
        5 => {
            if host.state == NativeProductHostState::OPEN {
                host.state = NativeProductHostState::CLOSING;
            }

            status_for_state(host)
        }
        7 => acquire(&mut host.thread_attachments, host.state),
        8 => release(&mut host.thread_attachments),
        _ => NativeProductHostStatus::INVALID_ARGUMENT,
    };

    let cleanup = prepare_cleanup(product, host);

    let status = if cleanup.is_some()
        || (status == NativeProductHostStatus::SUCCESS
            && host.state == NativeProductHostState::CLOSING)
    {
        NativeProductHostStatus::PENDING
    } else {
        status
    };

    Ok((host.observation(status), cleanup))
}

fn attach_current_thread(product: usize) -> NativeProductHostObservation {
    let attached = FOREIGN_THREAD_ATTACHMENT.with(|attachment| {
        let mut attachment = attachment.borrow_mut();

        if attachment.is_some() || bray_platform::current_runtime_thread().is_some() {
            return false;
        }

        let Ok(scope) = bray_platform::RuntimeThreadScope::enter() else {
            return false;
        };

        *attachment = Some(scope);

        true
    });

    observation_with_status(
        product,
        if attached {
            NativeProductHostStatus::SUCCESS
        } else {
            NativeProductHostStatus::INVALID_ARGUMENT
        },
    )
}

fn detach_current_thread(product: usize) -> NativeProductHostObservation {
    let attachment = FOREIGN_THREAD_ATTACHMENT.with(|attachment| attachment.borrow_mut().take());

    let Some(attachment) = attachment else {
        return observation_with_status(product, NativeProductHostStatus::INVALID_ARGUMENT);
    };

    drop(attachment);

    observation_with_status(product, NativeProductHostStatus::SUCCESS)
}

fn observation_with_status(
    product: usize,
    status: NativeProductHostStatus,
) -> NativeProductHostObservation {
    let Ok(hosts) = product_hosts().lock() else {
        return NativeProductHostObservation::invalid();
    };

    hosts
        .get(&product)
        .map(|host| host.observation(status))
        .unwrap_or_else(NativeProductHostObservation::invalid)
}

fn acquire(count: &mut usize, state: NativeProductHostState) -> NativeProductHostStatus {
    if state != NativeProductHostState::OPEN {
        return NativeProductHostStatus::CLOSED;
    }

    let Some(next) = count.checked_add(1) else {
        return NativeProductHostStatus::RUNTIME_FAILURE;
    };

    *count = next;

    NativeProductHostStatus::SUCCESS
}

fn release(count: &mut usize) -> NativeProductHostStatus {
    let Some(next) = count.checked_sub(1) else {
        return NativeProductHostStatus::INVALID_ARGUMENT;
    };

    *count = next;

    NativeProductHostStatus::SUCCESS
}

fn status_for_state(host: &ProductHost) -> NativeProductHostStatus {
    match host.state {
        NativeProductHostState::UNFORMED => NativeProductHostStatus::INVALID_ARGUMENT,
        NativeProductHostState::OPEN => NativeProductHostStatus::SUCCESS,
        NativeProductHostState::CLOSING => NativeProductHostStatus::PENDING,
        NativeProductHostState::CLOSED if host.cleanup_incidents == 0 => {
            NativeProductHostStatus::CLOSED
        }
        NativeProductHostState::CLOSED => NativeProductHostStatus::INCIDENTS,
        _ => NativeProductHostStatus::RUNTIME_FAILURE,
    }
}

fn prepare_cleanup(product: usize, host: &mut ProductHost) -> Option<PendingCleanup> {
    if host.state != NativeProductHostState::CLOSING || !host.is_quiescent() || host.cleanup_running
    {
        return None;
    }

    host.cleanup_running = true;

    Some(PendingCleanup {
        product,
        statics: host.product_cleanups(),
    })
}

fn finish_cleanup(cleanup: PendingCleanup) -> NativeProductHostObservation {
    let mut incidents = Vec::new();

    for entry in &cleanup.statics {
        if catch_unwind(AssertUnwindSafe(|| (entry.cleanup)())).is_err() {
            incidents.push(entry.identity);
        }
    }

    let Ok(mut hosts) = product_hosts().lock() else {
        return NativeProductHostObservation::new(
            NativeProductHostStatus::RUNTIME_FAILURE,
            NativeProductHostState::FAILED,
            0,
            0,
            0,
            0,
            0,
            incidents.len(),
            incidents
                .last()
                .copied()
                .unwrap_or(NativeStaticIdentity::new([0; 32])),
        );
    };

    let Some(host) = hosts.get_mut(&cleanup.product) else {
        return NativeProductHostObservation::invalid();
    };

    host.cleaned_statics = host.cleaned_statics.saturating_add(cleanup.statics.len());
    host.cleanup_incidents = host.cleanup_incidents.saturating_add(incidents.len());
    host.last_incident = incidents.last().copied().unwrap_or(host.last_incident);
    host.cleanup_running = false;
    host.state = NativeProductHostState::CLOSED;

    host.observation(status_for_state(host))
}

fn ensure_formed(
    descriptor: &NativeProductHostDescriptor,
) -> Result<(), NativeProductHostObservation> {
    let product = product_key(descriptor);

    let mut hosts = product_hosts().lock().map_err(|_| {
        NativeProductHostObservation::new(
            NativeProductHostStatus::RUNTIME_FAILURE,
            NativeProductHostState::FAILED,
            0,
            0,
            0,
            0,
            0,
            0,
            NativeStaticIdentity::new([0; 32]),
        )
    })?;

    if hosts.contains_key(&product) {
        return Ok(());
    }

    let host = read_descriptor(descriptor).ok_or_else(NativeProductHostObservation::invalid)?;

    hosts.insert(product, host);

    Ok(())
}

fn reform_closed(descriptor: &NativeProductHostDescriptor, product: usize) -> Option<()> {
    let mut hosts = product_hosts().lock().ok()?;

    if hosts
        .get(&product)
        .is_none_or(|host| host.state != NativeProductHostState::CLOSED)
    {
        return Some(());
    }

    let host = read_descriptor(descriptor)?;

    hosts.insert(product, host);

    Some(())
}

fn read_descriptor(descriptor: &NativeProductHostDescriptor) -> Option<ProductHost> {
    if descriptor.abi_version() != PRODUCT_HOST_ABI_VERSION
        || descriptor.static_count() > MAXIMUM_STATIC_ENTRIES
    {
        return None;
    }

    let mut identities = BTreeSet::new();
    let mut orders = BTreeSet::new();
    let mut statics = Vec::with_capacity(descriptor.static_count());
    let mut dependency_tables = Vec::with_capacity(descriptor.static_count());

    for index in 0..descriptor.static_count() {
        let entry = (descriptor.static_entry())(index);

        if entry.abi_version() != PRODUCT_HOST_ABI_VERSION
            || !entry.duration().is_known()
            || !identities.insert(entry.identity())
            || !orders.insert(entry.order())
            || entry.dependency_count() > MAXIMUM_STATIC_ENTRIES
        {
            return None;
        }

        let dependency = entry.dependency();

        let dependencies = (0..entry.dependency_count())
            .map(|index| dependency(index))
            .collect::<BTreeSet<_>>();

        if dependencies.contains(&entry.identity()) {
            return None;
        }

        if dependencies.len() != entry.dependency_count() {
            return None;
        }

        statics.push(ProductStatic {
            identity: entry.identity(),
            duration: entry.duration(),
            order: entry.order(),
            cleanup: entry.cleanup(),
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
        return None;
    }

    let initialized_statics = statics
        .iter()
        .filter(|entry| entry.duration == NativeStaticDuration::PRODUCT)
        .count();

    Some(ProductHost {
        identity: descriptor.identity(),
        state: NativeProductHostState::OPEN,
        active_entries: 0,
        external_roots: 0,
        thread_attachments: 0,
        initialized_statics,
        cleaned_statics: 0,
        cleanup_incidents: 0,
        last_incident: NativeStaticIdentity::new([0; 32]),
        cleanup_running: false,
        statics,
    })
}

fn static_entry(
    product: usize,
    identity: NativeStaticIdentity,
) -> Option<(NativeProductIdentity, ProductStatic)> {
    let hosts = product_hosts().lock().ok()?;
    let host = hosts.get(&product)?;
    let entry = host.static_entry(identity)?;

    Some((host.identity, entry))
}

fn report_incident(product: usize, identity: NativeStaticIdentity) {
    let Ok(mut hosts) = product_hosts().lock() else {
        return;
    };

    let Some(host) = hosts.get_mut(&product) else {
        return;
    };

    host.cleanup_incidents = host.cleanup_incidents.saturating_add(1);
    host.last_incident = identity;
}

extern "C-unwind" fn drain_thread_statics() {
    let (mut entries, products) = THREAD_STATICS.with(|registry| {
        let mut registry = registry.borrow_mut();
        let entries = std::mem::take(&mut registry.entries);
        let products = std::mem::take(&mut registry.products);

        registry.callback_registered = false;

        (entries, products)
    });

    entries.sort_unstable_by_key(|entry| (entry.product_identity, entry.product, entry.order));

    for entry in entries {
        if catch_unwind(AssertUnwindSafe(|| (entry.callback)())).is_err() {
            report_incident(entry.product, entry.static_identity);
        }
    }

    for (_, descriptor) in products {
        let _ = control(descriptor, NativeProductHostOperation::RELEASE_ATTACHMENT);
    }
}

fn product_hosts() -> &'static Mutex<BTreeMap<usize, ProductHost>> {
    PRODUCT_HOSTS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn product_key(descriptor: &NativeProductHostDescriptor) -> usize {
    std::ptr::from_ref(descriptor) as usize
}

fn runtime_status(status: NativeProductHostStatus) -> NativeRuntimeStatus {
    match status {
        NativeProductHostStatus::SUCCESS => NativeRuntimeStatus::SUCCESS,
        NativeProductHostStatus::CLOSED | NativeProductHostStatus::INVALID_ARGUMENT => {
            NativeRuntimeStatus::INVALID_ARGUMENT
        }
        _ => NativeRuntimeStatus::RUNTIME_FAILURE,
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::sync::Arc;
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{
        NativeProductHostDescriptor, NativeProductHostOperation, NativeProductHostState,
        NativeProductHostStatus, NativeProductIdentity, NativeStaticDuration,
        NativeStaticHostEntry, NativeStaticIdentity, NativeThreadStaticCleanupRegistration,
    };

    use super::{control, register_thread_static};

    static CLEANUPS: AtomicUsize = AtomicUsize::new(0);
    static CONTINUING_PRODUCT_CLEANUPS: AtomicUsize = AtomicUsize::new(0);

    thread_local! {
        static THREAD_CLEANUP_ORDER: Cell<usize> = const { Cell::new(0) };
    }

    extern "C" fn access() -> usize {
        1
    }

    extern "C-unwind" fn cleanup() {
        CLEANUPS.fetch_add(1, Ordering::SeqCst);
    }

    extern "C-unwind" fn first_thread_cleanup() {
        append_thread_cleanup(1);
    }

    extern "C-unwind" fn second_thread_cleanup() {
        append_thread_cleanup(2);
    }

    extern "C-unwind" fn panicking_thread_cleanup() {
        panic!("test thread-static cleanup incident");
    }

    extern "C-unwind" fn panicking_product_cleanup() {
        panic!("test product-static cleanup incident");
    }

    extern "C-unwind" fn continuing_product_cleanup() {
        CONTINUING_PRODUCT_CLEANUPS.fetch_add(1, Ordering::SeqCst);
    }

    extern "C" fn no_dependency(_: usize) -> NativeStaticIdentity {
        NativeStaticIdentity::new([0; 32])
    }

    extern "C" fn first_thread_dependency(index: usize) -> NativeStaticIdentity {
        match index {
            0 => NativeStaticIdentity::new([12; 32]),
            _ => NativeStaticIdentity::new([0; 32]),
        }
    }

    static DELAYED_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::PRODUCT,
        NativeStaticIdentity::new([7; 32]),
        0,
        1,
        access,
        cleanup,
        no_dependency,
        0,
    );

    static PRODUCT_PANICKING_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::PRODUCT,
        NativeStaticIdentity::new([41; 32]),
        0,
        1,
        access,
        panicking_product_cleanup,
        no_dependency,
        0,
    );

    static PRODUCT_CONTINUING_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::PRODUCT,
        NativeStaticIdentity::new([42; 32]),
        1,
        2,
        access,
        continuing_product_cleanup,
        no_dependency,
        0,
    );

    static THREAD_FIRST_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::EXACT_THREAD,
        NativeStaticIdentity::new([11; 32]),
        0,
        0,
        access,
        first_thread_cleanup,
        first_thread_dependency,
        1,
    );

    static THREAD_SECOND_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::EXACT_THREAD,
        NativeStaticIdentity::new([12; 32]),
        1,
        0,
        access,
        second_thread_cleanup,
        no_dependency,
        0,
    );

    static THREAD_PANICKING_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::EXACT_THREAD,
        NativeStaticIdentity::new([21; 32]),
        0,
        0,
        access,
        panicking_thread_cleanup,
        no_dependency,
        0,
    );

    static THREAD_CONTINUING_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::EXACT_THREAD,
        NativeStaticIdentity::new([22; 32]),
        1,
        0,
        access,
        second_thread_cleanup,
        no_dependency,
        0,
    );

    extern "C" fn delayed_entry(_: usize) -> NativeStaticHostEntry {
        DELAYED_ENTRY
    }

    extern "C" fn product_incident_entry(index: usize) -> NativeStaticHostEntry {
        match index {
            0 => PRODUCT_PANICKING_ENTRY,
            _ => PRODUCT_CONTINUING_ENTRY,
        }
    }

    extern "C" fn thread_order_entry(index: usize) -> NativeStaticHostEntry {
        match index {
            0 => THREAD_FIRST_ENTRY,
            _ => THREAD_SECOND_ENTRY,
        }
    }

    extern "C" fn thread_incident_entry(index: usize) -> NativeStaticHostEntry {
        match index {
            0 => THREAD_PANICKING_ENTRY,
            _ => THREAD_CONTINUING_ENTRY,
        }
    }

    fn append_thread_cleanup(value: usize) {
        THREAD_CLEANUP_ORDER.with(|order| {
            let value = order
                .get()
                .checked_mul(10)
                .and_then(|current| current.checked_add(value))
                .unwrap_or_else(|| panic!("test cleanup order must remain representable"));

            order.set(value);
        });
    }

    #[test]
    fn product_host_delays_cleanup_until_every_obligation_is_released() {
        CLEANUPS.store(0, Ordering::SeqCst);

        let descriptor =
            NativeProductHostDescriptor::new(NativeProductIdentity::new([9; 32]), delayed_entry, 1);

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::ACQUIRE_ENTRY).status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::ACQUIRE_EXTERNAL).status(),
            NativeProductHostStatus::SUCCESS
        );

        let closing = control(&descriptor, NativeProductHostOperation::CLOSE);

        assert_eq!(closing.state(), NativeProductHostState::CLOSING);
        assert_eq!(closing.status(), NativeProductHostStatus::PENDING);
        assert_eq!(CLEANUPS.load(Ordering::SeqCst), 0);

        let pending = control(&descriptor, NativeProductHostOperation::RELEASE_ENTRY);

        assert_eq!(pending.state(), NativeProductHostState::CLOSING);
        assert_eq!(CLEANUPS.load(Ordering::SeqCst), 0);

        let closed = control(&descriptor, NativeProductHostOperation::RELEASE_EXTERNAL);

        assert_eq!(closed.state(), NativeProductHostState::CLOSED);
        assert_eq!(closed.status(), NativeProductHostStatus::CLOSED);
        assert_eq!(closed.cleaned_statics(), 1);
        assert_eq!(CLEANUPS.load(Ordering::SeqCst), 1);

        let observed = control(&descriptor, NativeProductHostOperation::OBSERVE);

        assert_eq!(observed.state(), NativeProductHostState::CLOSED);
        assert_eq!(CLEANUPS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn product_host_serializes_entry_acquisition_with_closure() {
        let descriptor: &'static NativeProductHostDescriptor =
            Box::leak(Box::new(NativeProductHostDescriptor::new(
                NativeProductIdentity::new([31; 32]),
                delayed_entry,
                0,
            )));

        let barrier = Arc::new(Barrier::new(9));

        assert_eq!(
            control(descriptor, NativeProductHostOperation::ACQUIRE_EXTERNAL).status(),
            NativeProductHostStatus::SUCCESS
        );

        let workers = (0..8)
            .map(|_| {
                let barrier = Arc::clone(&barrier);

                std::thread::spawn(move || {
                    barrier.wait();

                    for _ in 0..100 {
                        let acquired =
                            control(descriptor, NativeProductHostOperation::ACQUIRE_ENTRY);

                        match acquired.status() {
                            NativeProductHostStatus::SUCCESS => {
                                assert!(matches!(
                                    control(descriptor, NativeProductHostOperation::RELEASE_ENTRY,)
                                        .status(),
                                    NativeProductHostStatus::SUCCESS
                                        | NativeProductHostStatus::PENDING
                                ));
                            }
                            NativeProductHostStatus::CLOSED => {}
                            status => panic!("entry race returned unexpected status {status:?}"),
                        }
                    }
                })
            })
            .collect::<Vec<_>>();

        barrier.wait();

        let closing = control(descriptor, NativeProductHostOperation::CLOSE);

        assert_eq!(closing.status(), NativeProductHostStatus::PENDING);

        for worker in workers {
            worker
                .join()
                .unwrap_or_else(|error| panic!("entry worker must finish: {error:?}"));
        }

        let closed = control(descriptor, NativeProductHostOperation::RELEASE_EXTERNAL);

        assert_eq!(closed.state(), NativeProductHostState::CLOSED);
        assert_eq!(closed.active_entries(), 0);
    }

    #[test]
    fn product_cleanup_reports_incidents_and_continues() {
        CONTINUING_PRODUCT_CLEANUPS.store(0, Ordering::SeqCst);

        let panicking_identity = NativeStaticIdentity::new([41; 32]);

        let descriptor = NativeProductHostDescriptor::new(
            NativeProductIdentity::new([43; 32]),
            product_incident_entry,
            2,
        );

        let closed = control(&descriptor, NativeProductHostOperation::CLOSE);

        assert_eq!(closed.state(), NativeProductHostState::CLOSED);
        assert_eq!(closed.status(), NativeProductHostStatus::INCIDENTS);
        assert_eq!(closed.cleanup_incidents(), 1);
        assert_eq!(closed.last_incident(), panicking_identity);
        assert_eq!(closed.cleaned_statics(), 2);
        assert_eq!(CONTINUING_PRODUCT_CLEANUPS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn thread_static_cleanup_follows_host_order_for_each_exact_attachment() {
        THREAD_CLEANUP_ORDER.set(0);

        let first_identity = NativeStaticIdentity::new([11; 32]);
        let second_identity = NativeStaticIdentity::new([12; 32]);

        let descriptor = Box::leak(Box::new(NativeProductHostDescriptor::new(
            NativeProductIdentity::new([13; 32]),
            thread_order_entry,
            2,
        )));

        let first_registration = NativeThreadStaticCleanupRegistration::new(
            descriptor,
            first_identity,
            first_thread_cleanup,
        );

        let second_registration = NativeThreadStaticCleanupRegistration::new(
            descriptor,
            second_identity,
            second_thread_cleanup,
        );

        let scope = bray_platform::RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("test thread must attach: {error:?}"));

        assert!(register_thread_static(&second_registration).is_success());
        assert!(register_thread_static(&first_registration).is_success());
        assert!(register_thread_static(&first_registration).is_success());

        drop(scope);

        assert_eq!(THREAD_CLEANUP_ORDER.get(), 12);

        let observed = control(descriptor, NativeProductHostOperation::OBSERVE);

        assert_eq!(observed.state(), NativeProductHostState::OPEN);
        assert_eq!(observed.thread_attachments(), 0);

        let scope = bray_platform::RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("test thread must reattach: {error:?}"));

        assert!(register_thread_static(&first_registration).is_success());

        drop(scope);

        assert_eq!(THREAD_CLEANUP_ORDER.get(), 121);

        assert_eq!(
            control(descriptor, NativeProductHostOperation::CLOSE).state(),
            NativeProductHostState::CLOSED
        );
    }

    #[test]
    fn thread_static_cleanup_reports_incidents_and_continues() {
        THREAD_CLEANUP_ORDER.set(0);

        let panicking_identity = NativeStaticIdentity::new([21; 32]);
        let continuing_identity = NativeStaticIdentity::new([22; 32]);

        let descriptor = Box::leak(Box::new(NativeProductHostDescriptor::new(
            NativeProductIdentity::new([23; 32]),
            thread_incident_entry,
            2,
        )));

        let panicking_registration = NativeThreadStaticCleanupRegistration::new(
            descriptor,
            panicking_identity,
            panicking_thread_cleanup,
        );

        let continuing_registration = NativeThreadStaticCleanupRegistration::new(
            descriptor,
            continuing_identity,
            second_thread_cleanup,
        );

        let scope = bray_platform::RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("test thread must attach: {error:?}"));

        assert!(register_thread_static(&panicking_registration).is_success());
        assert!(register_thread_static(&continuing_registration).is_success());

        drop(scope);

        let observed = control(descriptor, NativeProductHostOperation::OBSERVE);

        assert_eq!(THREAD_CLEANUP_ORDER.get(), 2);
        assert_eq!(observed.cleanup_incidents(), 1);
        assert_eq!(observed.last_incident(), panicking_identity);
    }
}
