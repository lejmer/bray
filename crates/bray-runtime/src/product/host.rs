use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Mutex, OnceLock};

use bray_runtime_abi::{
    NativeProductHostDescriptor, NativeProductHostObservation, NativeProductHostOperation,
    NativeProductHostState, NativeProductHostStatus, NativeProductIdentity, NativeRuntimeStatus,
    NativeStaticCleanupCallback, NativeStaticDuration, NativeStaticFinalizer, NativeStaticIdentity,
    NativeStaticTransitionCallback, NativeThreadStaticCleanupRegistration,
    PRODUCT_HOST_ABI_VERSION,
};

use super::cleanup::run_static_cleanup;

const MAXIMUM_STATIC_ENTRIES: usize = 1_000_000;

// Loaded products share one process registry so archive and shared-library hosts coordinate with
// exact-thread attachments owned by the same runtime.
static PRODUCT_HOSTS: OnceLock<Mutex<BTreeMap<usize, ProductHost>>> = OnceLock::new();

thread_local! {
    static THREAD_STATICS: RefCell<ThreadStaticRegistry> =
        const { RefCell::new(ThreadStaticRegistry::new()) };
}

#[derive(Clone, Copy)]
struct ProductStatic {
    identity: NativeStaticIdentity,
    duration: NativeStaticDuration,
    order: u64,
    prepare: NativeStaticTransitionCallback,
    finalizer: NativeStaticFinalizer,
    destroy: NativeStaticCleanupCallback,
    detach: NativeStaticTransitionCallback,
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
    prepare: NativeStaticTransitionCallback,
    finalizer: NativeStaticFinalizer,
    destroy: NativeStaticCleanupCallback,
    detach: NativeStaticTransitionCallback,
}

#[derive(Clone, Copy)]
struct ThreadProductAttachment {
    identity: u64,
    acquired: bool,
}

struct ThreadStaticRegistry {
    entries: Vec<ThreadStaticEntry>,
    products: BTreeMap<usize, ThreadProductAttachment>,
    callback_registered: bool,
    next_identity: u64,
}

impl ThreadStaticRegistry {
    const fn new() -> Self {
        Self {
            entries: Vec::new(),
            products: BTreeMap::new(),
            callback_registered: false,
            next_identity: 1,
        }
    }

    fn attachment(&mut self, product: usize) -> Option<ThreadProductAttachment> {
        if let Some(attachment) = self.products.get(&product) {
            return Some(*attachment);
        }

        let identity = self.next_identity;

        if identity == 0 || identity == u64::MAX {
            return None;
        }

        self.next_identity = identity.checked_add(1).unwrap_or(0);

        let attachment = ThreadProductAttachment {
            identity,
            acquired: false,
        };

        self.products.insert(product, attachment);

        Some(attachment)
    }

