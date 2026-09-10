use bray_runtime_abi::{
    NativeProductHostDescriptor, NativeProductHostObservation, NativeProductHostState,
    NativeProductHostStatus, NativeStaticDuration, NativeStaticIdentity,
};

use super::descriptor::read_statics;
use super::model::{ProductHost, host_status, product_hosts, product_key};

pub(super) fn ensure_formed(
    descriptor: &NativeProductHostDescriptor,
) -> Result<(), NativeProductHostObservation> {
    let product = product_key(descriptor);

    let hosts = product_hosts()
        .lock()
        .map_err(|_| unformed(NativeProductHostStatus::RUNTIME_FAILURE))?;

    if hosts.contains_key(&product) {
        return Ok(());
    }

    // Descriptor callbacks and runtime creation must not hold the shared host registry.
    drop(hosts);
    let statics = read_statics(descriptor).map_err(unformed)?;

    let cleanup_thread = bray_platform::RuntimeThreadReservation::reserve()
        .map_err(|error| unformed(host_status(crate::native::thread_attachment_status(error))))?;

    let runtime =
        crate::native::retain_runtime().map_err(|status| unformed(host_status(status)))?;

    let initialized_statics = statics
        .iter()
        .filter(|entry| entry.duration == NativeStaticDuration::PRODUCT)
        .count();

    let host = ProductHost {
        identity: descriptor.identity(),
        runtime: runtime.clone(),
        state: NativeProductHostState::OPEN,
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
        cleanup_thread: Some(cleanup_thread),
    };

    // Release a losing or failed owner only after its registry guard has been dropped.
    let inserted = insert_host(product, host);

    if !matches!(inserted, Ok(true)) {
        runtime.release();
    }

    inserted.map(|_| ()).map_err(unformed)
}

fn insert_host(product: usize, host: ProductHost) -> Result<bool, NativeProductHostStatus> {
    let mut hosts = product_hosts()
        .lock()
        .map_err(|_| NativeProductHostStatus::RUNTIME_FAILURE)?;

    if hosts.contains_key(&product) {
        return Ok(false);
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
        NativeStaticIdentity::new([0; 32]),
    )
}
