use std::mem::align_of;
use std::panic::{AssertUnwindSafe, catch_unwind, panic_any};

use bray_runtime_abi::{
    NativeExecutionLaneResult, NativeFrameProgress, NativeFrameProgressKind, NativeInactiveFrame,
    NativePanicCause, NativeProtectedFrame, NativeProtectedFrameTransfer, NativeRootHandle,
    NativeRootStart, NativeRunOutcome, NativeRunState, NativeRuntimeConfiguration,
    NativeRuntimeEventCallback, NativeRuntimeStatus, NativeSourceAnchor, NativeStringView,
    NativeTaskAllocation, NativeTaskHandle, NativeWakeCallback,
};

use crate::current_run_cancellation_requested;
use crate::root::propagate_current_run_cancellation;

use super::callback::PropagatedPanicReport;
use super::state::{initialize, runtime_failure, shutdown, with_runtime};

native_export! {
    pub extern "C" fn bray_runtime_root_execution_v1(
        frame: NativeProtectedFrameTransfer,
        configuration: NativeRuntimeConfiguration,
    ) -> NativeRootStart {
        catch_unwind(AssertUnwindSafe(|| {
            let Some(frame) = take_transferred_frame(frame) else {
                return NativeRootStart::failure(NativeRuntimeStatus::INVALID_ARGUMENT);
            };

            let start = super::host::with_output(|| execute_root(frame, configuration));

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

#[derive(Debug)]
struct NativePanicReport {
    cause: NativePanicCause,
    source: NativeSourceAnchor,
    message: String,
}

native_export! {
    pub extern "C" fn bray_runtime_root_cancellation_request_v1(
        root: NativeRootHandle,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.request_root_cancellation(root))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_root_terminal_observation_v1(
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
    pub extern "C" fn bray_runtime_root_completion_resolution_v1(
        root: NativeRootHandle,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.resolve_root_completion(root))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    #[expect(
        unsafe_code,
        reason = "the reporting role receives the exact owned report allocation"
    )]
    pub extern "C" fn bray_runtime_panic_reporting_v1(
        payload: usize,
    ) -> NativeRuntimeStatus {
        if payload == 0 {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        contain_status(|| {
            let report = unsafe {
                // The construction and propagation roles transfer this exact allocation.
                Box::from_raw(payload as *mut NativePanicReport)
            };

            if !report.cause.is_known() || !report.source.is_valid() {
                return NativeRuntimeStatus::INVALID_ARGUMENT;
            }

            if super::host::active() {
                super::host::record_panic(report.cause, report.source, report.message);
            } else {
                eprintln!("{}", report.message);
            }

            NativeRuntimeStatus::SUCCESS
        })
    }
}

native_export! {
    #[expect(
        unsafe_code,
        reason = "the entry failure role borrows the validated native payload for this call"
    )]
    pub extern "C" fn bray_runtime_entry_failure_reporting_v1(
        payload: usize,
        size: usize,
    ) -> NativeRuntimeStatus {
        if payload == 0 && size != 0 {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        contain_status(|| {
            let bytes = if size == 0 {
                &[][..]
            } else {
                unsafe {
                    // The host retains the reported value for lifecycle resolution.
                    std::slice::from_raw_parts(payload as *const u8, size)
                }
            };

            if super::host::active() {
                super::host::record_returned_error();
            } else {
                eprintln!("{bytes:02x?}");
            }

            NativeRuntimeStatus::SUCCESS
        })
    }
}

native_export! {
    #[expect(
        unsafe_code,
        reason = "the construction role borrows the validated native string view for this call"
    )]
    pub extern "C" fn bray_runtime_panic_report_construction_v1(
        cause: NativePanicCause,
        source: NativeSourceAnchor,
        message: NativeStringView,
    ) -> usize {
        catch_unwind(AssertUnwindSafe(|| {
            if !cause.is_known()
                || !source.is_valid()
                || (message.length() != 0 && message.data().is_null())
            {
                return 0;
            }

            let bytes = if message.length() == 0 {
                &[][..]
            } else {
                unsafe {
                    // The view is borrowed only for this construction call.
                    std::slice::from_raw_parts(message.data(), message.length())
                }
            };

            Box::into_raw(Box::new(NativePanicReport {
                cause,
                source,
                message: String::from_utf8_lossy(bytes).into_owned(),
            })) as usize
        }))
        .unwrap_or(0)
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_panic_propagation_v1(
        payload: usize,
    ) -> ! {
        panic_any(PropagatedPanicReport(payload))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_cleanup_incident_reporting_v1(
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
    pub extern "C" fn bray_runtime_main_thread_lane_startup_v1(
        configuration: NativeRuntimeConfiguration,
    ) -> NativeRuntimeStatus {
        contain_status(|| initialize(configuration))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_main_thread_lane_drive_v1() -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.drive_main_thread())
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_task_allocation_v1() -> NativeTaskAllocation {
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
    pub extern "C" fn bray_runtime_task_start_v1(
        task: NativeTaskHandle,
        frame: NativeProtectedFrameTransfer,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            let Some(frame) = take_transferred_frame(frame) else {
                return NativeRuntimeStatus::INVALID_ARGUMENT;
            };

            with_runtime(|runtime| runtime.start(task, frame))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_awaited_frame_composition_v1(
        frame: NativeInactiveFrame,
    ) {
        let status = with_runtime(|runtime| runtime.compose_awaited(frame))
            .unwrap_or_else(|status| status);

        assert!(status.is_success(), "awaited-frame composition failed");
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_frame_completion_move_v1() -> usize {
        with_runtime(|runtime| runtime.resolve_awaited_completion())
            .and_then(|result| result)
            .unwrap_or_else(|_| panic!("awaited-frame completion resolution failed"))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_suspension_registration_v1(
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
    pub extern "C" fn bray_runtime_wake_v1(
        task: NativeTaskHandle,
        state: u32,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.wake(task, state))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_task_cancellation_request_v1(
        task: NativeTaskHandle,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.request_cancellation(task))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_current_run_cancellation_observation_v1() -> u8 {
        catch_unwind(AssertUnwindSafe(current_run_cancellation_requested))
            .map(u8::from)
            .unwrap_or(0)
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_current_run_cancellation_propagation_v1() -> ! {
        propagate_current_run_cancellation()
    }
}

native_export! {
    pub extern "C" fn bray_runtime_join_registration_v1(
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
    pub extern "C" fn bray_runtime_terminal_publication_v1(
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
    pub extern "C" fn bray_runtime_event_v1(
        callback: NativeRuntimeEventCallback,
        context: usize,
    ) -> NativeRuntimeStatus {
        contain_status(|| callback(context))
    }
}

native_export! {
    pub extern "C" fn bray_runtime_compatible_lane_selection_v1(
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

native_export! {
    pub extern "C" fn bray_runtime_structured_shutdown_v1() -> NativeRuntimeStatus {
        contain_status(|| {
            let runtime_status = shutdown();

            if super::host::finish().is_err() {
                return NativeRuntimeStatus::RUNTIME_FAILURE;
            }

            runtime_status
        })
    }
}

fn contain_status(callback: impl FnOnce() -> NativeRuntimeStatus) -> NativeRuntimeStatus {
    catch_unwind(AssertUnwindSafe(callback)).unwrap_or(NativeRuntimeStatus::PANICKED)
}

#[expect(
    unsafe_code,
    reason = "the native ownership-transfer ABI exposes a validated descriptor address"
)]
fn take_transferred_frame(transfer: NativeProtectedFrameTransfer) -> Option<NativeProtectedFrame> {
    let address = transfer.address();

    if address == 0 || !address.is_multiple_of(align_of::<NativeProtectedFrame>()) {
        return None;
    }

    let frame = unsafe {
        // The native ABI requires the caller to keep this descriptor live for
        // the call and transfers its generated context to this copied value.
        (address as *const NativeProtectedFrame).read()
    };

    Some(frame)
}

fn execute_root(
    frame: NativeProtectedFrame,
    configuration: NativeRuntimeConfiguration,
) -> NativeRootStart {
    let status = initialize(configuration);

    if !status.is_success() {
        return NativeRootStart::failure(status);
    }

    let allocation =
        with_runtime(|runtime| runtime.allocate()).unwrap_or_else(NativeTaskAllocation::failure);

    let Some(task) = allocation.task() else {
        return NativeRootStart::failure(allocation.status());
    };

    let status = with_runtime(|runtime| runtime.start(task, frame)).unwrap_or_else(|status| status);

    if !status.is_success() {
        return NativeRootStart::failure(status);
    }

    NativeRootHandle::new(task.raw()).map_or_else(
        || NativeRootStart::failure(NativeRuntimeStatus::RUNTIME_FAILURE),
        NativeRootStart::success,
    )
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{
        NativeFrameAffinity, NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind,
        NativeFrameState, NativeLaneRequirements, NativePanicCause, NativeProtectedFrame,
        NativeProtectedFrameTransfer, NativeRunState, NativeRuntimeConfiguration,
        NativeRuntimeStatus, NativeSourceAnchor, NativeStringView,
    };

    use super::super::callback::{
        bray_runtime_foreign_callback_execution_v1, bray_runtime_synchronous_root_execution_v1,
    };

    use super::{
        bray_runtime_join_registration_v1, bray_runtime_main_thread_lane_drive_v1,
        bray_runtime_main_thread_lane_startup_v1, bray_runtime_root_completion_resolution_v1,
        bray_runtime_root_execution_v1, bray_runtime_root_terminal_observation_v1,
        bray_runtime_structured_shutdown_v1, bray_runtime_task_allocation_v1,
        bray_runtime_task_start_v1,
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

        let outcome = bray_runtime_root_terminal_observation_v1(root);

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_ne!(outcome.payload(), 0);

        assert_eq!(
            outcome.payload(),
            ROOT_COMPLETION_DESTINATION.load(Ordering::Relaxed)
        );

        assert_eq!(ROOT_DESTROYED.load(Ordering::Relaxed), 1);

        assert_eq!(
            bray_runtime_root_completion_resolution_v1(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_root_completion_resolution_v1(root),
            NativeRuntimeStatus::UNKNOWN_TASK
        );

        assert_eq!(
            bray_runtime_task_allocation_v1().status(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(ROOT_DESTROYED.load(Ordering::Relaxed), 1);
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

        let outcome = bray_runtime_root_terminal_observation_v1(root);

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_eq!(SUSPENDED_RESUMES.load(Ordering::Relaxed), 2);

        assert_eq!(
            bray_runtime_root_completion_resolution_v1(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn host_cancellation_wakes_a_suspended_root_into_cancellation_entry() {
        CANCEL_ENTRIES.store(0, Ordering::Relaxed);

        let frame = NativeProtectedFrame::new(
            0,
            [8; 32],
            2,
            8,
            8,
            8,
            8,
            frame_state,
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
            bray_runtime_main_thread_lane_drive_v1(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            super::bray_runtime_root_cancellation_request_v1(root),
            NativeRuntimeStatus::SUCCESS
        );

        let outcome = bray_runtime_root_terminal_observation_v1(root);

        assert_eq!(outcome.state(), NativeRunState::CANCELLED);
        assert_eq!(CANCEL_ENTRIES.load(Ordering::Relaxed), 1);

        assert_eq!(
            bray_runtime_root_completion_resolution_v1(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn cleanup_callback_failures_are_owned_until_host_drain() {
        let frame = NativeProtectedFrame::new(
            0,
            [9; 32],
            1,
            8,
            8,
            8,
            8,
            frame_state,
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
            bray_runtime_root_terminal_observation_v1(root).state(),
            NativeRunState::COMPLETED
        );

        let pending = super::with_runtime(|runtime| runtime.pending_cleanup_incidents())
            .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"));

        assert_eq!(pending, 3);

        assert_eq!(
            super::bray_runtime_cleanup_incident_reporting_v1(),
            NativeRuntimeStatus::SUCCESS
        );

        let pending = super::with_runtime(|runtime| runtime.pending_cleanup_incidents())
            .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"));

        assert_eq!(pending, 0);

        assert_eq!(
            bray_runtime_root_completion_resolution_v1(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
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
            [10; 32],
            2,
            8,
            8,
            8,
            8,
            frame_state,
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
            bray_runtime_root_terminal_observation_v1(root).state(),
            NativeRunState::RUNTIME_FAILURE
        );

        assert_eq!(FAILURE_RESUMES.load(Ordering::Relaxed), 2);
        assert_eq!(FAILURE_BROADCASTS.load(Ordering::Relaxed), 1);
        assert_eq!(FAILURE_RESOLUTIONS.load(Ordering::Relaxed), 1);
        assert_eq!(FAILURE_DESTRUCTIONS.load(Ordering::Relaxed), 1);

        assert_eq!(
            bray_runtime_root_completion_resolution_v1(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn native_runtime_boundary_starts_executes_and_shuts_down() {
        assert_eq!(
            bray_runtime_main_thread_lane_startup_v1(NativeRuntimeConfiguration::new(8, 8)),
            NativeRuntimeStatus::SUCCESS
        );

        let allocation = bray_runtime_task_allocation_v1();

        let Some(task) = allocation.task() else {
            panic!("native frame must allocate");
        };

        assert_eq!(start_test_task(task, frame()), NativeRuntimeStatus::SUCCESS);

        assert_eq!(
            bray_runtime_main_thread_lane_drive_v1(),
            NativeRuntimeStatus::SUCCESS
        );

        let outcome = bray_runtime_join_registration_v1(task, ignore_wake, 0);

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_ne!(outcome.payload(), 0);

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(DESTROYED.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn native_runtime_boundary_validates_state_and_capacities() {
        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::NOT_INITIALIZED
        );

        assert_eq!(
            bray_runtime_main_thread_lane_startup_v1(NativeRuntimeConfiguration::new(0, 1)),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            bray_runtime_main_thread_lane_startup_v1(NativeRuntimeConfiguration::new(1, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_task_allocation_v1().status(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_task_allocation_v1().status(),
            NativeRuntimeStatus::RUNTIME_FAILURE
        );

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn task_start_transfers_each_frame_exactly_once() {
        assert_eq!(
            bray_runtime_main_thread_lane_startup_v1(NativeRuntimeConfiguration::new(1, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        let allocation = bray_runtime_task_allocation_v1();

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

        assert_eq!(
            start_test_task(
                task,
                protected_frame(8, resume_frame, ignore_completion_move, started_destroy,)
            ),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_main_thread_lane_drive_v1(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_join_registration_v1(task, ignore_wake, 0).state(),
            NativeRunState::COMPLETED
        );

        assert_eq!(STARTED_DESTROYED.load(Ordering::Relaxed), 1);

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn frame_contract_failures_are_not_published_as_panics() {
        assert_eq!(
            bray_runtime_main_thread_lane_startup_v1(NativeRuntimeConfiguration::new(2, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        for resume in [
            resume_runtime_failure as extern "C-unwind" fn(usize) -> NativeFrameProgress,
            resume_unknown_progress,
        ] {
            let allocation = bray_runtime_task_allocation_v1();

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
                bray_runtime_main_thread_lane_drive_v1(),
                NativeRuntimeStatus::RUNTIME_FAILURE
            );

            assert_eq!(
                bray_runtime_join_registration_v1(task, ignore_wake, 0).state(),
                NativeRunState::RUNTIME_FAILURE
            );
        }

        assert_eq!(
            bray_runtime_structured_shutdown_v1(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    fn frame() -> NativeProtectedFrame {
        protected_frame(8, resume_frame, ignore_completion_move, destroy_frame)
    }

    fn execute_test_root(
        frame: NativeProtectedFrame,
        configuration: NativeRuntimeConfiguration,
    ) -> super::NativeRootStart {
        let transfer = NativeProtectedFrameTransfer::new(&frame);

        bray_runtime_root_execution_v1(transfer, configuration)
    }

    fn start_test_task(
        task: super::NativeTaskHandle,
        frame: NativeProtectedFrame,
    ) -> NativeRuntimeStatus {
        let transfer = NativeProtectedFrameTransfer::new(&frame);

        bray_runtime_task_start_v1(task, transfer)
    }

    #[test]
    fn synchronous_root_boundary_catches_reports_and_resolves_panic() {
        let outcome = bray_runtime_synchronous_root_execution_v1(propagate_test_panic, 0);

        assert_eq!(outcome.state(), NativeRunState::PANICKED);
        assert_ne!(outcome.payload(), 0);

        assert_eq!(
            super::bray_runtime_panic_reporting_v1(outcome.payload()),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn synchronous_root_boundary_catches_current_run_cancellation() {
        let outcome = bray_runtime_synchronous_root_execution_v1(propagate_test_cancellation, 0);

        assert_eq!(outcome.state(), NativeRunState::CANCELLED);
        assert_eq!(outcome.payload(), 0);
    }

    #[test]
    fn foreign_callback_boundary_attaches_threads_for_the_callback_lifetime() {
        let (outcome, attached_after_return) = std::thread::spawn(|| {
            assert!(bray_platform::current_runtime_thread().is_none());

            let outcome =
                bray_runtime_foreign_callback_execution_v1(assert_callback_runtime_thread, 41);

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

        let outcome =
            bray_runtime_foreign_callback_execution_v1(assert_callback_runtime_thread, 41);

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_eq!(outcome.payload(), 41);
    }

    #[test]
    fn entry_failure_reporting_borrows_the_complete_payload() {
        let payload = 42_i32;

        assert_eq!(
            super::bray_runtime_entry_failure_reporting_v1(
                (&raw const payload).addr(),
                size_of::<i32>(),
            ),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            super::bray_runtime_entry_failure_reporting_v1(0, size_of::<i32>()),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );
    }

    fn protected_frame(
        alignment: usize,
        resume: extern "C-unwind" fn(usize) -> NativeFrameProgress,
        move_completion: extern "C-unwind" fn(usize, usize),
        destroy: extern "C-unwind" fn(usize),
    ) -> NativeProtectedFrame {
        NativeProtectedFrame::new(
            0,
            [7; 32],
            2,
            8,
            alignment,
            8,
            8,
            frame_state,
            resume,
            cancel_frame,
            ignore_action,
            ignore_resolution,
            move_completion,
            destroy,
        )
    }

    extern "C" fn frame_state(_: usize, _: u32) -> NativeFrameState {
        NativeFrameState::new(
            NativeFrameAffinity::MAIN_THREAD,
            NativeLaneRequirements::MAIN_THREAD,
        )
    }

    extern "C-unwind" fn resume_frame(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 17)
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

    extern "C-unwind" fn propagate_test_panic(_: usize) {
        const MESSAGE: &[u8] = b"synchronous root panic";

        let report = super::bray_runtime_panic_report_construction_v1(
            NativePanicCause::MESSAGE,
            NativeSourceAnchor::new(0, 0, 1, 0),
            NativeStringView::new(MESSAGE.as_ptr(), MESSAGE.len()),
        );

        super::bray_runtime_panic_propagation_v1(report)
    }

    extern "C-unwind" fn propagate_test_cancellation(_: usize) {
        super::bray_runtime_current_run_cancellation_propagation_v1()
    }

    extern "C-unwind" fn assert_callback_runtime_thread(destination: usize) {
        assert_eq!(destination, 41);
        assert!(bray_platform::current_runtime_thread().is_some());
    }

    extern "C-unwind" fn suspend_and_self_wake(_: usize) -> NativeFrameProgress {
        let resume = SUSPENDED_RESUMES.fetch_add(1, Ordering::Relaxed);

        if resume == 0 {
            let raw = SUSPENDED_ROOT.load(Ordering::Relaxed);

            let raw = u64::try_from(raw).unwrap_or_else(|_| panic!("test root must fit u64"));

            let task = bray_runtime_abi::NativeTaskHandle::new(raw)
                .unwrap_or_else(|| panic!("test root task must be nonzero"));

            assert_eq!(
                super::bray_runtime_wake_v1(task, 1),
                NativeRuntimeStatus::SUCCESS
            );

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

            let task = bray_runtime_abi::NativeTaskHandle::new(raw)
                .unwrap_or_else(|| panic!("test root task must be nonzero"));

            assert_eq!(
                super::bray_runtime_wake_v1(task, 1),
                NativeRuntimeStatus::SUCCESS
            );

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
