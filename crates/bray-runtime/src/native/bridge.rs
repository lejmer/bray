use std::cell::RefCell;
use std::marker::PhantomData;
use std::mem::{ManuallyDrop, MaybeUninit};
use std::rc::Rc;

use bray_runtime_abi::{NativeRunOutcome, NativeRuntimeStatus};

use crate::bootstrap::{RuntimeThreadEntry, RuntimeThreadScope};

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
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Callbacks {
    pub(crate) context: usize,
    pub(crate) report: usize,
    pub(crate) control: usize,
}

impl Callbacks {
    fn selected() -> Self {
        Self {
            context: invoke as *const u8 as usize,
            report: observe as *const u8 as usize,
            control: control as *const u8 as usize,
        }
    }
}

extern "C" fn control(operation: u32) -> u32 {
    match operation {
        0 => u32::from(crate::current_run_cancellation_requested()),
        1 => {
            crate::context::enter_current_run_cleanup_shield();

            0
        }
        2 => {
            crate::context::leave_current_run_cleanup_shield();

            0
        }
        3 if !super::host::active() => 0,
        3 => {
            if super::host::finish().is_ok() {
                1
            } else {
                NativeRuntimeStatus::RUNTIME_FAILURE.code() + 1
            }
        }
        _ => panic!("unknown execution control operation {operation}"),
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

// One provider registration owns its original callbacks. Independent native
// and Source driver scopes retain that same exact-thread owner until the last
// scope ends, including when a driver outlives the call that created it.
struct Provider {
    thread: bray_platform::RuntimeThreadId,
    previous: Callbacks,
    scopes: usize,
}

thread_local! {
    static PROVIDER: ManuallyDrop<RefCell<Option<Provider>>> = const { ManuallyDrop::new(RefCell::new(None)) };
}

/// Retains the exact-thread provider through its attachment and nested cleanup.
pub(crate) struct Scope {
    attachment: Option<RuntimeThreadEntry>,
    _thread: PhantomData<Rc<()>>,
}

impl Scope {
    fn empty() -> Self {
        Self {
            attachment: None,
            _thread: PhantomData,
        }
    }

    pub(crate) fn enter() -> Result<Self, NativeRuntimeStatus> {
        let attachment = RuntimeThreadScope::enter_or_reuse()?;
        let thread = attachment.runtime().id();

        let retained = PROVIDER.with(|provider| {
            let mut provider = provider.borrow_mut();

            let Some(provider) = provider.as_mut() else {
                return false;
            };

            assert_eq!(
                provider.thread, thread,
                "the native provider belongs to this live exact-thread epoch"
            );

            provider.scopes = provider
                .scopes
                .checked_add(1)
                .expect("live provider scope count fits its native word");

            true
        });

        if !retained {
            let mut previous = Callbacks::default();

            let status =
                bray_runtime_entry_context_registration(&Callbacks::selected(), &mut previous);

            if status != 0 {
                if attachment.finish() != 0 {
                    super::host::record_cleanup_failure(1);
                }

                return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
            }

            PROVIDER.with(|provider| {
                let replaced = provider.borrow_mut().replace(Provider {
                    thread,
                    previous,
                    scopes: 1,
                });

                assert!(
                    replaced.is_none(),
                    "provider registration publishes once before callbacks can reenter"
                );
            });
        }

        Ok(Self {
            attachment: Some(attachment),
            _thread: PhantomData,
        })
    }

    pub(crate) fn finish(mut self) -> usize {
        self.retire()
    }

    fn retire(&mut self) -> usize {
        let Some(attachment) = self.attachment.take() else {
            return 0;
        };

        let thread = attachment.runtime().id();

        // No provider-table borrow crosses attachment cleanup. It can retire a
        // Source context that holds another scope for this same provider.
        let failures = attachment.finish();

        let retired = PROVIDER.with(|provider| {
            let mut provider = provider.borrow_mut();

            let current = provider
                .as_mut()
                .expect("every live scope retains its provider registration");

            assert_eq!(
                current.thread, thread,
                "retirement resolves the provider from its acquired epoch"
            );

            current.scopes = current
                .scopes
                .checked_sub(1)
                .expect("each scope releases its provider claim once");

            if current.scopes == 0 {
                provider.take()
            } else {
                None
            }
        });

        let Some(provider) = retired else {
            return failures;
        };

        if crate::bootstrap::current_runtime_thread().is_some() {
            let mut replaced = Callbacks::default();

            assert_eq!(
                bray_runtime_entry_context_registration(&provider.previous, &mut replaced),
                0,
                "the last provider scope restores its original callbacks"
            );

            assert_eq!(
                replaced,
                Callbacks::selected(),
                "the retained provider remains installed through every scope"
            );
        } else {
            assert_eq!(
                provider.previous,
                Callbacks::default(),
                "the last attachment cannot retire a retained outer provider"
            );
        }

        failures
    }
}

// Source owns aligned opaque storage for the complete Rust scope. The callbacks
// share its actual private type, initialize it once, and consume it on that thread.
#[expect(
    improper_ctypes_definitions,
    reason = "only this provider interprets its opaque scope storage"
)]
pub(in crate::native) extern "C" fn enter_provider(
    output: &mut MaybeUninit<Scope>,
    bytes: usize,
    align: usize,
) -> u32 {
    assert!(
        bytes >= size_of::<Scope>() && align >= align_of::<Scope>(),
        "Source provider storage satisfies the native scope layout"
    );

    if crate::bootstrap::current_runtime_thread().is_none() {
        return NativeRuntimeStatus::NOT_INITIALIZED.code();
    }

    match Scope::enter() {
        Ok(scope) => {
            output.write(scope);

            NativeRuntimeStatus::SUCCESS.code()
        }
        Err(status) => status.code(),
    }
}

