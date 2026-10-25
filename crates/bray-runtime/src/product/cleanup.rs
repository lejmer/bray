use bray_runtime_abi::{
    NativeCleanupIncident, NativeInactiveFrame, NativeStaticCleanupCallback, NativeStaticFinalizer,
    NativeStaticFinalizerExecution, NativeStaticFinalizerStatus, NativeStaticTransitionCallback,
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

    prepare();

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

    let abandoned = [
        first.as_ref(),
        second.as_ref(),
        third.as_ref(),
        resolution.as_ref(),
    ]
    .into_iter()
    .any(|incident| incident.is_some());

    // Finalizer errors may retain their receiver. Consume them before its destructor
    // or thread-static detachment can invalidate those dependencies.
    let mut count = consume_incidents([first, second, third, resolution]);

    let mut published =
        bray_runtime_abi::NativeRunOutcome::new(bray_runtime_abi::NativeRunState::COMPLETED, 0);

    destroy(u32::from(abandoned), &mut published);

    count += consume_incidents([CleanupIncident::outcome(published)]);

    detach();

    count
}

fn consume_incidents(incidents: impl IntoIterator<Item = Option<CleanupIncident>>) -> usize {
    let mut count = 0;

    for incident in incidents.into_iter().flatten() {
        count += 1;

        let _ = incident.report();
    }

    count
}

pub(super) fn run_asynchronous_finalizer(
    admission: crate::TaskAdmission,
    finalizer: NativeStaticFinalizer,
) -> [Option<CleanupIncident>; 4] {
    let mut frame = NativeInactiveFrame::empty();
    let destination = (&raw mut frame).addr();

    let mut published =
        bray_runtime_abi::NativeRunOutcome::new(bray_runtime_abi::NativeRunState::COMPLETED, 0);

    let started = (finalizer.start())(destination, &mut published);
    let incident = CleanupIncident::outcome(published);

    match started {
        _ if incident.is_some() => [incident, None, None, None],
        NativeStaticFinalizerStatus::SUCCESS => {
            crate::native::run_static_finalizer(admission, frame, finalizer.resolve())
        }
        _ => [Some(CleanupIncident::runtime_failure()), None, None, None],
    }
}

pub(crate) fn collect_finalizer_incidents(
    callback: impl FnOnce(
        &mut NativeCleanupIncident,
        &mut bray_runtime_abi::NativeRunOutcome,
    ) -> NativeStaticFinalizerStatus,
) -> [Option<CleanupIncident>; 3] {
    let mut destination = NativeCleanupIncident::empty();

    let mut published =
        bray_runtime_abi::NativeRunOutcome::new(bray_runtime_abi::NativeRunState::COMPLETED, 0);

    let outcome = callback(&mut destination, &mut published);

    let incident = CleanupIncident::native(destination);

    let failure = match outcome {
        NativeStaticFinalizerStatus::SUCCESS => None,
        NativeStaticFinalizerStatus::INCIDENT if incident.is_some() => None,
        _ => Some(CleanupIncident::runtime_failure()),
    };

    [incident, CleanupIncident::outcome(published), failure]
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

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
    extern "C" fn release_incident(payload: usize, _: usize, _: &mut NativeRunOutcome) {
        drop(Dependent(
            u32::try_from(payload).expect("test receiver identity fits u32"),
        ));
    }

    fn incident(receiver: usize) -> NativeRunOutcome {
        NativeRunOutcome::panicked(crate::frame::native_report(
            bray_runtime_abi::NativePanicPrimary::new(
                bray_runtime_abi::NativePanicCause::MESSAGE,
                bray_runtime_abi::NativeSourceAnchor::unavailable(),
                bray_runtime_abi::NativePanicMessage::new(
                    receiver,
                    0,
                    None,
                    Some(release_incident),
                ),
            ),
        ))
    }

    extern "C" fn finalize(_: usize, output: &mut NativeRunOutcome) -> NativeStaticFinalizerStatus {
        *output = incident(1);

        NativeStaticFinalizerStatus::SUCCESS
    }
    extern "C" fn resolve(
        _: usize,
        _: usize,
        _: &mut NativeRunOutcome,
    ) -> NativeStaticFinalizerStatus {
        unreachable!("synchronous finalization has no separate resolution");
    }
    extern "C" fn destroy(abandoned: u32, output: &mut NativeRunOutcome) {
        assert_eq!(abandoned, 1);
        assert_eq!(OBSERVATIONS.get(), 1);
        RECEIVER.set(2);
        *output = incident(2);
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