    fn ensure_exit_callback(&mut self) -> bool {
        if self.callback_registered {
            return true;
        }

        if !bray_platform::register_runtime_thread_exit_callback(drain_thread_statics) {
            return false;
        }

        self.callback_registered = true;

        true
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

    if let Err(status) = ensure_formed(descriptor) {
        return status;
    }

    if operation == NativeProductHostOperation::ATTACH_CURRENT_THREAD {
        return super::attachment::attach_current_thread(product);
    }

    if operation == NativeProductHostOperation::DETACH_CURRENT_THREAD {
        return super::attachment::detach_current_thread(product);
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

pub(crate) fn thread_attachment_identity(descriptor: &'static NativeProductHostDescriptor) -> u64 {
    if bray_platform::current_runtime_thread().is_none() {
        return 0;
    }

    if ensure_formed(descriptor).is_err() {
        return 0;
    }

    let product = product_key(descriptor);

    let allowed = THREAD_STATICS.with(|registry| {
        if registry.borrow().products.contains_key(&product) {
            return true;
        }

        product_hosts()
            .lock()
            .ok()
            .and_then(|hosts| hosts.get(&product).map(|host| host.state))
            == Some(NativeProductHostState::OPEN)
    });

    if !allowed {
        return 0;
    }

    THREAD_STATICS.with(|registry| {
        registry
            .borrow_mut()
            .attachment(product)
            .map_or(0, |attachment| attachment.identity)
    })
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

        if !registry.ensure_exit_callback() {
            return NativeRuntimeStatus::NOT_INITIALIZED;
        }

        let Some(attachment) = registry.attachment(product) else {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        };

        if !attachment.acquired {
            let observation = control(
                registration.product(),
                NativeProductHostOperation::ACQUIRE_ATTACHMENT,
            );

            if observation.status() != NativeProductHostStatus::SUCCESS {
                registry.products.remove(&product);

                return runtime_status(observation.status());
            }

            let Some(attachment) = registry.products.get_mut(&product) else {
                return NativeRuntimeStatus::RUNTIME_FAILURE;
            };

            attachment.acquired = true;
        }

        registry.entries.push(ThreadStaticEntry {
            product,
            product_identity,
            static_identity: registration.static_identity(),
            order: entry.order,
            prepare: registration.prepare(),
            finalizer: registration.finalizer(),
            destroy: registration.destroy(),
            detach: registration.detach(),
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

pub(super) fn prepare_thread_attachment(product: usize) -> Option<bool> {
    THREAD_STATICS.with(|registry| {
        let mut registry = registry.borrow_mut();

        if !registry.ensure_exit_callback() {
            return None;
        }

        registry
            .attachment(product)
            .map(|attachment| attachment.acquired)
    })
}

pub(super) fn acquire_thread_attachment(product: usize) -> NativeProductHostObservation {
    mutate_host(product, NativeProductHostOperation::ACQUIRE_ATTACHMENT)
        .map(|(observation, _)| observation)
        .unwrap_or_else(|_| NativeProductHostObservation::invalid())
}

pub(super) fn mark_thread_attachment_acquired(product: usize) {
    THREAD_STATICS.with(|registry| {
        if let Some(attachment) = registry.borrow_mut().products.get_mut(&product) {
            attachment.acquired = true;
        }
    });
}

pub(super) fn discard_thread_attachment(product: usize) {
    THREAD_STATICS.with(|registry| {
        registry.borrow_mut().products.remove(&product);
    });
}

pub(super) fn observation_with_status(
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
    let (mut incidents, runtime_incidents) = crate::native::with_static_cleanup_runtime(|| {
        let mut incidents = Vec::new();

        for entry in &cleanup.statics {
            incidents.extend(
                run_static_cleanup(entry.prepare, entry.finalizer, entry.destroy, entry.detach)
                    .into_iter()
                    .map(|incident| (entry.identity, incident)),
            );
        }

        incidents
    });

    let runtime_identity = cleanup
        .statics
        .first()
        .map_or(NativeStaticIdentity::new([0; 32]), |entry| entry.identity);

    incidents.extend(
        runtime_incidents
            .into_iter()
            .map(|incident| (runtime_identity, incident)),
    );

    let incident_count = incidents.len();
    let last_incident = incidents.last().map(|(identity, _)| *identity);

    for (_, incident) in incidents {
        let _ = incident.report();
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
            incident_count,
            last_incident.unwrap_or(NativeStaticIdentity::new([0; 32])),
        );
    };

    let Some(host) = hosts.get_mut(&cleanup.product) else {
        return NativeProductHostObservation::invalid();
    };

    host.cleaned_statics = host.cleaned_statics.saturating_add(cleanup.statics.len());
    host.cleanup_incidents = host.cleanup_incidents.saturating_add(incident_count);
    host.last_incident = last_incident.unwrap_or(host.last_incident);
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

fn report_incidents(product: usize, identity: NativeStaticIdentity, count: usize) {
    let Ok(mut hosts) = product_hosts().lock() else {
        return;
    };

    let Some(host) = hosts.get_mut(&product) else {
        return;
    };

    host.cleanup_incidents = host.cleanup_incidents.saturating_add(count);

    if count != 0 {
        host.last_incident = identity;
    }
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

    run_thread_cleanups(entries);

    for (product, attachment) in products {
        if attachment.acquired {
            let _ = release_attachment(product);
        }
    }
}

pub(super) fn drain_product_thread_statics(product: usize) -> Option<NativeProductHostObservation> {
    let (mut entries, attachment) = THREAD_STATICS.with(|registry| {
        let mut registry = registry.borrow_mut();
        let mut selected = Vec::new();

        registry.entries.retain(|entry| {
            if entry.product == product {
                selected.push(*entry);

                false
            } else {
                true
            }
        });

        let attachment = registry.products.remove(&product);

        (selected, attachment)
    });

    entries.sort_unstable_by_key(|entry| entry.order);

    run_thread_cleanups(entries);

    if let Some(attachment) = attachment
        && attachment.acquired
    {
        return release_attachment(product);
    }

    None
}

fn run_thread_cleanups(entries: Vec<ThreadStaticEntry>) {
    let runtime_owner = entries.first().copied();

    let (_, runtime_incidents) = crate::native::with_static_cleanup_runtime(|| {
        for entry in entries {
            let incidents =
                run_static_cleanup(entry.prepare, entry.finalizer, entry.destroy, entry.detach);

            let count = incidents.len();

            for incident in incidents {
                let _ = incident.report();
            }

            report_incidents(entry.product, entry.static_identity, count);
        }
    });

    let Some(owner) = runtime_owner else {
        return;
    };

    let count = runtime_incidents.len();

    for incident in runtime_incidents {
        let _ = incident.report();
    }

    report_incidents(owner.product, owner.static_identity, count);
}

fn release_attachment(product: usize) -> Option<NativeProductHostObservation> {
    let Ok((observation, cleanup)) =
        mutate_host(product, NativeProductHostOperation::RELEASE_ATTACHMENT)
    else {
        return None;
    };

    if let Some(cleanup) = cleanup {
        return Some(finish_cleanup(cleanup));
    }

    Some(observation)
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
        NativeStaticFinalizer, NativeStaticFinalizerExecution, NativeStaticFinalizerStartCallback,
        NativeStaticFinalizerStatus, NativeStaticHostEntry, NativeStaticIdentity,
        NativeThreadStaticCleanupRegistration,
    };

    use super::{control, register_thread_static, thread_attachment_identity};

    static CLEANUPS: AtomicUsize = AtomicUsize::new(0);
    static CONTINUING_PRODUCT_CLEANUPS: AtomicUsize = AtomicUsize::new(0);
    static PRODUCT_DESTRUCTIONS: AtomicUsize = AtomicUsize::new(0);
    static PRODUCT_PHASE_ORDER: AtomicUsize = AtomicUsize::new(0);

    thread_local! {
        static THREAD_CLEANUP_ORDER: Cell<usize> = const { Cell::new(0) };
    }

    extern "C" fn access() -> usize {
        1
    }

    extern "C-unwind" fn cleanup(_: usize) -> NativeStaticFinalizerStatus {
        CLEANUPS.fetch_add(1, Ordering::SeqCst);

        NativeStaticFinalizerStatus::SUCCESS
    }

    extern "C-unwind" fn first_thread_cleanup(_: usize) -> NativeStaticFinalizerStatus {
        append_thread_cleanup(1);

        NativeStaticFinalizerStatus::SUCCESS
    }

    extern "C-unwind" fn second_thread_cleanup(_: usize) -> NativeStaticFinalizerStatus {
        append_thread_cleanup(2);

        NativeStaticFinalizerStatus::SUCCESS
    }

    extern "C-unwind" fn panicking_thread_cleanup(_: usize) -> NativeStaticFinalizerStatus {
        panic!("test thread-static cleanup incident");
    }

    extern "C-unwind" fn panicking_product_cleanup(_: usize) -> NativeStaticFinalizerStatus {
        record_product_phase(1);
        panic!("test product-static cleanup incident");
    }

    extern "C-unwind" fn continuing_product_cleanup(_: usize) -> NativeStaticFinalizerStatus {
        record_product_phase(1);
        CONTINUING_PRODUCT_CLEANUPS.fetch_add(1, Ordering::SeqCst);

        NativeStaticFinalizerStatus::SUCCESS
    }

    extern "C-unwind" fn resolve_success(_: usize, _: usize) -> NativeStaticFinalizerStatus {
        NativeStaticFinalizerStatus::SUCCESS
    }

    const fn finalizer(start: NativeStaticFinalizerStartCallback) -> NativeStaticFinalizer {
        NativeStaticFinalizer::new(
            NativeStaticFinalizerExecution::SYNCHRONOUS,
            0,
            1,
            start,
            resolve_success,
        )
    }

    extern "C" fn detach_thread_static() {}

    extern "C-unwind" fn no_cleanup() {}

    extern "C-unwind" fn record_product_destruction() {
        record_product_phase(2);
        PRODUCT_DESTRUCTIONS.fetch_add(1, Ordering::SeqCst);
    }

    fn record_product_phase(phase: usize) {
        PRODUCT_PHASE_ORDER
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |order| {
                order.checked_mul(10)?.checked_add(phase)
            })
            .unwrap_or_else(|order| panic!("cleanup phase order overflowed from {order}"));
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
        detach_thread_static,
        finalizer(cleanup),
        no_cleanup,
        detach_thread_static,
        no_dependency,
        0,
    );

    static PRODUCT_PANICKING_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::PRODUCT,
        NativeStaticIdentity::new([41; 32]),
        0,
        1,
        access,
        detach_thread_static,
        finalizer(panicking_product_cleanup),
        record_product_destruction,
        detach_thread_static,
        no_dependency,
        0,
    );

    static PRODUCT_CONTINUING_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::PRODUCT,
        NativeStaticIdentity::new([42; 32]),
        1,
        2,
        access,
        detach_thread_static,
        finalizer(continuing_product_cleanup),
        record_product_destruction,
        detach_thread_static,
        no_dependency,
        0,
    );

    static THREAD_FIRST_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::EXACT_THREAD,
        NativeStaticIdentity::new([11; 32]),
        0,
        0,
        access,
        detach_thread_static,
        finalizer(first_thread_cleanup),
        no_cleanup,
        detach_thread_static,
        first_thread_dependency,
        1,
    );

    static THREAD_SECOND_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::EXACT_THREAD,
        NativeStaticIdentity::new([12; 32]),
        1,
        0,
        access,
        detach_thread_static,
        finalizer(second_thread_cleanup),
        no_cleanup,
        detach_thread_static,
        no_dependency,
        0,
    );

