use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{
    NativeCleanupIncident, NativeInactiveFrame, NativeRuntimeStatus, NativeSourceAnchor,
    NativeStaticCleanupCallback, NativeStaticFinalizer, NativeStaticFinalizerExecution,
    NativeStaticFinalizerStatus, NativeStaticTransitionCallback, NativeTypeIdentity,
};

use crate::incident::OwnedCleanupIncident as CleanupIncident;

pub(super) struct StaticAdmission {
    task: Option<crate::TaskAdmission>,
    source: usize,
}

impl Drop for StaticAdmission {
    fn drop(&mut self) {
        crate::outgoing::OutgoingRecords::discharge_source(self.source);
    }
}

pub(super) fn admit_finalizer(
    finalizer: NativeStaticFinalizer,
) -> Result<StaticAdmission, crate::TaskStartError> {
    let task = (finalizer.execution() == NativeStaticFinalizerExecution::ASYNCHRONOUS)
        .then(crate::TaskAdmission::new)
        .transpose()?;

    let source = finalizer.outgoing_capacity() as usize;

    crate::outgoing::OutgoingRecords::admit_source(source)
        .map_err(|_| crate::TaskStartError::OutgoingStorageUnavailable)?;

    Ok(StaticAdmission { task, source })
}

pub(super) fn run_static_cleanup(
    admission: &mut StaticAdmission,
    prepare: NativeStaticTransitionCallback,
    finalizer: NativeStaticFinalizer,
    destroy: NativeStaticCleanupCallback,
    detach: NativeStaticTransitionCallback,
) -> [Option<CleanupIncident>; 8] {
    // Generated owner destruction discharges the accepted allowance. Drop rolls it back only
    // if formation fails before cleanup starts.
    admission.source = 0;

    let prepare = catch_unwind(AssertUnwindSafe(|| prepare()))
        .err()
        .map(CleanupIncident::panic);

    let [first, second, third, resolution] = match finalizer.execution() {
        NativeStaticFinalizerExecution::NONE => [None, None, None, None],
        NativeStaticFinalizerExecution::SYNCHRONOUS => {
            let [first, second, third] = collect_finalizer_incidents(|incident, outcome| {
                (finalizer.start())((&raw mut *incident).addr(), outcome)
            });

            [first, second, third, None]
        }
        NativeStaticFinalizerExecution::ASYNCHRONOUS => run_asynchronous_finalizer(
            admission
                .task
                .take()
                .expect("asynchronous finalizer storage was admitted with its static owner"),
            finalizer,
        ),
        _ => [Some(CleanupIncident::runtime_failure()), None, None, None],
    };

    let mut published =
        bray_runtime_abi::NativeRunOutcome::new(bray_runtime_abi::NativeRunState::COMPLETED, 0);

    let destroy = catch_unwind(AssertUnwindSafe(|| destroy(&mut published)))
        .err()
        .map(CleanupIncident::panic);

    let detach = catch_unwind(AssertUnwindSafe(|| detach()))
        .err()
        .map(CleanupIncident::panic);

    [
        prepare,
        first,
        second,
        third,
        resolution,
        CleanupIncident::outcome(published),
        destroy,
        detach,
    ]
}

fn run_asynchronous_finalizer(
    admission: crate::TaskAdmission,
    finalizer: NativeStaticFinalizer,
) -> [Option<CleanupIncident>; 4] {
    extern "C" fn invalid_frame(_: usize) -> bray_runtime_abi::NativeProtectedFrame {
        panic!("inactive static finalizer frame was not initialized")
    }

    let mut frame = NativeInactiveFrame::new(0, invalid_frame);
    let destination = (&raw mut frame).addr();

    let mut published =
        bray_runtime_abi::NativeRunOutcome::new(bray_runtime_abi::NativeRunState::COMPLETED, 0);

    let started = catch_unwind(AssertUnwindSafe(|| {
        (finalizer.start())(destination, &mut published)
    }));

    let incident = CleanupIncident::outcome(published);

    match started {
        Ok(_) if incident.is_some() => [incident, None, None, None],
        Ok(NativeStaticFinalizerStatus::SUCCESS) => {
            crate::native::run_static_finalizer(admission, frame, finalizer.resolve())
        }
        Ok(_) => [Some(CleanupIncident::runtime_failure()), None, None, None],
        Err(payload) => [incident, Some(CleanupIncident::panic(payload)), None, None],
    }
}

pub(crate) fn collect_finalizer_incidents(
    callback: impl FnOnce(
        &mut NativeCleanupIncident,
        &mut bray_runtime_abi::NativeRunOutcome,
    ) -> NativeStaticFinalizerStatus,
) -> [Option<CleanupIncident>; 3] {
    let mut destination = empty_native_incident();

    let mut published =
        bray_runtime_abi::NativeRunOutcome::new(bray_runtime_abi::NativeRunState::COMPLETED, 0);

    let outcome = catch_unwind(AssertUnwindSafe(|| {
        callback(&mut destination, &mut published)
    }));

    let incident = CleanupIncident::native(destination);

    // A published incident has transferred ownership even if the callback then unwinds.
    let failure = match outcome {
        Ok(NativeStaticFinalizerStatus::SUCCESS) => None,
        Ok(NativeStaticFinalizerStatus::INCIDENT) if incident.is_some() => None,
        Ok(_) => Some(CleanupIncident::runtime_failure()),
        Err(payload) => Some(CleanupIncident::panic(payload)),
    };

    [incident, CleanupIncident::outcome(published), failure]
}

const fn empty_native_incident() -> NativeCleanupIncident {
    extern "C-unwind" fn invalid_report(_: usize) -> NativeRuntimeStatus {
        NativeRuntimeStatus::INVALID_ARGUMENT
    }

    extern "C-unwind" fn invalid_destroy(
        _: usize,
        _: &mut bray_runtime_abi::NativeRunOutcome,
        _: &mut bray_runtime_abi::NativeRunOutcome,
    ) {
    }

    NativeCleanupIncident::new(
        0,
        NativeTypeIdentity::new([0; 32]),
        NativeSourceAnchor::unavailable(),
        invalid_report,
        invalid_destroy,
    )
}
