use bray_platform::RuntimeThreadScope;
use bray_runtime_abi::{
    NativeBrayCallOutcome, NativePanicCause, NativeRunOutcome, NativeRunState, NativeRuntimeStatus,
    NativeSourceAnchor, NativeSynchronousRootCallback, NativeThreadCancellationCallback,
    NativeThreadOperationCallback,
};

use crate::{RunOutcome, execute_synchronous_root};

use super::state::runtime_failure;

#[cfg(test)]
fn bray_runtime_native_thread_execution(
    callback: NativeThreadOperationCallback,
    context: usize,
    cancellation: NativeThreadCancellationCallback,
    cancellation_context: usize,
    panic_payload: &mut usize,
    cleanup: *const (),
) -> u32 {
    bray_runtime_substrate_native_thread_execution(
        callback,
        context,
        cancellation,
        cancellation_context,
        panic_payload,
        cleanup,
    )
}

native_export! {
    #[expect(
        unsafe_code,
        reason = "the bootstrap lends its validated report message for this reporting call"
    )]
    pub extern "C" fn bray_runtime_substrate_panic_reporting(
        cause: u32,
        source_present: u32,
        source_identity: u32,
        source_start: u32,
        source_end: u32,
        source_version: u64,
        message: *const u8,
        message_length: usize,
    ) -> NativeRuntimeStatus {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let cause = match cause {
                0 => NativePanicCause::MESSAGE,
                1 => NativePanicCause::ASSERTION,
                2 => NativePanicCause::EXPLICIT_TEST_FAILURE,
                _ => return NativeRuntimeStatus::INVALID_ARGUMENT,
            };

            let source = match source_present {
                0 if source_identity == 0
                    && source_start == 0
                    && source_end == 0
                    && source_version == 0 => NativeSourceAnchor::unavailable(),
                1 if source_start <= source_end => NativeSourceAnchor::new(
                    source_identity,
                    source_start,
                    source_end,
                    source_version,
                ),
                _ => return NativeRuntimeStatus::INVALID_ARGUMENT,
            };

            if message.is_null() && message_length != 0 {
                return NativeRuntimeStatus::INVALID_ARGUMENT;
            }

            let message = if message_length == 0 {
                String::new()
            } else {
                let bytes = unsafe {
                    // The bootstrap keeps this report-owned allocation live for the call.
                    std::slice::from_raw_parts(message, message_length)
                };

                String::from_utf8_lossy(bytes).into_owned()
            };

            super::export::report_panic(cause, source, message);

            NativeRuntimeStatus::SUCCESS
        }))
        .unwrap_or(NativeRuntimeStatus::PANICKED)
    }
}

native_export! {
    pub extern "C" fn bray_runtime_substrate_native_thread_execution(
        callback: NativeThreadOperationCallback,
        context: usize,
        cancellation: NativeThreadCancellationCallback,
        cancellation_context: usize,
        panic_payload: &mut usize,
        cleanup: *const (),
    ) -> u32 {
        let outcome = crate::context::with_native_thread_cancellation(
            cancellation,
            cancellation_context,
            || {
                execute_callback_boundary(
                    || execute_native_thread_operation(callback, context),
                    |_| {},
                    false,
                    cleanup,
                )
            },
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

        super::host::record_outcome(outcome);

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
        || {
            let mut outcome = runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE);

            callback(destination, &mut outcome);

            outcome
        },
        on_started,
        main_thread,
        cleanup,
    )
}

fn execute_native_thread_operation(
    callback: NativeThreadOperationCallback,
    context: usize,
) -> NativeRunOutcome {
    let mut outcome = NativeBrayCallOutcome::completed();

    callback(context, &mut outcome);

    if outcome.is_completed() {
        return NativeRunOutcome::new(NativeRunState::COMPLETED, 0);
    }

    if outcome.is_cancelled() {
        return NativeRunOutcome::new(NativeRunState::CANCELLED, 0);
    }

    match outcome.panic_report() {
        Some(report) => NativeRunOutcome::new(NativeRunState::PANICKED, report),
        None => runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE),
    }
}

fn execute_callback_boundary(
    callback: impl FnOnce() -> NativeRunOutcome,
    on_started: impl FnOnce(crate::RootCancellationHandle),
    main_thread: bool,
    cleanup: *const (),
) -> NativeRunOutcome {
    #[cfg(test)]
    let _test_isolation = super::state::test_runtime_isolation();

    let thread = RuntimeThreadScope::enter_or_reuse();

    let outcome = match &thread {
        Ok(_) if !main_thread || bray_platform::mark_current_runtime_thread_as_main() => {
            execute_synchronous_root(|| super::host::with_output(callback), on_started)
        }
        Ok(_) | Err(_) => {
            RunOutcome::Completed(runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE))
        }
    };

    let mut outcome = match outcome {
        RunOutcome::Completed(outcome) => outcome,
        RunOutcome::Cancelled => NativeRunOutcome::new(NativeRunState::CANCELLED, 0),
        RunOutcome::Panicked(_) => runtime_failure(NativeRuntimeStatus::PANICKED),
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
    use bray_runtime_abi::{NativeBrayCallOutcome, NativeRunState};

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

    extern "C" fn observe_cancellation(_: usize, _: &mut NativeBrayCallOutcome) {
        assert!(crate::current_run_cancellation_requested());
    }

    extern "C" fn propagate_cancellation(_: usize, outcome: &mut NativeBrayCallOutcome) {
        *outcome = NativeBrayCallOutcome::cancelled();
    }

    extern "C" fn propagate_report(_: usize, outcome: &mut NativeBrayCallOutcome) {
        let Some(panicked) = NativeBrayCallOutcome::panicked(47) else {
            panic!("test panic report handle must be valid");
        };

        *outcome = panicked;
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
            cleanup as *const (),
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
            cleanup as *const (),
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
            cleanup as *const (),
        );

        assert_eq!(state, NativeRunState::CANCELLED.code());
        assert_eq!(payload, 0);
    }
}
