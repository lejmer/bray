use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{
    NativeCleanupIncident, NativeInactiveFrame, NativeRuntimeStatus, NativeSourceAnchor,
    NativeStaticCleanupCallback, NativeStaticFinalizer, NativeStaticFinalizerExecution,
    NativeStaticFinalizerStatus, NativeStaticTransitionCallback, NativeTypeIdentity,
};

use super::incident::CleanupIncident;

pub(super) fn run_static_cleanup(
    prepare: NativeStaticTransitionCallback,
    finalizer: NativeStaticFinalizer,
    destroy: NativeStaticCleanupCallback,
    detach: NativeStaticTransitionCallback,
) -> Vec<CleanupIncident> {
    let mut incidents = Vec::new();

    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| prepare())) {
        incidents.push(CleanupIncident::panic(payload));
    }

    incidents.extend(run_finalizer(finalizer));

    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| destroy())) {
        incidents.push(CleanupIncident::panic(payload));
    }

    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| detach())) {
        incidents.push(CleanupIncident::panic(payload));
    }

    incidents
}

fn run_finalizer(finalizer: NativeStaticFinalizer) -> Vec<CleanupIncident> {
    match finalizer.execution() {
        NativeStaticFinalizerExecution::NONE => Vec::new(),
        NativeStaticFinalizerExecution::SYNCHRONOUS => run_synchronous_finalizer(finalizer),
        NativeStaticFinalizerExecution::ASYNCHRONOUS => run_asynchronous_finalizer(finalizer),
        _ => vec![CleanupIncident::runtime_failure()],
    }
}

fn run_synchronous_finalizer(finalizer: NativeStaticFinalizer) -> Vec<CleanupIncident> {
    let mut incident = empty_native_incident();
    let destination = (&raw mut incident).addr();

    match catch_unwind(AssertUnwindSafe(|| (finalizer.start())(destination))) {
        Ok(status) => incidents_from_status(status, incident),
        Err(payload) => vec![CleanupIncident::panic(payload)],
    }
}

fn run_asynchronous_finalizer(finalizer: NativeStaticFinalizer) -> Vec<CleanupIncident> {
    extern "C" fn invalid_frame(_: usize) -> bray_runtime_abi::NativeProtectedFrame {
        panic!("inactive static finalizer frame was not initialized")
    }

    let mut frame = NativeInactiveFrame::new(0, invalid_frame);
    let destination = (&raw mut frame).addr();

    match catch_unwind(AssertUnwindSafe(|| (finalizer.start())(destination))) {
        Ok(NativeStaticFinalizerStatus::SUCCESS) => {
            crate::native::run_static_finalizer(frame, finalizer.resolve())
        }
        Ok(_) => vec![CleanupIncident::runtime_failure()],
        Err(payload) => vec![CleanupIncident::panic(payload)],
    }
}

pub(crate) fn incidents_from_status(
    status: NativeStaticFinalizerStatus,
    incident: NativeCleanupIncident,
) -> Vec<CleanupIncident> {
    match status {
        NativeStaticFinalizerStatus::SUCCESS => Vec::new(),
        NativeStaticFinalizerStatus::INCIDENT => CleanupIncident::native(incident).map_or_else(
            || vec![CleanupIncident::runtime_failure()],
            |incident| vec![incident],
        ),
        _ => vec![CleanupIncident::runtime_failure()],
    }
}

pub(crate) const fn empty_native_incident() -> NativeCleanupIncident {
    extern "C-unwind" fn invalid_report(_: usize) -> NativeRuntimeStatus {
        NativeRuntimeStatus::INVALID_ARGUMENT
    }

    extern "C-unwind" fn invalid_destroy(_: usize) {}

    NativeCleanupIncident::new(
        0,
        NativeTypeIdentity::new([0; 32]),
        NativeSourceAnchor::unavailable(),
        invalid_report,
        invalid_destroy,
    )
}
