use std::mem::align_of;
use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{
    NativeExecutionLaneResult, NativeFrameProgress, NativeFrameProgressKind, NativeInactiveFrame,
    NativePanicCause, NativeProductHostDescriptor, NativeProductHostObservation,
    NativeProductHostOperation, NativeProtectedFrame, NativeProtectedFrameTransfer,
    NativeRootHandle, NativeRootStart, NativeRunOutcome, NativeRunResultLayout,
    NativeRuntimeConfiguration, NativeRuntimeEventCallback, NativeRuntimeStatus,
    NativeSourceAnchor, NativeTaskAllocation, NativeTaskHandle,
    NativeThreadStaticCleanupRegistration, NativeWakeCallback,
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
            .unwrap_or_else(|_| crate::product::host_failure())
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

        super::host::record_outcome(&outcome);

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

native_export! {
    pub extern "C" fn bray_runtime_panic_reporting(report: &mut bray_runtime_abi::NativePanicReport) -> NativeRuntimeStatus {
        report.consume(true)
    }
}

native_export! {
    pub extern "C" fn bray_runtime_panic_report_destruction(report: &mut bray_runtime_abi::NativePanicReport) -> NativeRuntimeStatus {
        report.consume(false)
    }
}

pub(crate) fn report_primary(
    primary: &bray_runtime_abi::NativePanicPrimary,
) -> NativeRuntimeStatus {
    if !primary.cause().is_known() || !primary.source().is_valid() {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    }

    let mut message = Vec::new();

    if message.try_reserve_exact(primary.message().len()).is_err() {
        report_panic(primary.cause(), primary.source(), String::new());

        return NativeRuntimeStatus::RUNTIME_FAILURE;
    }

    message.resize(primary.message().len(), 0);

    let status = primary.message().copy_to(0, &mut message);

    if !status.is_success() {
        return status;
    }

    let Ok(message) = String::from_utf8(message) else {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    };

    report_panic(primary.cause(), primary.source(), message);

    NativeRuntimeStatus::SUCCESS
}

pub(crate) fn report_panic(cause: NativePanicCause, source: NativeSourceAnchor, message: String) {
    if super::host::active() {
        super::host::record_panic(cause, source, message);
    } else {
        eprintln!("{message}");
    }
}

native_export! {
    #[expect(
        unsafe_code,
        reason = "the entry failure role borrows the validated native payload for this call"
    )]
    pub extern "C" fn bray_runtime_entry_failure_reporting(
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
    pub extern "C" fn bray_runtime_panic_propagation(report: &mut bray_runtime_abi::NativePanicReport) -> ! {
        report.consume(true);
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
            with_runtime(|runtime| runtime.start(task, frame.into_protected()))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_awaited_frame_composition(
        frame: NativeInactiveFrame,
    ) {
        let status = with_runtime(|runtime| runtime.compose_awaited(frame))
            .unwrap_or_else(|status| status);

        assert!(status.is_success(), "awaited-frame composition failed");
    }
}

native_export! {
    #[expect(
        unsafe_code,
        reason = "the factory copies the compiler-owned layout during this call"
    )]
    pub extern "C-unwind" fn bray_runtime_task_observation_creation(
        task: NativeTaskHandle,
        request_cancellation: u8,
        layout: *const NativeRunResultLayout,
        cancellation: Option<super::task_observation::NativeValueCleanupCallback>,
        lifecycle: Option<super::task_observation::NativeValueCleanupCallback>,
    ) -> NativeInactiveFrame {
        let layout = unsafe { layout.as_ref() }
            .copied()
            .unwrap_or_else(|| panic!("task observation layout is required"));

        super::task_observation::create(
            task,
            request_cancellation != 0,
            layout,
            cancellation,
            lifecycle,
        )
        .unwrap_or_else(|| panic!("task observation layout is invalid"))
    }
}

native_export! {
    #[expect(
        unsafe_code,
        reason = "the resolver borrows the compiler-owned layout only for this call"
    )]
    pub extern "C" fn bray_runtime_task_resolution(
        task: NativeTaskHandle,
        destination: *mut u8,
        layout: *const NativeRunResultLayout,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            let Some(layout) = (unsafe { layout.as_ref() }).copied() else {
                return NativeRuntimeStatus::INVALID_ARGUMENT;
            };
            let outcome = with_runtime(|runtime| runtime.resolve_task(task))
                .unwrap_or_else(runtime_failure);

            super::task_observation::transfer_outcome(outcome, destination.addr(), layout)
                .map_or_else(|status| status, |()| NativeRuntimeStatus::SUCCESS)
        })
    }
}

