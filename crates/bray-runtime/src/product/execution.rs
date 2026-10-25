use std::collections::BTreeMap;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use bray_runtime_abi::{
    NativeProductCleanupBody, NativeProductCleanupResult, NativeProductExecution,
    NativeProductHostObservation, NativeProductHostOperation, NativeProductHostStatus,
    NativeRuntimeStatus, NativeStaticFinalizer,
};

use crate::native::RetainedRuntime;

static NEXT_RESOURCE: AtomicUsize = AtomicUsize::new(1);
static RESOURCES: OnceLock<Mutex<BTreeMap<usize, Option<RetainedRuntime>>>> = OnceLock::new();
static FRAMES: OnceLock<Mutex<BTreeMap<usize, crate::TaskAdmission>>> = OnceLock::new();

fn resources() -> &'static Mutex<BTreeMap<usize, Option<RetainedRuntime>>> {
    RESOURCES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn frames() -> &'static Mutex<BTreeMap<usize, crate::TaskAdmission>> {
    FRAMES.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(super) fn runtime(resource: usize) -> Option<RetainedRuntime> {
    // Callouts can reenter product control, so retain the runtime outside the resource lock.
    resources()
        .lock()
        .expect("execution retention remains available")
        .get(&resource)
        .expect("Bray retains this execution resource until release")
        .clone()
}

pub(crate) fn acquire(
    asynchronous: u32,
    output: &mut MaybeUninit<NativeProductExecution>,
) -> NativeRuntimeStatus {
    if asynchronous > 1 {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    }

    let resource = match NEXT_RESOURCE.try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        value.checked_add(1)
    }) {
        Ok(resource) => resource,
        Err(_) => return NativeRuntimeStatus::IDENTITY_EXHAUSTED,
    };

    let retained = match crate::native::retain_current_runtime() {
        Ok(Some(runtime)) => Some(runtime),
        Ok(None) if asynchronous == 0 => None,
        Ok(None) => match crate::native::retain_runtime() {
            Ok(runtime) => Some(runtime),
            Err(status) => return status,
        },
        Err(status) => return status,
    };

    resources()
        .lock()
        .expect("execution retention remains available")
        .insert(resource, retained);

    output.write(NativeProductExecution {
        resource,
        with_cleanup,
        admit,
        discard,
        finalize,
        detach_workers,
        release,
        attach,
        detach,
        finish_root,
    });

    NativeRuntimeStatus::SUCCESS
}

extern "C" fn with_cleanup(
    resource: usize,
    context: usize,
    body: NativeProductCleanupBody,
) -> NativeProductCleanupResult {
    let runtime = runtime(resource);

    let result = crate::native::with_static_cleanup_context(runtime.as_ref(), || {
        assert!(
            body(context).is_success(),
            "Bray terminal work completes after its cleanup context has entered"
        );
    });

    match result {
        Ok(((), incident)) => {
            let incidents = usize::from(incident.is_some());

            if let Some(incident) = incident {
                let _ = incident.report();
            }

            NativeProductCleanupResult {
                status: NativeRuntimeStatus::SUCCESS,
                incidents,
            }
        }
        Err(status) => NativeProductCleanupResult {
            status,
            incidents: 0,
        },
    }
}

extern "C" fn admit(output: &mut usize) -> NativeRuntimeStatus {
    *output = 0;

    let admission = match crate::TaskAdmission::new() {
        Ok(admission) => admission,
        Err(error) => return admission_failure(error),
    };

    let Ok(identity) = usize::try_from(admission.id.raw()) else {
        return NativeRuntimeStatus::IDENTITY_EXHAUSTED;
    };

    frames()
        .lock()
        .expect("admitted finalizer storage remains available")
        .insert(identity, admission);

    *output = identity;

    NativeRuntimeStatus::SUCCESS
}

pub(super) const fn admission_failure(error: crate::TaskStartError) -> NativeRuntimeStatus {
    match error {
        crate::TaskStartError::IdentityExhausted => NativeRuntimeStatus::IDENTITY_EXHAUSTED,
        crate::TaskStartError::OutgoingStorageUnavailable => {
            NativeRuntimeStatus::OUTGOING_STORAGE_UNAVAILABLE
        }
    }
}

fn take_frame(identity: usize) -> crate::TaskAdmission {
    frames()
        .lock()
        .expect("admitted finalizer storage remains available")
        .remove(&identity)
        .expect("Bray consumes each admitted finalizer frame exactly once")
}

extern "C" fn discard(identity: usize) {
    drop(take_frame(identity));
}

extern "C" fn finalize(identity: usize, finalizer: NativeStaticFinalizer) -> usize {
    let incidents = super::cleanup::run_asynchronous_finalizer(take_frame(identity), finalizer);
    let mut count: usize = 0;

    for incident in incidents.into_iter().flatten() {
        count += 1;

        let _ = incident.report();
    }

    count
}

extern "C" fn detach_workers(resource: usize, product: usize) {
    if let Some(runtime) = runtime(resource) {
        runtime.detach_product_workers(product);
    }
}

extern "C" fn release(resource: usize) {
    let runtime = resources()
        .lock()
        .expect("execution retention remains available")
        .remove(&resource)
        .expect("Bray releases each execution resource exactly once");

    if let Some(runtime) = runtime {
        runtime.release();
    }
}

extern "C" fn attach(product: usize) -> NativeProductHostObservation {
    super::attachment::attach_current_thread(product)
}

extern "C" fn detach(product: usize) -> NativeProductHostObservation {
    super::attachment::detach_current_thread(product)
}

extern "C" fn finish_root(product: usize) -> NativeProductHostObservation {
    if super::attachment::has_foreign_attachment(product) {
        return super::host::observation_with_status(
            product,
            NativeProductHostStatus::INVALID_ARGUMENT,
        );
    }

    let observation = super::host::observe(product, NativeProductHostOperation::CLOSE);

    super::host::drain_product_thread_statics(product).unwrap_or(observation)
}
