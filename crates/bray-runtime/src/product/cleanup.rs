use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{
    NativeBrayCallOutcome, NativeCleanupExecution, NativeCleanupIncident,
    NativePanicReportCallbacks, NativeRuntimeStatus, NativeSourceAnchor,
    NativeStaticCleanupCallback, NativeStaticFinalizer, NativeStaticFinalizerStatus,
    NativeStaticTransitionCallback, NativeTypeIdentity,
};

use crate::incident::OwnedCleanupIncident;

pub(super) fn run_static_cleanup(
    prepare: NativeStaticTransitionCallback,
    finalizer: NativeStaticFinalizer,
    destroy: NativeStaticCleanupCallback,
    detach: NativeStaticTransitionCallback,
) -> Vec<OwnedCleanupIncident> {
    let ((), incidents) = crate::native::with_cleanup_incident_owner(|record| {
        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| prepare())) {
            record(OwnedCleanupIncident::host(payload));
        }

        for incident in run_finalizer(finalizer) {
            record(incident);
        }

        match catch_unwind(AssertUnwindSafe(|| destroy())) {
            Ok(outcome) => {
                if let Some(incident) = OwnedCleanupIncident::boundary(outcome, finalizer.panics())
                {
                    record(incident);
                }
            }
            Err(payload) => record(OwnedCleanupIncident::host(payload)),
        }

        if let Err(payload) = catch_unwind(AssertUnwindSafe(|| detach())) {
            record(OwnedCleanupIncident::host(payload));
        }
    });

    incidents
}

fn run_finalizer(finalizer: NativeStaticFinalizer) -> Vec<OwnedCleanupIncident> {
    match finalizer.execution() {
        NativeCleanupExecution::NONE => Vec::new(),
        NativeCleanupExecution::SYNCHRONOUS => run_synchronous_finalizer(finalizer),
        NativeCleanupExecution::ASYNCHRONOUS => run_asynchronous_finalizer(finalizer),
        _ => vec![OwnedCleanupIncident::runtime_failure()],
    }
}

fn run_synchronous_finalizer(finalizer: NativeStaticFinalizer) -> Vec<OwnedCleanupIncident> {
    let mut incident = empty_native_incident();
    let destination = (&raw mut incident).addr();
    let mut outcome = NativeBrayCallOutcome::completed();

    let result = catch_unwind(AssertUnwindSafe(|| {
        (finalizer.start())(destination, &mut outcome)
    }));

    finish_finalizer_callback(result, outcome, incident, finalizer.panics())
}

fn run_asynchronous_finalizer(finalizer: NativeStaticFinalizer) -> Vec<OwnedCleanupIncident> {
    let mut frame = crate::native::inactive_frame_output();
    let destination = (&raw mut frame).addr();
    let mut outcome = NativeBrayCallOutcome::completed();

    let result = catch_unwind(AssertUnwindSafe(|| {
        (finalizer.start())(destination, &mut outcome)
    }));

    if let Some(incident) = OwnedCleanupIncident::boundary(outcome, finalizer.panics()) {
        let mut incidents = vec![incident];

        if let Err(payload) = result {
            incidents.push(OwnedCleanupIncident::host(payload));
        }

        return incidents;
    }

    match result {
        Ok(NativeStaticFinalizerStatus::SUCCESS) => {
            crate::native::run_static_finalizer(frame, finalizer.resolve(), finalizer.panics())
        }
        Ok(_) => vec![OwnedCleanupIncident::runtime_failure()],
        Err(payload) => vec![OwnedCleanupIncident::host(payload)],
    }
}

pub(crate) fn finish_finalizer_callback(
    result: std::thread::Result<NativeStaticFinalizerStatus>,
    outcome: NativeBrayCallOutcome,
    incident: NativeCleanupIncident,
    callbacks: NativePanicReportCallbacks,
) -> Vec<OwnedCleanupIncident> {
    let mut incidents = OwnedCleanupIncident::boundary(outcome, callbacks)
        .into_iter()
        .collect::<Vec<_>>();

    match result {
        Ok(status) if outcome.is_completed() => {
            incidents.extend(incidents_from_status(status, incident))
        }
        Ok(_) => {}
        Err(payload) => incidents.push(OwnedCleanupIncident::host(payload)),
    }

    incidents
}

fn incidents_from_status(
    status: NativeStaticFinalizerStatus,
    incident: NativeCleanupIncident,
) -> Vec<OwnedCleanupIncident> {
    match status {
        NativeStaticFinalizerStatus::SUCCESS => Vec::new(),
        NativeStaticFinalizerStatus::INCIDENT => OwnedCleanupIncident::native(incident)
            .map_or_else(
                || vec![OwnedCleanupIncident::runtime_failure()],
                |incident| vec![incident],
            ),
        _ => vec![OwnedCleanupIncident::runtime_failure()],
    }
}

