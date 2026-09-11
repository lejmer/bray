use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{
    NativeExecutionLaneResult, NativeFrameProgress, NativeFrameProgressKind, NativeInactiveFrame,
    NativePanicCause, NativeProductHostDescriptor, NativeProductHostObservation,
    NativeProductHostOperation, NativeProtectedFrame, NativeRootHandle, NativeRootStart,
    NativeRunOutcome, NativeRunResultLayout, NativeRunState, NativeRuntimeConfiguration,
    NativeRuntimeEventCallback, NativeRuntimeStatus, NativeSourceAnchor, NativeTaskAllocation,
    NativeTaskHandle, NativeThreadStaticCleanupRegistration, NativeWakeCallback,
};

use crate::current_run_cancellation_requested;
use crate::root::propagate_current_run_cancellation;

use super::state::{initialize, runtime_failure, shutdown, with_runtime};

native_export! {
    pub extern "C" fn bray_runtime_substrate_initialization(
        worker_capacity: usize,
        timer_capacity: usize,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            initialize(NativeRuntimeConfiguration::new(
                worker_capacity,
                timer_capacity,
            ))
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_product_host_control(
        descriptor: &NativeProductHostDescriptor,
        operation: NativeProductHostOperation,
    ) -> NativeProductHostObservation {
        catch_unwind(AssertUnwindSafe(|| crate::product::control(descriptor, operation)))
            .unwrap_or_else(|_| NativeProductHostObservation::new(
                bray_runtime_abi::NativeProductHostStatus::RUNTIME_FAILURE,
                bray_runtime_abi::NativeProductHostState::FAILED,
                0,
                0,
                0,
                0,
                0,
                0,
                bray_runtime_abi::NativeStaticIdentity::new([0; 32]),
            ))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_substrate_thread_attachment_identity(
        descriptor: &'static NativeProductHostDescriptor,
    ) -> u64 {
        catch_unwind(AssertUnwindSafe(|| {
            crate::product::thread_attachment_identity(descriptor)
        }))
        .unwrap_or(0)
    }
}

native_export! {
    pub extern "C" fn bray_runtime_substrate_thread_static_cleanup_registration(
        registration: &NativeThreadStaticCleanupRegistration,
    ) -> NativeRuntimeStatus {
        contain_status(|| crate::product::register_thread_static(registration))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_root_execution(
        frame: NativeInactiveFrame,
        configuration: NativeRuntimeConfiguration,
    ) -> NativeRootStart {
        catch_unwind(AssertUnwindSafe(|| {
            let start = super::host::with_output(|| execute_root(frame.into_protected(bray_runtime_abi::NativeFrameEntry::Body), configuration));

            if let Some(root) = start.root()
                && let Ok(Ok(cancellation)) =
                    with_runtime(|runtime| runtime.root_cancellation(root))
            {
                super::host::register_timeout(cancellation);
            }

            start
        }))
            .unwrap_or_else(|_| {
                NativeRootStart::failure(NativeRuntimeStatus::PANICKED)
            })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_root_cancellation_request(
        root: NativeRootHandle,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.request_root_cancellation(root))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_root_terminal_observation(
        root: NativeRootHandle,
    ) -> NativeRunOutcome {
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            with_runtime(|runtime| runtime.observe_root(root))
                .unwrap_or_else(runtime_failure)
        }))
        .unwrap_or_else(|_| runtime_failure(NativeRuntimeStatus::PANICKED));

        super::host::record_outcome(outcome);

        outcome
    }
}

native_export! {
    pub extern "C" fn bray_runtime_root_completion_resolution(
        root: NativeRootHandle,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.resolve_root_completion(root))
                .unwrap_or_else(|status| status)
        })
    }
}

pub(super) fn report_panic(cause: NativePanicCause, source: NativeSourceAnchor, message: String) {
    if super::host::active() {
        super::host::record_panic(cause, source, message);
    } else {
        eprintln!("{message}");
    }
}