native_export! {
    pub extern "C" fn bray_runtime_task_destruction(
        task: NativeTaskHandle,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.destroy_task(task))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_frame_completion_move() -> usize {
        with_runtime(|runtime| runtime.resolve_awaited_completion())
            .and_then(|result| result)
            .unwrap_or_else(|_| panic!("awaited-frame completion resolution failed"))
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
    pub extern "C" fn bray_runtime_wake(
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
    let status = initialize_for_execution(configuration);

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
    use std::mem::size_of;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_abi::{
        NativeFrameAffinity, NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind,
        NativeFrameState, NativeInactiveFrame, NativeLaneRequirements, NativePanicCause,
        NativeProtectedFrame, NativeProtectedFrameTransfer, NativeRunOutcome, NativeRunState,
        NativeRuntimeConfiguration, NativeRuntimeStatus, NativeTaskHandle,
    };

    use super::super::callback::{
        bray_runtime_substrate_foreign_callback_execution,
        bray_runtime_substrate_synchronous_root_execution,
    };

    use super::{
        bray_runtime_awaited_frame_composition, bray_runtime_frame_completion_move,
        bray_runtime_join_registration, bray_runtime_main_thread_lane_drive,
        bray_runtime_main_thread_lane_startup, bray_runtime_root_completion_resolution,
        bray_runtime_root_execution, bray_runtime_root_terminal_observation,
        bray_runtime_structured_shutdown, bray_runtime_suspension_registration,
        bray_runtime_task_allocation, bray_runtime_task_start,
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
                movable_frame_state,
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
                movable_frame_state,
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
        for fail_completion in [false, true] {
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
                if fail_completion {
                    panic_completion_move
                } else {
                    ignore_completion_move
                },
                panic_action,
            );

            let start = execute_test_root(frame, NativeRuntimeConfiguration::new(2, 1));

            let Some(root) = start.root() else {
                panic!("cleanup-failing root must transfer");
            };

            let denied = crate::outgoing::tests::reject_admission();

            assert_eq!(
                bray_runtime_root_terminal_observation(root).state(),
                if fail_completion {
                    NativeRunState::RUNTIME_FAILURE
                } else {
                    NativeRunState::COMPLETED
                }
            );

            let pending = super::with_runtime(|runtime| runtime.pending_cleanup_incidents())
                .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"));

            assert_eq!(pending, if fail_completion { 4 } else { 3 });

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

            drop(denied);
        }
    }

    static FRAME_BRIDGE_RELEASES: AtomicUsize = AtomicUsize::new(0);

    struct FrameBridgeFailure {
        _failure: crate::outgoing::tests::AdmissionFailure,
    }

    impl Drop for FrameBridgeFailure {
        fn drop(&mut self) {
            assert_eq!(FRAME_BRIDGE_RELEASES.fetch_add(10, Ordering::Relaxed), 1);
        }
    }

    extern "C" fn release_frame_bridge_primary(_: usize, _: usize) {
        assert_eq!(FRAME_BRIDGE_RELEASES.fetch_add(1, Ordering::Relaxed), 0);
    }

    extern "C-unwind" fn write_frame_report_then_unwind(
        destination: &mut NativeFrameProgress,
        _: usize,
    ) {
        let report = crate::frame::native_report(bray_runtime_abi::NativePanicPrimary::new(
            bray_runtime_abi::NativePanicCause::ASSERTION,
            bray_runtime_abi::NativeSourceAnchor::unavailable(),
            bray_runtime_abi::NativePanicMessage::new(
                0,
                0,
                None,
                Some(release_frame_bridge_primary),
            ),
        ));

        *destination = NativeFrameProgress::panicked(report);
        let failure = crate::outgoing::tests::reject_admission();
        assert!(crate::outgoing::OutgoingRecords::admit(1).is_err());
        std::panic::panic_any(FrameBridgeFailure { _failure: failure });
    }

    extern "C-unwind" fn resolve_frame_report_then_unwind(
        destination: &mut NativeFrameProgress,
        context: usize,
        _: NativeFrameExit,
    ) {
        write_frame_report_then_unwind(destination, context);
    }

    #[test]
    fn frame_callbacks_retain_written_reports_before_unwind() {
        for phase in 0..3 {
            FRAME_BRIDGE_RELEASES.store(0, Ordering::Relaxed);

            let frame = NativeProtectedFrame::new(
                0,
                [7; 32],
                2,
                8,
                8,
                8,
                8,
                frame_state,
                if phase == 2 {
                    resume_runtime_failure
                } else {
                    write_frame_report_then_unwind
                },
                write_frame_report_then_unwind,
                ignore_action,
                if phase == 2 {
                    resolve_frame_report_then_unwind
                } else {
                    ignore_resolution
                },
                ignore_completion_move,
                ignore_action,
            );

            let start = execute_test_root(frame, NativeRuntimeConfiguration::new(2, 1));
            let root = start.root().expect("frame bridge root must transfer");

            if phase == 1 {
                assert_eq!(
                    super::bray_runtime_root_cancellation_request(root),
                    NativeRuntimeStatus::SUCCESS
                );
            }

            let mut outcome = bray_runtime_root_terminal_observation(root);

            assert_eq!(
                outcome.state(),
                if phase == 2 {
                    NativeRunState::RUNTIME_FAILURE
                } else {
                    NativeRunState::PANICKED
                }
            );

            assert_eq!(
                bray_runtime_root_completion_resolution(root),
                NativeRuntimeStatus::SUCCESS
            );

            assert_eq!(FRAME_BRIDGE_RELEASES.load(Ordering::Relaxed), 0);

            if phase == 2 {
                assert_eq!(
                    super::bray_runtime_cleanup_incident_reporting(),
                    NativeRuntimeStatus::SUCCESS
                );
            } else {
                let mut report = outcome.take_report();
                assert_eq!(report.consume(false), NativeRuntimeStatus::SUCCESS);
                assert_eq!(report.consume(false), NativeRuntimeStatus::SUCCESS);
            }

            assert_eq!(FRAME_BRIDGE_RELEASES.load(Ordering::Relaxed), 11);

            assert_eq!(
                bray_runtime_structured_shutdown(),
                NativeRuntimeStatus::SUCCESS
            );
        }
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

        assert_eq!(
            start_test_task(
                task,
                protected_frame(8, resume_frame, ignore_completion_move, started_destroy,)
            ),
            NativeRuntimeStatus::SUCCESS
        );

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
    fn failed_outgoing_admission_preserves_the_inactive_frame_for_retry() {
        static DESTROYED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

        extern "C-unwind" fn destroy(_: usize) {
            DESTROYED.fetch_add(1, Ordering::Relaxed);
        }

        assert!(
            bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(1, 1))
                .is_success()
        );

        let frame = protected_frame(8, resume_frame, ignore_completion_move, destroy);

        let inactive =
            NativeInactiveFrame::new(Box::into_raw(Box::new(frame)).addr(), move_test_frame);

        let failure = crate::outgoing::tests::reject_admission();

        let rejected = bray_runtime_task_allocation();

        assert_eq!(rejected.status(), NativeRuntimeStatus::RUNTIME_FAILURE);
        assert!(rejected.task().is_none());
        assert_eq!(DESTROYED.load(Ordering::Relaxed), 0);

        drop(failure);

        let task = bray_runtime_task_allocation().task().unwrap();

        assert!(bray_runtime_task_start(task, inactive).is_success());
        assert!(bray_runtime_main_thread_lane_drive().is_success());
        assert_eq!(DESTROYED.load(Ordering::Relaxed), 1);
        assert!(bray_runtime_structured_shutdown().is_success());
    }

    #[test]
    fn frame_contract_failures_are_not_published_as_panics() {
        assert_eq!(
            bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(2, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        for resume in [
            resume_runtime_failure as extern "C-unwind" fn(&mut NativeFrameProgress, usize),
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

    fn frame() -> NativeProtectedFrame {
        protected_frame(8, resume_frame, ignore_completion_move, destroy_frame)
    }

    fn execute_test_root(
        frame: NativeProtectedFrame,
        configuration: NativeRuntimeConfiguration,
    ) -> super::NativeRootStart {
        let transfer = NativeProtectedFrameTransfer::new(&frame);

        bray_runtime_root_execution(transfer, configuration)
    }

    fn start_test_task(task: NativeTaskHandle, frame: NativeProtectedFrame) -> NativeRuntimeStatus {
        bray_runtime_task_start(
            task,
            NativeInactiveFrame::new(Box::into_raw(Box::new(frame)).addr(), move_test_frame),
        )
    }

    #[expect(
        unsafe_code,
        reason = "the inactive test frame transfers its exact descriptor allocation"
    )]
    extern "C" fn move_test_frame(context: usize) -> NativeProtectedFrame {
        unsafe { *Box::from_raw(context as *mut NativeProtectedFrame) }
    }

    #[test]
    fn synchronous_root_boundary_catches_reports_and_resolves_panic() {
        let mut outcome = bray_runtime_substrate_synchronous_root_execution(
            propagate_test_panic,
            0,
            assert_attached_cleanup as *const (),
        );

        assert_eq!(outcome.state(), NativeRunState::PANICKED);
        assert_eq!(outcome.payload(), 0);

        assert_eq!(
            outcome.take_report().consume(true),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn synchronous_root_boundary_catches_current_run_cancellation() {
        let outcome = bray_runtime_substrate_synchronous_root_execution(
            propagate_test_cancellation,
            0,
            assert_attached_cleanup as *const (),
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
                assert_attached_cleanup as *const (),
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
            assert_attached_cleanup as *const (),
        );

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_eq!(outcome.payload(), 41);
    }

    #[test]
    fn entry_failure_reporting_borrows_the_complete_payload() {
        let payload = 42_i32;

        assert_eq!(
            super::bray_runtime_entry_failure_reporting(
                (&raw const payload).addr(),
                size_of::<i32>(),
            ),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            super::bray_runtime_entry_failure_reporting(0, size_of::<i32>()),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );
    }

    fn protected_frame(
        alignment: usize,
        resume: extern "C-unwind" fn(&mut NativeFrameProgress, usize),
        move_completion: extern "C-unwind" fn(usize, usize),
        destroy: extern "C-unwind" fn(usize),
    ) -> NativeProtectedFrame {
        protected_frame_with_state(alignment, frame_state, resume, move_completion, destroy)
    }

    fn protected_frame_with_state(
        alignment: usize,
        state: extern "C" fn(usize, u32) -> NativeFrameState,
        resume: extern "C-unwind" fn(&mut NativeFrameProgress, usize),
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
            state,
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

    extern "C" fn movable_frame_state(_: usize, _: u32) -> NativeFrameState {
        NativeFrameState::new(NativeFrameAffinity::MOVABLE, NativeLaneRequirements::NONE)
    }

    extern "C" fn movable_blocking_frame_state(_: usize, _: u32) -> NativeFrameState {
        NativeFrameState::new(
            NativeFrameAffinity::MOVABLE,
            NativeLaneRequirements::BLOCKING,
        )
    }

    extern "C-unwind" fn resume_frame(destination: &mut NativeFrameProgress, _: usize) {
        *destination = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 17);
    }

    extern "C-unwind" fn await_blocking_children(destination: &mut NativeFrameProgress, _: usize) {
        *destination = (|| {
            let completed = AWAITED_BLOCKING_COMPLETIONS.load(Ordering::Relaxed);

            if completed != 0 {
                let _ = bray_runtime_frame_completion_move();
            }

            if completed == AWAITED_BLOCKING_CHILD_COUNT {
                return NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 17);
            }

            AWAITED_BLOCKING_COMPLETIONS.store(completed + 1, Ordering::Relaxed);

            bray_runtime_awaited_frame_composition(NativeInactiveFrame::new(
                0,
                move_blocking_child,
            ));

            bray_runtime_suspension_registration(1)
        })();
    }

    extern "C" fn move_blocking_child(_: usize) -> NativeProtectedFrame {
        protected_frame_with_state(
            8,
            movable_blocking_frame_state,
            resume_frame,
            ignore_completion_move,
            ignore_action,
        )
    }

    extern "C-unwind" fn resume_runtime_failure(destination: &mut NativeFrameProgress, _: usize) {
        *destination = NativeFrameProgress::new(NativeFrameProgressKind::RUNTIME_FAILURE, 0, 0);
    }

    extern "C-unwind" fn resume_unknown_progress(destination: &mut NativeFrameProgress, _: usize) {
        *destination = NativeFrameProgress::new(NativeFrameProgressKind::from_code(u32::MAX), 0, 0);
    }

    extern "C-unwind" fn cancel_frame(destination: &mut NativeFrameProgress, _: usize) {
        *destination = NativeFrameProgress::new(NativeFrameProgressKind::CANCELLED, 0, 0);
    }

    extern "C-unwind" fn propagate_test_panic(_: usize, outcome: &mut NativeRunOutcome) {
        let report = crate::frame::native_report(bray_runtime_abi::NativePanicPrimary::new(
            NativePanicCause::MESSAGE,
            bray_runtime_abi::NativeSourceAnchor::new(0, 0, 1, 0),
            bray_runtime_abi::NativePanicMessage::empty(),
        ));

        *outcome = NativeRunOutcome::panicked(report);
    }

    extern "C-unwind" fn propagate_test_cancellation(_: usize, outcome: &mut NativeRunOutcome) {
        *outcome = NativeRunOutcome::new(NativeRunState::CANCELLED, 0);
    }

    extern "C-unwind" fn assert_callback_runtime_thread(
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

    extern "C-unwind" fn suspend_and_self_wake(destination: &mut NativeFrameProgress, _: usize) {
        let resume = SUSPENDED_RESUMES.fetch_add(1, Ordering::Relaxed);

        if resume == 0 {
            let raw = SUSPENDED_ROOT.load(Ordering::Relaxed);

            let raw = u64::try_from(raw).unwrap_or_else(|_| panic!("test root must fit u64"));

            let task = NativeTaskHandle::new(raw)
                .unwrap_or_else(|| panic!("test root task must be nonzero"));

            assert_eq!(
                super::bray_runtime_wake(task, 1),
                NativeRuntimeStatus::SUCCESS
            );

            *destination = NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0);
            return;
        }

        resume_frame(destination, 0)
    }

    extern "C-unwind" fn suspend_without_wake(destination: &mut NativeFrameProgress, _: usize) {
        *destination = NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0);
    }

    extern "C-unwind" fn suspend_then_fail(destination: &mut NativeFrameProgress, _: usize) {
        if FAILURE_RESUMES.fetch_add(1, Ordering::Relaxed) == 0 {
            let raw = FAILURE_ROOT.load(Ordering::Relaxed);

            let raw = u64::try_from(raw).unwrap_or_else(|_| panic!("test root must fit u64"));

            let task = NativeTaskHandle::new(raw)
                .unwrap_or_else(|| panic!("test root task must be nonzero"));

            assert_eq!(
                super::bray_runtime_wake(task, 1),
                NativeRuntimeStatus::SUCCESS
            );

            *destination = NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0);
            return;
        }

        resume_runtime_failure(destination, 0)
    }

    extern "C-unwind" fn record_cancel_entry(destination: &mut NativeFrameProgress, _: usize) {
        CANCEL_ENTRIES.fetch_add(1, Ordering::Relaxed);

        cancel_frame(destination, 0)
    }

    extern "C-unwind" fn ignore_action(_: usize) {}

    extern "C-unwind" fn panic_action(_: usize) {
        panic!("cleanup action failed");
    }

    extern "C" fn ignore_wake(_: usize) {}

    extern "C-unwind" fn ignore_resolution(
        _: &mut bray_runtime_abi::NativeFrameProgress,
        _: usize,
        _: NativeFrameExit,
    ) {
    }

    extern "C-unwind" fn panic_resolution(
        _: &mut bray_runtime_abi::NativeFrameProgress,
        _: usize,
        _: NativeFrameExit,
    ) {
        panic!("cleanup resolution failed");
    }

    extern "C-unwind" fn record_failure_broadcast(_: usize) {
        FAILURE_BROADCASTS.fetch_add(1, Ordering::Relaxed);
    }

    extern "C-unwind" fn record_failure_resolution(
        _: &mut bray_runtime_abi::NativeFrameProgress,
        _: usize,
        _: NativeFrameExit,
    ) {
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

    extern "C-unwind" fn panic_completion_move(_: usize, _: usize) {
        panic_action(0);
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
