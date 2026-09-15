use bray_platform::RuntimeThreadScope;
use bray_runtime_abi::{
    NativeRunOutcome, NativeRunState, NativeRuntimeStatus, NativeSynchronousRootCallback,
    NativeThreadCancellationCallback, NativeThreadOperationCallback,
};

use crate::{RunOutcome, execute_synchronous_root};

use super::state::runtime_failure;

#[cfg(test)]
fn bray_runtime_native_thread_execution(
    callback: NativeThreadOperationCallback,
    context: usize,
    cancellation: NativeThreadCancellationCallback,
    cancellation_context: usize,
    panic_report: &mut bray_runtime_abi::NativePanicReport,
    cleanup: *const (),
) -> u32 {
    bray_runtime_substrate_native_thread_execution(
        callback,
        context,
        cancellation,
        cancellation_context,
        panic_report,
        cleanup,
    )
}

native_export! {
    pub extern "C" fn bray_runtime_substrate_panic_report_initialization(report: &mut bray_runtime_abi::NativePanicReport) -> NativeRuntimeStatus {
        let (primary, head, tail, count, reserved) = report.take_parts();
        if !primary.cause().is_known() || !primary.source().is_valid() || head != 0 || tail != 0 || count != 0 || reserved != 0 {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }
        *report = crate::frame::native_report(primary);
        NativeRuntimeStatus::SUCCESS
    }
}

native_export! {
    pub extern "C" fn bray_runtime_substrate_native_thread_execution(
        callback: NativeThreadOperationCallback,
        context: usize,
        cancellation: NativeThreadCancellationCallback,
        cancellation_context: usize,
        panic_report: &mut bray_runtime_abi::NativePanicReport,
        cleanup: *const (),
    ) -> u32 {
        let mut outcome = crate::context::with_native_thread_cancellation(
            cancellation,
            cancellation_context,
            || {
                execute_callback_boundary(
                    |outcome| callback(context, outcome),
                    |_| {},
                    false,
                    cleanup,
                )
            },
        );

        if outcome.state() == NativeRunState::PANICKED {
            *panic_report = outcome.take_report();
        }

        outcome.state().code()
    }
}

native_export! {
    pub extern "C" fn bray_runtime_current_native_thread_identity() -> u64 {
        bray_platform::current_runtime_thread()
            .map_or(0, |thread| thread.id().raw())
    }
}

native_export! {
    pub extern "C" fn bray_runtime_main_native_thread_identity() -> u64 {
        bray_platform::main_runtime_thread().map_or(0, |thread| thread.id().raw())
    }
}

native_export! {
    pub extern "C" fn bray_runtime_substrate_synchronous_root_execution(
        callback: NativeSynchronousRootCallback,
        destination: usize,
        cleanup: *const (),
    ) -> NativeRunOutcome {
        let outcome = execute_synchronous_callback(
            callback,
            destination,
            super::host::register_timeout,
            true,
            cleanup,
        );

        super::host::record_outcome(&outcome);

        outcome
    }
}

native_export! {
    pub extern "C" fn bray_runtime_substrate_foreign_callback_execution(
        callback: NativeSynchronousRootCallback,
        destination: usize,
        cleanup: *const (),
    ) -> NativeRunOutcome {
        execute_synchronous_callback(callback, destination, |_| {}, false, cleanup)
    }
}

fn execute_synchronous_callback(
    callback: NativeSynchronousRootCallback,
    destination: usize,
    on_started: impl FnOnce(crate::RootCancellationHandle),
    main_thread: bool,
    cleanup: *const (),
) -> NativeRunOutcome {
    execute_callback_boundary(
        |outcome| callback(destination, outcome),
        on_started,
        main_thread,
        cleanup,
    )
}

