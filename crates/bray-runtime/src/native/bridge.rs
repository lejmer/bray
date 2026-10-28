use std::marker::PhantomData;
use std::rc::Rc;

use bray_runtime_abi::{NativeRunOutcome, NativeRunState, NativeRuntimeStatus};

type Continuation = extern "C" fn(*const u8, usize, u32, &mut NativeRunOutcome) -> u32;

#[expect(
    unsafe_code,
    reason = "the native scope owns the exact-thread acquisition and callback provider"
)]
unsafe extern "C" {
    pub(crate) safe fn bray_runtime_entry_context_registration(
        callbacks: &Callbacks,
        previous: &mut Callbacks,
    ) -> u32;
    safe fn bray_runtime_thread_attachment_enter() -> u32;
    safe fn bray_runtime_thread_attachment_leave() -> u32;
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Callbacks {
    pub(crate) context: usize,
    pub(crate) report: usize,
}

impl Callbacks {
    fn selected() -> Self {
        Self {
            context: invoke as *const u8 as usize,
            report: observe as *const u8 as usize,
        }
    }
}

extern "C" fn observe(
    kind: u32,
    primary: &bray_runtime_abi::NativePanicPrimary,
    identity: &bray_runtime_abi::NativeTypeIdentity,
) -> u32 {
    if !super::host::active() {
        return NativeRuntimeStatus::NOT_INITIALIZED.code();
    }

    match kind {
        0 => super::host::record_report(primary, None),
        2 => super::host::record_report(primary, Some(*identity)),
        1 => super::host::record_returned_error(),
        _ => panic!("unknown host report observation kind {kind}"),
    }
    .code()
}

/// Registration cannot leave this thread or outlive its enclosing native provider call.
pub(crate) struct Scope {
    previous: Option<Callbacks>,
    thread: PhantomData<Rc<()>>,
}

impl Scope {
    pub(crate) fn enter() -> Result<Self, NativeRuntimeStatus> {
        if bray_runtime_thread_attachment_enter() != 0 {
            return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
        }

        let mut previous = Callbacks::default();
        let status = bray_runtime_entry_context_registration(&Callbacks::selected(), &mut previous);

        if status != 0 {
            if bray_runtime_thread_attachment_leave() != 0 {
                super::host::record_cleanup_failure(1);
            }

            return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
        }

        Ok(Self {
            previous: Some(previous),
            thread: PhantomData,
        })
    }

    pub(crate) fn finish(mut self) -> usize {
        self.retire()
    }

    fn retire(&mut self) -> usize {
        let Some(previous) = self.previous.take() else {
            return 0;
        };

        // The provider stays installed while the last acquisition runs cleanup.
        // Nested acquisition retirement leaves the row alive, so restore its caller afterward.
        let failures = usize::from(bray_runtime_thread_attachment_leave() != 0);

        if crate::bootstrap::current_runtime_thread().is_some() {
            let mut replaced = Callbacks::default();

            assert_eq!(
                bray_runtime_entry_context_registration(&previous, &mut replaced),
                0,
                "a live native bridge scope restores its retained attachment before returning"
            );

            assert_eq!(
                replaced,
                Callbacks::selected(),
                "native bridge registrations retire in their exact-thread nesting order"
            );
        } else {
            assert_eq!(
                previous,
                Callbacks::default(),
                "the last attachment acquisition cannot retire a retained outer provider"
            );
        }

        failures
    }
}

impl Drop for Scope {
    fn drop(&mut self) {
        let failures = self.retire();

        if failures != 0 {
            super::host::record_cleanup_failure(failures);
        }
    }
}

extern "C" fn invoke(
    continuation: Continuation,
    callback: *const u8,
    context: usize,
    main: u32,
    outcome: &mut NativeRunOutcome,
) -> u32 {
    // Rust callback shims contain unwind before returning through this continuation.
    super::state::with_independent_execution_context(|| {
        // Retirement can release a Rust payload after the body has completed.
        // Its report owner and suppression record must exist before user code runs.
        let Ok(mut admitted) = crate::outgoing::OutgoingRecords::admit(1) else {
            *outcome = super::outgoing::allocation_failure();

            if main != 0 {
                super::host::record_outcome(outcome);
            }

            return NativeRuntimeStatus::SUCCESS.code();
        };

        let cancellation = crate::CancellationContext::root();

        if main != 0 {
            super::host::register_timeout(crate::RootCancellationHandle::new(cancellation.clone()));
        }

        let mut status = crate::context::with_run_cancellation_context(cancellation, || {
            super::host::with_output(|| continuation(callback, context, main, outcome))
        });

        // Only Rust-owned retirement is caught here. The Bray continuation has
        // already returned, and its body result remains in the caller's slot.
        let retirement = match std::panic::catch_unwind(super::state::shutdown) {
            Ok(status) if status == NativeRuntimeStatus::NOT_INITIALIZED => {
                NativeRuntimeStatus::SUCCESS
            }
            Ok(status) => status,
            Err(payload) => report_retirement(outcome, payload, &mut admitted),
        };

        if !retirement.is_success()
            && (status == 0 || status == NativeRuntimeStatus::PANICKED.code())
        {
            status = retirement.code();
        }

        if main != 0 {
            super::host::record_outcome(outcome);
        }

        if status != 0 {
            super::host::record_cleanup_failure(1);
        }

        status
    })
}

fn report_retirement(
    outcome: &mut NativeRunOutcome,
    payload: Box<dyn std::any::Any + Send>,
    admitted: &mut crate::outgoing::OutgoingRecords,
) -> NativeRuntimeStatus {
    let mut panic = crate::RuntimePanic::from_payload(payload, admitted);

    if outcome.state() == NativeRunState::COMPLETED {
        // The caller still owns the completed result. Reporting takes only cleanup.
        let status = panic.consume(true);

        if !status.is_success() && status != NativeRuntimeStatus::PANICKED {
            return status;
        }
    } else {
        super::outgoing::append_panic(outcome, panic, admitted);
    }

    // EntryStatus distinguishes a represented panic from an unreported failure.
    NativeRuntimeStatus::PANICKED
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::{NativeRunOutcome, NativeRunState};

    #[test]
    fn last_scope_without_cleanup_does_not_leave_callbacks_in_a_reusable_row() {
        let _isolation = crate::native::test_runtime_isolation();
        let scope = super::Scope::enter().expect("fixture execution scope must admit entry");

        assert_eq!(scope.finish(), 0);

        let thread = crate::bootstrap::RuntimeThreadScope::enter()
            .expect("fixture execution scope must admit entry");

        let mut previous = super::Callbacks::default();

        assert_eq!(
            super::bray_runtime_entry_context_registration(
                &super::Callbacks::default(),
                &mut previous
            ),
            0
        );

        assert_eq!(previous, super::Callbacks::default());
        drop(thread);
    }

    extern "C" fn continuation(
        _: *const u8,
        context: usize,
        main: u32,
        outcome: &mut NativeRunOutcome,
    ) -> u32 {
        assert_eq!(context, 41);
        assert_eq!(main, 1);
        assert!(super::super::state::current_native_task().is_none());
        assert!(!crate::current_run_cancellation_requested());
        *outcome = NativeRunOutcome::new(NativeRunState::COMPLETED, 73);

        5
    }

    #[test]
    fn bridge_restores_context_and_keeps_body_result_separate_from_terminal_status() {
        let _isolation = super::super::state::test_runtime_isolation();

        let task = bray_runtime_abi::NativeTaskHandle::new(9)
            .expect("fixture native task handle must be valid");

        let cancellation = crate::CancellationContext::root();

        assert!(cancellation.request());

        crate::context::with_run_cancellation_context(cancellation, || {
            super::super::state::with_current_task(task, || {
                let mut outcome = NativeRunOutcome::new(NativeRunState::RUNTIME_FAILURE, 4);
                let status = super::invoke(continuation, std::ptr::null(), 41, 1, &mut outcome);

                assert_eq!(status, 5);
                assert_eq!(outcome.state(), NativeRunState::COMPLETED);
                assert_eq!(outcome.payload(), 73);
                assert_eq!(super::super::state::current_native_task(), Some(task));
                assert!(crate::current_run_cancellation_requested());
            });
        });
    }
    thread_local! {
        static RELEASES: std::cell::RefCell<Vec<usize>> = const { std::cell::RefCell::new(Vec::new()) };
    }

    struct Release(usize);

    impl Drop for Release {
        fn drop(&mut self) {
            RELEASES.with_borrow_mut(|released| released.push(self.0));
        }
    }

    #[test]
    fn retirement_panic_follows_body_without_admitting_after_the_callback() {
        let mut body_backing = crate::outgoing::OutgoingRecords::admit(1)
            .expect("fixture outgoing records must admit the supplied values");

        let body = crate::RuntimePanic::new(Release(1), &mut body_backing);
        let mut outcome = NativeRunOutcome::panicked(body.into_native());

        let mut retirement = crate::outgoing::OutgoingRecords::admit(1)
            .expect("fixture outgoing records must admit the supplied values");

        let _failure = crate::outgoing::tests::reject_admission();

        assert_eq!(
            super::report_retirement(&mut outcome, Box::new(Release(2)), &mut retirement),
            bray_runtime_abi::NativeRuntimeStatus::PANICKED
        );

        assert_eq!(retirement.len(), 0);
        assert_eq!(outcome.state(), NativeRunState::PANICKED);
        assert!(RELEASES.with_borrow(Vec::is_empty));

        let mut report = outcome.take_report();

        assert_eq!(report.outgoing_count(), 1);
        assert!(report.consume(false).is_success());
        assert!(report.consume(false).is_success());
        assert_eq!(RELEASES.with_borrow_mut(std::mem::take), [1, 2]);
    }

    #[test]
    fn retirement_reports_without_taking_the_completed_result() {
        let completed = Box::new(Release(2));
        let destination = std::ptr::from_ref(completed.as_ref()).addr();
        let mut outcome = NativeRunOutcome::new(NativeRunState::COMPLETED, destination);

        let mut retirement = crate::outgoing::OutgoingRecords::admit(1)
            .expect("fixture outgoing records must admit the supplied values");

        let _failure = crate::outgoing::tests::reject_admission();

        assert_eq!(
            super::report_retirement(&mut outcome, Box::new(Release(1)), &mut retirement),
            bray_runtime_abi::NativeRuntimeStatus::PANICKED
        );

        assert_eq!(retirement.len(), 0);
        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_eq!(outcome.payload(), destination);
        assert_eq!(RELEASES.with_borrow_mut(std::mem::take), [1]);

        drop(outcome);
        drop(completed);
        assert_eq!(RELEASES.with_borrow_mut(std::mem::take), [2]);
    }

    #[test]
    fn transferred_retirement_report_uses_the_admitted_suppression_record() {
        extern "C" fn release(id: usize, _: usize, _: &mut NativeRunOutcome) {
            RELEASES.with_borrow_mut(|released| released.push(id));
        }

        let mut body_backing = crate::outgoing::OutgoingRecords::admit(1)
            .expect("fixture outgoing records must admit the supplied values");

        let body = crate::RuntimePanic::new(Release(1), &mut body_backing);
        let mut outcome = NativeRunOutcome::panicked(body.into_native());

        let cleanup = crate::RuntimePanic::from_native(crate::frame::native_report(
            bray_runtime_abi::NativePanicPrimary::new(
                bray_runtime_abi::NativePanicCause::MESSAGE,
                bray_runtime_abi::NativeSourceAnchor::unavailable(),
                bray_runtime_abi::NativePanicMessage::new(2, 0, None, Some(release)),
            ),
        ));

        let mut retirement = crate::outgoing::OutgoingRecords::admit(1)
            .expect("fixture outgoing records must admit the supplied values");

        let _failure = crate::outgoing::tests::reject_admission();

        assert_eq!(
            super::report_retirement(&mut outcome, Box::new(cleanup), &mut retirement),
            bray_runtime_abi::NativeRuntimeStatus::PANICKED
        );

        assert_eq!(retirement.len(), 0);
        assert!(outcome.take_report().consume(false).is_success());
        assert_eq!(RELEASES.with_borrow_mut(std::mem::take), [1, 2]);
    }

    #[test]
    fn missing_retirement_backing_rejects_before_continuation() {
        extern "C" fn rejected(_: *const u8, _: usize, _: u32, _: &mut NativeRunOutcome) -> u32 {
            panic!("entry must admit terminal backing before invoking its continuation");
        }

        let mut outcome = NativeRunOutcome::new(NativeRunState::RUNTIME_FAILURE, 0);
        let _failure = crate::outgoing::tests::reject_admission();

        assert_eq!(
            super::invoke(rejected, std::ptr::null(), 0, 0, &mut outcome),
            0
        );

        assert_eq!(outcome.state(), NativeRunState::PANICKED);

        let mut report = outcome.take_report();

        assert!(report.consume(false).is_success());
    }
    #[cfg(feature = "test-output")]
    #[test]
    fn completed_body_reports_the_exact_retirement_message_once() {
        thread_local! {
            static MESSAGES: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
        }

        let _isolation = super::super::state::test_runtime_isolation();

        let callbacks = super::super::host::NativeHostCallbacks::new(
            || true,
            || None,
            |_| {},
            |_| {},
            |primary, identity| {
                assert!(identity.is_none());

                assert_eq!(
                    primary.cause(),
                    bray_runtime_abi::NativePanicCause::RUNTIME_PANIC
                );

                let mut message = vec![0; primary.message().len()];

                assert!(primary.message().copy_to(0, &mut message).is_success());

                MESSAGES.with_borrow_mut(|messages| {
                    messages
                        .push(String::from_utf8(message).expect("fixture text must be valid UTF-8"))
                });

                bray_runtime_abi::NativeRuntimeStatus::SUCCESS
            },
            || panic!("retirement panic is not a returned error"),
            |_| {},
            || Ok(()),
        );

        assert!(super::super::host::register_host_callbacks(callbacks).is_success());

        let mut outcome = NativeRunOutcome::new(NativeRunState::COMPLETED, 73);

        let mut retirement = crate::outgoing::OutgoingRecords::admit(1)
            .expect("fixture outgoing records must admit the supplied values");

        let _failure = crate::outgoing::tests::reject_admission();

        assert_eq!(
            super::report_retirement(&mut outcome, Box::new("retirement detail"), &mut retirement),
            bray_runtime_abi::NativeRuntimeStatus::PANICKED
        );

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_eq!(outcome.payload(), 73);

        assert_eq!(
            MESSAGES.with_borrow_mut(std::mem::take),
            ["retirement detail"]
        );

        assert!(super::super::host::finish().is_ok());
    }
}