    static THREAD_PANICKING_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::EXACT_THREAD,
        NativeStaticIdentity::new([21; 32]),
        0,
        0,
        access,
        detach_thread_static,
        finalizer(panicking_thread_cleanup),
        no_cleanup,
        detach_thread_static,
        no_dependency,
        0,
    );

    static THREAD_CONTINUING_ENTRY: NativeStaticHostEntry = NativeStaticHostEntry::new(
        NativeStaticDuration::EXACT_THREAD,
        NativeStaticIdentity::new([22; 32]),
        1,
        0,
        access,
        detach_thread_static,
        finalizer(second_thread_cleanup),
        no_cleanup,
        detach_thread_static,
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

        let formed_again = control(&descriptor, NativeProductHostOperation::FORM);

        assert_eq!(formed_again.state(), NativeProductHostState::CLOSED);
        assert_eq!(formed_again.status(), NativeProductHostStatus::CLOSED);
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
        PRODUCT_DESTRUCTIONS.store(0, Ordering::SeqCst);
        PRODUCT_PHASE_ORDER.store(0, Ordering::SeqCst);

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
        assert_eq!(PRODUCT_DESTRUCTIONS.load(Ordering::SeqCst), 2);
        assert_eq!(PRODUCT_PHASE_ORDER.load(Ordering::SeqCst), 1212);
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
            detach_thread_static,
            finalizer(first_thread_cleanup),
            no_cleanup,
            detach_thread_static,
        );

        let second_registration = NativeThreadStaticCleanupRegistration::new(
            descriptor,
            second_identity,
            detach_thread_static,
            finalizer(second_thread_cleanup),
            no_cleanup,
            detach_thread_static,
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
            detach_thread_static,
            finalizer(panicking_thread_cleanup),
            no_cleanup,
            detach_thread_static,
        );

        let continuing_registration = NativeThreadStaticCleanupRegistration::new(
            descriptor,
            continuing_identity,
            detach_thread_static,
            finalizer(second_thread_cleanup),
            no_cleanup,
            detach_thread_static,
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

    #[test]
    fn foreign_thread_attachments_are_product_scoped() {
        THREAD_CLEANUP_ORDER.set(0);

        let first_identity = NativeStaticIdentity::new([11; 32]);
        let second_identity = NativeStaticIdentity::new([12; 32]);

        let first_product = Box::leak(Box::new(NativeProductHostDescriptor::new(
            NativeProductIdentity::new([51; 32]),
            thread_order_entry,
            2,
        )));

        let second_product = Box::leak(Box::new(NativeProductHostDescriptor::new(
            NativeProductIdentity::new([52; 32]),
            thread_order_entry,
            2,
        )));

        assert_eq!(
            control(
                first_product,
                NativeProductHostOperation::ATTACH_CURRENT_THREAD,
            )
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(
            control(
                second_product,
                NativeProductHostOperation::ATTACH_CURRENT_THREAD,
            )
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        let first_attachment = thread_attachment_identity(first_product);
        let second_attachment = thread_attachment_identity(second_product);

        assert_ne!(first_attachment, 0);
        assert_ne!(second_attachment, 0);
        assert_ne!(first_attachment, second_attachment);

        let first_registration = NativeThreadStaticCleanupRegistration::new(
            first_product,
            first_identity,
            detach_thread_static,
            finalizer(first_thread_cleanup),
            no_cleanup,
            detach_thread_static,
        );

        let second_registration = NativeThreadStaticCleanupRegistration::new(
            second_product,
            second_identity,
            detach_thread_static,
            finalizer(second_thread_cleanup),
            no_cleanup,
            detach_thread_static,
        );

        assert!(register_thread_static(&first_registration).is_success());
        assert!(register_thread_static(&second_registration).is_success());

        assert_eq!(
            control(
                first_product,
                NativeProductHostOperation::DETACH_CURRENT_THREAD,
            )
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(THREAD_CLEANUP_ORDER.get(), 1);

        assert_eq!(
            thread_attachment_identity(second_product),
            second_attachment
        );

        assert_eq!(
            control(
                second_product,
                NativeProductHostOperation::DETACH_CURRENT_THREAD,
            )
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(THREAD_CLEANUP_ORDER.get(), 12);

        assert_eq!(
            control(
                first_product,
                NativeProductHostOperation::ATTACH_CURRENT_THREAD,
            )
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        let reattached = thread_attachment_identity(first_product);

        assert_ne!(reattached, first_attachment);

        assert_eq!(
            control(
                first_product,
                NativeProductHostOperation::DETACH_CURRENT_THREAD,
            )
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(
            control(first_product, NativeProductHostOperation::CLOSE).state(),
            NativeProductHostState::CLOSED
        );

        assert_eq!(
            control(second_product, NativeProductHostOperation::CLOSE).state(),
            NativeProductHostState::CLOSED
        );
    }

    #[test]
    fn foreign_attachment_retains_product_before_thread_static_access() {
        let descriptor = NativeProductHostDescriptor::new(
            NativeProductIdentity::new([61; 32]),
            delayed_entry,
            0,
        );

        let attached = control(
            &descriptor,
            NativeProductHostOperation::ATTACH_CURRENT_THREAD,
        );

        assert_eq!(attached.status(), NativeProductHostStatus::SUCCESS);
        assert_eq!(attached.thread_attachments(), 1);

        let closing = control(&descriptor, NativeProductHostOperation::CLOSE);

        assert_eq!(closing.state(), NativeProductHostState::CLOSING);
        assert_eq!(closing.status(), NativeProductHostStatus::PENDING);

        let detached = control(
            &descriptor,
            NativeProductHostOperation::DETACH_CURRENT_THREAD,
        );

        assert_eq!(detached.state(), NativeProductHostState::CLOSED);
        assert_eq!(detached.thread_attachments(), 0);
    }
}