fn execute_callback_boundary(
    callback: impl FnOnce(&mut NativeRunOutcome),
    on_started: impl FnOnce(crate::RootCancellationHandle),
    main_thread: bool,
    cleanup: *const (),
) -> NativeRunOutcome {
    #[cfg(test)]
    let _test_isolation = super::state::test_runtime_isolation();

    let Ok(mut admitted) = crate::outgoing::OutgoingRecords::admit(2) else {
        return super::outgoing::allocation_failure();
    };

    let mut published = runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE);
    let thread = RuntimeThreadScope::enter_or_reuse();

    let outcome = match &thread {
        Ok(_) if !main_thread || bray_platform::mark_current_runtime_thread_as_main() => {
            execute_synchronous_root(
                || super::host::with_output(|| callback(&mut published)),
                on_started,
            )
        }
        Ok(_) | Err(_) => RunOutcome::Completed(()),
    };

    let mut outcome = match outcome {
        RunOutcome::Completed(()) => published,
        RunOutcome::Cancelled if published.state() == NativeRunState::PANICKED => published,
        RunOutcome::Cancelled => NativeRunOutcome::new(NativeRunState::CANCELLED, 0),
        RunOutcome::Panicked(panic) => {
            let report = if published.state() == NativeRunState::PANICKED {
                let mut report = crate::RuntimePanic::from_native(published.take_report());

                report.append(panic, &mut admitted);

                report
            } else {
                panic
            };

            NativeRunOutcome::panicked(report.into_native(&mut admitted))
        }
    };

    run_substrate_cleanup(cleanup);

    let cleanup_incidents = thread.map_or(0, bray_platform::RuntimeThreadEntry::finish);

    if cleanup_incidents != 0 {
        super::host::record_cleanup_failure(cleanup_incidents);

        if outcome.state() != NativeRunState::PANICKED {
            outcome = runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE);
        }
    }

    outcome
}

