use std::cell::RefCell;
use std::io;

use bray_runtime_abi::{
    NativePanicCause, NativeRunOutcome, NativeRunState, NativeRuntimeStatus, NativeSourceAnchor,
};

use crate::RootCancellationHandle;
use crate::context::{TaskOutput, with_task_output};

thread_local! {
    static CALLBACKS: RefCell<Option<Selection>> = const { RefCell::new(None) };
}

pub(in crate::native) fn initialize_thread_context() {
    CALLBACKS.with(|_| {});
}

struct Selection {
    callbacks: NativeHostCallbacks,
    bridge: Option<super::super::bridge::Scope>,
}

/// Test-host integration selected explicitly by the native test adapter.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct NativeHostCallbacks {
    active: fn() -> bool,
    output: fn() -> TaskOutput,
    register_timeout: fn(RootCancellationHandle),
    record_outcome: fn(&NativeRunOutcome),
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
        record_outcome: fn(&NativeRunOutcome),
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

/// Selects the executable test host until its structured shutdown finishes.
/// The test adapter and callback provider belong to that executable product.
#[doc(hidden)]
pub fn register_host_callbacks(callbacks: NativeHostCallbacks) -> NativeRuntimeStatus {
    super::super::state::initialize_thread_context();

    CALLBACKS.with_borrow_mut(|selected| {
        assert!(
            selected.is_none(),
            "one executable test session owns its host selection"
        );

        *selected = Some(Selection {
            callbacks,
            bridge: None,
        });
    });

    match super::super::bridge::Scope::enter() {
        Ok(bridge) => {
            CALLBACKS.with_borrow_mut(|selected| {
                selected
                    .as_mut()
                    .expect("test selection remains installed during admission")
                    .bridge = Some(bridge);
            });

            NativeRuntimeStatus::SUCCESS
        }
        Err(status) => {
            record_outcome(&NativeRunOutcome::new(
                NativeRunState::RUNTIME_FAILURE,
                status.code() as usize,
            ));

            // If result publication also fails, the test runner observes the missing result.
            let _ = finish();

            status
        }
    }
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

pub(in crate::native) fn record_outcome(outcome: &NativeRunOutcome) {
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
    let bridge = CALLBACKS.with_borrow_mut(|selected| {
        selected
            .as_mut()
            .and_then(|selected| selected.bridge.take())
    });

    // Retirement can report cleanup failure through the still-selected host.
    drop(bridge);

    let selected = CALLBACKS.take();

    selected.map_or(Ok(()), |selected| (selected.callbacks.finish)())
}

fn callbacks() -> Option<NativeHostCallbacks> {
    CALLBACKS.with_borrow(|selected| selected.as_ref().map(|selected| selected.callbacks))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io;

    use bray_runtime_abi::{
        NativePanicCause, NativeRunOutcome, NativeRunState, NativeSourceAnchor,
    };

    use crate::RootCancellationHandle;
    use crate::context::TaskOutput;

    #[derive(Default)]
    struct Observations {
        timeouts: usize,
        outcomes: usize,
        finished: bool,
        payload: usize,
        cleanup_failures: usize,
    }

    thread_local! {
        static OBSERVED: RefCell<Observations> = RefCell::new(Observations::default());
    }

    fn active() -> bool {
        OBSERVED.with_borrow(|observed| !observed.finished)
    }

    fn output() -> TaskOutput {
        Some(bray_platform::RunOutputContext::discarded())
    }

    fn register_timeout(cancellation: RootCancellationHandle) {
        OBSERVED.with_borrow_mut(|observed| observed.timeouts += 1);
        assert!(cancellation.request_timeout());
    }

    fn record_outcome(outcome: &NativeRunOutcome) {
        assert_eq!(outcome.state(), NativeRunState::COMPLETED);

        OBSERVED.with_borrow_mut(|observed| {
            observed.outcomes += 1;
            observed.payload = outcome.payload();
        });
    }

    fn record_panic(_: NativePanicCause, _: NativeSourceAnchor, _: String) {
        panic!("the test body must not panic");
    }

    fn record_returned_error() {
        panic!("the test body must not return an error");
    }

    fn record_cleanup_failure(count: usize) {
        OBSERVED.with_borrow_mut(|observed| observed.cleanup_failures += count);
    }

    fn finish() -> io::Result<()> {
        assert!(crate::bootstrap::current_runtime_thread().is_none());
        OBSERVED.with_borrow_mut(|observed| observed.finished = true);

        Ok(())
    }

    extern "C-unwind" fn observe_context(main: usize, outcome: &mut NativeRunOutcome) {
        assert_eq!(
            super::super::super::state::with_runtime(|_| ()),
            Err(bray_runtime_abi::NativeRuntimeStatus::NOT_INITIALIZED)
        );

        assert_eq!(crate::current_run_cancellation_requested(), main != 0);
        assert!(crate::context::current_task_output().is_some());
        *outcome = NativeRunOutcome::new(NativeRunState::COMPLETED, 73);
    }

    #[test]
    fn selected_test_host_scopes_production_entry_context_without_a_scheduler() {
        let _isolation = super::super::super::state::test_runtime_isolation();

        OBSERVED.set(Observations::default());

        let callbacks = super::NativeHostCallbacks::new(
            active,
            output,
            register_timeout,
            record_outcome,
            record_panic,
            record_returned_error,
            record_cleanup_failure,
            finish,
        );

        assert!(super::register_host_callbacks(callbacks).is_success());

        let foreign = super::super::super::entry::foreign(observe_context, 0);

        assert_eq!(foreign.state(), NativeRunState::COMPLETED);

        OBSERVED.with_borrow(|observed| {
            assert_eq!(observed.timeouts, 0);
            assert_eq!(observed.outcomes, 0);
        });

        let root = super::super::super::entry::root(observe_context, 1);

        assert_eq!(root.state(), NativeRunState::COMPLETED);
        assert_eq!(root.payload(), 73);
        assert!(!crate::current_run_cancellation_requested());
        assert!(crate::context::current_task_output().is_none());
        assert!(super::super::super::export::bray_runtime_structured_shutdown().is_success());
        assert!(!super::active());

        OBSERVED.with_borrow(|observed| {
            assert_eq!(observed.timeouts, 1);
            assert_eq!(observed.outcomes, 1);
            assert_eq!(observed.payload, 73);
            assert_eq!(observed.cleanup_failures, 0);
            assert!(observed.finished);
        });
    }
}
