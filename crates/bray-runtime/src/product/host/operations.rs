use bray_runtime_abi::{
    NativeProductHostDescriptor, NativeProductHostObservation, NativeProductHostOperation,
    NativeProductHostState, NativeProductHostStatus, NativeRuntimeStatus, NativeStaticIdentity,
    NativeThreadStaticCleanupRegistration,
};

use super::super::cleanup::StaticCleanup;

use super::attachment::ensure_attachment;
use super::formation::ensure_formed;

use super::model::{
    PendingCleanup, ProductHost, THREAD_STATICS, host_status, product_hosts, product_key,
    runtime_status,
};

pub(crate) fn control(
    descriptor: &NativeProductHostDescriptor,
    operation: NativeProductHostOperation,
) -> NativeProductHostObservation {
    control_with_execution(descriptor, operation, crate::product::retain_execution)
}

pub(crate) fn control_with_execution(
    descriptor: &NativeProductHostDescriptor,
    operation: NativeProductHostOperation,
    retain: fn() -> Result<Option<crate::product::RetainedProductExecution>, NativeRuntimeStatus>,
) -> NativeProductHostObservation {
    if !operation.is_known() {
        return NativeProductHostObservation::invalid();
    }

    let product = product_key(descriptor);

    if let Err(status) = ensure_formed(descriptor, retain) {
        return status;
    }

    if operation == NativeProductHostOperation::ATTACH_CURRENT_THREAD {
        return super::super::attachment::attach_current_thread(product);
    }

    if operation == NativeProductHostOperation::DETACH_CURRENT_THREAD {
        return super::super::attachment::detach_current_thread(product);
    }

    if operation == NativeProductHostOperation::CLOSE {
        return close_product(product);
    }

    if operation == NativeProductHostOperation::FINISH_ROOT {
        if super::super::attachment::has_foreign_attachment(product) {
            return observation_with_status(product, NativeProductHostStatus::INVALID_ARGUMENT);
        }

        let observation = close_product(product);

        return super::thread::drain_product_thread_statics(product).unwrap_or(observation);
    }

    let result = mutate_host(product, operation);

    match result {
        Ok(observation) => progress_closure(product).unwrap_or(observation),
        Err(()) => NativeProductHostObservation::new(
            NativeProductHostStatus::RUNTIME_FAILURE,
            NativeProductHostState::FAILED,
            0,
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

    if ensure_formed(descriptor, crate::product::retain_execution).is_err() {
        return 0;
    }

    let product = product_key(descriptor);

    let allowed = THREAD_STATICS.with(|registry| {
        if registry
            .borrow()
            .products
            .iter()
            .any(|attachment| attachment.product == product)
        {
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

    let worker = current_thread_is_product_worker(product);

    if ensure_attachment(product, worker).is_err() {
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

    if let Err(observation) =
        ensure_formed(registration.product(), crate::product::retain_execution)
    {
        return runtime_status(observation.status());
    }

    let product = product_key(registration.product());
    let worker = current_thread_is_product_worker(product);

    let Some(entry) = static_entry(product, registration.static_identity()) else {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    };

    let finalizer = registration.finalizer();

    if finalizer.execution() != entry.finalizer.execution()
        || finalizer.metadata().is_some() != entry.finalizer.metadata().is_some()
    {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    }

    if let Err(status) = ensure_attachment(product, worker) {
        return status;
    }

    THREAD_STATICS.with(|registry| {
        let mut registry = registry.borrow_mut();

        let Some(attachment) = registry.attachment(product) else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        if attachment
            .entries
            .iter()
            .any(|entry| entry.identity == registration.static_identity())
        {
            return NativeRuntimeStatus::SUCCESS;
        }

        // Each validated exact-thread declaration registers at most once in this attachment.
        assert!(
            attachment.entries.len() < attachment.entries.capacity(),
            "thread-static records must fit admitted attachment storage"
        );

        attachment.entries.push(StaticCleanup {
            identity: registration.static_identity(),
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
) -> Result<NativeProductHostObservation, ()> {
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

    let status = if status == NativeProductHostStatus::SUCCESS
        && host.state == NativeProductHostState::CLOSING
    {
        NativeProductHostStatus::PENDING
    } else {
        status
    };

    Ok(host.observation(status))
}

fn close_product(product: usize) -> NativeProductHostObservation {
    let observation = {
        let Ok(mut hosts) = product_hosts().lock() else {
            return NativeProductHostObservation::invalid();
        };

        let Some(host) = hosts.get_mut(&product) else {
            return NativeProductHostObservation::invalid();
        };

        if host.state != NativeProductHostState::OPEN {
            return host.observation(status_for_state(host));
        }

        host.state = NativeProductHostState::CLOSING;

        host.observation(NativeProductHostStatus::PENDING)
    };

    progress_closure(product).unwrap_or(observation)
}

pub(in crate::product) fn prepare_thread_attachment(
    product: usize,
) -> Result<(), NativeProductHostStatus> {
    ensure_attachment(product, false).map_err(host_status)
}

pub(super) fn release_admission_entry(product: usize) {
    let _ = mutate_host(product, NativeProductHostOperation::RELEASE_ENTRY);
    let _ = progress_closure(product);
}

pub(super) fn release_thread_attachment(
    product: usize,
    worker: bool,
) -> NativeProductHostObservation {
    let observation = mutate_thread_attachment(product, worker);

    progress_closure(product).unwrap_or(observation)
}

pub(in crate::product) fn observation_with_status(
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

fn current_thread_is_product_worker(product: usize) -> bool {
    product_hosts()
        .lock()
        .ok()
        .and_then(|hosts| hosts.get(&product).and_then(|host| host.execution.clone()))
        .is_some_and(|runtime| runtime.owns_current_worker())
}

fn mutate_thread_attachment(product: usize, worker: bool) -> NativeProductHostObservation {
    let Ok(mut hosts) = product_hosts().lock() else {
        return NativeProductHostObservation::invalid();
    };

    let Some(host) = hosts.get_mut(&product) else {
        return NativeProductHostObservation::invalid();
    };

    let status = if worker && host.worker_attachments == 0 {
        NativeProductHostStatus::INVALID_ARGUMENT
    } else {
        let status = release(&mut host.thread_attachments);

        if status == NativeProductHostStatus::SUCCESS && worker {
            host.worker_attachments -= 1;
        }

        status
    };

    let status = if status == NativeProductHostStatus::SUCCESS
        && host.state == NativeProductHostState::CLOSING
    {
        NativeProductHostStatus::PENDING
    } else {
        status
    };

    host.observation(status)
}

fn progress_closure(product: usize) -> Option<NativeProductHostObservation> {
    let (runtime, cleanup) = {
        let mut hosts = product_hosts().lock().ok()?;
        let host = hosts.get_mut(&product)?;

        if host.state != NativeProductHostState::CLOSING
            || host.active_entries != 0
            || host.external_roots != 0
            || host.thread_attachments != host.worker_attachments
            || host.cleanup_running
            || host.cleanup_blocked
        {
            return None;
        }

        if host.worker_attachments == 0 {
            (None, prepare_cleanup(product, host))
        } else {
            host.cleanup_blocked = true;

            (host.execution.clone(), None)
        }
    };

    if let Some(cleanup) = cleanup {
        return Some(finish_cleanup(cleanup));
    }

    let runtime = runtime?;

    runtime.detach_workers(product);

    let cleanup = {
        let mut hosts = product_hosts().lock().ok()?;
        let host = hosts.get_mut(&product)?;

        host.cleanup_blocked = false;

        prepare_cleanup(product, host)
    };

    cleanup.map(finish_cleanup)
}

pub(super) fn acquire(count: &mut usize, state: NativeProductHostState) -> NativeProductHostStatus {
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

pub(super) fn status_for_state(host: &ProductHost) -> NativeProductHostStatus {
    match host.state {
        NativeProductHostState::UNFORMED => NativeProductHostStatus::INVALID_ARGUMENT,
        NativeProductHostState::OPEN => NativeProductHostStatus::SUCCESS,
        NativeProductHostState::CLOSING | NativeProductHostState::RETIRING => {
            NativeProductHostStatus::PENDING
        }
        NativeProductHostState::CLOSED if host.cleanup_incidents == 0 => {
            NativeProductHostStatus::CLOSED
        }
        NativeProductHostState::CLOSED => NativeProductHostStatus::INCIDENTS,
        _ => NativeProductHostStatus::RUNTIME_FAILURE,
    }
}

fn prepare_cleanup(product: usize, host: &mut ProductHost) -> Option<PendingCleanup> {
    if host.state != NativeProductHostState::CLOSING
        || !host.is_quiescent()
        || host.cleanup_running
        || host.cleanup_blocked
    {
        return None;
    }

    let thread = host
        .cleanup_thread
        .take()
        .unwrap_or_else(|| unreachable!("an unclosed product owns its cleanup thread reservation"));

    host.cleanup_running = true;

    Some(PendingCleanup {
        thread,
        product,
        // Keep execution alive throughout callbacks outside the registry lock.
        execution: host.execution.clone(),
        cleanup_driver: host.cleanup_driver.take(),
        statics: std::mem::take(&mut host.statics),
    })
}

fn finish_cleanup(mut cleanup: PendingCleanup) -> NativeProductHostObservation {
    let thread = cleanup.thread.enter_or_reuse();
    let product = cleanup.product;

    let runtime_identity = cleanup
        .statics
        .first()
        .map_or(NativeStaticIdentity::new([0; 32]), |entry| entry.identity);

    let execution = cleanup
        .execution
        .as_ref()
        .map(|owner| owner.as_ref().as_ref());

    let result = super::cleanup::run_cleanup_batch(
        product,
        std::mem::take(&mut cleanup.statics),
        cleanup.cleanup_driver.take(),
        execution,
        product_static_completed,
    );

    drop(thread);

    let Ok(mut hosts) = product_hosts().lock() else {
        return NativeProductHostObservation::new(
            NativeProductHostStatus::RUNTIME_FAILURE,
            NativeProductHostState::FAILED,
            0,
            0,
            0,
            0,
            0,
            0,
            0,
            runtime_identity,
        );
    };

    let Some(host) = hosts.get_mut(&product) else {
        return NativeProductHostObservation::invalid();
    };

    host.cleanup_running = false;

    if let Err(status) = result {
        // Unresolved mandatory execution retains its provider and never authorizes unload.
        host.state = NativeProductHostState::FAILED;

        return host.observation(host_status(status));
    }

    host.state = NativeProductHostState::RETIRING;
    drop(hosts);

    super::retention::finish_retirement(product)
}

fn static_entry(product: usize, identity: NativeStaticIdentity) -> Option<StaticCleanup> {
    let hosts = product_hosts().lock().ok()?;
    let host = hosts.get(&product)?;

    host.static_entry(identity)
}

pub(super) fn report_incidents(product: usize, identity: NativeStaticIdentity, count: usize) {
    record_cleanup(product, identity, count, 0);
}

fn product_static_completed(product: usize, identity: NativeStaticIdentity, count: usize) {
    record_cleanup(product, identity, count, 1);
}

fn record_cleanup(product: usize, identity: NativeStaticIdentity, count: usize, cleaned: usize) {
    let Ok(mut hosts) = product_hosts().lock() else {
        return;
    };

    let Some(host) = hosts.get_mut(&product) else {
        return;
    };

    host.cleaned_statics = host.cleaned_statics.saturating_add(cleaned);
    host.cleanup_incidents = host.cleanup_incidents.saturating_add(count);

    if count != 0 {
        host.last_incident = identity;
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::sync::Arc;
    use std::sync::Barrier;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{
        NativeCleanupExecution, NativeProductHostDescriptor, NativeProductHostOperation,
        NativeProductHostState, NativeProductHostStatus, NativeProductIdentity,
        NativeStaticDuration, NativeStaticFinalizer, NativeStaticFinalizerStartCallback,
        NativeStaticFinalizerStatus, NativeStaticHostEntry, NativeStaticIdentity,
        NativeThreadStaticCleanupRegistration,
    };

    use super::{control, register_thread_static, thread_attachment_identity};

    use bray_runtime_abi::NativeBrayCallOutcome;

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

    extern "C-unwind" fn cleanup(
        _: usize,
        _: &mut NativeBrayCallOutcome,
    ) -> NativeStaticFinalizerStatus {
        CLEANUPS.fetch_add(1, Ordering::SeqCst);

        NativeStaticFinalizerStatus::SUCCESS
    }

    extern "C-unwind" fn first_thread_cleanup(
        _: usize,
        _: &mut NativeBrayCallOutcome,
    ) -> NativeStaticFinalizerStatus {
        append_thread_cleanup(1);

        NativeStaticFinalizerStatus::SUCCESS
    }

    extern "C-unwind" fn second_thread_cleanup(
        _: usize,
        _: &mut NativeBrayCallOutcome,
    ) -> NativeStaticFinalizerStatus {
        append_thread_cleanup(2);

        NativeStaticFinalizerStatus::SUCCESS
    }

    extern "C-unwind" fn panicking_thread_cleanup(
        _: usize,
        _: &mut NativeBrayCallOutcome,
    ) -> NativeStaticFinalizerStatus {
        panic!("test thread-static cleanup incident");
    }

    extern "C-unwind" fn panicking_product_cleanup(
        _: usize,
        _: &mut NativeBrayCallOutcome,
    ) -> NativeStaticFinalizerStatus {
        record_product_phase(1);
        panic!("test product-static cleanup incident");
    }

    extern "C-unwind" fn continuing_product_cleanup(
        _: usize,
        _: &mut NativeBrayCallOutcome,
    ) -> NativeStaticFinalizerStatus {
        record_product_phase(1);
        CONTINUING_PRODUCT_CLEANUPS.fetch_add(1, Ordering::SeqCst);

        NativeStaticFinalizerStatus::SUCCESS
    }

    const fn finalizer(start: NativeStaticFinalizerStartCallback) -> NativeStaticFinalizer {
        NativeStaticFinalizer::new(
            NativeCleanupExecution::SYNCHRONOUS,
            None,
            start,
            crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic),
        )
    }

    extern "C" fn detach_thread_static() {}

    extern "C-unwind" fn unexpected_panic(_: usize) -> bray_runtime_abi::NativeRuntimeStatus {
        panic!("this fixture does not return a native panic payload");
    }

    extern "C-unwind" fn no_cleanup() -> NativeBrayCallOutcome {
        NativeBrayCallOutcome::completed()
    }

    extern "C-unwind" fn record_product_destruction() -> NativeBrayCallOutcome {
        record_product_phase(2);
        PRODUCT_DESTRUCTIONS.fetch_add(1, Ordering::SeqCst);

        NativeBrayCallOutcome::completed()
    }

    fn record_product_phase(phase: usize) {
        PRODUCT_PHASE_ORDER
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |order| {
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
    fn admitted_product_cleanup_attaches_a_foreign_thread_and_drains_it_before_closure_returns() {
        extern "C-unwind" fn thread_exit() {
            append_thread_cleanup(3);
        }

        extern "C-unwind" fn finish(
            _: usize,
            _: &mut NativeBrayCallOutcome,
        ) -> NativeStaticFinalizerStatus {
            assert!(bray_platform::current_runtime_thread().is_some());
            append_thread_cleanup(1);
            bray_platform::register_runtime_thread_exit_callback(thread_exit).unwrap();

            NativeStaticFinalizerStatus::SUCCESS
        }

        extern "C-unwind" fn destroy() -> NativeBrayCallOutcome {
            append_thread_cleanup(2);

            NativeBrayCallOutcome::completed()
        }

        extern "C" fn entry(_: usize) -> NativeStaticHostEntry {
            NativeStaticHostEntry::new(
                NativeStaticDuration::PRODUCT,
                NativeStaticIdentity::new([156; 32]),
                0,
                1,
                access,
                detach_thread_static,
                finalizer(finish),
                destroy,
                detach_thread_static,
                no_dependency,
                0,
            )
        }

        let descriptor: &'static NativeProductHostDescriptor = Box::leak(Box::new(
            NativeProductHostDescriptor::new(NativeProductIdentity::new([156; 32]), entry, 1),
        ));

        assert_eq!(
            control(descriptor, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::SUCCESS
        );

        std::thread::spawn(move || {
            assert!(bray_platform::current_runtime_thread().is_none());
            THREAD_CLEANUP_ORDER.set(0);

            let closed = crate::test_support::with_allocation_failure(|| {
                control(descriptor, NativeProductHostOperation::CLOSE)
            });

            assert_eq!(closed.state(), NativeProductHostState::CLOSED);
            assert_eq!(closed.cleanup_incidents(), 0);
            assert_eq!(closed.cleaned_statics(), 1);
            assert_eq!(THREAD_CLEANUP_ORDER.get(), 123);
            assert!(bray_platform::current_runtime_thread().is_none());

            assert_eq!(
                control(descriptor, NativeProductHostOperation::CLOSE).state(),
                NativeProductHostState::CLOSED
            );

            assert_eq!(THREAD_CLEANUP_ORDER.get(), 123);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn product_closure_transfers_admitted_storage_and_preserves_cleanup_order() {
        extern "C" fn mixed_entry(index: usize) -> NativeStaticHostEntry {
            let (order, duration, start): (
                u64,
                NativeStaticDuration,
                NativeStaticFinalizerStartCallback,
            ) = match index {
                0 => (2, NativeStaticDuration::PRODUCT, second_thread_cleanup),
                1 => (
                    1,
                    NativeStaticDuration::EXACT_THREAD,
                    panicking_thread_cleanup,
                ),
                _ => (0, NativeStaticDuration::PRODUCT, first_thread_cleanup),
            };

            NativeStaticHostEntry::new(
                duration,
                NativeStaticIdentity::new([u8::try_from(order + 1).unwrap(); 32]),
                order,
                1,
                access,
                detach_thread_static,
                finalizer(start),
                no_cleanup,
                detach_thread_static,
                no_dependency,
                0,
            )
        }

        THREAD_CLEANUP_ORDER.set(0);

        let descriptor = Box::leak(Box::new(NativeProductHostDescriptor::new(
            NativeProductIdentity::new([151; 32]),
            mixed_entry,
            3,
        )));

        assert_eq!(
            control(descriptor, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::SUCCESS
        );

        let product = super::product_key(descriptor);

        let (pending, original, capacity) = {
            let mut hosts = super::product_hosts().lock().unwrap();
            let host = hosts.get_mut(&product).unwrap();
            host.state = NativeProductHostState::CLOSING;
            assert_eq!(host.statics.len(), 2);
            assert_eq!(host.thread_statics.len(), 1);
            let original = host.statics.as_ptr();
            let capacity = host.statics.capacity();

            let pending = crate::test_support::with_allocation_failure(|| {
                super::prepare_cleanup(product, host)
            })
            .unwrap();

            assert!(super::prepare_cleanup(product, host).is_none());

            (pending, original, capacity)
        };

        assert_eq!(
            pending.statics.as_ptr(),
            original,
            "closure must transfer the admitted allocation"
        );

        assert_eq!(pending.statics.capacity(), capacity);
        assert_eq!(pending.statics.len(), 2);
        let closed = super::finish_cleanup(pending);
        assert_eq!(closed.state(), NativeProductHostState::CLOSED);
        assert_eq!(closed.cleaned_statics(), 2);
        assert_eq!(closed.cleanup_incidents(), 0);
        assert_eq!(THREAD_CLEANUP_ORDER.get(), 12);

        assert_eq!(
            control(descriptor, NativeProductHostOperation::CLOSE).cleaned_statics(),
            2
        );

        assert_eq!(THREAD_CLEANUP_ORDER.get(), 12);
    }

    #[test]
    fn product_metadata_validation_preserves_dependency_and_order_contracts() {
        thread_local! {
            static CASE: Cell<u8> = const { Cell::new(0) };
        }

        extern "C" fn dependency(_: usize) -> NativeStaticIdentity {
            NativeStaticIdentity::new(
                [match CASE.get() {
                    3 => 3,
                    4 => 2,
                    _ => 1,
                }; 32],
            )
        }

        extern "C" fn entry(index: usize) -> NativeStaticHostEntry {
            let case = CASE.get();
            let first = index == 0;
            let identity = if first || case == 1 { 2 } else { 1 };

            let order = match (case, first) {
                (2, _) | (5, false) => 0,
                (5, true) => 1,
                (_, true) => 0,
                (_, false) => 1,
            };

            let count = if first {
                if case == 6 { 2 } else { 1 }
            } else {
                0
            };

            NativeStaticHostEntry::new(
                NativeStaticDuration::EXACT_THREAD,
                NativeStaticIdentity::new([identity; 32]),
                order,
                0,
                access,
                detach_thread_static,
                finalizer(first_thread_cleanup),
                no_cleanup,
                detach_thread_static,
                dependency,
                count,
            )
        }

        let descriptor =
            NativeProductHostDescriptor::new(NativeProductIdentity::new([155; 32]), entry, 2);

        let (product_statics, valid) = super::super::descriptor::read_statics(&descriptor).unwrap();

        assert!(product_statics.is_empty());

        assert_eq!(
            valid.iter().map(|entry| entry.order).collect::<Vec<_>>(),
            [0, 1]
        );

        assert_eq!(valid[0].identity, NativeStaticIdentity::new([2; 32]));

        assert_eq!(valid[1].identity, NativeStaticIdentity::new([1; 32]));

        for case in 1..=6 {
            CASE.set(case);

            assert!(
                matches!(
                    super::super::descriptor::read_statics(&descriptor),
                    Err(NativeProductHostStatus::INVALID_ARGUMENT)
                ),
                "invalid metadata case {case}"
            );
        }
    }

    #[test]
    fn host_formation_requires_execution_only_for_asynchronous_cleanup() {
        use bray_runtime_abi::{NativeFrameMetadata, NativeRuntimeStatus};

        extern "C" fn metadata() -> Option<&'static NativeFrameMetadata> {
            static METADATA: NativeFrameMetadata = NativeFrameMetadata::new(
                [155; 32],
                1,
                1,
                1,
                0,
                1,
                crate::test_support::native_movable_frame_state,
            );

            Some(&METADATA)
        }

        extern "C" fn asynchronous_entry(_: usize) -> NativeStaticHostEntry {
            NativeStaticHostEntry::new(
                NativeStaticDuration::PRODUCT,
                NativeStaticIdentity::new([155; 32]),
                0,
                1,
                access,
                detach_thread_static,
                NativeStaticFinalizer::new(
                    NativeCleanupExecution::ASYNCHRONOUS,
                    Some(metadata),
                    cleanup,
                    crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic),
                ),
                no_cleanup,
                detach_thread_static,
                no_dependency,
                0,
            )
        }

        std::thread::spawn(|| {
            assert!(crate::product::retain_execution().unwrap().is_none());

            static SYNCHRONOUS: NativeProductHostDescriptor = NativeProductHostDescriptor::new(
                NativeProductIdentity::new([155; 32]),
                thread_order_entry,
                2,
            );

            assert_eq!(
                control(&SYNCHRONOUS, NativeProductHostOperation::FORM).status(),
                NativeProductHostStatus::SUCCESS
            );

            assert!(crate::product::retain_execution().unwrap().is_none());

            let scope = bray_platform::RuntimeThreadScope::enter().unwrap();

            for finalizer in [
                NativeStaticFinalizer::new(
                    NativeCleanupExecution::NONE,
                    None,
                    first_thread_cleanup,
                    crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic),
                ),
                NativeStaticFinalizer::new(
                    NativeCleanupExecution::SYNCHRONOUS,
                    Some(metadata),
                    first_thread_cleanup,
                    crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic),
                ),
            ] {
                let registration = NativeThreadStaticCleanupRegistration::new(
                    &SYNCHRONOUS,
                    NativeStaticIdentity::new([11; 32]),
                    detach_thread_static,
                    finalizer,
                    no_cleanup,
                    detach_thread_static,
                );

                assert_eq!(
                    register_thread_static(&registration),
                    NativeRuntimeStatus::INVALID_ARGUMENT
                );

                assert_eq!(
                    control(&SYNCHRONOUS, NativeProductHostOperation::OBSERVE).thread_attachments(),
                    0
                );
            }

            drop(scope);

            assert_eq!(
                control(&SYNCHRONOUS, NativeProductHostOperation::CLOSE).state(),
                NativeProductHostState::CLOSED
            );

            assert!(crate::product::retain_execution().unwrap().is_none());

            let asynchronous = NativeProductHostDescriptor::new(
                NativeProductIdentity::new([156; 32]),
                asynchronous_entry,
                1,
            );

            let rejected = control(&asynchronous, NativeProductHostOperation::FORM);

            assert_eq!(rejected.status(), NativeProductHostStatus::RUNTIME_FAILURE);
            assert_eq!(rejected.state(), NativeProductHostState::UNFORMED);
            assert!(crate::product::retain_execution().unwrap().is_none());
        })
        .join()
        .unwrap();
    }

    #[test]
    fn product_formation_allocation_failure_preserves_existing_host_and_retry() {
        assert!(
            crate::native::implementation::bray_runtime_substrate_initialization(4, 1).is_success()
        );

        let existing = Box::leak(Box::new(NativeProductHostDescriptor::new(
            NativeProductIdentity::new([153; 32]),
            thread_order_entry,
            2,
        )));

        let candidate = Box::leak(Box::new(NativeProductHostDescriptor::new(
            NativeProductIdentity::new([154; 32]),
            thread_order_entry,
            2,
        )));

        assert_eq!(
            control(existing, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::SUCCESS
        );

        // Entry storage, dependency scratch, final metadata and three execution-owner allocations.
        for successful in 0..6 {
            let failed = crate::test_support::with_allocation_failure_after(successful, || {
                control(candidate, NativeProductHostOperation::FORM)
            });

            assert_eq!(failed.status(), NativeProductHostStatus::ALLOCATION_FAILURE);
            assert_eq!(failed.state(), NativeProductHostState::UNFORMED);

            assert!(
                !super::product_hosts()
                    .lock()
                    .unwrap()
                    .contains_key(&super::product_key(candidate))
            );
        }

        assert_eq!(
            control(existing, NativeProductHostOperation::OBSERVE).state(),
            NativeProductHostState::OPEN
        );

        assert_eq!(
            control(candidate, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(
            control(candidate, NativeProductHostOperation::CLOSE).state(),
            NativeProductHostState::CLOSED
        );

        assert_eq!(
            control(existing, NativeProductHostOperation::CLOSE).state(),
            NativeProductHostState::CLOSED
        );

        assert!(crate::native::implementation::bray_runtime_substrate_shutdown().is_success());
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
    fn product_cleanup_disposes_incidents_before_destroying_static_dependencies() {
        use bray_runtime_abi::{
            NativeCleanupIncident, NativeRuntimeStatus, NativeSourceAnchor, NativeTypeIdentity,
        };

        static CALLBACKS: AtomicUsize = AtomicUsize::new(0);
        static DEPENDENCY_DESTROYED: AtomicUsize = AtomicUsize::new(0);

        fn observe_incident_callback() {
            assert_eq!(DEPENDENCY_DESTROYED.load(Ordering::SeqCst), 0);
            assert!(bray_platform::current_runtime_thread().is_some());
            CALLBACKS.fetch_add(1, Ordering::SeqCst);
        }

        extern "C-unwind" fn report(_: &NativeCleanupIncident) -> NativeRuntimeStatus {
            observe_incident_callback();

            NativeRuntimeStatus::SUCCESS
        }

        extern "C-unwind" fn destroy_incident(_: usize) -> NativeBrayCallOutcome {
            observe_incident_callback();

            NativeBrayCallOutcome::completed()
        }

        extern "C-unwind" fn transfer(
            _: usize,
            _: &mut NativeBrayCallOutcome,
        ) -> NativeStaticFinalizerStatus {
            let incident = NativeCleanupIncident::new(
                1,
                NativeTypeIdentity::new([91; 32]),
                NativeSourceAnchor::unavailable(),
                report,
                destroy_incident,
                crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic),
            );

            assert_eq!(
                crate::native::implementation::bray_runtime_cleanup_incident_transfer(&incident),
                NativeRuntimeStatus::SUCCESS
            );

            NativeStaticFinalizerStatus::SUCCESS
        }

        extern "C-unwind" fn destroy_dependency() -> NativeBrayCallOutcome {
            assert_eq!(CALLBACKS.load(Ordering::SeqCst), 2);
            DEPENDENCY_DESTROYED.store(1, Ordering::SeqCst);

            NativeBrayCallOutcome::completed()
        }

        extern "C" fn dependency(_: usize) -> NativeStaticIdentity {
            NativeStaticIdentity::new([92; 32])
        }

        extern "C" fn entry(index: usize) -> NativeStaticHostEntry {
            NativeStaticHostEntry::new(
                NativeStaticDuration::PRODUCT,
                NativeStaticIdentity::new([if index == 0 { 91 } else { 92 }; 32]),
                if index == 0 { 0 } else { 1 },
                index + 1,
                access,
                detach_thread_static,
                if index == 0 {
                    finalizer(transfer)
                } else {
                    NativeStaticFinalizer::new(
                        NativeCleanupExecution::NONE,
                        None,
                        transfer,
                        crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic),
                    )
                },
                if index == 0 {
                    no_cleanup
                } else {
                    destroy_dependency
                },
                detach_thread_static,
                dependency,
                usize::from(index == 0),
            )
        }

        let descriptor =
            NativeProductHostDescriptor::new(NativeProductIdentity::new([93; 32]), entry, 2);

        let closed = control(&descriptor, NativeProductHostOperation::CLOSE);

        assert_eq!(closed.state(), NativeProductHostState::CLOSED);
        assert_eq!(closed.status(), NativeProductHostStatus::INCIDENTS);
        assert_eq!(closed.cleanup_incidents(), 1);
        assert_eq!(closed.cleaned_statics(), 2);
        assert_eq!(closed.last_incident(), NativeStaticIdentity::new([91; 32]));
        assert_eq!(CALLBACKS.load(Ordering::SeqCst), 2);
        assert_eq!(DEPENDENCY_DESTROYED.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn cleanup_retention_survives_closure_and_clones_without_allocation() {
        use bray_runtime_abi::{NativeProviderRetention, NativeRuntimeStatus};
        use std::cell::RefCell;

        fn acquire(
            descriptor: &NativeProductHostDescriptor,
        ) -> Result<NativeProviderRetention, NativeRuntimeStatus> {
            let mut retention = NativeProviderRetention::empty();

            let status = crate::native::implementation::bray_runtime_provider_retention(
                descriptor,
                &mut retention,
            );

            if status.is_success() {
                Ok(retention)
            } else {
                Err(status)
            }
        }

        thread_local! {
            static RETAINED: RefCell<Option<NativeProviderRetention>> = const { RefCell::new(None) };
        }

        static CLEANED: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn finish(
            _: usize,
            _: &mut NativeBrayCallOutcome,
        ) -> NativeStaticFinalizerStatus {
            CLEANED.fetch_add(1, Ordering::SeqCst);

            let retained =
                crate::test_support::with_allocation_failure(|| acquire(&DESCRIPTOR).unwrap());

            RETAINED.with(|slot| {
                assert!(slot.borrow_mut().replace(retained).is_none());
            });

            NativeStaticFinalizerStatus::SUCCESS
        }

        extern "C" fn entry(_: usize) -> NativeStaticHostEntry {
            NativeStaticHostEntry::new(
                NativeStaticDuration::PRODUCT,
                NativeStaticIdentity::new([181; 32]),
                0,
                1,
                access,
                detach_thread_static,
                finalizer(finish),
                no_cleanup,
                detach_thread_static,
                no_dependency,
                0,
            )
        }

        static DESCRIPTOR: NativeProductHostDescriptor =
            NativeProductHostDescriptor::new(NativeProductIdentity::new([181; 32]), entry, 1);

        assert_eq!(
            control(&DESCRIPTOR, NativeProductHostOperation::ACQUIRE_ENTRY).status(),
            NativeProductHostStatus::SUCCESS
        );

        let mut entry_retention = acquire(&DESCRIPTOR).unwrap();

        assert_eq!(
            crate::native::implementation::bray_runtime_provider_retention(
                &DESCRIPTOR,
                &mut entry_retention,
            ),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            control(&DESCRIPTOR, NativeProductHostOperation::OBSERVE).retirement_roots(),
            1
        );

        assert_eq!(
            control(&DESCRIPTOR, NativeProductHostOperation::CLOSE).state(),
            NativeProductHostState::CLOSING
        );

        assert_eq!(
            control(&DESCRIPTOR, NativeProductHostOperation::RELEASE_ENTRY).state(),
            NativeProductHostState::RETIRING
        );

        assert_eq!(CLEANED.load(Ordering::SeqCst), 1);

        assert!(matches!(
            acquire(&DESCRIPTOR),
            Err(NativeRuntimeStatus::INVALID_ARGUMENT)
        ));

        let cleanup_retention = RETAINED.with(|slot| slot.borrow_mut().take().unwrap());
        let cloned = crate::test_support::with_allocation_failure(|| cleanup_retention.clone());

        drop(entry_retention);
        drop(cleanup_retention);

        let pending = control(&DESCRIPTOR, NativeProductHostOperation::OBSERVE);
        assert_eq!(pending.state(), NativeProductHostState::RETIRING);
        assert_eq!(pending.status(), NativeProductHostStatus::PENDING);
        assert_eq!(pending.retirement_roots(), 1);

        drop(cloned);

        let closed = control(&DESCRIPTOR, NativeProductHostOperation::OBSERVE);
        assert_eq!(closed.state(), NativeProductHostState::CLOSED);
        assert_eq!(closed.retirement_roots(), 0);
        assert_eq!(CLEANED.load(Ordering::SeqCst), 1);

        assert!(matches!(
            acquire(&DESCRIPTOR),
            Err(NativeRuntimeStatus::INVALID_ARGUMENT)
        ));
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
    fn thread_attachment_admission_preserves_existing_cleanup_on_failure() {
        THREAD_CLEANUP_ORDER.set(0);
        let scope = bray_platform::RuntimeThreadScope::enter().unwrap();

        let first = Box::leak(Box::new(NativeProductHostDescriptor::new(
            NativeProductIdentity::new([131; 32]),
            thread_order_entry,
            2,
        )));

        let second = Box::leak(Box::new(NativeProductHostDescriptor::new(
            NativeProductIdentity::new([132; 32]),
            thread_order_entry,
            2,
        )));

        for descriptor in [&*first, &*second] {
            assert_eq!(
                control(descriptor, NativeProductHostOperation::OBSERVE).state(),
                NativeProductHostState::OPEN
            );
        }

        let original = thread_attachment_identity(first);
        assert_ne!(original, 0);

        let first_registration = NativeThreadStaticCleanupRegistration::new(
            first,
            NativeStaticIdentity::new([11; 32]),
            detach_thread_static,
            finalizer(first_thread_cleanup),
            no_cleanup,
            detach_thread_static,
        );

        assert!(register_thread_static(&first_registration).is_success());

        crate::test_support::with_allocation_failure(|| {
            assert_eq!(thread_attachment_identity(first), original);
            assert_eq!(thread_attachment_identity(second), 0);
        });

        assert_eq!(thread_attachment_identity(second), original + 1);

        let registrations = [
            NativeThreadStaticCleanupRegistration::new(
                second,
                NativeStaticIdentity::new([11; 32]),
                detach_thread_static,
                finalizer(first_thread_cleanup),
                no_cleanup,
                detach_thread_static,
            ),
            NativeThreadStaticCleanupRegistration::new(
                second,
                NativeStaticIdentity::new([12; 32]),
                detach_thread_static,
                finalizer(second_thread_cleanup),
                no_cleanup,
                detach_thread_static,
            ),
        ];

        crate::test_support::with_allocation_failure(|| {
            for registration in registrations.iter().rev() {
                assert!(register_thread_static(registration).is_success());
            }
        });

        drop(scope);
        assert_eq!(THREAD_CLEANUP_ORDER.get(), 112);

        for descriptor in [&*first, &*second] {
            assert_eq!(
                control(descriptor, NativeProductHostOperation::CLOSE).state(),
                NativeProductHostState::CLOSED
            );
        }
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
    fn thread_metadata_reentry_preserves_admission_lifetime_and_nested_depth() {
        use bray_runtime_abi::NativeFrameMetadata;

        thread_local! {
            static REENTRY: Cell<Option<(u8, &'static NativeProductHostDescriptor)>> = const { Cell::new(None) };
        }

        extern "C" fn metadata() -> Option<&'static NativeFrameMetadata> {
            static METADATA: NativeFrameMetadata = NativeFrameMetadata::new(
                [160; 32], 1, 1, 1, 0, 1, crate::test_support::native_origin_frame_state);

            if let Some((mode, descriptor)) = REENTRY.take() {
                let observed = control(descriptor, NativeProductHostOperation::OBSERVE);
                assert_eq!(observed.active_entries(), 1);
                assert_eq!(observed.thread_attachments(), 0);

                if mode == 0 {
                    let closing = control(descriptor, NativeProductHostOperation::CLOSE);
                    assert_eq!(closing.state(), NativeProductHostState::CLOSING);
                    assert_eq!(closing.active_entries(), 1);
                } else {
                    let nested = control(descriptor, NativeProductHostOperation::ATTACH_CURRENT_THREAD);
                    assert_eq!(nested.status(), NativeProductHostStatus::SUCCESS);
                    assert_eq!(nested.thread_attachments(), 1);

                    if mode == 2 { return None; }
                }
            }

            Some(&METADATA)
        }

        extern "C" fn entry(_: usize) -> NativeStaticHostEntry {
            NativeStaticHostEntry::new(NativeStaticDuration::EXACT_THREAD,
                NativeStaticIdentity::new([160; 32]), 0, 1, access, detach_thread_static,
                NativeStaticFinalizer::new(NativeCleanupExecution::ASYNCHRONOUS,
                    Some(metadata), cleanup, crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic)),
                no_cleanup, detach_thread_static, no_dependency, 0)
        }

        let ((), incidents) = crate::native::with_static_cleanup_runtime(|| {
            for mode in 0..3 {
                let descriptor = Box::leak(Box::new(NativeProductHostDescriptor::new(
                    NativeProductIdentity::new([160 + mode; 32]), entry, 1)));

                assert_eq!(control(descriptor, NativeProductHostOperation::FORM).status(), NativeProductHostStatus::SUCCESS);
                REENTRY.set(Some((mode, descriptor)));
                let attached = control(descriptor, NativeProductHostOperation::ATTACH_CURRENT_THREAD);
                assert_eq!(attached.active_entries(), 0);

                if mode == 0 {
                    assert_eq!(attached.state(), NativeProductHostState::CLOSED);
                    assert_eq!(attached.thread_attachments(), 0);
                    assert_ne!(attached.status(), NativeProductHostStatus::SUCCESS);
                    continue;
                }

                assert_eq!(attached.thread_attachments(), 1);
                assert_eq!(attached.status() == NativeProductHostStatus::SUCCESS, mode == 1);

                if mode == 1 {
                    assert_eq!(control(descriptor, NativeProductHostOperation::DETACH_CURRENT_THREAD).thread_attachments(), 1);
                }

                assert_eq!(control(descriptor, NativeProductHostOperation::DETACH_CURRENT_THREAD).thread_attachments(), 0);
                assert_eq!(control(descriptor, NativeProductHostOperation::CLOSE).state(), NativeProductHostState::CLOSED);
            }
        });

        assert!(incidents.is_empty());
    }

    #[test]
    fn foreign_attachment_failure_preserves_prepared_cleanup_and_retry_depth() {
        let _scope = bray_platform::RuntimeThreadScope::enter().unwrap();

        let descriptor = Box::leak(Box::new(NativeProductHostDescriptor::new(
            NativeProductIdentity::new([152; 32]),
            thread_order_entry,
            2,
        )));

        assert_eq!(
            control(descriptor, NativeProductHostOperation::FORM).status(),
            NativeProductHostStatus::SUCCESS
        );

        let identity = thread_attachment_identity(descriptor);
        assert_ne!(identity, 0);

        let failed = crate::test_support::with_allocation_failure(|| {
            control(
                descriptor,
                NativeProductHostOperation::ATTACH_CURRENT_THREAD,
            )
        });

        assert_eq!(failed.status(), NativeProductHostStatus::ALLOCATION_FAILURE);
        assert_eq!(failed.thread_attachments(), 1);
        assert_eq!(thread_attachment_identity(descriptor), identity);

        assert_eq!(
            control(
                descriptor,
                NativeProductHostOperation::ATTACH_CURRENT_THREAD
            )
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        let nested = crate::test_support::with_allocation_failure(|| {
            control(
                descriptor,
                NativeProductHostOperation::ATTACH_CURRENT_THREAD,
            )
        });

        assert_eq!(nested.status(), NativeProductHostStatus::SUCCESS);
        assert_eq!(nested.thread_attachments(), 1);

        assert_eq!(
            control(
                descriptor,
                NativeProductHostOperation::DETACH_CURRENT_THREAD
            )
            .thread_attachments(),
            1
        );

        assert_eq!(
            control(
                descriptor,
                NativeProductHostOperation::DETACH_CURRENT_THREAD
            )
            .thread_attachments(),
            0
        );

        assert_eq!(
            control(descriptor, NativeProductHostOperation::CLOSE).state(),
            NativeProductHostState::CLOSED
        );
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
    fn foreign_thread_exit_releases_implicit_attachment() {
        let descriptor: &'static NativeProductHostDescriptor =
            Box::leak(Box::new(NativeProductHostDescriptor::new(
                NativeProductIdentity::new([53; 32]),
                delayed_entry,
                0,
            )));

        std::thread::spawn(move || {
            assert_eq!(
                control(
                    descriptor,
                    NativeProductHostOperation::ATTACH_CURRENT_THREAD,
                )
                .status(),
                NativeProductHostStatus::SUCCESS
            );
        })
        .join()
        .unwrap_or_else(|_| panic!("foreign thread exit must complete without a TLS panic"));

        let observed = control(descriptor, NativeProductHostOperation::OBSERVE);

        assert_eq!(observed.thread_attachments(), 0);

        assert_eq!(
            control(descriptor, NativeProductHostOperation::CLOSE).state(),
            NativeProductHostState::CLOSED
        );
    }

    #[test]
    fn finishing_root_drains_implicit_thread_statics_once() {
        THREAD_CLEANUP_ORDER.set(0);

        let descriptor = Box::leak(Box::new(NativeProductHostDescriptor::new(
            NativeProductIdentity::new([71; 32]),
            thread_order_entry,
            2,
        )));

        let scope = bray_platform::RuntimeThreadScope::enter().expect("runtime thread enters");

        let registration = NativeThreadStaticCleanupRegistration::new(
            descriptor,
            NativeStaticIdentity::new([11; 32]),
            detach_thread_static,
            finalizer(first_thread_cleanup),
            no_cleanup,
            detach_thread_static,
        );

        assert!(register_thread_static(&registration).is_success());

        assert_eq!(
            control(descriptor, NativeProductHostOperation::OBSERVE).thread_attachments(),
            1
        );

        let closed = control(descriptor, NativeProductHostOperation::FINISH_ROOT);

        assert_eq!(closed.state(), NativeProductHostState::CLOSED);
        assert_eq!(closed.status(), NativeProductHostStatus::CLOSED);
        assert_eq!(closed.thread_attachments(), 0);
        assert_eq!(THREAD_CLEANUP_ORDER.get(), 1);

        assert_eq!(
            control(descriptor, NativeProductHostOperation::FINISH_ROOT).state(),
            NativeProductHostState::CLOSED
        );

        drop(scope);
        assert_eq!(THREAD_CLEANUP_ORDER.get(), 1);
    }

    #[test]
    fn finishing_root_preserves_foreign_and_external_obligations() {
        let descriptor = NativeProductHostDescriptor::new(
            NativeProductIdentity::new([72; 32]),
            delayed_entry,
            0,
        );

        assert_eq!(
            control(
                &descriptor,
                NativeProductHostOperation::ATTACH_CURRENT_THREAD
            )
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        let rejected = control(&descriptor, NativeProductHostOperation::FINISH_ROOT);
        assert_eq!(rejected.status(), NativeProductHostStatus::INVALID_ARGUMENT);
        assert_eq!(rejected.state(), NativeProductHostState::OPEN);
        assert_eq!(rejected.thread_attachments(), 1);

        assert_eq!(
            control(
                &descriptor,
                NativeProductHostOperation::DETACH_CURRENT_THREAD
            )
            .status(),
            NativeProductHostStatus::SUCCESS
        );

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::ACQUIRE_EXTERNAL).status(),
            NativeProductHostStatus::SUCCESS
        );

        let pending = control(&descriptor, NativeProductHostOperation::FINISH_ROOT);

        assert_eq!(pending.status(), NativeProductHostStatus::PENDING);
        assert_eq!(pending.state(), NativeProductHostState::CLOSING);

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::RELEASE_EXTERNAL).state(),
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
