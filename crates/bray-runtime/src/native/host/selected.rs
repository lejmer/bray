use std::cell::Cell;
use std::io;

use bray_runtime_abi::{NativePanicCause, NativeRunOutcome, NativeSourceAnchor};

use crate::RootCancellationHandle;
use crate::context::{TaskOutput, with_task_output};

thread_local! {
    static CALLBACKS: Cell<Option<NativeHostCallbacks>> = const { Cell::new(None) };
}

/// Test-host integration selected explicitly by the native test adapter.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct NativeHostCallbacks {
    active: fn() -> bool,
    output: fn() -> TaskOutput,
    register_timeout: fn(RootCancellationHandle),
    record_outcome: fn(NativeRunOutcome),
    record_panic: fn(NativePanicCause, NativeSourceAnchor, String),
    record_returned_error: fn(),
    record_cleanup_failure: fn(usize),
    finish: fn() -> io::Result<()>,
}

impl NativeHostCallbacks {
    /// Creates the complete test-host callback boundary.
    #[expect(
        clippy::too_many_arguments,
        reason = "each callback represents a distinct native host lifecycle operation"
    )]
    pub const fn new(
        active: fn() -> bool,
        output: fn() -> TaskOutput,
        register_timeout: fn(RootCancellationHandle),
        record_outcome: fn(NativeRunOutcome),
        record_panic: fn(NativePanicCause, NativeSourceAnchor, String),
        record_returned_error: fn(),
        record_cleanup_failure: fn(usize),
        finish: fn() -> io::Result<()>,
    ) -> Self {
        Self {
            active,
            output,
            register_timeout,
            record_outcome,
            record_panic,
            record_returned_error,
            record_cleanup_failure,
            finish,
        }
    }
}

/// Selects test-host integration for the current native execution thread.
#[doc(hidden)]
pub fn register_host_callbacks(callbacks: NativeHostCallbacks) {
    CALLBACKS.set(Some(callbacks));
}

pub(in crate::native) fn active() -> bool {
    callbacks().is_some_and(|callbacks| (callbacks.active)())
}

pub(in crate::native) fn with_output<T>(callback: impl FnOnce() -> T) -> T {
    let output = callbacks().map_or(None, |callbacks| (callbacks.output)());

    with_task_output(output, callback)
}

pub(in crate::native) fn register_timeout(cancellation: RootCancellationHandle) {
    if let Some(callbacks) = callbacks() {
        (callbacks.register_timeout)(cancellation);
    }
}

pub(in crate::native) fn record_outcome(outcome: NativeRunOutcome) {
    if let Some(callbacks) = callbacks() {
        (callbacks.record_outcome)(outcome);
    }
}

pub(in crate::native) fn record_panic(
    cause: NativePanicCause,
    source: NativeSourceAnchor,
    message: String,
) {
    if let Some(callbacks) = callbacks() {
        (callbacks.record_panic)(cause, source, message);
    }
}

pub(in crate::native) fn record_returned_error() {
    if let Some(callbacks) = callbacks() {
        (callbacks.record_returned_error)();
    }
}

pub(in crate::native) fn record_cleanup_failure(count: usize) {
    if let Some(callbacks) = callbacks() {
        (callbacks.record_cleanup_failure)(count);
    }
}

pub(in crate::native) fn finish() -> io::Result<()> {
    callbacks().map_or(Ok(()), |callbacks| (callbacks.finish)())
}

fn callbacks() -> Option<NativeHostCallbacks> {
    CALLBACKS.get()
}
