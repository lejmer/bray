use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_platform::RuntimeThreadScope;
use bray_runtime_abi::{
    NativeRunOutcome, NativeRunState, NativeRuntimeStatus, NativeSynchronousRootCallback,
};

use crate::root::is_propagated_cancellation;
use crate::{RunOutcome, execute_synchronous_root};

use super::state::runtime_failure;

#[derive(Debug)]
pub(super) struct PropagatedPanicReport(pub(super) usize);

native_export! {
    pub extern "C" fn bray_runtime_synchronous_root_execution(
        callback: NativeSynchronousRootCallback,
        destination: usize,
    ) -> NativeRunOutcome {
        let outcome = execute_synchronous_callback(
            callback,
            destination,
            super::host::register_timeout,
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
        execute_synchronous_callback(callback, destination, |_| {})
    }
}

fn execute_synchronous_callback(
    callback: NativeSynchronousRootCallback,
    destination: usize,
    on_started: impl FnOnce(crate::RootCancellationHandle),
) -> NativeRunOutcome {
    let Ok(_thread) = RuntimeThreadScope::enter_or_reuse() else {
        return runtime_failure(NativeRuntimeStatus::RUNTIME_FAILURE);
    };

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

    match outcome {
        RunOutcome::Completed(outcome) => outcome,
        RunOutcome::Cancelled => NativeRunOutcome::new(NativeRunState::CANCELLED, 0),
        RunOutcome::Panicked(_) => runtime_failure(NativeRuntimeStatus::PANICKED),
    }
}
