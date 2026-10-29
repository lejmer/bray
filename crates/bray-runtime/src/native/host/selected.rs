use std::cell::RefCell;
use std::io;
use std::mem::ManuallyDrop;

use bray_runtime_abi::{NativePanicPrimary, NativeRunOutcome, NativeRunState, NativeRuntimeStatus};

use crate::RootCancellationHandle;
use crate::context::{TaskOutput, with_task_output};

// Context contents belong to explicit scopes and runtime retirement. The slot itself
// remains available to native TLS callbacks regardless of Rust destructor order.
thread_local! {
    static CALLBACKS: ManuallyDrop<RefCell<Option<Selection>>> = const { ManuallyDrop::new(RefCell::new(None)) };
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
    record_report: fn(
        &NativePanicPrimary,
        Option<bray_runtime_abi::NativeTypeIdentity>,
    ) -> NativeRuntimeStatus,
    record_returned_error: fn() -> NativeRuntimeStatus,
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
        record_report: fn(
            &NativePanicPrimary,
            Option<bray_runtime_abi::NativeTypeIdentity>,
        ) -> NativeRuntimeStatus,
        record_returned_error: fn() -> NativeRuntimeStatus,
        record_cleanup_failure: fn(usize),
        finish: fn() -> io::Result<()>,
    ) -> Self {
        Self {
            active,
            output,
            register_timeout,
            record_outcome,
            record_report,
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

    CALLBACKS.with(|cell| {
        let selected = &mut *cell.borrow_mut();

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
            CALLBACKS.with(|cell| {
                let selected = &mut *cell.borrow_mut();

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
    let output = callbacks().and_then(|callbacks| (callbacks.output)());

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

pub(in crate::native) fn record_report(
    primary: &NativePanicPrimary,
    identity: Option<bray_runtime_abi::NativeTypeIdentity>,
) -> NativeRuntimeStatus {
    callbacks().map_or(NativeRuntimeStatus::NOT_INITIALIZED, |callbacks| {
        (callbacks.record_report)(primary, identity)
    })
}

pub(in crate::native) fn record_returned_error() -> NativeRuntimeStatus {
    callbacks().map_or(NativeRuntimeStatus::NOT_INITIALIZED, |callbacks| {
        (callbacks.record_returned_error)()
    })
}

pub(in crate::native) fn record_cleanup_failure(count: usize) {
    if let Some(callbacks) = callbacks() {
        (callbacks.record_cleanup_failure)(count);
    }
}

pub(in crate::native) fn finish() -> io::Result<()> {
    let bridge = CALLBACKS.with(|cell| {
        let selected = &mut *cell.borrow_mut();

        selected
            .as_mut()
            .and_then(|selected| selected.bridge.take())
    });

    // Retirement can report cleanup failure through the still-selected host.
    drop(bridge);

    let selected = CALLBACKS.with(|cell| cell.take());

    selected.map_or(Ok(()), |selected| (selected.callbacks.finish)())
}

fn callbacks() -> Option<NativeHostCallbacks> {
    CALLBACKS.with(|cell| {
        let selected = &*cell.borrow();

        selected.as_ref().map(|selected| selected.callbacks)
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::io;

    use bray_runtime_abi::{
        NativePanicCause, NativePanicPrimary, NativeRunOutcome, NativeRunState, NativeRuntimeStatus,
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
        reports: Vec<String>,
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

    fn record_report(
        _: &NativePanicPrimary,
        _: Option<bray_runtime_abi::NativeTypeIdentity>,
    ) -> NativeRuntimeStatus {
        panic!("the test body must not panic");
    }

    fn record_returned_error() -> NativeRuntimeStatus {
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
            record_report,
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

    #[test]
    fn final_attachment_cleanup_retains_the_selected_report_observer() {
        fn capture(
            primary: &bray_runtime_abi::NativePanicPrimary,
            identity: Option<bray_runtime_abi::NativeTypeIdentity>,
        ) -> NativeRuntimeStatus {
            assert!(identity.is_none());
            assert_eq!(primary.cause(), NativePanicCause::RUNTIME_PANIC);

            let mut bytes = vec![0; primary.message().len()];

            assert!(primary.message().copy_to(0, &mut bytes).is_success());

            OBSERVED.with_borrow_mut(|observed| {
                observed
                    .reports
                    .push(String::from_utf8(bytes).expect("fixture text must be valid UTF-8"))
            });

            NativeRuntimeStatus::SUCCESS
        }

        extern "C" fn cleanup(_: u64) -> u32 {
            assert!(crate::bootstrap::current_runtime_thread().is_none());

            let mut admitted = crate::outgoing::OutgoingRecords::admit(1)
                .expect("fixture outgoing records must admit the supplied values");

            let mut panic =
                crate::RuntimePanic::new("terminal cleanup \u{3b2}".repeat(4096), &mut admitted);

            assert!(panic.consume(true).is_success());

            0
        }

        let _isolation = super::super::super::state::test_runtime_isolation();

        OBSERVED.set(Observations::default());

        let callbacks = super::NativeHostCallbacks::new(
            active,
            output,
            register_timeout,
            record_outcome,
            capture,
            record_returned_error,
            record_cleanup_failure,
            finish,
        );

        assert!(super::register_host_callbacks(callbacks).is_success());
        assert!(crate::bootstrap::register_thread_cleanup(cleanup));
        super::finish().expect("fixture operation must finish successfully");

        OBSERVED.with_borrow(|observed| {
            assert_eq!(observed.reports, ["terminal cleanup \u{3b2}".repeat(4096)]);
            assert!(observed.finished);
        });
    }
    #[test]
    fn typed_cleanup_observation_borrows_its_own_type_and_source() {
        fn capture(
            primary: &bray_runtime_abi::NativePanicPrimary,
            identity: Option<bray_runtime_abi::NativeTypeIdentity>,
        ) -> NativeRuntimeStatus {
            assert_eq!(
                identity
                    .expect("fixture `identity` call must succeed")
                    .bytes(),
                [29; 32]
            );

            assert_eq!(
                primary.source(),
                bray_runtime_abi::NativeSourceAnchor::new([7; 32], 2, 3, 9, 11)
            );

            assert_eq!(primary.cause(), NativePanicCause::ENTRY_FAILURE);
            OBSERVED.with_borrow_mut(|observed| observed.outcomes += 1);

            NativeRuntimeStatus::SUCCESS
        }

        extern "C" fn report(
            _: usize,
            _: &mut NativeRunOutcome,
        ) -> bray_runtime_abi::NativeRuntimeStatus {
            bray_runtime_abi::NativeRuntimeStatus::SUCCESS
        }

        extern "C" fn destroy(
            _: &mut bray_runtime_abi::NativeCleanupIncident,
            _: &mut NativeRunOutcome,
            _: &mut NativeRunOutcome,
        ) {
        }

        let _isolation = super::super::super::state::test_runtime_isolation();

        OBSERVED.set(Observations::default());

        let callbacks = super::NativeHostCallbacks::new(
            active,
            output,
            register_timeout,
            record_outcome,
            capture,
            record_returned_error,
            record_cleanup_failure,
            finish,
        );

        assert!(super::register_host_callbacks(callbacks).is_success());

        let incident = bray_runtime_abi::NativeCleanupIncident::new(
            73,
            bray_runtime_abi::NativeTypeIdentity::new([29; 32]),
            bray_runtime_abi::NativeSourceAnchor::new([7; 32], 2, 3, 9, 11),
            report,
            destroy,
        );

        assert!(
            crate::incident::OwnedCleanupIncident::native(incident)
                .expect("fixture `crate::incident::OwnedCleanupIncident::native` call must succeed")
                .report()
                .is_success()
        );

        super::finish().expect("fixture operation must finish successfully");
        OBSERVED.with_borrow(|observed| assert_eq!(observed.outcomes, 1));
    }
    #[test]
    fn failed_capture_does_not_skip_typed_disposal_or_later_reports() {
        fn reject(
            _: &NativePanicPrimary,
            _: Option<bray_runtime_abi::NativeTypeIdentity>,
        ) -> NativeRuntimeStatus {
            NativeRuntimeStatus::RUNTIME_FAILURE
        }

        extern "C" fn report(_: usize, _: &mut NativeRunOutcome) -> NativeRuntimeStatus {
            OBSERVED.with_borrow_mut(|observed| observed.outcomes += 1);

            NativeRuntimeStatus::SUCCESS
        }

        extern "C" fn destroy(
            _: &mut bray_runtime_abi::NativeCleanupIncident,
            _: &mut NativeRunOutcome,
            _: &mut NativeRunOutcome,
        ) {
            OBSERVED.with_borrow_mut(|observed| observed.payload += 1);
        }

        extern "C" fn release(_: usize, _: usize, _: &mut NativeRunOutcome) {
            OBSERVED.with_borrow_mut(|observed| observed.payload += 1);
        }

        let _isolation = super::super::super::state::test_runtime_isolation();

        OBSERVED.set(Observations::default());

        let callbacks = super::NativeHostCallbacks::new(
            active,
            output,
            register_timeout,
            record_outcome,
            reject,
            record_returned_error,
            record_cleanup_failure,
            finish,
        );

        assert!(super::register_host_callbacks(callbacks).is_success());

        let _denied = crate::outgoing::tests::reject_admission();

        for payload in [73, 74] {
            let primary = NativePanicPrimary::new(
                NativePanicCause::MESSAGE,
                bray_runtime_abi::NativeSourceAnchor::unavailable(),
                bray_runtime_abi::NativePanicMessage::new(payload, 0, None, Some(release)),
            );

            let mut report_owner = bray_runtime_abi::NativePanicReport::new(
                primary,
                crate::report_provider::bray_runtime_report_consumer,
            );

            assert_eq!(
                report_owner.consume(true),
                NativeRuntimeStatus::RUNTIME_FAILURE
            );

            let incident = bray_runtime_abi::NativeCleanupIncident::new(
                payload,
                bray_runtime_abi::NativeTypeIdentity::new([29; 32]),
                bray_runtime_abi::NativeSourceAnchor::unavailable(),
                report,
                destroy,
            );

            assert_eq!(
                crate::incident::OwnedCleanupIncident::native(incident)
                    .expect(
                        "fixture `crate::incident::OwnedCleanupIncident::native` call must succeed"
                    )
                    .report(),
                NativeRuntimeStatus::RUNTIME_FAILURE
            );
        }

        super::finish().expect("fixture operation must finish successfully");

        OBSERVED.with_borrow(|observed| {
            assert_eq!(observed.outcomes, 2);
            assert_eq!(observed.payload, 4);
            assert!(observed.finished);
        });
    }
}