native_export! {
    pub extern "C" fn bray_runtime_entry_failure_resolution(
        identity: &bray_runtime_abi::NativeTypeIdentity,
        source: &NativeSourceAnchor,
        value: usize,
        broadcast: Option<&bray_runtime_abi::NativeValueCleanup>,
        lifecycle: Option<&bray_runtime_abi::NativeValueCleanup>,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            super::entry::resolve_failure(*identity, *source, value, broadcast, lifecycle)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_panic_propagation(
        _: usize,
    ) -> ! {
        std::process::abort()
    }
}

native_export! {
    pub extern "C" fn bray_runtime_cleanup_incident_reporting(
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            if super::host::active() {
                let count = with_runtime(|runtime| runtime.discard_cleanup_incidents())
                    .unwrap_or_default();

                super::host::record_cleanup_failure(count);

                return NativeRuntimeStatus::SUCCESS;
            }

            with_runtime(|runtime| runtime.report_cleanup_incidents())
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_main_thread_lane_startup(
        configuration: NativeRuntimeConfiguration,
    ) -> NativeRuntimeStatus {
        contain_status(|| initialize_for_execution(configuration))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_main_thread_lane_drive() -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.drive_main_thread())
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_task_allocation() -> NativeTaskAllocation {
        catch_unwind(AssertUnwindSafe(|| {
            with_runtime(|runtime| runtime.allocate()).unwrap_or_else(
                NativeTaskAllocation::failure,
            )
        }))
        .unwrap_or_else(|_| {
            NativeTaskAllocation::failure(NativeRuntimeStatus::PANICKED)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_task_start(
        task: NativeTaskHandle,
        frame: NativeInactiveFrame,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            // Source caller storage owns rejection cleanup, including unwinding before publication.
            let mut transfer = super::frame::NativeFrameTransfer::borrowed(frame.into_protected(bray_runtime_abi::NativeFrameEntry::Body));

            with_runtime(|runtime| runtime.start(task, &mut transfer, None))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_awaited_frame_composition(
        frame: NativeInactiveFrame,
        entry: u8,
    ) {
        // Validated MIR admits only these awaited-frame entry modes at this private ABI boundary.
        let entry = bray_runtime_abi::NativeFrameEntry::from_code(entry)
            .filter(|entry| *entry != bray_runtime_abi::NativeFrameEntry::CaptureDestruction)
            .expect("awaited-frame entry selection must be valid");
        let transfer = super::frame::NativeFrameTransfer::from_inactive(frame, entry);

        let status = with_runtime(|runtime| runtime.compose_awaited(transfer))
            .unwrap_or_else(|status| status);

        assert!(status.is_success(), "awaited-frame composition failed: {status:?}");
    }
}

native_export! {
    pub extern "C" fn bray_runtime_task_completion_borrow(
        task: NativeTaskHandle,
        destination: Option<&mut usize>,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            let Some(destination) = destination else {
                return NativeRuntimeStatus::INVALID_ARGUMENT;
            };

            match with_runtime(|runtime| runtime.borrow_task_completion(task)).and_then(|result| result) {
                Ok(address) => {
                    *destination = address.map_or(0, std::num::NonZeroUsize::get);

                    NativeRuntimeStatus::SUCCESS
                }
                Err(status) => status,
            }
        })
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_inactive_capture_destruction(frame: NativeInactiveFrame) -> usize {
        let transfer = super::frame::NativeFrameTransfer::new(frame.into_protected(bray_runtime_abi::NativeFrameEntry::CaptureDestruction));
        let frame = transfer.frame();
        let progress = frame.resume()(frame.context());
        let outcome = match progress.kind() {
            NativeFrameProgressKind::COMPLETED => bray_runtime_abi::NativeBrayCallOutcome::completed(),
            NativeFrameProgressKind::CANCELLED => bray_runtime_abi::NativeBrayCallOutcome::cancelled(),
            // Generated capture destruction transfers a valid owned report for a panicked outcome.
            NativeFrameProgressKind::PANICKED => bray_runtime_abi::NativeBrayCallOutcome::panicked(progress.payload())
                .expect("capture destruction must preserve an owned panic report"),
            _ => panic!("capture destruction must finish synchronously"),
        };

        outcome.raw()
    }
}

native_export! {
    pub extern "C" fn bray_runtime_task_completion_borrow_release(task: NativeTaskHandle) -> NativeRuntimeStatus {
        contain_status(|| with_runtime(|runtime| runtime.release_task_completion_borrow(task)).unwrap_or_else(|status| status))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_task_resolution(
        task: NativeTaskHandle,
        destination: *mut u8,
        layout: Option<&NativeRunResultLayout>,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            let Some(layout) = layout.copied() else {
                return NativeRuntimeStatus::INVALID_ARGUMENT;
            };
            with_runtime(|runtime| {
                runtime.transfer_task_outcome(task, |outcome| {
                    super::task_outcome::transfer_outcome(outcome, destination.addr(), layout)
                })
            }).and_then(|result| result)
                .map_or_else(|status| status, |()| NativeRuntimeStatus::SUCCESS)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_task_destruction(
        task: NativeTaskHandle,
        cleanup: Option<&'static bray_runtime_abi::NativeTaskTerminalCleanup>,
    ) -> NativeRuntimeStatus {
        contain_status(|| super::task_outcome::destroy_terminal_task(task, cleanup))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_task_event_creation() -> usize {
        with_runtime(|runtime| super::event::create(&runtime.core)).unwrap_or(0)
    }
}

native_export! {
    pub extern "C" fn bray_runtime_task_event_signal(event: usize) -> NativeRuntimeStatus {
        contain_status(|| super::event::signal(event))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_task_event_destruction(
        event: usize,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| super::event::destroy(&runtime.core, event))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_suspension_registration(
        state: u32,
    ) -> NativeFrameProgress {
        NativeFrameProgress::new(
            NativeFrameProgressKind::SUSPENDED,
            state,
            0,
        )
    }
}

native_export! {
    pub extern "C" fn bray_runtime_wake(task: NativeTaskHandle) -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.wake(task))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_awaited_frame_cancellation_request() {
        let status = with_runtime(|runtime| runtime.request_awaited_cancellation())
            .unwrap_or_else(|status| status);

        assert!(status.is_success(), "awaited-frame cancellation failed: {status:?}");
    }
}

native_export! {
    pub extern "C" fn bray_runtime_task_cancellation_request(
        task: NativeTaskHandle,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.request_cancellation(task))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_current_run_cancellation_observation() -> u8 {
        catch_unwind(AssertUnwindSafe(current_run_cancellation_requested))
            .map(u8::from)
            .unwrap_or(0)
    }
}

native_export! {
    pub extern "C" fn bray_runtime_cleanup_shield_enter() {
        crate::context::enter_current_run_cleanup_shield();
    }
}

native_export! {
    pub extern "C" fn bray_runtime_awaited_frame_resolution(
        destination: *mut u8,
        layout: &NativeRunResultLayout,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            if destination.is_null() || !layout.is_valid() {
                return NativeRuntimeStatus::INVALID_ARGUMENT;
            }

            with_runtime(|runtime| {
                runtime.resolve_awaited_terminal(|outcome| super::task_outcome::transfer_outcome(outcome, destination.addr(), *layout))
            }).unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_cleanup_shield_leave() {
        crate::context::leave_current_run_cleanup_shield();
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_current_run_cancellation_propagation() -> ! {
        propagate_current_run_cancellation()
    }
}

native_export! {
    pub extern "C" fn bray_runtime_join_registration(
        task: NativeTaskHandle,
        callback: NativeWakeCallback,
        context: usize,
    ) -> NativeRunOutcome {
        catch_unwind(AssertUnwindSafe(|| {
            with_runtime(|runtime| {
                runtime.join(task, callback, context)
            })
            .unwrap_or_else(runtime_failure)
        }))
        .unwrap_or_else(|_| runtime_failure(NativeRuntimeStatus::PANICKED))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_terminal_publication(
        state: NativeRunState,
        payload: usize,
    ) -> NativeFrameProgress {
        if state == NativeRunState::COMPLETED {
            return NativeFrameProgress::new(
                NativeFrameProgressKind::COMPLETED,
                0,
                payload,
            );
        }

        if state == NativeRunState::CANCELLED {
            return NativeFrameProgress::new(
                NativeFrameProgressKind::CANCELLED,
                0,
                0,
            );
        }

        if state == NativeRunState::PANICKED {
            return NativeFrameProgress::new(
                NativeFrameProgressKind::PANICKED,
                0,
                payload,
            );
        }

        NativeFrameProgress::new(
            NativeFrameProgressKind::RUNTIME_FAILURE,
            0,
            0,
        )
    }
}

native_export! {
    pub extern "C" fn bray_runtime_event(
        callback: NativeRuntimeEventCallback,
        context: usize,
    ) -> NativeRuntimeStatus {
        contain_status(|| callback(context))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_compatible_lane_selection(
        task: NativeTaskHandle,
        state: u32,
    ) -> NativeExecutionLaneResult {
        catch_unwind(AssertUnwindSafe(|| {
            with_runtime(|runtime| runtime.lane(task, state)).unwrap_or_else(
                NativeExecutionLaneResult::failure,
            )
        }))
        .unwrap_or_else(|_| {
            NativeExecutionLaneResult::failure(NativeRuntimeStatus::PANICKED)
        })
    }
}

pub(crate) extern "C" fn bray_runtime_structured_shutdown() -> NativeRuntimeStatus {
    contain_status(|| {
        let runtime_status = shutdown();

        if super::host::finish().is_err() {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        runtime_status
    })
}

native_export! {
    pub extern "C" fn bray_runtime_substrate_shutdown() -> NativeRuntimeStatus {
        bray_runtime_structured_shutdown()
    }
}

pub(super) fn contain_status(
    callback: impl FnOnce() -> NativeRuntimeStatus,
) -> NativeRuntimeStatus {
    catch_unwind(AssertUnwindSafe(callback)).unwrap_or(NativeRuntimeStatus::PANICKED)
}

fn execute_root(
    frame: NativeProtectedFrame,
    configuration: NativeRuntimeConfiguration,
) -> NativeRootStart {
    let mut transfer = super::frame::NativeFrameTransfer::new(frame);
    let status = initialize_for_execution(configuration);

    if !status.is_success() {
        return NativeRootStart::failure(status);
    }

    let allocation =
        with_runtime(|runtime| runtime.allocate()).unwrap_or_else(NativeTaskAllocation::failure);

    let Some(task) = allocation.task() else {
        return NativeRootStart::failure(allocation.status());
    };

    let status = with_runtime(|runtime| runtime.start(task, &mut transfer, None))
        .unwrap_or_else(|status| status);

    if !status.is_success() {
        return NativeRootStart::failure(status);
    }

    NativeRootHandle::new(task.raw()).map_or_else(
        || NativeRootStart::failure(NativeRuntimeStatus::RUNTIME_FAILURE),
        NativeRootStart::success,
    )
}

fn initialize_for_execution(configuration: NativeRuntimeConfiguration) -> NativeRuntimeStatus {
    let status = initialize(configuration);

    if status == NativeRuntimeStatus::ALREADY_INITIALIZED {
        NativeRuntimeStatus::SUCCESS
    } else {
        status
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{
        NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind, NativeInactiveFrame,
        NativePanicCause, NativeProtectedFrame, NativeRunOutcome, NativeRunState,
        NativeRuntimeConfiguration, NativeRuntimeStatus, NativeTaskHandle,
    };

    use super::super::callback::{
        bray_runtime_substrate_foreign_callback_execution,
        bray_runtime_substrate_synchronous_root_execution,
    };

    use super::{
        bray_runtime_awaited_frame_composition, bray_runtime_join_registration,
        bray_runtime_main_thread_lane_drive, bray_runtime_main_thread_lane_startup,
        bray_runtime_root_completion_resolution, bray_runtime_root_execution,
        bray_runtime_root_terminal_observation, bray_runtime_structured_shutdown,
        bray_runtime_suspension_registration, bray_runtime_task_allocation,
        bray_runtime_task_start,
    };

    static DESTROYED: AtomicUsize = AtomicUsize::new(0);
    static ROOT_DESTROYED: AtomicUsize = AtomicUsize::new(0);
    static ROOT_COMPLETION_DESTINATION: AtomicUsize = AtomicUsize::new(0);
    static REJECTED_DESTROYED: AtomicUsize = AtomicUsize::new(0);
    static STARTED_DESTROYED: AtomicUsize = AtomicUsize::new(0);
    static SUSPENDED_ROOT: AtomicUsize = AtomicUsize::new(0);
    static SUSPENDED_RESUMES: AtomicUsize = AtomicUsize::new(0);
    static CANCEL_ENTRIES: AtomicUsize = AtomicUsize::new(0);
    static FAILURE_ROOT: AtomicUsize = AtomicUsize::new(0);
    static FAILURE_RESUMES: AtomicUsize = AtomicUsize::new(0);
    static FAILURE_BROADCASTS: AtomicUsize = AtomicUsize::new(0);
    static FAILURE_RESOLUTIONS: AtomicUsize = AtomicUsize::new(0);
    static FAILURE_DESTRUCTIONS: AtomicUsize = AtomicUsize::new(0);
    static AWAITED_BLOCKING_COMPLETIONS: AtomicUsize = AtomicUsize::new(0);
    const AWAITED_BLOCKING_CHILD_COUNT: usize = 8;
    static AWAITED_OUTCOME_STAGE: AtomicUsize = AtomicUsize::new(0);
    static AFFINED_CLEANUP_STAGE: AtomicUsize = AtomicUsize::new(0);
    static AFFINED_CLEANUP_THREAD: std::sync::Mutex<Option<std::thread::ThreadId>> =
        std::sync::Mutex::new(None);

    struct TestPanicReport {
        cause: NativePanicCause,
        source: bray_runtime_abi::NativeSourceAnchor,
        message: String,
    }

    static PANIC_REPORTS: crate::test_support::NativeTestValues<TestPanicReport> =
        crate::test_support::NativeTestValues::new();
    static INACTIVE_FRAMES: crate::test_support::NativeTestValues<NativeProtectedFrame> =
        crate::test_support::NativeTestValues::new();

    #[test]
    fn inactive_capture_destruction_preserves_outcomes_and_releases_context_on_every_exit() {
        use bray_runtime_abi::{NativeBrayCallOutcome, NativeFrameEntry};

        static RELEASES: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn release(_: usize) {
            RELEASES.fetch_add(1, Ordering::SeqCst);
        }

        extern "C-unwind" fn destroy(context: usize) -> NativeFrameProgress {
            let kind = match context {
                0 => NativeFrameProgressKind::COMPLETED,
                1 => NativeFrameProgressKind::CANCELLED,
                2 => NativeFrameProgressKind::PANICKED,
                3 => NativeFrameProgressKind::YIELDED,
                _ => panic!("fixture native unwind"),
            };

            NativeFrameProgress::new(kind, 0, 64)
        }

        extern "C" fn select(context: usize, entry: NativeFrameEntry) -> NativeProtectedFrame {
            assert_eq!(entry, NativeFrameEntry::CaptureDestruction);

            NativeProtectedFrame::new(
                context,
                bray_runtime_abi::NativeFrameMetadata::new(
                    [21; 32],
                    1,
                    8,
                    8,
                    8,
                    8,
                    crate::test_support::native_main_frame_state,
                ),
                destroy,
                cancel_frame,
                ignore_action,
                ignore_resolution,
                ignore_completion_move,
                release,
            )
        }

        for (context, expected) in [
            NativeBrayCallOutcome::completed(),
            NativeBrayCallOutcome::cancelled(),
            NativeBrayCallOutcome::panicked(64).unwrap(),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(
                super::bray_runtime_inactive_capture_destruction(NativeInactiveFrame::new(
                    context, select
                )),
                expected.raw()
            );

            assert_eq!(RELEASES.load(Ordering::SeqCst), context + 1);
        }

        for context in [3, 4] {
            assert!(
                std::panic::catch_unwind(|| super::bray_runtime_inactive_capture_destruction(
                    NativeInactiveFrame::new(context, select)
                ))
                .is_err()
            );

            assert_eq!(RELEASES.load(Ordering::SeqCst), context + 1);
        }
    }

    #[test]
    fn capture_quiescence_adapter_retains_the_original_context_and_has_unit_completion() {
        use bray_runtime_abi::NativeFrameEntry;

        static RELEASES: AtomicUsize = AtomicUsize::new(0);
        static QUIESCENCE: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn release(_: usize) {
            RELEASES.fetch_add(1, Ordering::SeqCst);
        }

        extern "C-unwind" fn quiesce(_: usize) -> NativeFrameProgress {
            QUIESCENCE.fetch_add(1, Ordering::SeqCst);

            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
        }

        extern "C" fn select(context: usize, entry: NativeFrameEntry) -> NativeProtectedFrame {
            assert_eq!(entry, NativeFrameEntry::CaptureQuiescence);

            NativeProtectedFrame::new(
                context,
                bray_runtime_abi::NativeFrameMetadata::new(
                    [22; 32],
                    1,
                    8,
                    8,
                    8,
                    8,
                    crate::test_support::native_main_frame_state,
                ),
                quiesce,
                cancel_frame,
                ignore_action,
                ignore_resolution,
                forbidden_capture_completion,
                release,
            )
        }

        extern "C-unwind" fn forbidden_capture_completion(_: usize, _: usize) {
            panic!("quiescence must not read the ordinary completion slot");
        }

        let frame =
            NativeInactiveFrame::new(9, select).into_protected(NativeFrameEntry::CaptureQuiescence);

        assert_eq!(
            (
                frame.metadata().completion_size(),
                frame.metadata().completion_alignment()
            ),
            (0, 1)
        );

        assert_eq!(
            frame.cancel()(frame.context()).kind(),
            NativeFrameProgressKind::COMPLETED
        );

        frame.move_completion()(frame.context(), 0);
        frame.broadcast_tasks()(frame.context());
        frame.resolve_lifecycle()(frame.context(), NativeFrameExit::CANCELLED);
        drop(super::super::frame::NativeFrameTransfer::new(frame));

        let rejected = std::thread::spawn(|| {
            std::panic::catch_unwind(|| {
                super::bray_runtime_awaited_frame_composition(
                    NativeInactiveFrame::new(9, select),
                    NativeFrameEntry::CaptureQuiescence.code(),
                );
            })
        })
        .join()
        .unwrap();

        assert!(rejected.is_err());

        assert_eq!(QUIESCENCE.load(Ordering::SeqCst), 1);
        assert_eq!(RELEASES.load(Ordering::SeqCst), 0);
    }

    fn report_test_panic(payload: usize) -> NativeRuntimeStatus {
        let Some(report) = PANIC_REPORTS.take(payload) else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        super::report_panic(report.cause, report.source, report.message);

        NativeRuntimeStatus::SUCCESS
    }

    #[test]
    fn missing_native_layout_is_rejected_before_runtime_access() {
        assert_eq!(
            super::bray_runtime_task_resolution(
                NativeTaskHandle::new(1).unwrap(),
                std::ptr::null_mut(),
                None
            ),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );
    }

    #[test]
    fn inactive_capture_quiescence_suspends_without_consuming_the_original_owner() {
        use bray_runtime_abi::NativeFrameEntry;

        static ROOT_STAGE: AtomicUsize = AtomicUsize::new(0);
        static CAPTURE_STAGE: AtomicUsize = AtomicUsize::new(0);
        static RELEASES: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn release(_: usize) {
            assert_eq!(CAPTURE_STAGE.load(Ordering::SeqCst), 3);
            RELEASES.fetch_add(1, Ordering::SeqCst);
        }

        extern "C-unwind" fn quiesce(_: usize) -> NativeFrameProgress {
            assert_eq!(RELEASES.load(Ordering::SeqCst), 0);

            match CAPTURE_STAGE.fetch_add(1, Ordering::SeqCst) {
                0 => NativeFrameProgress::new(NativeFrameProgressKind::YIELDED, 1, 0),
                1 => NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0),
                _ => panic!("capture quiescence resumed after completion"),
            }
        }

        extern "C-unwind" fn destroy(_: usize) -> NativeFrameProgress {
            assert_eq!(CAPTURE_STAGE.swap(3, Ordering::SeqCst), 2);
            assert_eq!(RELEASES.load(Ordering::SeqCst), 0);

            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
        }

        extern "C-unwind" fn forbidden_completion(_: usize, _: usize) {
            panic!("capture completion must not read the body result");
        }

        extern "C" fn select(context: usize, entry: NativeFrameEntry) -> NativeProtectedFrame {
            let resume = match entry {
                NativeFrameEntry::CaptureQuiescence => quiesce,
                NativeFrameEntry::CaptureDestruction => destroy,
                _ => forbidden_capture_body,
            };

            NativeProtectedFrame::new(
                context,
                bray_runtime_abi::NativeFrameMetadata::new(
                    [23; 32],
                    2,
                    8,
                    8,
                    8,
                    8,
                    crate::test_support::native_origin_frame_state,
                ),
                resume,
                forbidden_capture_body,
                ignore_action,
                ignore_resolution,
                forbidden_completion,
                release,
            )
        }

        extern "C-unwind" fn root(_: usize) -> NativeFrameProgress {
            if ROOT_STAGE.fetch_add(1, Ordering::SeqCst) == 0 {
                bray_runtime_awaited_frame_composition(
                    NativeInactiveFrame::new(19, select),
                    NativeFrameEntry::CaptureQuiescence.code(),
                );

                return bray_runtime_suspension_registration(1);
            }

            let mut result = [0u64; 2];

            let layout = bray_runtime_abi::NativeRunResultLayout::new(
                16,
                8,
                crate::test_support::record_run_result_transfer,
            );

            assert_eq!(
                super::bray_runtime_awaited_frame_resolution(result.as_mut_ptr().cast(), &layout),
                NativeRuntimeStatus::SUCCESS
            );

            assert_eq!(
                crate::test_support::take_run_result_transfer()
                    .unwrap()
                    .1
                    .state(),
                NativeRunState::COMPLETED
            );

            assert_eq!(RELEASES.load(Ordering::SeqCst), 0);

            assert_eq!(
                super::bray_runtime_inactive_capture_destruction(NativeInactiveFrame::new(
                    19, select
                )),
                bray_runtime_abi::NativeBrayCallOutcome::completed().raw()
            );

            assert_eq!(RELEASES.load(Ordering::SeqCst), 1);

            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 17)
        }

        let start = execute_test_root(
            protected_frame(8, root, ignore_completion_move, ignore_action),
            NativeRuntimeConfiguration::new(2, 1),
        );

        let root = start.root().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);

        loop {
            assert_eq!(
                bray_runtime_main_thread_lane_drive(),
                NativeRuntimeStatus::SUCCESS
            );

            let outcome = bray_runtime_root_terminal_observation(root);

            if outcome.state() != NativeRunState::PENDING {
                assert_eq!(outcome.state(), NativeRunState::COMPLETED);
                break;
            }

            assert!(
                std::time::Instant::now() < deadline,
                "capture quiescence must finish"
            );

            std::thread::yield_now();
        }

        assert_eq!(CAPTURE_STAGE.load(Ordering::SeqCst), 3);
        assert_eq!(RELEASES.load(Ordering::SeqCst), 1);

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn rejected_root_start_destroys_its_transferred_frame() {
        static RELEASES: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn release(_: usize) {
            RELEASES.fetch_add(1, Ordering::Relaxed);
        }

        let start = execute_test_root(
            protected_frame(8, resume_frame, ignore_completion_move, release),
            NativeRuntimeConfiguration::new(0, 1),
        );

        assert_eq!(start.status(), NativeRuntimeStatus::INVALID_ARGUMENT);
        assert_eq!(start.root(), None);
        assert_eq!(RELEASES.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn unwinding_native_frame_retains_its_runtime_failure() {
        let start = execute_test_root(
            protected_frame(8, unwind_frame, ignore_completion_move, ignore_action),
            NativeRuntimeConfiguration::new(2, 1),
        );

        let root = start.root().unwrap();
        let outcome = bray_runtime_root_terminal_observation(root);
        let incidents = super::with_runtime(|runtime| runtime.discard_cleanup_incidents()).unwrap();
        let resolved = bray_runtime_root_completion_resolution(root);
        let shutdown = bray_runtime_structured_shutdown();

        assert_eq!(outcome.state(), NativeRunState::RUNTIME_FAILURE);

        assert_eq!(
            outcome.payload(),
            NativeRuntimeStatus::PANICKED.code() as usize
        );

        assert_eq!(incidents, 1);
        assert_eq!(resolved, NativeRuntimeStatus::SUCCESS);
        assert_eq!(shutdown, NativeRuntimeStatus::SUCCESS);
    }

    #[test]
    fn terminal_completion_storage_honors_overaligned_native_results() {
        let frame = NativeProtectedFrame::new(
            0,
            bray_runtime_abi::NativeFrameMetadata::new(
                [8; 32],
                1,
                8,
                8,
                65,
                64,
                crate::test_support::native_main_frame_state,
            ),
            resume_frame,
            cancel_frame,
            ignore_action,
            ignore_resolution,
            assert_overaligned_completion,
            ignore_action,
        );

        let root = execute_test_root(frame, NativeRuntimeConfiguration::new(2, 1))
            .root()
            .unwrap();

        let outcome = bray_runtime_root_terminal_observation(root);

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_ne!(outcome.payload(), 0);
        assert_eq!(outcome.payload() % 64, 0);

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    extern "C-unwind" fn assert_overaligned_completion(_: usize, destination: usize) {
        assert_ne!(destination, 0);
        assert_eq!(destination % 64, 0);
    }

    #[test]
    fn inactive_frame_cleanup_uses_capture_entry_without_running_body() {
        let frame = NativeInactiveFrame::new(0, move_capture_cleanup_child)
            .into_protected(bray_runtime_abi::NativeFrameEntry::CaptureCleanup);

        let root = execute_test_root(frame, NativeRuntimeConfiguration::new(2, 1))
            .root()
            .unwrap();

        let outcome = bray_runtime_root_terminal_observation(root);

        assert_eq!(outcome.state(), NativeRunState::CANCELLED);

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    extern "C" fn move_capture_cleanup_child(
        _: usize,
        _: bray_runtime_abi::NativeFrameEntry,
    ) -> NativeProtectedFrame {
        protected_frame(
            8,
            forbidden_inactive_body,
            ignore_completion_move,
            ignore_action,
        )
    }

    extern "C-unwind" fn forbidden_inactive_body(_: usize) -> NativeFrameProgress {
        panic!("discarding an inactive future must not execute its body");
    }

    #[test]
    fn completion_borrows_preserve_noncompletion_outcomes() {
        let resumptions: [extern "C-unwind" fn(usize) -> NativeFrameProgress; 2] =
            [cancel_frame, retained_child_panic];

        for resume in resumptions {
            let root = execute_test_root(
                protected_frame(8, resume, ignore_completion_move, ignore_action),
                NativeRuntimeConfiguration::new(2, 1),
            )
            .root()
            .unwrap();

            let task = NativeTaskHandle::new(root.raw()).unwrap();
            let original = bray_runtime_root_terminal_observation(root);
            let mut address = usize::MAX;

            assert!(matches!(
                original.state(),
                NativeRunState::CANCELLED | NativeRunState::PANICKED
            ));

            assert_eq!(
                super::bray_runtime_task_completion_borrow(task, Some(&mut address)),
                NativeRuntimeStatus::SUCCESS
            );

            assert_eq!(address, 0);

            assert_eq!(
                super::bray_runtime_task_completion_borrow_release(task),
                NativeRuntimeStatus::INVALID_ARGUMENT
            );

            assert_eq!(bray_runtime_root_terminal_observation(root), original);

            assert_eq!(
                super::with_runtime(|runtime| runtime.transfer_task_outcome(task, |outcome| {
                    assert_eq!(outcome, original);

                    Ok(())
                })),
                Ok(Ok(()))
            );

            assert_eq!(
                bray_runtime_root_completion_resolution(root),
                NativeRuntimeStatus::SUCCESS
            );

            assert_eq!(
                bray_runtime_structured_shutdown(),
                NativeRuntimeStatus::SUCCESS
            );
        }
    }

    extern "C-unwind" fn retained_child_panic(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::PANICKED, 0, 64)
    }

    #[test]
    fn terminal_result_transfer_is_exclusive_and_restores_failed_moves() {
        let root = execute_test_root(
            protected_frame(8, resume_frame, ignore_completion_move, ignore_action),
            NativeRuntimeConfiguration::new(2, 1),
        )
        .root()
        .unwrap();

        let task = NativeTaskHandle::new(root.raw()).unwrap();
        let observed = bray_runtime_root_terminal_observation(root);

        assert_eq!(observed.state(), NativeRunState::COMPLETED);

        let mut address = usize::MAX;

        assert_eq!(
            super::bray_runtime_task_completion_borrow_release(task),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            super::bray_runtime_task_completion_borrow(task, None),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            super::bray_runtime_task_completion_borrow(task, Some(&mut address)),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(address, observed.payload());
        assert_ne!(address, 0);

        super::with_runtime(|runtime| {
            assert_eq!(runtime.observe(task).state(), NativeRunState::PENDING);
            assert_eq!(runtime.destroy_task(task), NativeRuntimeStatus::PENDING);

            assert_eq!(
                runtime.borrow_task_completion(task),
                Err(NativeRuntimeStatus::PENDING)
            );

            assert_eq!(
                runtime.transfer_task_outcome(task, |_| panic!(
                    "an outstanding completion borrow excludes transfer"
                )),
                Err(NativeRuntimeStatus::PENDING)
            );

            assert_eq!(
                runtime.consume_task_outcome(task, |_| panic!(
                    "an outstanding completion borrow excludes destruction"
                )),
                Err(NativeRuntimeStatus::PENDING)
            );
        })
        .unwrap();

        let mut rejected = usize::MAX;

        assert_eq!(
            super::bray_runtime_task_completion_borrow(task, Some(&mut rejected)),
            NativeRuntimeStatus::PENDING
        );

        assert_eq!(rejected, usize::MAX);

        assert_eq!(
            super::bray_runtime_task_completion_borrow_release(task),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            super::bray_runtime_task_completion_borrow_release(task),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(bray_runtime_root_terminal_observation(root), observed);

        super::with_runtime(|runtime| {
            let rejected = runtime.transfer_task_outcome(task, |outcome| {
                assert_eq!(outcome, observed);
                assert_eq!(runtime.observe(task).state(), NativeRunState::PENDING);
                assert_eq!(runtime.destroy_task(task), NativeRuntimeStatus::PENDING);

                assert_eq!(
                    runtime.transfer_task_outcome(task, |_| panic!(
                        "a second transfer must not enter"
                    )),
                    Err(NativeRuntimeStatus::PENDING)
                );

                Err(NativeRuntimeStatus::INVALID_ARGUMENT)
            });

            assert_eq!(rejected, Err(NativeRuntimeStatus::INVALID_ARGUMENT));
            assert_eq!(runtime.observe(task), observed);

            let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.transfer_task_outcome(task, |_| panic!("failed native result transfer"))
            }));

            assert!(unwound.is_err());
            assert_eq!(runtime.observe(task), observed);

            assert_eq!(
                runtime.transfer_task_outcome(task, |outcome| {
                    assert_eq!(outcome, observed);

                    Ok(())
                }),
                Ok(())
            );

            assert_eq!(
                runtime.transfer_task_outcome(task, |_| panic!(
                    "a consumed result must not move again"
                )),
                Err(NativeRuntimeStatus::INVALID_ARGUMENT)
            );
        })
        .unwrap();

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    extern "C-unwind" fn unwind_frame(_: usize) -> NativeFrameProgress {
        panic!("unexpected native frame unwind");
    }

    #[test]
    fn awaited_outcome_transfer_preserves_failures_and_resumes_remaining_work() {
        AWAITED_OUTCOME_STAGE.store(0, Ordering::Relaxed);

        let start = execute_test_root(
            protected_frame(8, await_all_outcomes, ignore_completion_move, ignore_action),
            NativeRuntimeConfiguration::new(2, 1),
        );

        let root = start.root().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);

        loop {
            assert_eq!(
                bray_runtime_main_thread_lane_drive(),
                NativeRuntimeStatus::SUCCESS
            );

            let outcome = bray_runtime_root_terminal_observation(root);

            if outcome.state() != NativeRunState::PENDING {
                assert_eq!(outcome.state(), NativeRunState::COMPLETED);
                break;
            }

            assert!(
                std::time::Instant::now() < deadline,
                "awaited outcome transfer must complete"
            );

            std::thread::yield_now();
        }

        assert_eq!(AWAITED_OUTCOME_STAGE.load(Ordering::Relaxed), 3);

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn task_storage_reservation_failures_preserve_inactive_ownership_and_allow_retry() {
        static RESUMES: AtomicUsize = AtomicUsize::new(0);
        static RELEASES: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn resume(_: usize) -> NativeFrameProgress {
            RESUMES.fetch_add(1, Ordering::Relaxed);

            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
        }

        extern "C-unwind" fn release(_: usize) {
            RELEASES.fetch_add(1, Ordering::Relaxed);
        }

        for successful_allocations in 0..64 {
            RESUMES.store(0, Ordering::Relaxed);
            RELEASES.store(0, Ordering::Relaxed);

            assert_eq!(
                super::initialize(NativeRuntimeConfiguration::new(1, 1)),
                NativeRuntimeStatus::SUCCESS
            );

            // Use fresh tables for each failure site so retained capacity cannot skip a later allocation.
            let admitted =
                super::with_runtime(|runtime| {
                    for continuation in [false, true] {
                        let rejected = crate::test_support::with_allocation_failure(|| {
                            if continuation {
                                runtime.allocate_continuation()
                            } else {
                                runtime.allocate()
                            }
                        });

                        assert_eq!(rejected.status(), NativeRuntimeStatus::ALLOCATION_FAILURE);
                        assert!(rejected.task().is_none());
                    }

                    let handle = runtime.allocate().task().unwrap();

                    assert_eq!(
                        handle.raw(),
                        1,
                        "failed admission must not consume an identity"
                    );

                    let mut transfer = super::super::frame::NativeFrameTransfer::new(
                        protected_frame(8, resume, ignore_completion_move, release),
                    );

                    let status = crate::test_support::with_allocation_failure_after(
                        successful_allocations,
                        || runtime.start(handle, &mut transfer, None),
                    );

                    let accepted = if status.is_success() {
                        handle
                    } else {
                        assert_eq!(
                            status,
                            NativeRuntimeStatus::ALLOCATION_FAILURE,
                            "allocation {successful_allocations}"
                        );

                        assert_eq!(transfer.frame().metadata().identity(), [7; 32]);
                        assert_eq!(RESUMES.load(Ordering::Relaxed), 0);
                        assert_eq!(RELEASES.load(Ordering::Relaxed), 0);

                        assert_eq!(
                            runtime.destroy_task(handle),
                            NativeRuntimeStatus::UNKNOWN_TASK
                        );

                        let retry = runtime.allocate().task().unwrap();

                        assert_eq!(
                            runtime.start(retry, &mut transfer, None),
                            NativeRuntimeStatus::SUCCESS
                        );

                        retry
                    };

                    assert_eq!(
                        runtime.resolve_task(accepted).state(),
                        NativeRunState::COMPLETED
                    );

                    assert_eq!(runtime.destroy_task(accepted), NativeRuntimeStatus::SUCCESS);

                    status.is_success()
                })
                .unwrap();

            assert_eq!(RESUMES.load(Ordering::Relaxed), 1);
            assert_eq!(RELEASES.load(Ordering::Relaxed), 1);

            assert_eq!(
                bray_runtime_structured_shutdown(),
                NativeRuntimeStatus::SUCCESS
            );

            if admitted {
                assert!(
                    successful_allocations >= 8,
                    "every task-storage allocation must be exercised"
                );

                return;
            }
        }

        panic!("task admission did not reach the end of its allocation sequence");
    }

    extern "C-unwind" fn await_all_outcomes(_: usize) -> NativeFrameProgress {
        let stage = AWAITED_OUTCOME_STAGE.load(Ordering::Relaxed);

        if stage != 0 {
            let mut result = [0u64; 2];

            let layout = bray_runtime_abi::NativeRunResultLayout::new(
                16,
                8,
                crate::test_support::record_run_result_transfer,
            );

            assert_eq!(
                super::with_runtime(|runtime| runtime
                    .resolve_awaited_terminal(|_| Err(NativeRuntimeStatus::INVALID_ARGUMENT))),
                Ok(NativeRuntimeStatus::INVALID_ARGUMENT)
            );

            assert_eq!(
                super::bray_runtime_awaited_frame_resolution(result.as_mut_ptr().cast(), &layout),
                NativeRuntimeStatus::SUCCESS
            );

            let (destination, transferred) =
                crate::test_support::take_run_result_transfer().unwrap();

            assert_eq!(destination, result.as_ptr().addr());

            assert_eq!(
                transferred.state(),
                match stage {
                    1 => NativeRunState::CANCELLED,
                    2 => NativeRunState::PANICKED,
                    _ => NativeRunState::COMPLETED,
                }
            );

            if stage == 2 {
                assert_ne!(transferred.payload(), 0);

                assert_eq!(
                    report_test_panic(transferred.payload()),
                    NativeRuntimeStatus::SUCCESS
                );
            }

            assert_eq!(
                super::bray_runtime_awaited_frame_resolution(result.as_mut_ptr().cast(), &layout),
                NativeRuntimeStatus::UNKNOWN_TASK
            );
        }

        if stage == 3 {
            return NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 17);
        }

        AWAITED_OUTCOME_STAGE.store(stage + 1, Ordering::Relaxed);

        bray_runtime_awaited_frame_composition(
            NativeInactiveFrame::new(stage, move_outcome_child),
            0,
        );

        assert_eq!(
            super::with_runtime(|runtime| runtime.compose_awaited(
                super::super::frame::NativeFrameTransfer::new(move_outcome_child(
                    stage,
                    bray_runtime_abi::NativeFrameEntry::Body,
                ))
            )),
            Ok(NativeRuntimeStatus::RUNTIME_FAILURE)
        );

        bray_runtime_suspension_registration(1)
    }

    extern "C" fn move_outcome_child(
        stage: usize,
        _: bray_runtime_abi::NativeFrameEntry,
    ) -> NativeProtectedFrame {
        let resume = match stage {
            0 => cancel_frame as extern "C-unwind" fn(usize) -> NativeFrameProgress,
            1 => panic_outcome_child,
            _ => resume_frame,
        };

        protected_frame(8, resume, ignore_completion_move, ignore_action)
    }

    extern "C-unwind" fn panic_outcome_child(_: usize) -> NativeFrameProgress {
        let mut outcome = NativeRunOutcome::new(NativeRunState::PENDING, 0);

        propagate_test_panic(0, &mut outcome);

        NativeFrameProgress::new(NativeFrameProgressKind::PANICKED, 0, outcome.payload())
    }

    #[test]
    fn root_execution_moves_completion_before_frame_destruction() {
        ROOT_DESTROYED.store(0, Ordering::Relaxed);
        ROOT_COMPLETION_DESTINATION.store(0, Ordering::Relaxed);

        let start = execute_test_root(
            protected_frame(
                8,
                resume_frame,
                record_root_completion_destination,
                root_destroy,
            ),
            NativeRuntimeConfiguration::new(2, 1),
        );

        let Some(root) = start.root() else {
            panic!("root frame must transfer");
        };

        let outcome = bray_runtime_root_terminal_observation(root);

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_ne!(outcome.payload(), 0);

        assert_eq!(
            outcome.payload(),
            ROOT_COMPLETION_DESTINATION.load(Ordering::Relaxed)
        );

        assert_eq!(ROOT_DESTROYED.load(Ordering::Relaxed), 1);

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::UNKNOWN_TASK
        );

        assert_eq!(
            bray_runtime_task_allocation().status(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(ROOT_DESTROYED.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn movable_native_root_runs_on_the_host_cooperative_lane() {
        let start = execute_test_root(
            protected_frame_with_state(
                8,
                crate::test_support::native_movable_frame_state,
                resume_frame,
                ignore_completion_move,
                ignore_action,
            ),
            NativeRuntimeConfiguration::new(2, 1),
        );

        let Some(root) = start.root() else {
            panic!("movable root frame must transfer");
        };

        assert_eq!(
            bray_runtime_main_thread_lane_drive(),
            NativeRuntimeStatus::PENDING
        );

        assert_eq!(
            bray_runtime_root_terminal_observation(root).state(),
            NativeRunState::COMPLETED
        );

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn awaited_blocking_children_wake_the_cooperative_parent() {
        AWAITED_BLOCKING_COMPLETIONS.store(0, Ordering::Relaxed);

        let start = execute_test_root(
            protected_frame_with_state(
                8,
                crate::test_support::native_movable_frame_state,
                await_blocking_children,
                ignore_completion_move,
                ignore_action,
            ),
            NativeRuntimeConfiguration::new(AWAITED_BLOCKING_CHILD_COUNT + 1, 1),
        );

        let Some(root) = start.root() else {
            panic!("awaiting root frame must transfer");
        };

        let task = NativeTaskHandle::new(root.raw())
            .unwrap_or_else(|| panic!("root handle must identify a task"));

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);

        loop {
            let outcome = super::with_runtime(|runtime| runtime.observe(task))
                .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"));

            if outcome.state() != NativeRunState::PENDING {
                assert_eq!(outcome.state(), NativeRunState::COMPLETED);
                break;
            }

            assert!(
                std::time::Instant::now() < deadline,
                "awaited blocking children must make progress after {} completions",
                AWAITED_BLOCKING_COMPLETIONS.load(Ordering::Relaxed)
            );

            std::thread::yield_now();
        }

        assert_eq!(
            AWAITED_BLOCKING_COMPLETIONS.load(Ordering::Relaxed),
            AWAITED_BLOCKING_CHILD_COUNT
        );

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn worker_drives_suspending_capture_cleanup_on_its_origin_thread() {
        for state in [
            crate::test_support::native_movable_frame_state,
            crate::test_support::native_blocking_frame_state,
        ] {
            AFFINED_CLEANUP_STAGE.store(0, Ordering::Relaxed);
            *AFFINED_CLEANUP_THREAD.lock().unwrap() = None;

            let start = execute_test_root(
                protected_frame_with_state(
                    8,
                    state,
                    await_affined_cleanup,
                    ignore_completion_move,
                    ignore_action,
                ),
                NativeRuntimeConfiguration::new(4, 1),
            );

            let root = start.root().unwrap();
            let task = NativeTaskHandle::new(root.raw()).unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);

            let outcome = loop {
                let outcome = super::with_runtime(|runtime| runtime.observe(task)).unwrap();

                if outcome.state() != NativeRunState::PENDING {
                    break outcome;
                }

                assert!(
                    std::time::Instant::now() < deadline,
                    "origin-thread cleanup must finish"
                );

                std::thread::yield_now();
            };

            assert_eq!(outcome.state(), NativeRunState::COMPLETED);
            assert_eq!(AFFINED_CLEANUP_STAGE.load(Ordering::Relaxed), 3);

            assert_ne!(
                *AFFINED_CLEANUP_THREAD.lock().unwrap(),
                Some(std::thread::current().id())
            );

            assert_eq!(
                bray_runtime_root_completion_resolution(root),
                NativeRuntimeStatus::SUCCESS
            );

            assert_eq!(
                bray_runtime_structured_shutdown(),
                NativeRuntimeStatus::SUCCESS
            );
        }
    }

    extern "C-unwind" fn await_affined_cleanup(_: usize) -> NativeFrameProgress {
        if AFFINED_CLEANUP_STAGE.load(Ordering::Relaxed) == 0 {
            *AFFINED_CLEANUP_THREAD.lock().unwrap() = Some(std::thread::current().id());
            AFFINED_CLEANUP_STAGE.store(1, Ordering::Relaxed);

            bray_runtime_awaited_frame_composition(
                NativeInactiveFrame::new(0, move_affined_cleanup),
                1,
            );

            return bray_runtime_suspension_registration(1);
        }

        let mut result = [0u64; 2];

        let layout = bray_runtime_abi::NativeRunResultLayout::new(
            16,
            8,
            crate::test_support::record_run_result_transfer,
        );

        assert_eq!(
            super::bray_runtime_awaited_frame_resolution(result.as_mut_ptr().cast(), &layout),
            NativeRuntimeStatus::SUCCESS
        );

        let (destination, transferred) = crate::test_support::take_run_result_transfer().unwrap();

        assert_eq!(destination, result.as_ptr().addr());
        assert_eq!(transferred.state(), NativeRunState::CANCELLED);

        NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 17)
    }

    extern "C" fn move_affined_cleanup(
        _: usize,
        _: bray_runtime_abi::NativeFrameEntry,
    ) -> NativeProtectedFrame {
        NativeProtectedFrame::new(
            0,
            bray_runtime_abi::NativeFrameMetadata::new(
                [8; 32],
                2,
                8,
                8,
                8,
                8,
                crate::test_support::native_origin_frame_state,
            ),
            forbidden_capture_body,
            resume_affined_cleanup,
            ignore_action,
            ignore_resolution,
            ignore_completion_move,
            ignore_action,
        )
    }

    extern "C-unwind" fn forbidden_capture_body(_: usize) -> NativeFrameProgress {
        panic!("capture cleanup must not enter the inactive body")
    }

    extern "C-unwind" fn resume_affined_cleanup(_: usize) -> NativeFrameProgress {
        assert_eq!(
            *AFFINED_CLEANUP_THREAD.lock().unwrap(),
            Some(std::thread::current().id())
        );

        match AFFINED_CLEANUP_STAGE.fetch_add(1, Ordering::Relaxed) {
            1 => NativeFrameProgress::new(NativeFrameProgressKind::YIELDED, 1, 0),
            2 => NativeFrameProgress::new(NativeFrameProgressKind::CANCELLED, 0, 0),
            _ => panic!("capture cleanup must finish exactly once"),
        }
    }

    #[test]
    fn root_terminal_observation_waits_for_real_suspend_and_resume() {
        SUSPENDED_RESUMES.store(0, Ordering::Relaxed);

        let start = execute_test_root(
            protected_frame(
                8,
                suspend_and_self_wake,
                ignore_completion_move,
                ignore_action,
            ),
            NativeRuntimeConfiguration::new(2, 1),
        );

        let Some(root) = start.root() else {
            panic!("suspending root must transfer");
        };

        SUSPENDED_ROOT.store(
            usize::try_from(root.raw()).unwrap_or_else(|_| panic!("test root must fit usize")),
            Ordering::Relaxed,
        );

        let outcome = bray_runtime_root_terminal_observation(root);

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_eq!(SUSPENDED_RESUMES.load(Ordering::Relaxed), 2);

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn host_cancellation_wakes_a_suspended_root_into_cancellation_entry() {
        CANCEL_ENTRIES.store(0, Ordering::Relaxed);

        let frame = NativeProtectedFrame::new(
            0,
            bray_runtime_abi::NativeFrameMetadata::new(
                [8; 32],
                2,
                8,
                8,
                8,
                8,
                crate::test_support::native_main_frame_state,
            ),
            suspend_without_wake,
            record_cancel_entry,
            ignore_action,
            ignore_resolution,
            ignore_completion_move,
            ignore_action,
        );

        let start = execute_test_root(frame, NativeRuntimeConfiguration::new(2, 1));

        let Some(root) = start.root() else {
            panic!("cancellable root must transfer");
        };

        assert_eq!(
            bray_runtime_main_thread_lane_drive(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            super::bray_runtime_root_cancellation_request(root),
            NativeRuntimeStatus::SUCCESS
        );

        let outcome = bray_runtime_root_terminal_observation(root);

        assert_eq!(outcome.state(), NativeRunState::CANCELLED);
        assert_eq!(CANCEL_ENTRIES.load(Ordering::Relaxed), 1);

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn cleanup_callback_failures_are_owned_until_host_drain() {
        let frame = NativeProtectedFrame::new(
            0,
            bray_runtime_abi::NativeFrameMetadata::new(
                [9; 32],
                1,
                8,
                8,
                8,
                8,
                crate::test_support::native_main_frame_state,
            ),
            resume_frame,
            cancel_frame,
            panic_action,
            panic_resolution,
            ignore_completion_move,
            panic_action,
        );

        let start = execute_test_root(frame, NativeRuntimeConfiguration::new(2, 1));

        let Some(root) = start.root() else {
            panic!("cleanup-failing root must transfer");
        };

        assert_eq!(
            bray_runtime_root_terminal_observation(root).state(),
            NativeRunState::COMPLETED
        );

        let pending = super::with_runtime(|runtime| runtime.pending_cleanup_incidents())
            .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"));

        assert_eq!(pending, 3);

        assert_eq!(
            super::bray_runtime_cleanup_incident_reporting(),
            NativeRuntimeStatus::SUCCESS
        );

        let pending = super::with_runtime(|runtime| runtime.pending_cleanup_incidents())
            .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"));

        assert_eq!(pending, 0);

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn runtime_failure_after_suspension_resolves_the_retained_frame() {
        FAILURE_RESUMES.store(0, Ordering::Relaxed);
        FAILURE_BROADCASTS.store(0, Ordering::Relaxed);
        FAILURE_RESOLUTIONS.store(0, Ordering::Relaxed);
        FAILURE_DESTRUCTIONS.store(0, Ordering::Relaxed);

        let frame = NativeProtectedFrame::new(
            0,
            bray_runtime_abi::NativeFrameMetadata::new(
                [10; 32],
                2,
                8,
                8,
                8,
                8,
                crate::test_support::native_main_frame_state,
            ),
            suspend_then_fail,
            cancel_frame,
            record_failure_broadcast,
            record_failure_resolution,
            ignore_completion_move,
            record_failure_destruction,
        );

        let start = execute_test_root(frame, NativeRuntimeConfiguration::new(2, 1));

        let Some(root) = start.root() else {
            panic!("failure-path root must transfer");
        };

        FAILURE_ROOT.store(
            usize::try_from(root.raw()).unwrap_or_else(|_| panic!("test root must fit usize")),
            Ordering::Relaxed,
        );

        assert_eq!(
            bray_runtime_root_terminal_observation(root).state(),
            NativeRunState::RUNTIME_FAILURE
        );

        assert_eq!(FAILURE_RESUMES.load(Ordering::Relaxed), 2);
        assert_eq!(FAILURE_BROADCASTS.load(Ordering::Relaxed), 1);
        assert_eq!(FAILURE_RESOLUTIONS.load(Ordering::Relaxed), 1);
        assert_eq!(FAILURE_DESTRUCTIONS.load(Ordering::Relaxed), 1);

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn native_runtime_boundary_starts_executes_and_shuts_down() {
        assert_eq!(
            bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(8, 8)),
            NativeRuntimeStatus::SUCCESS
        );

        let allocation = bray_runtime_task_allocation();

        let Some(task) = allocation.task() else {
            panic!("native frame must allocate");
        };

        assert_eq!(start_test_task(task, frame()), NativeRuntimeStatus::SUCCESS);

        assert_eq!(
            bray_runtime_main_thread_lane_drive(),
            NativeRuntimeStatus::SUCCESS
        );

        let outcome = bray_runtime_join_registration(task, ignore_wake, 0);

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_ne!(outcome.payload(), 0);

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(DESTROYED.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn native_runtime_boundary_validates_state_and_capacities() {
        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::NOT_INITIALIZED
        );

        assert_eq!(
            bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(0, 1)),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(1, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_task_allocation().status(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_task_allocation().status(),
            NativeRuntimeStatus::RUNTIME_FAILURE
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn task_start_transfers_each_frame_exactly_once() {
        assert_eq!(
            bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(1, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        let allocation = bray_runtime_task_allocation();

        let Some(task) = allocation.task() else {
            panic!("native task storage must allocate");
        };

        assert_eq!(
            start_test_task(
                task,
                protected_frame(0, resume_frame, ignore_completion_move, rejected_destroy,)
            ),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(REJECTED_DESTROYED.load(Ordering::Relaxed), 1);

        let replacement = bray_runtime_task_allocation();

        let task = replacement
            .task()
            .expect("rejected start must release its allocation slot");

        assert_eq!(
            start_test_task(
                task,
                protected_frame(8, resume_frame, ignore_completion_move, started_destroy,)
            ),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            start_test_task(
                task,
                protected_frame(8, resume_frame, ignore_completion_move, rejected_destroy)
            ),
            NativeRuntimeStatus::UNKNOWN_TASK
        );

        assert_eq!(REJECTED_DESTROYED.load(Ordering::Relaxed), 2);

        assert_eq!(
            bray_runtime_main_thread_lane_drive(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_join_registration(task, ignore_wake, 0).state(),
            NativeRunState::COMPLETED
        );

        assert_eq!(STARTED_DESTROYED.load(Ordering::Relaxed), 1);

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn frame_contract_failures_are_not_published_as_panics() {
        assert_eq!(
            bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(2, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        for resume in [
            resume_runtime_failure as extern "C-unwind" fn(usize) -> NativeFrameProgress,
            resume_unknown_progress,
        ] {
            let allocation = bray_runtime_task_allocation();

            let Some(task) = allocation.task() else {
                panic!("native task storage must allocate");
            };

            assert_eq!(
                start_test_task(
                    task,
                    protected_frame(8, resume, ignore_completion_move, ignore_action,)
                ),
                NativeRuntimeStatus::SUCCESS
            );

            assert_eq!(
                bray_runtime_main_thread_lane_drive(),
                NativeRuntimeStatus::RUNTIME_FAILURE
            );

            assert_eq!(
                bray_runtime_join_registration(task, ignore_wake, 0).state(),
                NativeRunState::RUNTIME_FAILURE
            );
        }

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn abandoned_task_destruction_releases_completed_values_and_retains_child_panics() {
        use bray_runtime_abi::{
            NativeBrayCallOutcome, NativeCleanupExecution, NativeTaskTerminalCleanup,
            NativeValueCleanup,
        };

        static VALUE_DESTRUCTIONS: AtomicUsize = AtomicUsize::new(0);
        static PANIC_REPORTS: AtomicUsize = AtomicUsize::new(0);
        static PANIC_RELEASES: AtomicUsize = AtomicUsize::new(0);
        static PAYLOAD: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn destroy_value(
            value: usize,
            _: &mut NativeInactiveFrame,
            _: &mut NativeBrayCallOutcome,
        ) -> NativeRuntimeStatus {
            assert_eq!(value, PAYLOAD.load(Ordering::SeqCst));
            VALUE_DESTRUCTIONS.fetch_add(1, Ordering::SeqCst);

            NativeRuntimeStatus::SUCCESS
        }

        extern "C-unwind" fn report(value: usize) -> NativeRuntimeStatus {
            assert_eq!(value, 64);
            PANIC_REPORTS.fetch_add(1, Ordering::SeqCst);

            NativeRuntimeStatus::SUCCESS
        }

        extern "C-unwind" fn release(value: usize) -> NativeRuntimeStatus {
            assert_eq!(value, 64);
            PANIC_RELEASES.fetch_add(1, Ordering::SeqCst);

            NativeRuntimeStatus::SUCCESS
        }

        const PANICS: bray_runtime_abi::NativePanicReportCallbacks =
            crate::test_support::panic_callbacks(report, release);

        static VALUE: NativeValueCleanup =
            NativeValueCleanup::new(NativeCleanupExecution::SYNCHRONOUS, destroy_value, PANICS);

        static CLEANUP: NativeTaskTerminalCleanup =
            match NativeTaskTerminalCleanup::try_new(Some(&VALUE), PANICS) {
                Some(cleanup) => cleanup,
                None => panic!("fixture destruction must be synchronous"),
            };

        assert_eq!(
            super::bray_runtime_substrate_initialization(4, 1),
            NativeRuntimeStatus::SUCCESS
        );

        let resumptions: [extern "C-unwind" fn(usize) -> NativeFrameProgress; 3] =
            [resume_frame, cancel_frame, retained_child_panic];

        for resume in resumptions {
            for already_consumed in [false, true] {
                let task = bray_runtime_task_allocation().task().unwrap();

                assert_eq!(
                    start_test_task(
                        task,
                        protected_frame(8, resume, ignore_completion_move, ignore_action)
                    ),
                    NativeRuntimeStatus::SUCCESS
                );

                let retained = super::with_runtime(|runtime| runtime.resolve_task(task)).unwrap();

                PAYLOAD.store(retained.payload(), Ordering::SeqCst);
                VALUE_DESTRUCTIONS.store(0, Ordering::SeqCst);
                PANIC_REPORTS.store(0, Ordering::SeqCst);
                PANIC_RELEASES.store(0, Ordering::SeqCst);

                if already_consumed {
                    assert_eq!(
                        super::with_runtime(
                            |runtime| runtime.transfer_task_outcome(task, |_| Ok(()))
                        )
                        .unwrap(),
                        Ok(())
                    );
                }

                assert_eq!(
                    super::bray_runtime_task_destruction(task, Some(&CLEANUP)),
                    NativeRuntimeStatus::SUCCESS
                );

                assert_eq!(
                    super::bray_runtime_task_destruction(task, Some(&CLEANUP)),
                    NativeRuntimeStatus::UNKNOWN_TASK
                );

                assert_eq!(
                    VALUE_DESTRUCTIONS.load(Ordering::SeqCst),
                    usize::from(!already_consumed && retained.state() == NativeRunState::COMPLETED)
                );

                assert_eq!(
                    super::with_runtime(|runtime| runtime.report_cleanup_incidents()).unwrap(),
                    NativeRuntimeStatus::SUCCESS
                );

                let panics =
                    usize::from(!already_consumed && retained.state() == NativeRunState::PANICKED);

                assert_eq!(PANIC_REPORTS.load(Ordering::SeqCst), panics);
                assert_eq!(PANIC_RELEASES.load(Ordering::SeqCst), panics);
            }
        }

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn terminal_result_destruction_consumes_ownership_even_when_it_unwinds() {
        let root = execute_test_root(
            protected_frame(8, resume_frame, ignore_completion_move, ignore_action),
            NativeRuntimeConfiguration::new(2, 1),
        )
        .root()
        .unwrap();

        let task = NativeTaskHandle::new(root.raw()).unwrap();

        assert_eq!(
            bray_runtime_root_terminal_observation(root).state(),
            NativeRunState::COMPLETED
        );

        super::with_runtime(|runtime| {
            let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                runtime.consume_task_outcome(task, |_| {
                    assert_eq!(runtime.observe(task).state(), NativeRunState::PENDING);
                    assert_eq!(runtime.destroy_task(task), NativeRuntimeStatus::PENDING);

                    panic!("destruction failed after taking ownership");
                })
            }));

            assert!(unwound.is_err());

            assert_eq!(
                runtime.consume_task_outcome(task, |_| panic!(
                    "a consumed value must not be destroyed again"
                )),
                Ok(None)
            );

            assert_eq!(
                runtime.transfer_task_outcome(task, |_| panic!("a destroyed value must not move")),
                Err(NativeRuntimeStatus::INVALID_ARGUMENT)
            );
        })
        .unwrap();

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn failed_task_registration_releases_captures_outside_the_task_table_lock() {
        static RELEASES: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn reentrant_destroy(_: usize) {
            let unlocked = super::with_runtime(|runtime| runtime.tasks.try_lock().is_ok());

            assert_eq!(unlocked, Ok(true));
            RELEASES.fetch_add(1, Ordering::Relaxed);
        }

        let ((), incidents) = crate::native::with_static_cleanup_runtime(|| {
            for _ in 0..2 {
                let task = bray_runtime_task_allocation().task().unwrap();

                assert_eq!(
                    start_test_task(
                        task,
                        protected_frame(8, resume_frame, ignore_completion_move, reentrant_destroy)
                    ),
                    NativeRuntimeStatus::RUNTIME_FAILURE
                );

                assert_eq!(
                    super::with_runtime(|runtime| runtime.tasks.lock().unwrap().len()),
                    Ok(0)
                );
            }
        });

        assert!(incidents.is_empty());
        assert_eq!(RELEASES.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn missing_runtime_leaves_rejected_frame_cleanup_to_its_caller() {
        static RELEASES: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn release(_: usize) {
            RELEASES.fetch_add(1, Ordering::Relaxed);
        }

        let task = NativeTaskHandle::new(1).unwrap();

        assert_eq!(
            start_test_task(
                task,
                protected_frame(8, resume_frame, ignore_completion_move, release)
            ),
            NativeRuntimeStatus::NOT_INITIALIZED
        );

        assert_eq!(RELEASES.load(Ordering::Relaxed), 1);
    }

    fn frame() -> NativeProtectedFrame {
        protected_frame(8, resume_frame, ignore_completion_move, destroy_frame)
    }

    fn execute_test_root(
        frame: NativeProtectedFrame,
        configuration: NativeRuntimeConfiguration,
    ) -> super::NativeRootStart {
        bray_runtime_root_execution(inactive_test_frame(frame), configuration)
    }

    fn start_test_task(task: NativeTaskHandle, frame: NativeProtectedFrame) -> NativeRuntimeStatus {
        let context = frame.context();
        let destroy = frame.destroy();
        let status = bray_runtime_task_start(task, inactive_test_frame(frame));

        if !status.is_success() {
            destroy(context);
        }

        status
    }

    #[test]
    fn rejected_task_admission_does_not_destroy_caller_owned_captures() {
        static RELEASES: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn release(_: usize) {
            RELEASES.fetch_add(1, Ordering::Relaxed);
        }

        let task = NativeTaskHandle::new(1).unwrap();
        let frame = protected_frame(8, resume_frame, ignore_completion_move, release);
        let context = frame.context();
        let status = bray_runtime_task_start(task, inactive_test_frame(frame));

        assert_eq!(status, NativeRuntimeStatus::NOT_INITIALIZED);
        assert_eq!(RELEASES.load(Ordering::Relaxed), 0);

        release(context);

        assert_eq!(RELEASES.load(Ordering::Relaxed), 1);
    }

    fn inactive_test_frame(frame: NativeProtectedFrame) -> NativeInactiveFrame {
        NativeInactiveFrame::new(INACTIVE_FRAMES.insert(frame), move_test_frame)
    }

    extern "C" fn move_test_frame(
        context: usize,
        _: bray_runtime_abi::NativeFrameEntry,
    ) -> NativeProtectedFrame {
        INACTIVE_FRAMES
            .take(context)
            .expect("the fixture frame transfers exactly once")
    }

    #[test]
    fn synchronous_root_boundary_catches_reports_and_resolves_panic() {
        let outcome = bray_runtime_substrate_synchronous_root_execution(
            propagate_test_panic,
            0,
            assert_attached_cleanup,
        );

        assert_eq!(outcome.state(), NativeRunState::PANICKED);
        assert_ne!(outcome.payload(), 0);

        assert_eq!(
            report_test_panic(outcome.payload()),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn synchronous_root_boundary_catches_current_run_cancellation() {
        let outcome = bray_runtime_substrate_synchronous_root_execution(
            propagate_test_cancellation,
            0,
            assert_attached_cleanup,
        );

        assert_eq!(outcome.state(), NativeRunState::CANCELLED);
        assert_eq!(outcome.payload(), 0);
    }

    #[test]
    fn foreign_callback_boundary_attaches_threads_for_the_callback_lifetime() {
        let (outcome, attached_after_return) = std::thread::spawn(|| {
            assert!(bray_platform::current_runtime_thread().is_none());

            let outcome = bray_runtime_substrate_foreign_callback_execution(
                assert_callback_runtime_thread,
                41,
                assert_attached_cleanup,
            );

            (outcome, bray_platform::current_runtime_thread().is_some())
        })
        .join()
        .unwrap_or_else(|_| panic!("foreign callback test thread must join"));

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_eq!(outcome.payload(), 41);
        assert!(!attached_after_return);
    }

    #[test]
    fn foreign_callback_boundary_reuses_an_attached_runtime_thread() {
        let _thread = bray_platform::RuntimeThreadScope::enter()
            .unwrap_or_else(|error| panic!("runtime thread must attach: {error:?}"));

        let outcome = bray_runtime_substrate_foreign_callback_execution(
            assert_callback_runtime_thread,
            41,
            assert_attached_cleanup,
        );

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_eq!(outcome.payload(), 41);
    }

    #[test]
    fn entry_failure_resolution_uses_type_and_source_metadata() {
        let identity = bray_runtime_abi::NativeTypeIdentity::new([0xab; 32]);

        assert_eq!(
            super::bray_runtime_entry_failure_resolution(
                &identity,
                &bray_runtime_abi::NativeSourceAnchor::unavailable(),
                0,
                None,
                None,
            ),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            super::bray_runtime_entry_failure_resolution(
                &identity,
                &bray_runtime_abi::NativeSourceAnchor::new(0, 2, 1, 0),
                0,
                None,
                None,
            ),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );
    }

    fn protected_frame(
        alignment: usize,
        resume: extern "C-unwind" fn(usize) -> NativeFrameProgress,
        move_completion: extern "C-unwind" fn(usize, usize),
        destroy: extern "C-unwind" fn(usize),
    ) -> NativeProtectedFrame {
        protected_frame_with_state(
            alignment,
            crate::test_support::native_main_frame_state,
            resume,
            move_completion,
            destroy,
        )
    }

    fn protected_frame_with_state(
        alignment: usize,
        state: bray_runtime_abi::NativeFrameStateCallback,
        resume: extern "C-unwind" fn(usize) -> NativeFrameProgress,
        move_completion: extern "C-unwind" fn(usize, usize),
        destroy: extern "C-unwind" fn(usize),
    ) -> NativeProtectedFrame {
        NativeProtectedFrame::new(
            0,
            bray_runtime_abi::NativeFrameMetadata::new([7; 32], 2, 8, alignment, 8, 8, state),
            resume,
            cancel_frame,
            ignore_action,
            ignore_resolution,
            move_completion,
            destroy,
        )
    }

    extern "C-unwind" fn resume_frame(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 17)
    }

    extern "C-unwind" fn await_blocking_children(_: usize) -> NativeFrameProgress {
        let completed = AWAITED_BLOCKING_COMPLETIONS.load(Ordering::Relaxed);

        if completed != 0 {
            let mut result = [0u64; 2];

            let layout = bray_runtime_abi::NativeRunResultLayout::new(
                16,
                8,
                crate::test_support::record_run_result_transfer,
            );

            assert_eq!(
                super::bray_runtime_awaited_frame_resolution(result.as_mut_ptr().cast(), &layout),
                NativeRuntimeStatus::SUCCESS
            );

            let (_, transferred) =
                crate::test_support::take_run_result_transfer().expect("completed child outcome");

            assert_eq!(transferred.state(), NativeRunState::COMPLETED);
        }

        if completed == AWAITED_BLOCKING_CHILD_COUNT {
            return NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 17);
        }

        AWAITED_BLOCKING_COMPLETIONS.store(completed + 1, Ordering::Relaxed);

        bray_runtime_awaited_frame_composition(NativeInactiveFrame::new(0, move_blocking_child), 0);

        bray_runtime_suspension_registration(1)
    }

    extern "C" fn move_blocking_child(
        _: usize,
        _: bray_runtime_abi::NativeFrameEntry,
    ) -> NativeProtectedFrame {
        protected_frame_with_state(
            8,
            crate::test_support::native_blocking_frame_state,
            resume_frame,
            ignore_completion_move,
            ignore_action,
        )
    }

    extern "C-unwind" fn resume_runtime_failure(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::RUNTIME_FAILURE, 0, 0)
    }

    extern "C-unwind" fn resume_unknown_progress(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::from_code(u32::MAX), 0, 0)
    }

    extern "C-unwind" fn cancel_frame(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::CANCELLED, 0, 0)
    }

    extern "C" fn propagate_test_panic(_: usize, outcome: &mut NativeRunOutcome) {
        const MESSAGE: &[u8] = b"synchronous root panic";

        let report = PANIC_REPORTS.insert(TestPanicReport {
            cause: NativePanicCause::MESSAGE,
            source: bray_runtime_abi::NativeSourceAnchor::new(0, 0, 1, 0),
            message: String::from_utf8_lossy(MESSAGE).into_owned(),
        });

        *outcome = NativeRunOutcome::new(NativeRunState::PANICKED, report);
    }

    extern "C" fn propagate_test_cancellation(_: usize, outcome: &mut NativeRunOutcome) {
        *outcome = NativeRunOutcome::new(NativeRunState::CANCELLED, 0);
    }

    extern "C" fn assert_callback_runtime_thread(
        destination: usize,
        outcome: &mut NativeRunOutcome,
    ) {
        assert_eq!(destination, 41);
        assert!(bray_platform::current_runtime_thread().is_some());

        *outcome = NativeRunOutcome::new(NativeRunState::COMPLETED, destination);
    }

    extern "C" fn assert_attached_cleanup() {
        assert!(bray_platform::current_runtime_thread().is_some());
    }

    extern "C-unwind" fn suspend_and_self_wake(_: usize) -> NativeFrameProgress {
        let resume = SUSPENDED_RESUMES.fetch_add(1, Ordering::Relaxed);

        if resume == 0 {
            let raw = SUSPENDED_ROOT.load(Ordering::Relaxed);

            let raw = u64::try_from(raw).unwrap_or_else(|_| panic!("test root must fit u64"));

            let task = NativeTaskHandle::new(raw)
                .unwrap_or_else(|| panic!("test root task must be nonzero"));

            assert_eq!(super::bray_runtime_wake(task), NativeRuntimeStatus::SUCCESS);

            return NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0);
        }

        resume_frame(0)
    }

    extern "C-unwind" fn suspend_without_wake(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0)
    }

    extern "C-unwind" fn suspend_then_fail(_: usize) -> NativeFrameProgress {
        if FAILURE_RESUMES.fetch_add(1, Ordering::Relaxed) == 0 {
            let raw = FAILURE_ROOT.load(Ordering::Relaxed);

            let raw = u64::try_from(raw).unwrap_or_else(|_| panic!("test root must fit u64"));

            let task = NativeTaskHandle::new(raw)
                .unwrap_or_else(|| panic!("test root task must be nonzero"));

            assert_eq!(super::bray_runtime_wake(task), NativeRuntimeStatus::SUCCESS);

            return NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0);
        }

        resume_runtime_failure(0)
    }

    extern "C-unwind" fn record_cancel_entry(_: usize) -> NativeFrameProgress {
        CANCEL_ENTRIES.fetch_add(1, Ordering::Relaxed);

        cancel_frame(0)
    }

    extern "C-unwind" fn ignore_action(_: usize) {}

    extern "C-unwind" fn panic_action(_: usize) {
        panic!("cleanup action failed");
    }

    extern "C" fn ignore_wake(_: usize) {}

    extern "C-unwind" fn ignore_resolution(_: usize, _: NativeFrameExit) {}

    extern "C-unwind" fn panic_resolution(_: usize, _: NativeFrameExit) {
        panic!("cleanup resolution failed");
    }

    extern "C-unwind" fn record_failure_broadcast(_: usize) {
        FAILURE_BROADCASTS.fetch_add(1, Ordering::Relaxed);
    }

    extern "C-unwind" fn record_failure_resolution(_: usize, _: NativeFrameExit) {
        FAILURE_RESOLUTIONS.fetch_add(1, Ordering::Relaxed);
    }

    extern "C-unwind" fn record_failure_destruction(_: usize) {
        FAILURE_DESTRUCTIONS.fetch_add(1, Ordering::Relaxed);
    }

    extern "C-unwind" fn destroy_frame(_: usize) {
        DESTROYED.fetch_add(1, Ordering::Relaxed);
    }

    extern "C-unwind" fn record_root_completion_destination(_: usize, destination: usize) {
        ROOT_COMPLETION_DESTINATION.store(destination, Ordering::Relaxed);
    }

    extern "C-unwind" fn ignore_completion_move(_: usize, _: usize) {}

    extern "C-unwind" fn root_destroy(_: usize) {
        ROOT_DESTROYED.fetch_add(1, Ordering::Relaxed);
    }

    extern "C-unwind" fn rejected_destroy(_: usize) {
        REJECTED_DESTROYED.fetch_add(1, Ordering::Relaxed);
    }

    extern "C-unwind" fn started_destroy(_: usize) {
        STARTED_DESTROYED.fetch_add(1, Ordering::Relaxed);
    }
}
