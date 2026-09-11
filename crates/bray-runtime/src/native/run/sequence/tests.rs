use crate::native::frame::{NativeFrame, NativeTerminalState};
use crate::native::run::NativeActivationReservation;
use crate::native::state::{NativeRunReservation, initialize, shutdown, with_runtime};
use crate::product::StaticCleanup;
use bray_runtime_abi::{
    NativeBrayCallOutcome, NativeFrameExit, NativeFrameMetadata, NativeFrameProgress,
    NativeFrameProgressKind, NativeProtectedFrame, NativeRuntimeConfiguration, NativeRuntimeStatus,
    NativeStaticIdentity,
};
use std::cell::Cell;
thread_local! { static DESTROYED: Cell<usize> = const { Cell::new(0) }; }
fn metadata() -> NativeFrameMetadata {
    NativeFrameMetadata::new(
        [163; 32],
        1,
        1,
        1,
        0,
        1,
        crate::test_support::native_origin_frame_state,
    )
}
extern "C-unwind" fn invalid(_: usize) -> NativeFrameProgress {
    NativeFrameProgress::new(NativeFrameProgressKind::YIELDED, 99, 0)
}
extern "C-unwind" fn ready(_: usize) -> NativeFrameProgress {
    NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
}
extern "C-unwind" fn ignore(_: usize) {}
extern "C-unwind" fn lifecycle(_: usize, _: NativeFrameExit) {}
extern "C-unwind" fn complete(_: usize, _: usize) {}
extern "C" fn transition() {}
extern "C-unwind" fn destroy() -> NativeBrayCallOutcome {
    DESTROYED.set(DESTROYED.get() + 1);

    NativeBrayCallOutcome::completed()
}
extern "C-unwind" fn start(
    _: usize,
    _: &mut NativeBrayCallOutcome,
) -> bray_runtime_abi::NativeStaticFinalizerStatus {
    panic!("a NONE finalizer is never started");
}
extern "C-unwind" fn unexpected(_: usize) -> NativeRuntimeStatus {
    panic!("no provider panic");
}
fn completed(_: usize, _: NativeStaticIdentity, count: usize) {
    assert_eq!(count, usize::from(DESTROYED.get() == 1));
}
#[test]
fn invalid_activation_drains_static_destruction_and_remaining_entries() {
    for depth in [0, 64] {
        assert!(initialize(NativeRuntimeConfiguration::new(1, 1)).is_success());
        DESTROYED.set(0);

        let entry = StaticCleanup {
            identity: NativeStaticIdentity::new([163; 32]),
            order: 0,
            prepare: transition,
            destroy,
            detach: transition,
            finalizer: bray_runtime_abi::NativeStaticFinalizer::new(
                bray_runtime_abi::NativeCleanupExecution::NONE,
                None,
                start,
                crate::test_support::panic_callbacks(unexpected, unexpected),
            ),
        };

        with_runtime(|runtime| {
            let mut reservation = NativeRunReservation::prepare(
                NativeFrame::checked_descriptor(&metadata()).unwrap(),
                NativeTerminalState::reserve().unwrap(),
                crate::task::TaskAdmissionKind::Continuation,
            )
            .unwrap();

            reservation.reserve(&runtime.scheduler).unwrap();

            let sequence = Box::new(super::NativeHostSequence::Static(
                super::NativeStaticSequence::new(
                    0,
                    vec![entry; 2],
                    NativeTerminalState::reserve().unwrap(),
                    completed,
                ),
            ));

            reservation.run().install_sequence(sequence);

            reservation.run().install_root(
                NativeActivationReservation::prepare(&metadata())
                    .unwrap()
                    .install(NativeProtectedFrame::new(
                        0,
                        metadata(),
                        invalid,
                        invalid,
                        ignore,
                        lifecycle,
                        complete,
                        ignore,
                    )),
            );

            if depth != 0 {
                reservation
                    .run()
                    .lock_state()
                    .current
                    .as_mut()
                    .unwrap()
                    .state = bray_runtime_model::ProtectedFrameStateId::new(99);
            }

            for _ in 0..depth {
                let mut child = NativeActivationReservation::prepare(&metadata())
                    .unwrap()
                    .install(NativeProtectedFrame::new(
                        0,
                        metadata(),
                        ready,
                        ready,
                        ignore,
                        lifecycle,
                        complete,
                        ignore,
                    ));

                let mut state = reservation.run().lock_state();
                child.parent = state.current.take();
                state.current = Some(child);
            }

            let handle = runtime.allocate_continuation().task().unwrap();
            assert!(runtime.start_run(handle, reservation).is_success());
            let outcome = runtime.resolve_task(handle);
            assert_eq!(outcome.state(), bray_runtime_abi::NativeRunState::COMPLETED);
            assert_eq!(DESTROYED.get(), 2);
            assert!(runtime.destroy_task(handle).is_success());
        })
        .unwrap();

        assert!(shutdown().is_success());
    }
}
