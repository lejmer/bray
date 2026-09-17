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
    pub extern "C" fn resident_thread_attachment_identity(
        descriptor: &'static NativeProductHostDescriptor,
    ) -> u64 {
        catch_unwind(AssertUnwindSafe(|| {
            crate::product::thread_attachment_identity(descriptor)
        }))
        .unwrap_or(0)
    }
}

native_export! {
    pub extern "C" fn resident_thread_static_cleanup_registration(
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
    pub extern "C" fn bray_runtime_wake(task: NativeTaskHandle) -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.wake(task))
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
    let mut frame = super::frame::NativeFrameTransfer::new(frame);
    let status = initialize_for_execution(configuration);

    if !status.is_success() {
        return NativeRootStart::failure(status);
    }

    let allocation =
        with_runtime(|runtime| runtime.allocate()).unwrap_or_else(NativeTaskAllocation::failure);

    let Some(task) = allocation.task() else {
        return NativeRootStart::failure(allocation.status());
    };

    let status =
        with_runtime(|runtime| runtime.start(task, frame.take())).unwrap_or_else(|status| status);

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
        bray_runtime_foreign_callback_execution, bray_runtime_synchronous_root_execution,
    };

    use super::{
        bray_runtime_awaited_frame_composition, bray_runtime_frame_completion_move,
        bray_runtime_join_registration, bray_runtime_main_thread_lane_drive,
        bray_runtime_main_thread_lane_startup, bray_runtime_root_completion_resolution,
        bray_runtime_root_execution, bray_runtime_root_terminal_observation,
        bray_runtime_structured_shutdown, bray_runtime_suspension_registration,
        bray_runtime_task_allocation, bray_runtime_task_destruction,
        bray_runtime_task_event_creation, bray_runtime_task_event_destruction,
        bray_runtime_task_event_signal, bray_runtime_task_start, bray_runtime_wake,
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
    static EVENT_CHILD_RESUMES: AtomicUsize = AtomicUsize::new(0);
    static EVENT_PARENT_RESUMES: AtomicUsize = AtomicUsize::new(0);
    static PENDING_CHILD_RESUMES: AtomicUsize = AtomicUsize::new(0);
    static PENDING_CHILD_BROADCASTS: AtomicUsize = AtomicUsize::new(0);
    static PENDING_CHILD_DESTRUCTIONS: AtomicUsize = AtomicUsize::new(0);
    static PENDING_CHILD_CLEANUP_THREAD: AtomicUsize = AtomicUsize::new(0);
    static CONFLICTING_CHILD_DESTRUCTIONS: AtomicUsize = AtomicUsize::new(0);
    static REJECTED_CHILD_MOVES: AtomicUsize = AtomicUsize::new(0);
    static REJECTED_CHILD_DESTRUCTIONS: AtomicUsize = AtomicUsize::new(0);
    static EXACT_BLOCKING_PARENT_RESUMES: AtomicUsize = AtomicUsize::new(0);
    static ORIGIN_PARENT_RESUMES: AtomicUsize = AtomicUsize::new(0);
    static ORIGIN_CHILD_RESUMES: AtomicUsize = AtomicUsize::new(0);
    const DEEP_AWAIT_DEPTH: usize = 256;
    static DEEP_AWAIT_STATES: [AtomicUsize; DEEP_AWAIT_DEPTH] =
        [const { AtomicUsize::new(0) }; DEEP_AWAIT_DEPTH];
    static DEEP_AWAIT_CALLBACK_DEPTH: AtomicUsize = AtomicUsize::new(0);
    static DEEP_AWAIT_MAX_CALLBACK_DEPTH: AtomicUsize = AtomicUsize::new(0);
    static DEEP_AWAIT_COMPLETIONS: AtomicUsize = AtomicUsize::new(0);
    static DEEP_AWAIT_COMPETITOR_AT: AtomicUsize = AtomicUsize::new(usize::MAX);
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
            NativeRuntimeConfiguration::new(1, 1),
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
            super::with_runtime(|runtime| runtime.scheduler.task_count())
                .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"))
                .unwrap_or_else(|error| panic!("scheduler observation must succeed: {error:?}")),
            1
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
    fn main_thread_parent_drives_a_blocking_child_on_its_exact_lane() {
        EXACT_BLOCKING_PARENT_RESUMES.store(0, Ordering::Relaxed);

        assert_eq!(
            bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(1, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        let allocation = bray_runtime_task_allocation();

        let task = allocation
            .task()
            .unwrap_or_else(|| panic!("exact-lane parent must allocate"));

        assert_eq!(
            start_test_task(
                task,
                protected_frame(
                    8,
                    await_exact_blocking_child,
                    ignore_completion_move,
                    ignore_action
                )
            ),
            NativeRuntimeStatus::SUCCESS
        );

        let mut outcome = NativeRunOutcome::new(NativeRunState::PENDING, 0);

        for _ in 0..8 {
            assert_ne!(
                bray_runtime_main_thread_lane_drive(),
                NativeRuntimeStatus::RUNTIME_FAILURE
            );

            outcome = super::with_runtime(|runtime| runtime.observe(task))
                .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"));

            if outcome.state() != NativeRunState::PENDING {
                break;
            }
        }

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_eq!(EXACT_BLOCKING_PARENT_RESUMES.load(Ordering::Relaxed), 2);

        assert_eq!(
            super::with_runtime(|runtime| runtime.scheduler.task_count())
                .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"))
                .unwrap_or_else(|error| panic!("scheduler observation must succeed: {error:?}")),
            1
        );

        assert_eq!(
            bray_runtime_task_destruction(task),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn stale_wakes_recheck_event_and_awaited_readiness() {
        EVENT_CHILD_RESUMES.store(0, Ordering::Relaxed);
        EVENT_PARENT_RESUMES.store(0, Ordering::Relaxed);

        assert_eq!(
            bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(2, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        let event = bray_runtime_task_event_creation();
        let allocation = bray_runtime_task_allocation();

        let Some(task) = allocation.task() else {
            panic!("event-awaiting task must allocate");
        };

        assert_eq!(
            start_test_task(
                task,
                protected_frame_with_context(
                    event,
                    8,
                    frame_state,
                    await_event_child,
                    ignore_completion_move,
                    ignore_action,
                ),
            ),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_main_thread_lane_drive(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_main_thread_lane_drive(),
            NativeRuntimeStatus::PENDING
        );

        assert_eq!(EVENT_PARENT_RESUMES.load(Ordering::Relaxed), 1);
        assert_eq!(EVENT_CHILD_RESUMES.load(Ordering::Relaxed), 1);

        assert_eq!(bray_runtime_wake(task), NativeRuntimeStatus::SUCCESS);

        assert_eq!(
            bray_runtime_main_thread_lane_drive(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(EVENT_PARENT_RESUMES.load(Ordering::Relaxed), 1);

        let pending = super::with_runtime(|runtime| runtime.observe(task))
            .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"));

        assert_eq!(pending.state(), NativeRunState::PENDING);

        assert_eq!(
            bray_runtime_task_event_signal(event),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_main_thread_lane_drive(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_main_thread_lane_drive(),
            NativeRuntimeStatus::PENDING
        );

        assert_eq!(EVENT_CHILD_RESUMES.load(Ordering::Relaxed), 2);
        assert_eq!(EVENT_PARENT_RESUMES.load(Ordering::Relaxed), 2);

        let completed = super::with_runtime(|runtime| runtime.observe(task))
            .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"));

        assert_eq!(completed.state(), NativeRunState::COMPLETED);

        assert_eq!(
            bray_runtime_task_destruction(task),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_task_event_destruction(event),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn parent_cleanup_incident_uses_the_restored_parent_execution_origin() {
        ORIGIN_PARENT_RESUMES.store(0, Ordering::Relaxed);
        ORIGIN_CHILD_RESUMES.store(0, Ordering::Relaxed);

        assert_eq!(
            bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(2, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        let event = bray_runtime_task_event_creation();
        let allocation = bray_runtime_task_allocation();

        let task = allocation
            .task()
            .unwrap_or_else(|| panic!("origin parent must allocate"));

        assert_eq!(
            start_test_task(
                task,
                protected_frame_with_context(
                    event,
                    8,
                    frame_state,
                    await_origin_child,
                    ignore_completion_move,
                    panic_action,
                )
            ),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_main_thread_lane_drive(),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_task_event_signal(event),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_main_thread_lane_drive(),
            NativeRuntimeStatus::SUCCESS
        );

        let completed = super::with_runtime(|runtime| runtime.observe(task))
            .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"));

        assert_eq!(completed.state(), NativeRunState::COMPLETED);

        let mut origin = None;

        super::with_runtime(|runtime| {
            runtime.cleanup_reports.drain(|incident| {
                assert!(origin.replace(incident.origin()).is_none());
            });
        })
        .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"));

        let origin = origin.unwrap_or_else(|| panic!("parent cleanup must publish one incident"));

        assert_eq!(
            origin.frame(),
            bray_runtime_model::ProtectedAsyncFrameId::new([7; 32])
        );

        assert_eq!(
            origin.state(),
            bray_runtime_model::ProtectedFrameStateId::new(1)
        );

        assert_eq!(
            bray_runtime_task_destruction(task),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_task_event_destruction(event),
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
    fn failed_root_admission_destroys_the_transferred_frame() {
        static DESTROYED: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn destroy(_: usize) {
            DESTROYED.fetch_add(1, Ordering::Relaxed);
        }

        let _isolation = super::super::state::test_runtime_isolation();
        let configuration = NativeRuntimeConfiguration::new(1, 1);

        assert!(bray_runtime_main_thread_lane_startup(configuration).is_success());

        let failure = crate::outgoing::tests::reject_admission();

        let start = execute_test_root(
            protected_frame(8, resume_frame, ignore_completion_move, destroy),
            configuration,
        );

        assert_eq!(start.status(), NativeRuntimeStatus::RUNTIME_FAILURE);
        assert!(start.root().is_none());
        assert_eq!(DESTROYED.load(Ordering::Relaxed), 1);

        drop(failure);

        assert!(bray_runtime_structured_shutdown().is_success());
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
        let mut outcome = bray_runtime_synchronous_root_execution(propagate_test_panic, 0);

        assert_eq!(outcome.state(), NativeRunState::PANICKED);
        assert_eq!(outcome.payload(), 0);

        assert_eq!(
            outcome.take_report().consume(true),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn synchronous_root_boundary_catches_current_run_cancellation() {
        let outcome =
            bray_runtime_synchronous_root_execution(propagate_test_cancellation, 0);

        assert_eq!(outcome.state(), NativeRunState::CANCELLED);
        assert_eq!(outcome.payload(), 0);
    }

    #[test]
    fn foreign_callback_boundary_attaches_threads_for_the_callback_lifetime() {
        let (outcome, attached_after_return) = std::thread::spawn(|| {
            assert!(bray_platform::current_runtime_thread().is_none());

            let outcome =
                bray_runtime_foreign_callback_execution(assert_callback_runtime_thread, 41);

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

        let outcome = bray_runtime_foreign_callback_execution(assert_callback_runtime_thread, 41);

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_eq!(outcome.payload(), 41);
    }

    #[test]
    fn parent_failure_cleans_a_suspended_child_and_preserves_its_incident() {
        PENDING_CHILD_RESUMES.store(0, Ordering::Relaxed);
        PENDING_CHILD_BROADCASTS.store(0, Ordering::Relaxed);
        PENDING_CHILD_DESTRUCTIONS.store(0, Ordering::Relaxed);
        PENDING_CHILD_CLEANUP_THREAD.store(0, Ordering::Relaxed);

        let start = execute_test_root(
            protected_frame_with_state(
                8,
                movable_frame_state,
                compose_child_then_panic,
                ignore_completion_move,
                ignore_action,
            ),
            NativeRuntimeConfiguration::new(1, 1),
        );

        let root = start
            .root()
            .unwrap_or_else(|| panic!("parent frame must transfer"));

        let mut outcome = bray_runtime_root_terminal_observation(root);

        assert_eq!(outcome.state(), NativeRunState::PANICKED);
        assert_eq!(PENDING_CHILD_RESUMES.load(Ordering::Relaxed), 0);
        assert_eq!(PENDING_CHILD_BROADCASTS.load(Ordering::Relaxed), 1);
        assert_eq!(PENDING_CHILD_DESTRUCTIONS.load(Ordering::Relaxed), 1);

        assert_eq!(
            PENDING_CHILD_CLEANUP_THREAD.load(Ordering::Relaxed),
            bray_platform::main_runtime_thread()
                .unwrap_or_else(|| panic!("test runtime must retain its main thread"))
                .id()
                .raw() as usize
        );

        assert_eq!(
            super::with_runtime(|runtime| runtime.pending_cleanup_incidents()),
            Ok(1)
        );

        assert_eq!(
            super::with_runtime(|runtime| runtime.discard_cleanup_incidents()),
            Ok(1)
        );

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::SUCCESS
        );

        let mut report = outcome.take_report();
        assert_eq!(report.consume(false), NativeRuntimeStatus::SUCCESS);

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn conflicting_composed_child_cleans_up_on_its_own_workload_lane() {
        CONFLICTING_CHILD_DESTRUCTIONS.store(0, Ordering::Relaxed);

        let start = execute_test_root(
            protected_frame_with_state(
                8,
                movable_blocking_frame_state,
                reject_conflicting_child,
                ignore_completion_move,
                ignore_action,
            ),
            NativeRuntimeConfiguration::new(2, 1),
        );

        let root = start
            .root()
            .unwrap_or_else(|| panic!("conflicting parent frame must transfer"));

        let mut outcome = bray_runtime_root_terminal_observation(root);

        assert_eq!(outcome.state(), NativeRunState::PANICKED);
        assert_eq!(CONFLICTING_CHILD_DESTRUCTIONS.load(Ordering::Relaxed), 1);

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            outcome.take_report().consume(false),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn rejected_child_admission_leaves_the_inactive_frame_with_its_caller() {
        REJECTED_CHILD_MOVES.store(0, Ordering::Relaxed);
        REJECTED_CHILD_DESTRUCTIONS.store(0, Ordering::Relaxed);

        let start = execute_test_root(
            protected_frame(
                8,
                reject_then_compose_child,
                ignore_completion_move,
                ignore_action,
            ),
            NativeRuntimeConfiguration::new(1, 1),
        );

        let root = start
            .root()
            .unwrap_or_else(|| panic!("parent frame must transfer"));

        assert_eq!(
            bray_runtime_root_terminal_observation(root).state(),
            NativeRunState::RUNTIME_FAILURE
        );

        assert_eq!(REJECTED_CHILD_MOVES.load(Ordering::Relaxed), 1);
        assert_eq!(REJECTED_CHILD_DESTRUCTIONS.load(Ordering::Relaxed), 1);

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
    fn deep_direct_awaits_are_iterative_and_yield_to_ready_work() {
        for state in &DEEP_AWAIT_STATES {
            state.store(0, Ordering::Relaxed);
        }

        DEEP_AWAIT_CALLBACK_DEPTH.store(0, Ordering::Relaxed);
        DEEP_AWAIT_MAX_CALLBACK_DEPTH.store(0, Ordering::Relaxed);
        DEEP_AWAIT_COMPLETIONS.store(0, Ordering::Relaxed);
        DEEP_AWAIT_COMPETITOR_AT.store(usize::MAX, Ordering::Relaxed);

        assert_eq!(
            bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(2, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        let root_allocation = bray_runtime_task_allocation();
        let competitor_allocation = bray_runtime_task_allocation();

        let root = root_allocation
            .task()
            .unwrap_or_else(|| panic!("deep-await root must allocate"));

        let competitor = competitor_allocation
            .task()
            .unwrap_or_else(|| panic!("competitor must allocate"));

        assert_eq!(
            start_test_task(
                root,
                deep_await_frame(1, resume_deep_await, record_deep_completion)
            ),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            start_test_task(
                competitor,
                protected_frame(
                    8,
                    run_deep_await_competitor,
                    ignore_completion_move,
                    ignore_action
                )
            ),
            NativeRuntimeStatus::SUCCESS
        );

        loop {
            let outcome = super::with_runtime(|runtime| runtime.observe(root))
                .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"));

            if outcome.state() != NativeRunState::PENDING {
                assert_eq!(outcome.state(), NativeRunState::COMPLETED);
                break;
            }

            let status = bray_runtime_main_thread_lane_drive();

            assert!(matches!(
                status,
                NativeRuntimeStatus::SUCCESS | NativeRuntimeStatus::PENDING
            ));
        }

        assert_eq!(
            DEEP_AWAIT_COMPLETIONS.load(Ordering::Relaxed),
            DEEP_AWAIT_DEPTH
        );

        assert_eq!(DEEP_AWAIT_MAX_CALLBACK_DEPTH.load(Ordering::Relaxed), 1);
        assert!(DEEP_AWAIT_COMPETITOR_AT.load(Ordering::Relaxed) < DEEP_AWAIT_DEPTH);

        assert_eq!(
            super::with_runtime(|runtime| runtime.scheduler.task_count())
                .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"))
                .unwrap_or_else(|error| panic!("scheduler observation must succeed: {error:?}")),
            2
        );

        assert_eq!(
            super::with_runtime(|runtime| runtime.observe(competitor))
                .unwrap_or_else(|status| panic!("runtime must remain available: {status:?}"))
                .state(),
            NativeRunState::COMPLETED
        );

        assert_eq!(
            bray_runtime_task_destruction(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_task_destruction(competitor),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(
            bray_runtime_structured_shutdown(),
            NativeRuntimeStatus::SUCCESS
        );
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
        protected_frame_with_context(0, alignment, state, resume, move_completion, destroy)
    }

    fn protected_frame_with_context(
        context: usize,
        alignment: usize,
        state: extern "C" fn(usize, u32) -> NativeFrameState,
        resume: extern "C-unwind" fn(&mut NativeFrameProgress, usize),
        move_completion: extern "C-unwind" fn(usize, usize),
        destroy: extern "C-unwind" fn(usize),
    ) -> NativeProtectedFrame {
        NativeProtectedFrame::new(
            context,
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

    extern "C" fn movable_compute_frame_state(_: usize, _: u32) -> NativeFrameState {
        NativeFrameState::new(
            NativeFrameAffinity::MOVABLE,
            NativeLaneRequirements::COMPUTE,
        )
    }

    extern "C-unwind" fn compose_child_then_panic(_: &mut NativeFrameProgress, _: usize) {
        bray_runtime_awaited_frame_composition(NativeInactiveFrame::new(0, move_pending_child));
        panic!("parent failed before publishing its awaited suspension");
    }

    extern "C-unwind" fn reject_conflicting_child(destination: &mut NativeFrameProgress, _: usize) {
        bray_runtime_awaited_frame_composition(NativeInactiveFrame::new(0, move_conflicting_child));

        *destination = NativeFrameProgress::new(NativeFrameProgressKind::RUNTIME_FAILURE, 0, 0);
    }

    extern "C-unwind" fn await_exact_blocking_child(
        destination: &mut NativeFrameProgress,
        _: usize,
    ) {
        let resumes = EXACT_BLOCKING_PARENT_RESUMES.fetch_add(1, Ordering::Relaxed);

        *destination = if resumes == 0 {
            bray_runtime_awaited_frame_composition(NativeInactiveFrame::new(
                0,
                move_blocking_child,
            ));

            bray_runtime_suspension_registration(1)
        } else {
            let _ = bray_runtime_frame_completion_move();

            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 17)
        };
    }

    extern "C-unwind" fn await_origin_child(destination: &mut NativeFrameProgress, event: usize) {
        let resumes = ORIGIN_PARENT_RESUMES.fetch_add(1, Ordering::Relaxed);

        *destination = if resumes == 0 {
            bray_runtime_awaited_frame_composition(NativeInactiveFrame::new(
                event,
                move_origin_child,
            ));

            bray_runtime_suspension_registration(1)
        } else {
            let _ = bray_runtime_frame_completion_move();

            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 23)
        };
    }

    extern "C-unwind" fn reject_then_compose_child(
        destination: &mut NativeFrameProgress,
        _: usize,
    ) {
        let rejection = crate::outgoing::tests::reject_admission();

        let rejected = super::with_runtime(|runtime| {
            runtime.compose_awaited(NativeInactiveFrame::new(0, move_rejected_child))
        })
        .unwrap_or_else(|status| status);

        assert_eq!(rejected, NativeRuntimeStatus::RUNTIME_FAILURE);
        assert_eq!(REJECTED_CHILD_MOVES.load(Ordering::Relaxed), 0);
        drop(rejection);

        let accepted = super::with_runtime(|runtime| {
            runtime.compose_awaited(NativeInactiveFrame::new(0, move_rejected_child))
        })
        .unwrap_or_else(|status| status);

        assert_eq!(accepted, NativeRuntimeStatus::SUCCESS);
        *destination = NativeFrameProgress::new(NativeFrameProgressKind::RUNTIME_FAILURE, 0, 0);
    }

    struct DeepAwaitCallback;

    impl DeepAwaitCallback {
        fn enter() -> Self {
            let depth = DEEP_AWAIT_CALLBACK_DEPTH.fetch_add(1, Ordering::Relaxed) + 1;
            DEEP_AWAIT_MAX_CALLBACK_DEPTH.fetch_max(depth, Ordering::Relaxed);

            Self
        }
    }

    impl Drop for DeepAwaitCallback {
        fn drop(&mut self) {
            DEEP_AWAIT_CALLBACK_DEPTH.fetch_sub(1, Ordering::Relaxed);
        }
    }

    fn deep_await_frame(
        depth: usize,
        resume: extern "C-unwind" fn(&mut NativeFrameProgress, usize),
        move_completion: extern "C-unwind" fn(usize, usize),
    ) -> NativeProtectedFrame {
        NativeProtectedFrame::new(
            depth,
            [33; 32],
            2,
            8,
            8,
            8,
            8,
            frame_state,
            resume,
            resume,
            ignore_action,
            ignore_resolution,
            move_completion,
            ignore_action,
        )
    }

    extern "C-unwind" fn resume_deep_await(destination: &mut NativeFrameProgress, depth: usize) {
        let _callback = DeepAwaitCallback::enter();
        let state = &DEEP_AWAIT_STATES[depth - 1];

        *destination = if state.load(Ordering::Relaxed) == 0 && depth < DEEP_AWAIT_DEPTH {
            state.store(1, Ordering::Relaxed);

            bray_runtime_awaited_frame_composition(NativeInactiveFrame::new(
                depth + 1,
                move_deep_await_child,
            ));

            NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0)
        } else {
            if depth < DEEP_AWAIT_DEPTH {
                let _ = bray_runtime_frame_completion_move();
            }

            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
        };
    }

    extern "C" fn move_deep_await_child(depth: usize) -> NativeProtectedFrame {
        deep_await_frame(depth, resume_deep_await, record_deep_completion)
    }

    extern "C-unwind" fn record_deep_completion(_: usize, _: usize) {
        DEEP_AWAIT_COMPLETIONS.fetch_add(1, Ordering::Relaxed);
    }

    extern "C-unwind" fn run_deep_await_competitor(
        destination: &mut NativeFrameProgress,
        _: usize,
    ) {
        DEEP_AWAIT_COMPETITOR_AT.store(
            DEEP_AWAIT_COMPLETIONS.load(Ordering::Relaxed),
            Ordering::Relaxed,
        );

        *destination = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
    }

    extern "C" fn move_rejected_child(_: usize) -> NativeProtectedFrame {
        REJECTED_CHILD_MOVES.fetch_add(1, Ordering::Relaxed);

        NativeProtectedFrame::new(
            0,
            [32; 32],
            1,
            8,
            8,
            8,
            8,
            movable_frame_state,
            resume_pending_child,
            resume_pending_child,
            ignore_action,
            ignore_resolution,
            ignore_completion_move,
            destroy_rejected_child,
        )
    }

    extern "C-unwind" fn destroy_rejected_child(_: usize) {
        REJECTED_CHILD_DESTRUCTIONS.fetch_add(1, Ordering::Relaxed);
    }

    extern "C" fn move_pending_child(_: usize) -> NativeProtectedFrame {
        NativeProtectedFrame::new(
            0,
            [31; 32],
            1,
            8,
            8,
            8,
            8,
            frame_state,
            resume_pending_child,
            resume_pending_child,
            panic_while_broadcasting_child,
            ignore_resolution,
            ignore_completion_move,
            destroy_pending_child,
        )
    }

    extern "C-unwind" fn resume_pending_child(destination: &mut NativeFrameProgress, _: usize) {
        PENDING_CHILD_RESUMES.fetch_add(1, Ordering::Relaxed);
        *destination = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
    }

    extern "C-unwind" fn panic_while_broadcasting_child(_: usize) {
        PENDING_CHILD_BROADCASTS.fetch_add(1, Ordering::Relaxed);

        PENDING_CHILD_CLEANUP_THREAD.store(
            bray_platform::current_runtime_thread()
                .unwrap_or_else(|| panic!("cleanup callback must run on a runtime thread"))
                .id()
                .raw() as usize,
            Ordering::Relaxed,
        );

        panic!("suspended child broadcast failed");
    }

    extern "C" fn move_origin_child(event: usize) -> NativeProtectedFrame {
        NativeProtectedFrame::new(
            event,
            [31; 32],
            2,
            8,
            8,
            8,
            8,
            frame_state,
            wait_for_origin_event,
            cancel_frame,
            ignore_action,
            ignore_resolution,
            ignore_completion_move,
            ignore_action,
        )
    }

    extern "C-unwind" fn wait_for_origin_event(
        destination: &mut NativeFrameProgress,
        event: usize,
    ) {
        let resumes = ORIGIN_CHILD_RESUMES.fetch_add(1, Ordering::Relaxed);

        *destination = if resumes == 0 {
            NativeFrameProgress::new(NativeFrameProgressKind::TASK_EVENT, 1, event)
        } else {
            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 17)
        };
    }

    extern "C-unwind" fn destroy_pending_child(_: usize) {
        PENDING_CHILD_DESTRUCTIONS.fetch_add(1, Ordering::Relaxed);
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

    extern "C-unwind" fn await_event_child(destination: &mut NativeFrameProgress, event: usize) {
        let resumes = EVENT_PARENT_RESUMES.fetch_add(1, Ordering::Relaxed);

        *destination = if resumes == 0 {
            bray_runtime_awaited_frame_composition(NativeInactiveFrame::new(
                event,
                move_event_child,
            ));

            bray_runtime_suspension_registration(1)
        } else {
            let _ = bray_runtime_frame_completion_move();

            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 19)
        };
    }

    extern "C" fn move_event_child(event: usize) -> NativeProtectedFrame {
        protected_frame_with_context(
            event,
            8,
            frame_state,
            wait_for_event,
            ignore_completion_move,
            ignore_action,
        )
    }

    extern "C-unwind" fn wait_for_event(destination: &mut NativeFrameProgress, event: usize) {
        let resumes = EVENT_CHILD_RESUMES.fetch_add(1, Ordering::Relaxed);

        *destination = if resumes == 0 {
            NativeFrameProgress::new(NativeFrameProgressKind::TASK_EVENT, 1, event)
        } else {
            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 17)
        };
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

    extern "C" fn move_conflicting_child(_: usize) -> NativeProtectedFrame {
        protected_frame_with_state(
            8,
            movable_compute_frame_state,
            resume_frame,
            ignore_completion_move,
            destroy_conflicting_child,
        )
    }

    extern "C-unwind" fn destroy_conflicting_child(_: usize) {
        assert_eq!(
            crate::context::current_task_execution_lane().map(crate::ExecutionLane::workload),
            Some(crate::ExecutionWorkload::Compute)
        );

        CONFLICTING_CHILD_DESTRUCTIONS.fetch_add(1, Ordering::Relaxed);
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

    extern "C-unwind" fn suspend_and_self_wake(destination: &mut NativeFrameProgress, _: usize) {
        let resume = SUSPENDED_RESUMES.fetch_add(1, Ordering::Relaxed);

        if resume == 0 {
            let raw = SUSPENDED_ROOT.load(Ordering::Relaxed);
            let raw = u64::try_from(raw).unwrap_or_else(|_| panic!("test root must fit u64"));

            let task = NativeTaskHandle::new(raw)
                .unwrap_or_else(|| panic!("test root task must be nonzero"));

            assert_eq!(super::bray_runtime_wake(task), NativeRuntimeStatus::SUCCESS);

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

            assert_eq!(super::bray_runtime_wake(task), NativeRuntimeStatus::SUCCESS);

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
        _: &mut NativeFrameProgress,
        _: usize,
        _: NativeFrameExit,
    ) {
    }

    extern "C-unwind" fn panic_resolution(
        _: &mut NativeFrameProgress,
        _: usize,
        _: NativeFrameExit,
    ) {
        panic!("cleanup resolution failed");
    }

    extern "C-unwind" fn record_failure_broadcast(_: usize) {
        FAILURE_BROADCASTS.fetch_add(1, Ordering::Relaxed);
    }

    extern "C-unwind" fn record_failure_resolution(
        _: &mut NativeFrameProgress,
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
