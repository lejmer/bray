use std::cell::Cell;

use bray_platform::{RuntimeThread, RuntimeThreadId};
use bray_runtime_abi::NativeRuntimeStatus;

#[expect(
    unsafe_code,
    reason = "the resident Bray provider supplies primitive attachment ownership"
)]
unsafe extern "C" {
    safe fn bray_runtime_thread_attachment_enter() -> u32;
    safe fn bray_runtime_thread_attachment_leave() -> u32;
    safe fn bray_runtime_thread_attachment_mark_main() -> u32;
    safe fn bray_runtime_thread_attachment_cleanup(callback: extern "C" fn(u64) -> u32) -> u32;
    safe fn bray_runtime_current_native_thread_identity() -> u64;
    #[cfg(test)]
    safe fn bray_runtime_main_native_thread_identity() -> u64;
}

pub(crate) fn current_runtime_thread() -> Option<RuntimeThread> {
    RuntimeThread::from_native_identity(bray_runtime_current_native_thread_identity()).or_else(
        || {
            CLEANUP_THREAD
                .get()
                .and_then(|thread| RuntimeThread::from_native_identity(thread.raw()))
        },
    )
}

#[cfg(test)]
pub(crate) fn main_runtime_thread() -> Option<RuntimeThread> {
    RuntimeThread::from_native_identity(bray_runtime_main_native_thread_identity())
}

pub(crate) fn mark_current_runtime_thread_as_main() -> bool {
    bray_runtime_thread_attachment_mark_main() == 0
}

pub(crate) fn register_thread_cleanup(callback: extern "C" fn(u64) -> u32) -> bool {
    bray_runtime_thread_attachment_cleanup(callback) == 0
}

thread_local! {
    static CLEANUP_THREAD: Cell<Option<RuntimeThreadId>> = const { Cell::new(None) };
}

pub(crate) fn in_thread_cleanup() -> bool {
    CLEANUP_THREAD.get().is_some()
}

/// Borrows the identity supplied by Bray while it owns this cleanup callback.
/// The target TLS slot is already clear. This view never acquires or retires an epoch.
pub(crate) fn with_thread_cleanup<T>(identity: u64, callback: impl FnOnce() -> T) -> T {
    let thread = RuntimeThread::from_native_identity(identity)
        .expect("Bray invokes attachment cleanup with its live nonzero identity");

    let _scope = CleanupThreadScope(CLEANUP_THREAD.replace(Some(thread.id())));

    callback()
}

struct CleanupThreadScope(Option<RuntimeThreadId>);

impl Drop for CleanupThreadScope {
    fn drop(&mut self) {
        CLEANUP_THREAD.set(self.0);
    }
}

pub(crate) struct RuntimeThreadScope {
    runtime: RuntimeThread,
    active: bool,
}

pub(crate) enum RuntimeThreadEntry {
    Current(RuntimeThread),
    Attached(RuntimeThreadScope),
}

impl RuntimeThreadEntry {
    pub(crate) const fn runtime(&self) -> &RuntimeThread {
        match self {
            Self::Current(runtime) => runtime,
            Self::Attached(scope) => scope.runtime(),
        }
    }

    pub(crate) fn finish(self) -> usize {
        match self {
            Self::Current(_) => 0,
            Self::Attached(scope) => scope.finish(),
        }
    }
}

impl RuntimeThreadScope {
    pub(crate) fn enter() -> Result<Self, NativeRuntimeStatus> {
        if current_runtime_thread().is_some() {
            return Err(NativeRuntimeStatus::ALREADY_INITIALIZED);
        }

        let status = bray_runtime_thread_attachment_enter();

        if status != 0 {
            return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
        }

        let runtime = current_runtime_thread()
            .expect("successful Bray attachment publishes a nonzero exact-thread identity");

        Ok(Self {
            runtime,
            active: true,
        })
    }

    pub(crate) fn enter_or_reuse() -> Result<RuntimeThreadEntry, NativeRuntimeStatus> {
        match current_runtime_thread() {
            Some(runtime) => Ok(RuntimeThreadEntry::Current(runtime)),
            None => Self::enter().map(RuntimeThreadEntry::Attached),
        }
    }

