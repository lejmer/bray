use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{NativeBrayCallOutcome, NativeCleanupExecution, NativeValueCleanup};

use crate::incident::OwnedCleanupIncident;

pub(super) fn run(cleanup: NativeValueCleanup, value: usize) -> Vec<OwnedCleanupIncident> {
    if !cleanup.execution().completes_synchronously() {
        return vec![OwnedCleanupIncident::runtime_failure()];
    }

    let mut incidents = Vec::new();
    let _ = start(cleanup, value, |incident| incidents.push(incident));

    incidents
}

pub(in crate::native) fn start(
    cleanup: NativeValueCleanup,
    value: usize,
    mut record: impl FnMut(OwnedCleanupIncident),
) -> Option<bray_runtime_abi::NativeInactiveFrame> {
    if cleanup.execution() == NativeCleanupExecution::NONE {
        return None;
    }

    if !cleanup.execution().is_known() {
        record(OwnedCleanupIncident::runtime_failure());

        return None;
    }

    let mut frame = super::inactive_frame_output();
    let mut outcome = NativeBrayCallOutcome::completed();

    let result = catch_unwind(AssertUnwindSafe(|| {
        (cleanup.start())(value, &mut frame, &mut outcome)
    }));

    if let Some(incident) = OwnedCleanupIncident::boundary(outcome, cleanup.panics()) {
        record(incident);
    }

    match result {
        Err(payload) => record(OwnedCleanupIncident::host(payload)),
        Ok(status) if !status.is_success() => record(OwnedCleanupIncident::runtime_failure()),
        Ok(_) if outcome.is_completed() => return Some(frame),
        Ok(_) => {}
    }

    None
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{
        NativeBrayCallOutcome, NativeCleanupExecution, NativeInactiveFrame, NativeRuntimeStatus,
        NativeValueCleanup,
    };

    static PANIC_RELEASES: AtomicUsize = AtomicUsize::new(0);
    static SYNCHRONOUS_CALLS: AtomicUsize = AtomicUsize::new(0);

    extern "C-unwind" fn unexpected_panic(_: usize) -> NativeRuntimeStatus {
        panic!("cleanup must not report a panic");
    }

    #[test]
    fn synchronous_value_cleanup_completes_without_a_runtime_or_frame() {
        extern "C-unwind" fn complete(
            value: usize,
            _: &mut NativeInactiveFrame,
            _: &mut NativeBrayCallOutcome,
        ) -> NativeRuntimeStatus {
            SYNCHRONOUS_CALLS.fetch_add(value, Ordering::Relaxed);

            NativeRuntimeStatus::SUCCESS
        }

        SYNCHRONOUS_CALLS.store(0, Ordering::Relaxed);

        let cleanup = NativeValueCleanup::new(
            NativeCleanupExecution::SYNCHRONOUS,
            None,
            complete,
            crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic),
        );

        assert!(super::run(cleanup, 3).is_empty());
        assert_eq!(SYNCHRONOUS_CALLS.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn synchronous_driver_rejects_async_cleanup_before_start() {
        extern "C-unwind" fn rejected(
            _: usize,
            _: &mut NativeInactiveFrame,
            _: &mut NativeBrayCallOutcome,
        ) -> NativeRuntimeStatus {
            panic!("asynchronous callback must not run");
        }

        let cleanup = NativeValueCleanup::new(
            NativeCleanupExecution::ASYNCHRONOUS,
            None,
            rejected,
            crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic),
        );

        assert_eq!(super::run(cleanup, 7).len(), 1);
    }

    #[test]
    fn abnormal_start_retains_its_panic_without_reading_the_inactive_frame() {
        extern "C-unwind" fn fail(
            _: usize,
            _: &mut NativeInactiveFrame,
            outcome: &mut NativeBrayCallOutcome,
        ) -> NativeRuntimeStatus {
            *outcome = NativeBrayCallOutcome::panicked(8).unwrap();

            NativeRuntimeStatus::SUCCESS
        }

        extern "C-unwind" fn release(payload: usize) -> NativeRuntimeStatus {
            PANIC_RELEASES.fetch_add(payload, Ordering::Relaxed);

            NativeRuntimeStatus::SUCCESS
        }

        PANIC_RELEASES.store(0, Ordering::Relaxed);

        let cleanup = NativeValueCleanup::new(
            NativeCleanupExecution::ASYNCHRONOUS,
            None,
            fail,
            crate::test_support::panic_callbacks(unexpected_panic, release),
        );

        let mut incidents = Vec::new();
        assert!(super::start(cleanup, 7, |incident| incidents.push(incident)).is_none());

        assert_eq!(incidents.len(), 1);
        assert_eq!(PANIC_RELEASES.load(Ordering::Relaxed), 0);
        drop(incidents);
        assert_eq!(PANIC_RELEASES.load(Ordering::Relaxed), 8);
    }
}
