use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_platform::RuntimeThreadScope;
use bray_runtime_abi::{
    NativeRunOutcome, NativeRunState, NativeRuntimeStatus, NativeSynchronousRootCallback,
    NativeThreadCancellationCallback,
};

use crate::root::is_propagated_cancellation;
use crate::{RunOutcome, execute_synchronous_root};

use super::state::runtime_failure;

#[derive(Debug)]
pub(super) struct PropagatedPanicReport(pub(super) usize);

native_export! {
    pub extern "C" fn bray_runtime_native_thread_execution(
        callback: NativeSynchronousRootCallback,
        context: usize,
        cancellation: NativeThreadCancellationCallback,
        cancellation_context: usize,
        panic_payload: &mut usize,
    ) -> u32 {
        let outcome = crate::context::with_native_thread_cancellation(
            cancellation,
            cancellation_context,
            || execute_synchronous_callback(callback, context, |_| {}, false),
        );

        *panic_payload = outcome.payload();

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
    pub extern "C" fn bray_runtime_native_thread_panic_report_recovery(payload: usize) -> usize {
        payload
    }
}

native_export! {
    pub extern "C" fn bray_runtime_synchronous_root_execution(
        callback: NativeSynchronousRootCallback,
        destination: usize,
    ) -> NativeRunOutcome {
        let outcome = execute_synchronous_callback(
            callback,
            destination,
            super::host::register_timeout,
            true,
        );

        super::host::record_outcome(outcome);

        outcome
    }
}

native_export! {
    pub extern "C" fn bray_runtime_foreign_callback_execution(
        callback: NativeSynchronousRootCallback,
        destination: usize,
    ) -> NativeRunOutcome {
        execute_synchronous_callback(callback, destination, |_| {}, false)
    }
}

fn execute_synchronous_callback(
    callback: NativeSynchronousRootCallback,
    destination: usize,
    on_started: impl FnOnce(crate::RootCancellationHandle),
    main_thread: bool,
) -> NativeRunOutcome {
    #[cfg(test)]
    let _test_isolation = super::state::test_runtime_isolation();

    let Ok(thread) = RuntimeThreadScope::enter_or_reuse() else {
        return runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE);
    };

    if main_thread && !bray_platform::mark_current_runtime_thread_as_main() {
        return runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE);
    }

    let outcome = execute_synchronous_root(
        || {
            super::host::with_output(|| {
                match catch_unwind(AssertUnwindSafe(|| callback(destination))) {
                    Ok(()) => NativeRunOutcome::new(NativeRunState::COMPLETED, destination),
                    Err(payload) if is_propagated_cancellation(payload.as_ref()) => {
                        NativeRunOutcome::new(NativeRunState::CANCELLED, 0)
                    }
                    Err(payload) => payload.downcast_ref::<PropagatedPanicReport>().map_or_else(
                        || runtime_failure(NativeRuntimeStatus::PANICKED),
                        |report| NativeRunOutcome::new(NativeRunState::PANICKED, report.0),
                    ),
                }
            })
        },
        on_started,
    );

    let mut outcome = match outcome {
        RunOutcome::Completed(outcome) => outcome,
        RunOutcome::Cancelled => NativeRunOutcome::new(NativeRunState::CANCELLED, 0),
        RunOutcome::Panicked(_) => runtime_failure(NativeRuntimeStatus::PANICKED),
    };

    let cleanup_incidents = thread.finish();

    if cleanup_incidents != 0 {
        super::host::record_cleanup_failure(cleanup_incidents);

        if outcome.state() != NativeRunState::PANICKED {
            outcome = runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE);
        }
    }

    outcome
}

#[cfg(test)]
mod tests {
    use std::panic::panic_any;

    use bray_runtime_abi::NativeRunState;

    use super::{PropagatedPanicReport, bray_runtime_native_thread_execution};

    extern "C" fn cancellation_requested(_: usize) -> u32 {
        1
    }

    extern "C" fn cancellation_not_requested(_: usize) -> u32 {
        0
    }

    extern "C-unwind" fn observe_cancellation(_: usize) {
        assert!(crate::current_run_cancellation_requested());
    }

    extern "C-unwind" fn propagate_cancellation(_: usize) {
        crate::root::propagate_current_run_cancellation();
    }

    extern "C-unwind" fn propagate_report(_: usize) {
        panic_any(PropagatedPanicReport(47));
    }

    #[test]
    fn native_thread_execution_uses_typed_cancellation_observation() {
        let mut payload = 0;

        let state = bray_runtime_native_thread_execution(
            observe_cancellation,
            0,
            cancellation_requested,
            0,
            &mut payload,
        );

        assert_eq!(state, NativeRunState::COMPLETED.code());
        assert_eq!(payload, 0);
    }

    #[test]
    fn native_thread_execution_returns_owned_panic_reports() {
        let mut payload = 0;

        let state = bray_runtime_native_thread_execution(
            propagate_report,
            0,
            cancellation_not_requested,
            0,
            &mut payload,
        );

        assert_eq!(state, NativeRunState::PANICKED.code());
        assert_eq!(payload, 47);
    }

    #[test]
    fn native_thread_execution_returns_propagated_cancellation() {
        let mut payload = 0;

        let state = bray_runtime_native_thread_execution(
            propagate_cancellation,
            0,
            cancellation_requested,
            0,
            &mut payload,
        );

        assert_eq!(state, NativeRunState::CANCELLED.code());
        assert_eq!(payload, 0);
    }
}