    pub(crate) const fn runtime(&self) -> &RuntimeThread {
        &self.runtime
    }

    pub(crate) fn finish(mut self) -> usize {
        self.finish_attachment()
    }

    fn finish_attachment(&mut self) -> usize {
        if !self.active {
            return 0;
        }

        let status = bray_runtime_thread_attachment_leave();

        if status <= 1 {
            self.active = false;
        }

        usize::from(status != 0)
    }
}

impl Drop for RuntimeThreadScope {
    fn drop(&mut self) {
        let _ = self.finish_attachment();
    }
}

#[cfg(test)]
mod tests {
    use std::panic::catch_unwind;

    use super::{
        RuntimeThreadEntry, RuntimeThreadScope, bray_runtime_current_native_thread_identity,
        current_runtime_thread, register_thread_cleanup, with_thread_cleanup,
    };

    #[test]
    fn retiring_attachment_rejects_source_entry_before_invoking_its_callback() {
        let _isolation = crate::native::test_runtime_isolation();

        use crate::native::bray_runtime_foreign_callback_execution;

        extern "C" fn rejected(
            _: usize,
            _: &mut bray_runtime_abi::NativeRunOutcome,
            _: &mut bray_runtime_abi::NativeReportRecords,
        ) {
            panic!("retirement must reject source entry before source access");
        }

        extern "C" fn cleanup(identity: u64) -> u32 {
            with_thread_cleanup(identity, || {
                assert!(super::in_thread_cleanup());
                assert_eq!(bray_runtime_current_native_thread_identity(), 0);

                let mut outcome = bray_runtime_abi::NativeRunOutcome::new(
                    bray_runtime_abi::NativeRunState::RUNTIME_FAILURE,
                    17,
                );

                let status = bray_runtime_foreign_callback_execution(rejected, 0, &mut outcome);

                assert_ne!(status, 0);

                assert_eq!(
                    outcome.state(),
                    bray_runtime_abi::NativeRunState::RUNTIME_FAILURE
                );

                assert_eq!(outcome.payload(), 17);
            });

            0
        }

        let attachment = RuntimeThreadScope::enter().unwrap();

        assert!(register_thread_cleanup(cleanup));
        assert_eq!(attachment.finish(), 0);
        assert!(!super::in_thread_cleanup());
    }

    #[test]
    fn nested_callback_entries_borrow_the_existing_attachment() {
        let _isolation = crate::native::test_runtime_isolation();

        let attached = RuntimeThreadScope::enter_or_reuse().expect("foreign thread attaches");

        assert!(matches!(attached, RuntimeThreadEntry::Attached(_)));

        let identity = attached.runtime().id();
        let nested = RuntimeThreadScope::enter_or_reuse().expect("nested entry reuses attachment");

        assert!(matches!(nested, RuntimeThreadEntry::Current(_)));
        assert_eq!(nested.runtime().id(), identity);
        assert_eq!(nested.finish(), 0);
        assert_eq!(current_runtime_thread().unwrap().id(), identity);
        assert_eq!(attached.finish(), 0);
        assert!(current_runtime_thread().is_none());
    }

    #[test]
    fn cleanup_borrows_the_retiring_identity_and_restores_its_view_after_panic() {
        let _isolation = crate::native::test_runtime_isolation();

        extern "C" fn cleanup(identity: u64) -> u32 {
            assert_eq!(bray_runtime_current_native_thread_identity(), 0);

            let result = catch_unwind(|| {
                with_thread_cleanup(identity, || {
                    assert_eq!(current_runtime_thread().unwrap().id().raw(), identity);
                    assert!(RuntimeThreadScope::enter().is_err());
                    panic!("exercise callback observation restoration");
                });
            });

            assert!(result.is_err());
            assert!(current_runtime_thread().is_none());

            1
        }

        std::thread::spawn(|| {
            let scope = RuntimeThreadScope::enter().unwrap();

            assert!(register_thread_cleanup(cleanup));
            assert_eq!(scope.finish(), 1);
            assert!(current_runtime_thread().is_none());

            let next = RuntimeThreadScope::enter().unwrap();

            assert_eq!(next.finish(), 0);
        })
        .join()
        .unwrap();
    }
}
