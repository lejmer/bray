use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{NativeBrayCallOutcome, NativeCleanupExecution, NativeValueCleanup};

use crate::incident::OwnedCleanupIncident;

pub(super) fn run(cleanup: NativeValueCleanup, value: usize) -> Vec<OwnedCleanupIncident> {
    if cleanup.execution() == NativeCleanupExecution::NONE {
        return Vec::new();
    }

    if !cleanup.execution().is_known() {
        return vec![OwnedCleanupIncident::runtime_failure()];
    }

    let mut frame = super::inactive_frame_output();
    let mut outcome = NativeBrayCallOutcome::completed();

    let result = catch_unwind(AssertUnwindSafe(|| {
        (cleanup.start())(value, &mut frame, &mut outcome)
    }));

    let mut incidents = OwnedCleanupIncident::boundary(outcome, cleanup.panics())
        .into_iter()
        .collect::<Vec<_>>();

    match result {
        Err(payload) => incidents.push(OwnedCleanupIncident::panic(payload)),
        Ok(status) if !status.is_success() => {
            incidents.push(OwnedCleanupIncident::runtime_failure())
        }
        Ok(_)
            if outcome.is_completed()
                && cleanup.execution() == NativeCleanupExecution::ASYNCHRONOUS =>
        {
            incidents.extend(
                super::state::with_runtime(|runtime| {
                    runtime.with_cleanup_driving(|| {
                        super::static_finalizer::run_cleanup_frame(frame, cleanup.panics(), |_| {
                            Vec::new()
                        })
                    })
                })
                .unwrap_or_else(|_| vec![OwnedCleanupIncident::runtime_failure()]),
            );
        }
        Ok(_) => {}
    }

    incidents
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{
        NativeBrayCallOutcome, NativeCleanupExecution, NativeFrameAffinity, NativeFrameExit,
        NativeFrameProgress, NativeFrameProgressKind, NativeFrameState, NativeInactiveFrame,
        NativeLaneRequirements, NativeProtectedFrame, NativeRuntimeStatus, NativeValueCleanup,
    };

    static RESUMES: AtomicUsize = AtomicUsize::new(0);
    static DESTROYS: AtomicUsize = AtomicUsize::new(0);
    static PANIC_RELEASES: AtomicUsize = AtomicUsize::new(0);
    static SYNCHRONOUS_CALLS: AtomicUsize = AtomicUsize::new(0);

    extern "C-unwind" fn start(
        value: usize,
        frame: &mut NativeInactiveFrame,
        _: &mut NativeBrayCallOutcome,
    ) -> NativeRuntimeStatus {
        *frame = NativeInactiveFrame::new(value, move_frame);

        NativeRuntimeStatus::SUCCESS
    }

    extern "C" fn move_frame(
        context: usize,
        _: bray_runtime_abi::NativeFrameEntry,
    ) -> NativeProtectedFrame {
        NativeProtectedFrame::new(
            context,
            [83; 32],
            2,
            8,
            8,
            0,
            1,
            state,
            resume,
            resume,
            ignore,
            resolve,
            move_completion,
            destroy,
        )
    }

    extern "C" fn state(_: usize, _: u32) -> NativeFrameState {
        NativeFrameState::new(
            NativeFrameAffinity::ORIGIN_THREAD,
            NativeLaneRequirements::NONE,
        )
    }

    extern "C-unwind" fn resume(context: usize) -> NativeFrameProgress {
        assert_eq!(context, 7);

        let kind = if RESUMES.fetch_add(1, Ordering::Relaxed) == 0 {
            NativeFrameProgressKind::YIELDED
        } else {
            NativeFrameProgressKind::COMPLETED
        };

        NativeFrameProgress::new(kind, 1, 0)
    }

    extern "C-unwind" fn ignore(_: usize) {}
    extern "C-unwind" fn resolve(_: usize, _: NativeFrameExit) {}
    extern "C-unwind" fn move_completion(_: usize, _: usize) {}
    extern "C-unwind" fn destroy(_: usize) {
        DESTROYS.fetch_add(1, Ordering::Relaxed);
    }
    extern "C-unwind" fn unexpected_panic(_: usize) -> NativeRuntimeStatus {
        panic!("cleanup must not panic")
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
            complete,
            crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic),
        );

        assert!(super::run(cleanup, 3).is_empty());
        assert_eq!(SYNCHRONOUS_CALLS.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn asynchronous_value_cleanup_drives_suspension_before_releasing_the_frame() {
        RESUMES.store(0, Ordering::Relaxed);
        DESTROYS.store(0, Ordering::Relaxed);

        let cleanup = NativeValueCleanup::new(
            NativeCleanupExecution::ASYNCHRONOUS,
            start,
            crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic),
        );

        let (incidents, runtime_incidents) =
            crate::native::with_static_cleanup_runtime(|| super::run(cleanup, 7));

        assert!(incidents.is_empty());
        assert!(runtime_incidents.is_empty());
        assert_eq!(RESUMES.load(Ordering::Relaxed), 2);
        assert_eq!(DESTROYS.load(Ordering::Relaxed), 1);
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
            fail,
            crate::test_support::panic_callbacks(unexpected_panic, release),
        );

        let incidents = super::run(cleanup, 7);

        assert_eq!(incidents.len(), 1);
        assert_eq!(PANIC_RELEASES.load(Ordering::Relaxed), 0);
        drop(incidents);
        assert_eq!(PANIC_RELEASES.load(Ordering::Relaxed), 8);
    }
}
