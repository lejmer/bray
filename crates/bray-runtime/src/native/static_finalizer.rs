pub(crate) fn run_static_finalizer(
    frame: bray_runtime_abi::NativeInactiveFrame,
    resolve: bray_runtime_abi::NativeStaticFinalizerResolveCallback,
    panics: bray_runtime_abi::NativePanicReportCallbacks,
) -> Vec<crate::incident::OwnedCleanupIncident> {
    run_cleanup_frame(frame, panics, |payload| {
        let mut incident = crate::product::empty_native_incident();
        let destination = (&raw mut incident).addr();
        let mut boundary = bray_runtime_abi::NativeBrayCallOutcome::completed();

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            resolve(payload, destination, &mut boundary)
        }));

        crate::product::finish_finalizer_callback(result, boundary, incident, panics)
    })
}

pub(super) fn run_cleanup_frame(
    frame: bray_runtime_abi::NativeInactiveFrame,
    panics: bray_runtime_abi::NativePanicReportCallbacks,
    resolve: impl FnOnce(usize) -> Vec<crate::incident::OwnedCleanupIncident>,
) -> Vec<crate::incident::OwnedCleanupIncident> {
    use bray_runtime_abi::{NativeRootHandle, NativeRunState};

    let mut transfer = super::frame::NativeFrameTransfer::new(
        frame.into_protected(bray_runtime_abi::NativeFrameEntry::Body),
    );

    let incidents = super::state::with_runtime(|runtime| {
        let allocation = runtime.allocate_frame_continuation(&transfer);

        let Some(task) = allocation.task() else {
            return vec![crate::incident::OwnedCleanupIncident::runtime_failure()];
        };

        if !runtime.start(task, &mut transfer).is_success() {
            return vec![crate::incident::OwnedCleanupIncident::runtime_failure()];
        }

        let Some(root) = NativeRootHandle::new(task.raw()) else {
            return vec![crate::incident::OwnedCleanupIncident::runtime_failure()];
        };

        let outcome = runtime.observe_root(root);

        let mut incidents = match outcome.state() {
            NativeRunState::COMPLETED => resolve(outcome.payload()),
            NativeRunState::PANICKED => {
                bray_runtime_abi::NativeBrayCallOutcome::panicked(outcome.payload())
                    .and_then(|outcome| {
                        crate::incident::OwnedCleanupIncident::boundary(outcome, panics)
                    })
                    .map_or_else(
                        || vec![crate::incident::OwnedCleanupIncident::runtime_failure()],
                        |incident| vec![incident],
                    )
            }
            NativeRunState::CANCELLED => crate::incident::OwnedCleanupIncident::boundary(
                bray_runtime_abi::NativeBrayCallOutcome::cancelled(),
                panics,
            )
            .into_iter()
            .collect(),
            NativeRunState::PENDING | NativeRunState::RUNTIME_FAILURE => {
                vec![crate::incident::OwnedCleanupIncident::runtime_failure()]
            }
            _ => vec![crate::incident::OwnedCleanupIncident::runtime_failure()],
        };

        if !runtime.resolve_root_completion(root).is_success() {
            incidents.push(crate::incident::OwnedCleanupIncident::runtime_failure());
        }

        incidents
    })
    .unwrap_or_else(|_| vec![crate::incident::OwnedCleanupIncident::runtime_failure()]);

    incidents
}

pub(crate) fn with_static_cleanup_runtime<T>(
    callback: impl FnOnce() -> T,
) -> (T, Vec<crate::incident::OwnedCleanupIncident>) {
    with_selected_static_cleanup_runtime(None, callback)
}

pub(crate) fn with_retained_static_cleanup_runtime<T>(
    runtime: &super::state::RetainedRuntime,
    callback: impl FnOnce() -> T,
) -> (T, Vec<crate::incident::OwnedCleanupIncident>) {
    with_selected_static_cleanup_runtime(Some(runtime), callback)
}

