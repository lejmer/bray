use bray_runtime_abi::{
    NativeCleanupIncident, NativeInactiveFrame, NativeStaticFinalizer, NativeStaticFinalizerStatus,
};

use crate::incident::OwnedCleanupIncident as CleanupIncident;

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

    use crate::product::host::control;
    use bray_runtime_abi::{
        NativeProductHostDescriptor, NativeProductHostOperation, NativeProductIdentity,
        NativeStaticDuration, NativeStaticHostEntry, NativeStaticIdentity,
    };

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

        extern "C" fn access() -> usize {
            1
        }

        extern "C" fn dependency(_: usize) -> NativeStaticIdentity {
            NativeStaticIdentity::new([0; 32])
        }

        extern "C" fn entry(_: usize) -> NativeStaticHostEntry {
            NativeStaticHostEntry::new(
                NativeStaticDuration::PRODUCT,
                NativeStaticIdentity::new([91; 32]),
                0,
                1,
                access,
                prepare,
                NativeStaticFinalizer::new(
                    NativeStaticFinalizerExecution::SYNCHRONOUS,
                    admission,
                    finalize,
                    resolve,
                ),
                destroy,
                detach,
                dependency,
                0,
            )
        }

        let descriptor =
            NativeProductHostDescriptor::new(NativeProductIdentity::new([92; 32]), entry, 1);

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::FORM).initialized_statics(),
            1
        );

        assert_eq!(
            control(&descriptor, NativeProductHostOperation::CLOSE).cleanup_incidents(),
            2
        );

        assert_eq!(RECEIVER.get(), 0);
        assert_eq!(OBSERVATIONS.get(), 12);
    }
}