#[expect(
    improper_ctypes_definitions,
    reason = "the successful entry initialized this same provider-owned scope type"
)]
pub(in crate::native) extern "C" fn finish_provider(scope: &mut Scope) {
    let acquired = std::mem::replace(scope, Scope::empty());
    let incidents = acquired.finish();

    assert_eq!(
        incidents, 0,
        "a Source-owned scope borrows its existing attachment through retirement"
    );
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
    // Source owns entry isolation, frame cleanup and execution retirement. Only
    // the native cancellation and output contexts belong to this continuation.
    crate::context::with_independent_execution_context(|| {
        let cancellation = crate::CancellationContext::root();

        if main != 0 {
            super::host::register_timeout(crate::RootCancellationHandle::new(cancellation.clone()));
        }

        let status = crate::context::with_run_cancellation_context(cancellation, || {
            super::host::with_output(|| continuation(callback, context, main, outcome))
        });

        if main != 0 {
            super::host::record_outcome(outcome);
        }

        if status != 0 {
            super::host::record_cleanup_failure(1);
        }

        status
    })
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

    #[test]
    fn nested_scopes_restore_callbacks_and_preserve_the_enclosing_attachment() {
        let _isolation = crate::native::test_runtime_isolation();

        let thread =
            crate::bootstrap::RuntimeThreadScope::enter().expect("fixture thread must attach");

        let identity = thread.runtime().id();
        let outer = super::Scope::enter().expect("outer bridge must enter");
        let nested = super::Scope::enter().expect("nested bridge must enter");

        assert_eq!(nested.finish(), 0);

        assert_eq!(
            crate::bootstrap::current_runtime_thread()
                .expect("outer attachment remains live")
                .id(),
            identity
        );

        assert_eq!(outer.finish(), 0);

        let mut previous = super::Callbacks::selected();

        assert_eq!(
            super::bray_runtime_entry_context_registration(
                &super::Callbacks::default(),
                &mut previous
            ),
            0
        );

        assert_eq!(previous, super::Callbacks::default());
        assert_eq!(thread.finish(), 0);
        assert!(crate::bootstrap::current_runtime_thread().is_none());
    }

    extern "C" fn continuation(
        _: *const u8,
        context: usize,
        main: u32,
        outcome: &mut NativeRunOutcome,
    ) -> u32 {
        assert_eq!(context, 41);
        assert_eq!(main, 1);
        assert!(crate::current_task_execution_context().is_none());
        assert!(!crate::current_run_cancellation_requested());
        *outcome = NativeRunOutcome::new(NativeRunState::COMPLETED, 73);

        5
    }

    #[test]
    fn bridge_restores_context_and_keeps_body_result_separate_from_terminal_status() {
        let _isolation = super::super::state::test_runtime_isolation();

        let cancellation = crate::CancellationContext::root();

        assert!(cancellation.request());

        crate::context::with_run_cancellation_context(cancellation, || {
            let mut outcome = NativeRunOutcome::new(NativeRunState::RUNTIME_FAILURE, 4);
            let status = super::invoke(continuation, std::ptr::null(), 41, 1, &mut outcome);

            assert_eq!(status, 5);
            assert_eq!(outcome.state(), NativeRunState::COMPLETED);
            assert_eq!(outcome.payload(), 73);
            assert!(crate::current_task_execution_context().is_none());
            assert!(crate::current_run_cancellation_requested());
        });
    }
}