fn with_selected_static_cleanup_runtime<T>(
    runtime: Option<&super::state::RetainedRuntime>,
    callback: impl FnOnce() -> T,
) -> (T, Vec<crate::incident::OwnedCleanupIncident>) {
    let mut callback = Some(callback);
    let mut result = None;
    let mut incidents = Vec::new();

    let runtime = super::state::with_cleanup_runtime(runtime, || {
        let Some(callback) = callback.take() else {
            return false;
        };

        result = Some(callback());

        super::state::with_runtime(|runtime| runtime.report_cleanup_incidents())
            .is_ok_and(|status| status.is_success())
    });

    let runtime_succeeded = match runtime {
        Ok((reported, shutdown)) => reported && shutdown.is_success(),
        Err(_) => false,
    };

    if result.is_none()
        && let Some(callback) = callback.take()
    {
        result = Some(callback());
    }

    if !runtime_succeeded {
        incidents.push(crate::incident::OwnedCleanupIncident::runtime_failure());
    }

    let Some(result) = result else {
        unreachable!("static cleanup callback must run exactly once")
    };

    (result, incidents)
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::{
        NativeBrayCallOutcome, NativeFrameAffinity, NativeFrameExit, NativeFrameProgress,
        NativeFrameProgressKind, NativeInactiveFrame, NativeLaneRequirements,
        NativePanicReportCallbacks, NativeProtectedFrame, NativeRuntimeStatus,
        NativeStaticFinalizerStatus,
    };

    use super::{run_static_finalizer, with_static_cleanup_runtime};

    #[test]
    fn cleanup_continuations_remain_admissible_at_the_independent_task_limit() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        static DESTROYED: AtomicUsize = AtomicUsize::new(0);
        static ENTERED: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn destroy(_: usize) {
            DESTROYED.fetch_add(1, Ordering::SeqCst);
        }

        extern "C-unwind" fn resume(_: usize) -> NativeFrameProgress {
            ENTERED.fetch_add(1, Ordering::SeqCst);

            assert_eq!(
                super::super::state::with_runtime(|runtime| runtime.allocate().status()).unwrap(),
                NativeRuntimeStatus::RUNTIME_FAILURE
            );

            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
        }

        extern "C" fn move_rejected(
            _: usize,
            _: bray_runtime_abi::NativeFrameEntry,
        ) -> NativeProtectedFrame {
            NativeProtectedFrame::new(
                0,
                bray_runtime_abi::NativeFrameMetadata::new(
                    [42; 32],
                    1,
                    1,
                    1,
                    0,
                    1,
                    crate::test_support::native_movable_frame_state,
                ),
                resume,
                resume,
                ignore_action,
                ignore_resolution,
                ignore_completion_move,
                destroy,
            )
        }

        let _isolation = super::super::state::test_runtime_isolation();

        DESTROYED.store(0, Ordering::SeqCst);
        ENTERED.store(0, Ordering::SeqCst);

        // Without an attached runtime the cleanup entry cannot execute.
        let incidents = super::run_cleanup_frame(
            NativeInactiveFrame::new(0, move_rejected),
            panic_callbacks(),
            |_| Vec::new(),
        );

        assert_eq!(incidents.len(), 1);
        assert_eq!(DESTROYED.load(Ordering::SeqCst), 1);
        assert_eq!(ENTERED.load(Ordering::SeqCst), 0);

        assert_eq!(
            super::super::state::initialize(bray_runtime_abi::NativeRuntimeConfiguration::new(
                1, 1
            )),
            NativeRuntimeStatus::SUCCESS
        );

        super::super::state::with_runtime(|runtime| {
            assert!(runtime.allocate().task().is_some());
        })
        .unwrap();

        let incidents = super::run_cleanup_frame(
            NativeInactiveFrame::new(0, move_rejected),
            panic_callbacks(),
            |_| Vec::new(),
        );

        assert!(incidents.is_empty());
        assert_eq!(DESTROYED.load(Ordering::SeqCst), 2);
        assert_eq!(ENTERED.load(Ordering::SeqCst), 1);

        assert_eq!(
            super::super::state::shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn cleanup_runtime_reuses_an_active_foreign_thread_attachment() {
        let _attachment = bray_platform::RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("test thread must attach: {error:?}"));

        let (incidents, runtime_incidents) = with_static_cleanup_runtime(|| {
            run_static_finalizer(
                inactive_frame(
                    NativeFrameAffinity::ORIGIN_THREAD,
                    NativeLaneRequirements::NONE,
                ),
                resolve,
                panic_callbacks(),
            )
        });

        assert!(incidents.is_empty());
        assert!(runtime_incidents.is_empty());
    }

    #[test]
    fn cleanup_runtime_services_every_owned_cleanup_lane() {
        let cases = [
            (NativeFrameAffinity::MOVABLE, NativeLaneRequirements::NONE),
            (
                NativeFrameAffinity::MOVABLE,
                NativeLaneRequirements::BLOCKING,
            ),
            (
                NativeFrameAffinity::MOVABLE,
                NativeLaneRequirements::COMPUTE,
            ),
        ];

        let (incidents, runtime_incidents) = with_static_cleanup_runtime(|| {
            cases
                .into_iter()
                .flat_map(|(affinity, requirements)| {
                    run_static_finalizer(
                        inactive_frame(affinity, requirements),
                        resolve,
                        panic_callbacks(),
                    )
                })
                .collect::<Vec<_>>()
        });

        assert!(incidents.is_empty());
        assert!(runtime_incidents.is_empty());
    }

    #[test]
    fn cleanup_runtime_requires_a_retained_main_thread_identity() {
        let main_frame = || {
            inactive_frame(
                NativeFrameAffinity::MAIN_THREAD,
                NativeLaneRequirements::MAIN_THREAD,
            )
        };

        let (incidents, runtime_incidents) = with_static_cleanup_runtime(|| {
            run_static_finalizer(main_frame(), resolve, panic_callbacks())
        });

        assert_eq!(incidents.len(), 1);
        assert!(runtime_incidents.is_empty());

        assert!(
            super::super::state::initialize(bray_runtime_abi::NativeRuntimeConfiguration::new(
                16, 16,
            ))
            .is_success()
        );

        let (incidents, runtime_incidents) = with_static_cleanup_runtime(|| {
            run_static_finalizer(main_frame(), resolve, panic_callbacks())
        });

        assert!(incidents.is_empty());
        assert!(runtime_incidents.is_empty());
        assert!(super::super::state::shutdown().is_success());
    }

    static FRAMES: crate::test_support::NativeTestValues<NativeProtectedFrame> =
        crate::test_support::NativeTestValues::new();

    fn inactive_frame(
        affinity: NativeFrameAffinity,
        requirements: NativeLaneRequirements,
    ) -> NativeInactiveFrame {
        use crate::test_support::{
            native_blocking_frame_state, native_compute_frame_state, native_main_frame_state,
            native_movable_frame_state, native_origin_frame_state,
        };

        let state: bray_runtime_abi::NativeFrameStateCallback = match (affinity, requirements) {
            (NativeFrameAffinity::MOVABLE, NativeLaneRequirements::NONE) => {
                native_movable_frame_state
            }
            (NativeFrameAffinity::MOVABLE, NativeLaneRequirements::BLOCKING) => {
                native_blocking_frame_state
            }
            (NativeFrameAffinity::MOVABLE, NativeLaneRequirements::COMPUTE) => {
                native_compute_frame_state
            }
            (NativeFrameAffinity::ORIGIN_THREAD, NativeLaneRequirements::NONE) => {
                native_origin_frame_state
            }
            (NativeFrameAffinity::MAIN_THREAD, NativeLaneRequirements::MAIN_THREAD) => {
                native_main_frame_state
            }
            _ => panic!("fixture frame state must be supported"),
        };

        let mut identity = [9; 32];
        identity[0] = u8::try_from(affinity.code()).unwrap();
        identity[1] = u8::try_from(requirements.bits()).unwrap();

        let metadata = bray_runtime_abi::NativeFrameMetadata::new(identity, 1, 1, 1, 1, 1, state);

        let frame = NativeProtectedFrame::new(
            0,
            metadata,
            resume,
            cancel,
            ignore_action,
            ignore_resolution,
            ignore_completion_move,
            ignore_action,
        );

        NativeInactiveFrame::new(FRAMES.insert(frame), move_before_start)
    }

    extern "C" fn move_before_start(
        context: usize,
        _: bray_runtime_abi::NativeFrameEntry,
    ) -> NativeProtectedFrame {
        FRAMES
            .take(context)
            .expect("fixture frame must remain owned")
    }

    extern "C-unwind" fn resume(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
    }

    extern "C-unwind" fn cancel(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::CANCELLED, 0, 0)
    }

    extern "C-unwind" fn ignore_action(_: usize) {}

    extern "C-unwind" fn ignore_resolution(_: usize, _: NativeFrameExit) {}

    extern "C-unwind" fn ignore_completion_move(_: usize, _: usize) {}

    fn panic_callbacks() -> NativePanicReportCallbacks {
        crate::test_support::panic_callbacks(unexpected_panic, unexpected_panic)
    }

    extern "C-unwind" fn unexpected_panic(_: usize) -> NativeRuntimeStatus {
        panic!("successful cleanup must not report or destroy a panic");
    }

    extern "C-unwind" fn resolve(
        _: usize,
        _: usize,
        _: &mut NativeBrayCallOutcome,
    ) -> NativeStaticFinalizerStatus {
        NativeStaticFinalizerStatus::SUCCESS
    }
}
