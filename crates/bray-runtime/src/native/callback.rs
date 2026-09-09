use bray_platform::RuntimeThreadScope;
use bray_runtime_abi::{
    NativeBrayCallOutcome, NativePanicCause, NativePanicMessageCopyCallback, NativeRunOutcome,
    NativeRunState, NativeRuntimeStatus, NativeSourceAnchor, NativeSubstrateCleanupCallback,
    NativeSynchronousRootCallback, NativeThreadCancellationCallback, NativeThreadOperationCallback,
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
    cleanup: NativeSubstrateCleanupCallback,
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
    pub extern "C" fn bray_runtime_substrate_panic_reporting(
        cause: u32,
        source_present: u32,
        source_identity: u32,
        source_start: u32,
        source_end: u32,
        source_version: u64,
        message: *const u8,
        message_length: usize,
        copy_message: Option<NativePanicMessageCopyCallback>,
    ) -> NativeRuntimeStatus {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let Some(cause) = NativePanicCause::from_code(cause) else {
                return NativeRuntimeStatus::INVALID_ARGUMENT;
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

            let message = match copy_panic_message(message, message_length, copy_message) {
                Ok(message) => message,
                Err(status) => return status,
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
        cleanup: NativeSubstrateCleanupCallback,
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
        cleanup: NativeSubstrateCleanupCallback,
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
        cleanup: NativeSubstrateCleanupCallback,
    ) -> NativeRunOutcome {
        execute_synchronous_callback(callback, destination, |_| {}, false, cleanup)
    }
}

fn execute_synchronous_callback(
    callback: NativeSynchronousRootCallback,
    destination: usize,
    on_started: impl FnOnce(crate::RootCancellationHandle),
    main_thread: bool,
    cleanup: NativeSubstrateCleanupCallback,
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
    cleanup: NativeSubstrateCleanupCallback,
) -> NativeRunOutcome {
    #[cfg(test)]
    let _test_isolation = super::state::test_runtime_isolation();

    let terminal = triomphe::Arc::new(super::frame::NativeTerminalState::new());
    let incidents = super::incident::IncidentOwnerScope::enter(&terminal);
    let thread = RuntimeThreadScope::enter_or_reuse();

    let outcome = match &thread {
        Ok(_) if !main_thread || bray_platform::mark_current_runtime_thread_as_main() => {
            execute_synchronous_root(|| super::host::with_output(callback), on_started)
        }
        Ok(_) | Err(_) => Ok(RunOutcome::Completed(runtime_failure(
            NativeRuntimeStatus::RUNTIME_FAILURE,
        ))),
    };

    let mut outcome = match outcome {
        Ok(RunOutcome::Completed(outcome)) => outcome,
        Ok(RunOutcome::Cancelled) => NativeRunOutcome::new(NativeRunState::CANCELLED, 0),
        Ok(RunOutcome::Panicked(_)) => runtime_failure(NativeRuntimeStatus::PANICKED),
        Err(crate::RootExecutionError::Cancellation(_)) => {
            runtime_failure(NativeRuntimeStatus::ALLOCATION_FAILURE)
        }
        Err(_) => runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE),
    };

    cleanup();

    outcome = super::incident::finish_synchronous_incidents(&terminal, outcome);
    drop(incidents);

    let cleanup_incidents = thread.map_or(0, bray_platform::RuntimeThreadEntry::finish);

    if cleanup_incidents != 0 {
        super::host::record_cleanup_failure(cleanup_incidents);

        if outcome.state() != NativeRunState::PANICKED {
            outcome = runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE);
        }
    }

    outcome
}

fn copy_panic_message(
    source: *const u8,
    length: usize,
    copy: Option<NativePanicMessageCopyCallback>,
) -> Result<String, NativeRuntimeStatus> {
    if length == 0 {
        return Ok(String::new());
    }

    let copy = copy.ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

    if source.is_null() || isize::try_from(length).is_err() {
        return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
    }

    let mut bytes = Vec::new();

    bytes
        .try_reserve_exact(length)
        .map_err(|_| NativeRuntimeStatus::RUNTIME_FAILURE)?;

    bytes.resize(length, 0);

    let status = copy(source, bytes.as_mut_ptr(), length);

    if !status.is_success() {
        return Err(status);
    }

    Ok(String::from_utf8(bytes)
        .unwrap_or_else(|error| String::from_utf8_lossy(error.as_bytes()).into_owned()))
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::{NativeBrayCallOutcome, NativeRunState};

    use super::bray_runtime_native_thread_execution;

    #[test]
    fn panic_message_borrows_validate_before_copy_and_preserve_callback_failures() {
        use bray_runtime_abi::NativeRuntimeStatus;

        extern "C" fn reject_copy(
            source: *const u8,
            destination: *mut u8,
            length: usize,
        ) -> NativeRuntimeStatus {
            assert!(!source.is_null());
            assert!(!destination.is_null());
            assert_ne!(source, destination.cast_const());
            assert_eq!(length, 1);

            NativeRuntimeStatus::UNKNOWN_TASK
        }

        let source = [b'x'];

        assert_eq!(
            super::copy_panic_message(std::ptr::null(), 0, None),
            Ok(String::new())
        );

        assert_eq!(
            super::copy_panic_message(std::ptr::null(), 1, Some(reject_copy)),
            Err(NativeRuntimeStatus::INVALID_ARGUMENT)
        );

        assert_eq!(
            super::copy_panic_message(source.as_ptr(), 1, None),
            Err(NativeRuntimeStatus::INVALID_ARGUMENT)
        );

        assert_eq!(
            super::copy_panic_message(source.as_ptr(), usize::MAX, Some(reject_copy)),
            Err(NativeRuntimeStatus::INVALID_ARGUMENT)
        );

        assert_eq!(
            super::copy_panic_message(source.as_ptr(), 1, Some(reject_copy)),
            Err(NativeRuntimeStatus::UNKNOWN_TASK)
        );

        for cause in 0..=4 {
            assert_eq!(
                super::bray_runtime_substrate_panic_reporting(
                    cause,
                    0,
                    0,
                    0,
                    0,
                    0,
                    source.as_ptr(),
                    1,
                    Some(reject_copy)
                ),
                NativeRuntimeStatus::UNKNOWN_TASK
            );
        }

        assert_eq!(
            super::bray_runtime_substrate_panic_reporting(
                u32::MAX,
                0,
                0,
                0,
                0,
                0,
                source.as_ptr(),
                1,
                Some(reject_copy)
            ),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            super::bray_runtime_substrate_panic_reporting(
                0,
                1,
                0,
                2,
                1,
                0,
                source.as_ptr(),
                1,
                Some(reject_copy)
            ),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );
    }

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
            cleanup,
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
            cleanup,
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
            cleanup,
        );

        assert_eq!(state, NativeRunState::CANCELLED.code());
        assert_eq!(payload, 0);
    }
}