#[expect(
    unsafe_code,
    reason = "the trusted Bray bootstrap passes its exact typed cleanup callback as an opaque code pointer"
)]
fn run_substrate_cleanup(cleanup: *const ()) {
    let cleanup: extern "C" fn() = unsafe {
        // The private substrate ABI receives the exact SubstrateCleanup pointer formed by Bray.
        std::mem::transmute(cleanup)
    };

    cleanup();
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::{NativeRunOutcome, NativeRunState};

    use super::bray_runtime_native_thread_execution;

    extern "C" fn cancellation_requested(_: usize) -> u32 {
        1
    }

    extern "C" fn cancellation_not_requested(_: usize) -> u32 {
        0
    }

    extern "C" fn cleanup() {
        assert!(bray_platform::current_runtime_thread().is_some());
    }

    extern "C-unwind" fn observe_cancellation(_: usize, outcome: &mut NativeRunOutcome) {
        assert!(crate::current_run_cancellation_requested());
        *outcome = NativeRunOutcome::new(NativeRunState::COMPLETED, 0);
    }

    extern "C-unwind" fn propagate_cancellation(_: usize, outcome: &mut NativeRunOutcome) {
        *outcome = NativeRunOutcome::new(NativeRunState::CANCELLED, 0);
    }

    extern "C-unwind" fn propagate_report(_: usize, outcome: &mut NativeRunOutcome) {
        *outcome = NativeRunOutcome::panicked(crate::frame::native_report(
            bray_runtime_abi::NativePanicPrimary::empty(),
        ));
    }

    extern "C-unwind" fn write_then_cancel(context: usize, outcome: &mut NativeRunOutcome) {
        propagate_report(context, outcome);
        crate::root::propagate_current_run_cancellation();
    }

    #[test]
    fn callback_cancellation_preserves_an_already_published_panic() {
        let mut outcome = super::bray_runtime_substrate_synchronous_root_execution(write_then_cancel, 0, cleanup as *const ());
        assert_eq!(outcome.state(), NativeRunState::PANICKED);
        assert!(outcome.take_report().consume(false).is_success());
    }

    thread_local! {
        static RELEASES: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    struct CaughtIncident {
        _failure: crate::outgoing::tests::AdmissionFailure,
    }

    impl Drop for CaughtIncident {
        fn drop(&mut self) {
            RELEASES.with_borrow_mut(|events| events.push(2));
        }
    }

    extern "C" fn release_written_incident(_: usize, _: usize) {
        assert!(crate::outgoing::OutgoingRecords::admit(1).is_err());

        RELEASES.with_borrow_mut(|events| events.push(1));
    }

    extern "C-unwind" fn write_then_unwind(_: usize, outcome: &mut NativeRunOutcome) {
        let primary = bray_runtime_abi::NativePanicPrimary::new(
            bray_runtime_abi::NativePanicCause::ASSERTION,
            bray_runtime_abi::NativeSourceAnchor::new(7, 11, 19, 23),
            bray_runtime_abi::NativePanicMessage::new(0, 0, None, Some(release_written_incident)),
        );

        *outcome = NativeRunOutcome::panicked(crate::frame::native_report(primary));

        std::panic::panic_any(CaughtIncident {
            _failure: crate::outgoing::tests::reject_admission(),
        });
    }

    #[test]
    fn callback_admission_failure_preserves_the_allocation_cause_and_inputs() {
        let _isolation = super::super::state::test_runtime_isolation();
        let _failure = crate::outgoing::tests::reject_admission();
        let mut called = false;

        let mut outcome = super::execute_callback_boundary(
            |_| called = true,
            |_| {},
            false,
            cleanup as *const (),
        );

        assert!(!called);
        assert_eq!(outcome.state(), NativeRunState::PANICKED);

        assert_eq!(
            outcome.take_report().take_parts().0.cause(),
            bray_runtime_abi::NativePanicCause::ALLOCATION_FAILURE
        );
    }

    #[test]
    fn callback_report_outlives_its_producer_and_keeps_written_then_caught_incidents() {
        let mut report = bray_runtime_abi::NativePanicReport::empty();

        let state = bray_runtime_native_thread_execution(
            write_then_unwind,
            0,
            cancellation_not_requested,
            0,
            &mut report,
            cleanup as *const (),
        );

        assert_eq!(state, NativeRunState::PANICKED.code());
        assert!(crate::outgoing::OutgoingRecords::admit(1).is_err());

        RELEASES.with_borrow(|events| assert!(events.is_empty()));

        assert!(report.consume(false).is_success());

        RELEASES.with_borrow_mut(|events| assert_eq!(std::mem::take(events), [1, 2]));

        assert!(report.consume(false).is_success());
    }

    #[test]
    fn native_thread_execution_uses_typed_cancellation_observation() {
        let mut payload = bray_runtime_abi::NativePanicReport::empty();

        let state = bray_runtime_native_thread_execution(
            observe_cancellation,
            0,
            cancellation_requested,
            0,
            &mut payload,
            cleanup as *const (),
        );

        assert_eq!(state, NativeRunState::COMPLETED.code());
        assert!(payload.consume(false).is_success());
    }

    #[test]
    fn native_thread_execution_returns_owned_panic_reports() {
        let mut payload = bray_runtime_abi::NativePanicReport::empty();

        let state = bray_runtime_native_thread_execution(
            propagate_report,
            0,
            cancellation_not_requested,
            0,
            &mut payload,
            cleanup as *const (),
        );

        assert_eq!(state, NativeRunState::PANICKED.code());
        assert!(payload.consume(false).is_success());
    }

    #[test]
    fn native_thread_execution_returns_propagated_cancellation() {
        let mut payload = bray_runtime_abi::NativePanicReport::empty();

        let state = bray_runtime_native_thread_execution(
            propagate_cancellation,
            0,
            cancellation_requested,
            0,
            &mut payload,
            cleanup as *const (),
        );

        assert_eq!(state, NativeRunState::CANCELLED.code());
        assert!(payload.consume(false).is_success());
    }
}
