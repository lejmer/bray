use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{
    NativeCleanupIncident, NativeInactiveFrame, NativeRuntimeStatus, NativeSourceAnchor,
    NativeStaticCleanupCallback, NativeStaticFinalizer, NativeStaticFinalizerExecution,
    NativeStaticFinalizerStatus, NativeStaticTransitionCallback, NativeTypeIdentity,
};

use crate::incident::OwnedCleanupIncident as CleanupIncident;

pub(super) struct StaticAdmission {
    task: Option<crate::TaskAdmission>,
    source: Option<bray_runtime_abi::NativeStaticAdmissionCallback>,
}

impl Drop for StaticAdmission {
    fn drop(&mut self) {
        if let Some(release) = self.source {
            let mut outcome = bray_runtime_abi::NativeRunOutcome::new(
                bray_runtime_abi::NativeRunState::COMPLETED,
                0,
            );

            release(1, &mut outcome);

            assert_eq!(
                outcome.state(),
                bray_runtime_abi::NativeRunState::COMPLETED,
                "unused static cleanup allowance must discharge without failure"
            );
        }
    }
}

pub(super) fn admit_finalizer(
    finalizer: NativeStaticFinalizer,
) -> Result<StaticAdmission, crate::TaskStartError> {
    let task = (finalizer.execution() == NativeStaticFinalizerExecution::ASYNCHRONOUS)
        .then(crate::TaskAdmission::new)
        .transpose()?;

    let source = finalizer.admission();

    let mut outcome =
        bray_runtime_abi::NativeRunOutcome::new(bray_runtime_abi::NativeRunState::COMPLETED, 0);

    source(0, &mut outcome);

    if outcome.state() != bray_runtime_abi::NativeRunState::COMPLETED {
        drop(crate::incident::OwnedCleanupIncident::report_owner(
            outcome.take_report(),
        ));

        return Err(crate::TaskStartError::OutgoingStorageUnavailable);
    }

    Ok(StaticAdmission {
        task,
        source: Some(source),
    })
}

pub(super) fn run_static_cleanup(
    admission: &mut StaticAdmission,
    prepare: NativeStaticTransitionCallback,
    finalizer: NativeStaticFinalizer,
    destroy: NativeStaticCleanupCallback,
    detach: NativeStaticTransitionCallback,
) -> usize {
    // Generated owner destruction discharges the accepted allowance. Drop rolls it back only
    // if formation fails before cleanup starts.
    admission.source = None;

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

    // Finalizer errors may retain their receiver. Consume them before its destructor
    // or thread-static detachment can invalidate those dependencies.
    let mut count = consume_incidents([prepare, first, second, third, resolution]);

    let mut published =
        bray_runtime_abi::NativeRunOutcome::new(bray_runtime_abi::NativeRunState::COMPLETED, 0);

    let destroy = catch_unwind(AssertUnwindSafe(|| destroy(&mut published)))
        .err()
        .map(CleanupIncident::panic);

    count += consume_incidents([CleanupIncident::outcome(published), destroy]);

    let detach = catch_unwind(AssertUnwindSafe(|| detach()))
        .err()
        .map(CleanupIncident::panic);

    count + consume_incidents([detach])
}

fn consume_incidents(incidents: impl IntoIterator<Item = Option<CleanupIncident>>) -> usize {
    let mut count = 0;

    for incident in incidents.into_iter().flatten() {
        count += 1;

        let _ = incident.report();
    }

    count
}

fn run_asynchronous_finalizer(
    admission: crate::TaskAdmission,
    finalizer: NativeStaticFinalizer,
) -> [Option<CleanupIncident>; 4] {
    let mut frame = NativeInactiveFrame::empty();
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
    extern "C" fn invalid_report(
        _: usize,
        _: &mut bray_runtime_abi::NativeRunOutcome,
    ) -> NativeRuntimeStatus {
        NativeRuntimeStatus::INVALID_ARGUMENT
    }

    extern "C" fn invalid_destroy(
        _: &mut NativeCleanupIncident,
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

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::panic::panic_any;

    use bray_runtime_abi::{
        NativeRunOutcome, NativeStaticFinalizer, NativeStaticFinalizerExecution,
        NativeStaticFinalizerStatus,
    };

    use super::{StaticAdmission, run_static_cleanup};

    thread_local! {
        static RECEIVER: Cell<u32> = const { Cell::new(0) };
        static OBSERVATIONS: Cell<u32> = const { Cell::new(0) };
    }

    struct Dependent(u32);

    impl Drop for Dependent {
        fn drop(&mut self) {
            assert_eq!(
                RECEIVER.get(),
                self.0,
                "incident must retain the receiver state it depends on"
            );

            OBSERVATIONS.set(OBSERVATIONS.get() * 10 + self.0);
        }
    }

    extern "C" fn prepare() {
        RECEIVER.set(1);
    }
    extern "C" fn detach() {
        assert_eq!(OBSERVATIONS.get(), 12);
        RECEIVER.set(0);
    }
    extern "C" fn admission(_: u32, _: &mut NativeRunOutcome) {}
    extern "C-unwind" fn finalize(
        _: usize,
        _: &mut NativeRunOutcome,
    ) -> NativeStaticFinalizerStatus {
        panic_any(Dependent(1));
    }
    extern "C-unwind" fn resolve(
        _: usize,
        _: usize,
        _: &mut NativeRunOutcome,
    ) -> NativeStaticFinalizerStatus {
        unreachable!("synchronous finalization has no separate resolution");
    }
    extern "C-unwind" fn destroy(_: &mut NativeRunOutcome) {
        assert_eq!(OBSERVATIONS.get(), 1);
        RECEIVER.set(2);
        panic_any(Dependent(2));
    }

    #[test]
    fn incidents_retire_before_receiver_destruction_and_detachment() {
        OBSERVATIONS.set(0);

        let mut storage = StaticAdmission {
            task: None,
            source: Some(admission),
        };

        let finalizer = NativeStaticFinalizer::new(
            NativeStaticFinalizerExecution::SYNCHRONOUS,
            admission,
            finalize,
            resolve,
        );

        assert_eq!(
            run_static_cleanup(&mut storage, prepare, finalizer, destroy, detach),
            2
        );

        assert_eq!(RECEIVER.get(), 0);
        assert_eq!(OBSERVATIONS.get(), 12);
    }
}