pub(crate) const fn empty_native_incident() -> NativeCleanupIncident {
    extern "C-unwind" fn invalid_incident_report(_: &NativeCleanupIncident) -> NativeRuntimeStatus {
        NativeRuntimeStatus::INVALID_ARGUMENT
    }

    extern "C-unwind" fn invalid_report(_: usize) -> NativeRuntimeStatus {
        NativeRuntimeStatus::INVALID_ARGUMENT
    }

    extern "C-unwind" fn invalid_destroy(_: usize) -> NativeBrayCallOutcome {
        NativeBrayCallOutcome::completed()
    }

    extern "C" fn invalid_construction(_: &NativeCleanupIncident) -> usize {
        0
    }

    extern "C" fn invalid_suppression(_: usize, _: usize) -> usize {
        0
    }

    NativeCleanupIncident::new(
        0,
        NativeTypeIdentity::new([0; 32]),
        NativeSourceAnchor::unavailable(),
        invalid_incident_report,
        invalid_destroy,
        NativePanicReportCallbacks::new(
            invalid_report,
            invalid_report,
            invalid_construction,
            invalid_suppression,
        ),
    )
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use bray_runtime_abi::{
        NativeBrayCallOutcome, NativeCleanupExecution, NativeCleanupIncident, NativeRuntimeStatus,
        NativeSourceAnchor, NativeStaticFinalizer, NativeStaticFinalizerStatus, NativeTypeIdentity,
    };

    thread_local! {
        static REPORTED: RefCell<Vec<usize>> = const { RefCell::new(Vec::new()) };
    }

    extern "C-unwind" fn report(payload: usize) -> NativeRuntimeStatus {
        REPORTED.with(|events| events.borrow_mut().push(payload));

        NativeRuntimeStatus::SUCCESS
    }

    extern "C-unwind" fn destroy_report(_: usize) -> NativeRuntimeStatus {
        NativeRuntimeStatus::SUCCESS
    }

    extern "C-unwind" fn report_error(incident: &NativeCleanupIncident) -> NativeRuntimeStatus {
        report(incident.payload())
    }

    extern "C-unwind" fn destroy_error(_: usize) -> NativeBrayCallOutcome {
        NativeBrayCallOutcome::completed()
    }

    fn transfer(payload: usize) {
        let incident = NativeCleanupIncident::new(
            payload,
            NativeTypeIdentity::new([1; 32]),
            NativeSourceAnchor::unavailable(),
            report_error,
            destroy_error,
            crate::test_support::panic_callbacks(report, destroy_report),
        );

        assert_eq!(
            crate::native::implementation::bray_runtime_cleanup_incident_transfer(&incident),
            NativeRuntimeStatus::SUCCESS
        );
    }

    extern "C" fn prepare() {
        transfer(3);
    }

    extern "C-unwind" fn finalize(
        _: usize,
        _: &mut NativeBrayCallOutcome,
    ) -> NativeStaticFinalizerStatus {
        transfer(7);

        NativeStaticFinalizerStatus::SUCCESS
    }

    extern "C-unwind" fn resolve(
        _: usize,
        _: usize,
        _: &mut NativeBrayCallOutcome,
    ) -> NativeStaticFinalizerStatus {
        panic!("synchronous finalization does not invoke a frame resolver");
    }

    extern "C-unwind" fn destroy() -> NativeBrayCallOutcome {
        transfer(9);

        NativeBrayCallOutcome::panicked(11).unwrap()
    }

    extern "C" fn detach() {
        transfer(13);
    }

    #[test]
    fn static_cleanup_merges_transferred_errors_and_callback_failures_in_encounter_order() {
        let finalizer = NativeStaticFinalizer::new(
            NativeCleanupExecution::SYNCHRONOUS,
            0,
            1,
            finalize,
            resolve,
            crate::test_support::panic_callbacks(report, destroy_report),
        );

        let incidents = super::run_static_cleanup(prepare, finalizer, destroy, detach);

        assert_eq!(incidents.len(), 5);
        assert!(REPORTED.with(|events| events.borrow().is_empty()));

        for incident in incidents.into_iter().rev() {
            assert!(!incident.report());
        }

        assert_eq!(
            REPORTED.with(|events| std::mem::take(&mut *events.borrow_mut())),
            [13, 11, 9, 7, 3]
        );
    }
}
