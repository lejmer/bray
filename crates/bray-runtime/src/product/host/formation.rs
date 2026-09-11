use bray_runtime_abi::{
    NativeCleanupExecution, NativeProductHostDescriptor, NativeProductHostObservation,
    NativeProductHostState, NativeProductHostStatus, NativeStaticIdentity,
};

use super::descriptor::read_statics;
use super::model::{ProductHost, host_status, product_hosts, product_key};

pub(super) fn ensure_formed(
    descriptor: &NativeProductHostDescriptor,
    capacity: Option<&bray_runtime_abi::NativeCleanupCapacityBinding>,
    retain: impl FnOnce() -> Result<
        Option<crate::product::RetainedProductExecution>,
        bray_runtime_abi::NativeRuntimeStatus,
    >,
) -> Result<(), NativeProductHostObservation> {
    let product = product_key(descriptor);

    let hosts = product_hosts()
        .lock()
        .map_err(|_| unformed(NativeProductHostStatus::RUNTIME_FAILURE))?;

    if let Some(host) = hosts.get(&product) {
        if host.capacity.is_empty() && host.state != NativeProductHostState::RETIRING {
            return Err(host.observation(super::operations::status_for_state(host)));
        }

        return match capacity {
            Some(capacity)
                if !host.capacity.is_empty()
                    && (!capacity.is_valid() || !host.capacity.same_domain(capacity)) =>
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
    let capacity = capacity.clone();

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
        capacity,
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

fn insert_host(product: usize, host: ProductHost) -> Result<bool, NativeProductHostStatus> {
    let mut hosts = product_hosts()
        .lock()
        .map_err(|_| NativeProductHostStatus::RUNTIME_FAILURE)?;

    if let Some(existing) = hosts.get(&product) {
        return if existing.capacity.same_domain(&host.capacity) {
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
