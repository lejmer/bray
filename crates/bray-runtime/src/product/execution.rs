use std::collections::BTreeMap;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use bray_runtime_abi::{
    NativeProductCleanupBody, NativeProductCleanupResult, NativeProductExecution,
    NativeRuntimeStatus, NativeStaticFinalizer,
};

use crate::native::RetainedRuntime;

static NEXT_RESOURCE: AtomicUsize = AtomicUsize::new(1);
static RESOURCES: OnceLock<Mutex<BTreeMap<usize, Option<RetainedRuntime>>>> = OnceLock::new();

fn resources() -> &'static Mutex<BTreeMap<usize, Option<RetainedRuntime>>> {
    RESOURCES.get_or_init(|| Mutex::new(BTreeMap::new()))
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
        current_worker,
    });

    NativeRuntimeStatus::SUCCESS
}

extern "C" fn with_cleanup(
    resource: usize,
    context: usize,
    body: NativeProductCleanupBody,
    attachment: u64,
) -> NativeProductCleanupResult {
    let runtime = runtime(resource);

    let cleanup = || {
        crate::native::with_static_cleanup_context(runtime.as_ref(), || {
            assert!(
                body(context).is_success(),
                "Bray terminal work completes after its cleanup context has entered"
            );
        })
    };

    let result = if attachment == 0 {
        cleanup()
    } else {
        assert_eq!(
            crate::bootstrap::current_runtime_thread().map(|thread| thread.id().raw()),
            Some(attachment),
            "Bray retains the selected exact-thread epoch through this callback"
        );

        crate::bootstrap::with_thread_cleanup(attachment, cleanup)
    };

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

extern "C" fn admit(execution: &NativeProductExecution, output: &mut usize) -> NativeRuntimeStatus {
    *output = 0;

    let owner = runtime(execution.resource)
        .expect("asynchronous product cleanup retains its Source execution");

    let admitted = crate::native::with_cleanup_runtime(&owner, || {
        crate::native::implementation::bray_runtime_task_allocation()
    });

    let (admission, status) = match admitted {
        Ok(admission) => admission,
        Err(status) => return status,
    };

    let Some(task) = admission.task() else {
        return admission.status();
    };

    // Entry retirement cannot revoke the independently admitted source claim.
    if !status.is_success() {
        crate::native::record_cleanup_failure(1);
    }

    *output =
        usize::try_from(task.raw()).expect("native product admission retains its Source task word");

    NativeRuntimeStatus::SUCCESS
}

extern "C" fn discard(execution: &NativeProductExecution, admission: usize) {
    let owner =
        runtime(execution.resource).expect("unused product admission retains its Source execution");

    let task = bray_runtime_abi::NativeTaskHandle::new(admission as u64)
        .expect("Source issued this nonzero claim");

    let (status, retired) = crate::native::with_cleanup_runtime(&owner, || {
        crate::native::implementation::bray_runtime_task_destruction(task)
    })
    .expect("the product's retained cleanup context releases its unused admission");

    assert!(
        status.is_success(),
        "formation rollback consumes its unstarted Source claim once"
    );

    if !retired.is_success() {
        crate::native::record_cleanup_failure(1);
    }
}

extern "C" fn finalize(
    execution: &NativeProductExecution,
    admission: usize,
    finalizer: NativeStaticFinalizer,
) -> usize {
    let owner =
        runtime(execution.resource).expect("product finalization retains its Source execution");

    let task = bray_runtime_abi::NativeTaskHandle::new(admission as u64)
        .expect("Source issued this nonzero claim");

    let (incidents, retired) = crate::native::with_cleanup_runtime(&owner, || {
        super::cleanup::run_asynchronous_finalizer(task, finalizer)
    })
    .expect("the selected product cleanup context drives its admitted finalizer");

    let mut count: usize = usize::from(!retired.is_success());

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

extern "C" fn current_worker(resource: usize) -> u32 {
    u32::from(runtime(resource).is_some_and(|runtime| runtime.owns_current_worker()))
}
