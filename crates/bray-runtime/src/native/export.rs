use std::mem::align_of;
use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{
    NativeExecutionLaneResult, NativeFrameProgress, NativeFrameProgressKind, NativeInactiveFrame,
    NativeInactiveFrameTransfer, NativeProductExecution, NativeProductHostDescriptor,
    NativeRootHandle, NativeRootStart, NativeRunOutcome, NativeRunResultLayout,
    NativeRuntimeConfiguration, NativeRuntimeEventCallback, NativeRuntimeStatus,
    NativeTaskAllocation, NativeTaskHandle, NativeThreadStaticCleanupRegistration,
    NativeWakeCallback,
};

use crate::current_run_cancellation_requested;

use super::state::{initialize, runtime_failure, shutdown, with_runtime};

native_export! {
    #[cfg_attr(test, expect(unsafe_code, reason = "runtime tests link this substrate export to the Bray provider"))]
    #[cfg_attr(test, unsafe(no_mangle))]
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
    pub extern "C" fn bray_runtime_product_execution_provider(
        asynchronous: u32,
        output: &mut std::mem::MaybeUninit<NativeProductExecution>,
    ) -> NativeRuntimeStatus {
        contain_status(|| crate::product::acquire_execution(asynchronous, output))
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
        frame: NativeInactiveFrameTransfer,
        configuration: NativeRuntimeConfiguration,
    ) -> NativeRootStart {
        catch_unwind(AssertUnwindSafe(|| {
            super::state::with_root_execution_context(|| {
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
            })
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
    #[cfg_attr(test, expect(unsafe_code, reason = "runtime tests link this substrate export to the Bray provider"))]
    #[cfg_attr(test, unsafe(no_mangle))]
    pub extern "C" fn bray_runtime_panic_propagation(report: &mut bray_runtime_abi::NativePanicReport) -> ! {
        report.consume(true);
        std::process::abort()
    }
}

native_export! {
    pub extern "C" fn bray_runtime_cleanup_incident_reporting(
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            match with_runtime(|runtime| runtime.report_cleanup_incidents()) {
                Ok(status) => status,
                Err(NativeRuntimeStatus::NOT_INITIALIZED) => NativeRuntimeStatus::SUCCESS,
                Err(status) => status,
            }
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
            with_runtime(|runtime| runtime.start(task, &frame))
                .unwrap_or_else(|status| status)
        })
    }
}

native_export! {
    pub extern "C-unwind" fn bray_runtime_awaited_frame_composition(
        frame: NativeInactiveFrame,
        resume_state: u32,
        destination: usize,
        composition: u8,
    ) {
        let composition = bray_runtime_abi::NativeFrameComposition::from_code(composition)
            .expect("frame composition requires a valid ownership disposition");

        let status = with_runtime(|runtime| runtime.compose_awaited(&frame, resume_state, destination, composition))
            .unwrap_or_else(|status| status);

        assert!(status.is_success(), "awaited-frame composition failed");
    }
}

native_export! {
    #[expect(
        unsafe_code,
        reason = "the factory copies the compiler-owned layout during this call"
    )]
    #[expect(
        clippy::not_unsafe_ptr_arg_deref,
        reason = "native callers provide a valid compiler-owned layout for this call"
    )]
    pub extern "C-unwind" fn bray_runtime_task_observation_creation(
        task: NativeTaskHandle,
        request_cancellation: u8,
        layout: *const NativeRunResultLayout,
        cancellation: Option<bray_runtime_abi::NativeValueCleanupCallback>,
        lifecycle: Option<bray_runtime_abi::NativeValueCleanupCallback>,
        requirements: u32,
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
            requirements,
        )
        .unwrap_or_else(|| panic!("task observation layout is invalid"))
    }
}

native_export! {
    #[expect(
        unsafe_code,
        reason = "the resolver borrows the compiler-owned layout only for this call"
    )]
    #[expect(
        clippy::not_unsafe_ptr_arg_deref,
        reason = "native callers provide a valid compiler-owned layout while the task is resolved"
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
            with_runtime(|runtime| runtime.with_cleanup_driving(|| {
                let outcome = runtime.resolve_task(task);

                if let Err(status) = super::task_observation::transfer_outcome(outcome, destination.addr(), layout) {
                    return status;
                }

                runtime.resolve_task_retirement(task)
            })).unwrap_or_else(|status| status)
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
    #[cfg_attr(test, expect(unsafe_code, reason = "runtime tests link this substrate export to the Bray provider"))]
    #[cfg_attr(test, unsafe(no_mangle))]
    pub extern "C" fn bray_runtime_cleanup_shield_enter() {
        crate::context::enter_current_run_cleanup_shield();
    }
}

native_export! {
    #[cfg_attr(test, expect(unsafe_code, reason = "runtime tests link this substrate export to the Bray provider"))]
    #[cfg_attr(test, unsafe(no_mangle))]
    pub extern "C" fn bray_runtime_cleanup_shield_leave() {
        crate::context::leave_current_run_cleanup_shield();
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
                runtime.join(task, std::sync::Arc::new(move || callback(context)))
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
        let selected_host = super::host::active();

        if super::host::finish().is_err() {
            return NativeRuntimeStatus::RUNTIME_FAILURE;
        }

        if selected_host && runtime_status == NativeRuntimeStatus::NOT_INITIALIZED {
            NativeRuntimeStatus::SUCCESS
        } else {
            runtime_status
        }
    })
}

native_export! {
    #[cfg_attr(test, expect(unsafe_code, reason = "runtime tests link this substrate export to the Bray provider"))]
    #[cfg_attr(test, unsafe(no_mangle))]
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
fn take_transferred_frame(transfer: NativeInactiveFrameTransfer) -> Option<NativeInactiveFrame> {
    let address = transfer.address();

    if address == 0 || !address.is_multiple_of(align_of::<NativeInactiveFrame>()) {
        return None;
    }

    let frame = unsafe {
        // The native ABI requires the caller to keep this descriptor live for
        // the call and transfers its generated context to this copied value.
        (address as *const NativeInactiveFrame).read()
    };

    Some(frame)
}

fn execute_root(
    frame: NativeInactiveFrame,
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

    let status =
        with_runtime(|runtime| runtime.start(task, &frame)).unwrap_or_else(|status| status);

    if !status.is_success() {
        let released = with_runtime(|runtime| runtime.destroy_task(task))
            .expect("root admission retains its initialized runtime");

        assert!(
            released.is_success(),
            "rejected root activation retains its unstarted task reservation"
        );

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
        NativeFrameAffinity, NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind,
        NativeFrameState, NativeInactiveFrame, NativeInactiveFrameTransfer, NativeLaneRequirements,
        NativePanicCause, NativeRunOutcome, NativeRunState, NativeRuntimeConfiguration,
        NativeRuntimeStatus, NativeTaskHandle,
    };

    use super::super::entry::{
        foreign as bray_runtime_foreign_callback_execution,
        root as bray_runtime_synchronous_root_execution,
    };

    use super::{
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
    static REJECTED_COMPUTE_CHILD: std::sync::Mutex<Option<NativeInactiveFrame>> =
        std::sync::Mutex::new(None);
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
    fn root_completion_accepts_large_and_overaligned_backing() {
        extern "C-unwind" fn check_alignment(alignment: usize, destination: usize) {
            assert_ne!(destination, 0);
            assert_eq!(destination % alignment, 0);
        }

        for (size, alignment) in [(0, 1), (65537, 256), (4096, 4096)] {
            let frame = super::super::fixtures::admit(
                alignment,
                bray_runtime_abi::NativeActivationDescriptor {
                    storage: bray_runtime_abi::NativeCleanupStorageShape::new([7; 32], 8, 8),
                    header: 0,
                    completion: 0,
                    completion_bytes: size,
                    completion_align: alignment,
                    states: 2,
                    state: frame_state,
                    resume: resume_frame,
                    cancel: cancel_frame,
                    broadcast: ignore_broadcast,
                    resolve: ignore_resolution,
                    move_completion: check_alignment,
                    destroy: ignore_action,
                },
            );

            let start = execute_test_root(frame, NativeRuntimeConfiguration::new(2, 1));

            let root = start
                .root()
                .expect("concrete completion layout must be accepted");

            let outcome = bray_runtime_root_terminal_observation(root);

            assert_eq!(outcome.state(), NativeRunState::COMPLETED);
            assert_ne!(outcome.payload(), 0);
            assert_eq!(outcome.payload() % alignment, 0);

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

        assert_eq!(ROOT_DESTROYED.load(Ordering::Relaxed), 0);

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
        extern "C-unwind" fn check_admission(
            destination: &mut NativeFrameProgress,
            context: usize,
        ) {
            let mut execution = bray_runtime_abi::NativeActivationExecution {
                address: context,
                identity: [7; 32],
                state: 0,
                requirements: movable_frame_state(context, 0, 0, 0),
                origin: 0,
            };

            assert_eq!(
                super::super::activation::readiness(&execution),
                NativeRuntimeStatus::SUCCESS.code(),
            );

            execution.requirements = movable_blocking_frame_state(context, 0, 0, 0);

            assert_eq!(
                super::super::activation::readiness(&execution),
                NativeRuntimeStatus::PENDING.code(),
            );

            resume_frame(destination, context);
        }

        let start = execute_test_root(
            protected_frame_with_state(
                8,
                movable_frame_state,
                check_admission,
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

        assert!(
            super::with_runtime(|runtime| runtime.resolve_task_retirement(task))
                .expect("fixture runtime must be active")
                .is_success()
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

        assert!(
            super::with_runtime(|runtime| runtime.resolve_task_retirement(task))
                .expect("fixture runtime must be active")
                .is_success()
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

        assert!(
            super::with_runtime(|runtime| runtime.resolve_task_retirement(task))
                .expect("fixture runtime must be active")
                .is_success()
        );

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

        let frame = super::super::fixtures::admit(
            0,
            bray_runtime_abi::NativeActivationDescriptor {
                storage: bray_runtime_abi::NativeCleanupStorageShape::new([8; 32], 8, 8),
                header: 0,
                completion: 0,
                completion_bytes: 8,
                completion_align: 8,
                states: 2,
                state: frame_state,
                resume: suspend_without_wake,
                cancel: record_cancel_entry,
                broadcast: ignore_broadcast,
                resolve: ignore_resolution,
                move_completion: ignore_completion_move,
                destroy: ignore_action,
            },
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
            let frame = super::super::fixtures::admit(
                0,
                bray_runtime_abi::NativeActivationDescriptor {
                    storage: bray_runtime_abi::NativeCleanupStorageShape::new([9; 32], 8, 8),
                    header: 0,
                    completion: 0,
                    completion_bytes: 8,
                    completion_align: 8,
                    states: 1,
                    state: frame_state,
                    resume: resume_frame,
                    cancel: cancel_frame,
                    broadcast: if fail_completion {
                        ignore_broadcast
                    } else {
                        panic_broadcast
                    },
                    resolve: if fail_completion {
                        ignore_resolution
                    } else {
                        panic_resolution
                    },
                    move_completion: if fail_completion {
                        panic_completion_move
                    } else {
                        ignore_completion_move
                    },
                    destroy: panic_action,
                },
            );

            let start = execute_test_root(frame, NativeRuntimeConfiguration::new(2, 1));
            let root = start.root().expect("cleanup-failing root must transfer");
            let denied = crate::outgoing::tests::reject_admission();
            let mut outcome = bray_runtime_root_terminal_observation(root);

            assert_eq!(outcome.state(), NativeRunState::PANICKED);

            let mut report = crate::RuntimePanic::from_native(outcome.take_report());

            // A failed normal resolver leaves its result owned: abandonment then
            // attempts both broadcast and resolution before retiring the frame.
            assert_eq!(report.suppressed_count(), 2 * usize::from(!fail_completion));

            assert_eq!(
                super::with_runtime(|runtime| runtime.pending_cleanup_incidents())
                    .expect("fixture runtime must be active"),
                0
            );

            assert!(bray_runtime_root_completion_resolution(root).is_success());

            assert_eq!(
                super::with_runtime(|runtime| runtime.pending_cleanup_incidents())
                    .expect("fixture runtime must be active"),
                1
            );

            assert!(super::bray_runtime_cleanup_incident_reporting().is_success());

            assert_eq!(
                super::with_runtime(|runtime| runtime.pending_cleanup_incidents())
                    .expect("fixture runtime must be active"),
                0
            );

            assert!(report.consume(false).is_success());
            assert!(bray_runtime_structured_shutdown().is_success());
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

    extern "C" fn release_frame_bridge_primary(_: usize, _: usize, _: &mut NativeRunOutcome) {
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
        for phase in 0..4 {
            FRAME_BRIDGE_RELEASES.store(0, Ordering::Relaxed);

            let frame = super::super::fixtures::admit(
                0,
                bray_runtime_abi::NativeActivationDescriptor {
                    storage: bray_runtime_abi::NativeCleanupStorageShape::new([7; 32], 8, 8),
                    header: 0,
                    completion: 0,
                    completion_bytes: 8,
                    completion_align: 8,
                    states: 2,
                    state: frame_state,
                    resume: if phase >= 2 {
                        resume_runtime_failure
                    } else {
                        write_frame_report_then_unwind
                    },
                    cancel: write_frame_report_then_unwind,
                    broadcast: if phase == 3 {
                        write_frame_report_then_unwind
                    } else {
                        ignore_broadcast
                    },
                    resolve: if phase == 2 {
                        resolve_frame_report_then_unwind
                    } else {
                        ignore_resolution
                    },
                    move_completion: ignore_completion_move,
                    destroy: ignore_action,
                },
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
                if phase >= 2 {
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

            if phase >= 2 {
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

        let frame = super::super::fixtures::admit(
            0,
            bray_runtime_abi::NativeActivationDescriptor {
                storage: bray_runtime_abi::NativeCleanupStorageShape::new([10; 32], 8, 8),
                header: 0,
                completion: 0,
                completion_bytes: 8,
                completion_align: 8,
                states: 2,
                state: frame_state,
                resume: suspend_then_fail,
                cancel: cancel_frame,
                broadcast: record_failure_broadcast,
                resolve: record_failure_resolution,
                move_completion: ignore_completion_move,
                destroy: record_failure_destruction,
            },
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
        assert_eq!(FAILURE_DESTRUCTIONS.load(Ordering::Relaxed), 0);

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(FAILURE_DESTRUCTIONS.load(Ordering::Relaxed), 1);

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
            bray_runtime_task_destruction(task),
            NativeRuntimeStatus::PENDING
        );

        assert!(
            super::with_runtime(|runtime| runtime.resolve_task_retirement(task))
                .expect("fixture runtime must be active")
                .is_success()
        );

        assert!(bray_runtime_task_destruction(task).is_success());

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
    fn task_start_queries_initial_state_once_outside_the_task_table_lock() {
        static QUERIES: AtomicUsize = AtomicUsize::new(0);
        static LOCKED: AtomicUsize = AtomicUsize::new(0);

        extern "C" fn state(_: usize, _: u32, _: u32, _: usize) -> NativeFrameState {
            QUERIES.fetch_add(1, Ordering::Relaxed);

            let locked =
                super::super::state::with_runtime(|runtime| runtime.tasks.try_lock().is_err())
                    .expect("admission retains the native runtime");

            if locked {
                LOCKED.fetch_add(1, Ordering::Relaxed);
            }

            NativeFrameState::new(NativeFrameAffinity::MOVABLE, NativeLaneRequirements::NONE)
        }

        let _isolation = super::super::state::test_runtime_isolation();

        QUERIES.store(0, Ordering::Relaxed);
        LOCKED.store(0, Ordering::Relaxed);

        assert!(
            bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(1, 1))
                .is_success()
        );

        let task = bray_runtime_task_allocation()
            .task()
            .expect("task admission succeeds");

        assert!(
            start_test_task(
                task,
                protected_frame_with_state(
                    8,
                    state,
                    resume_frame,
                    ignore_completion_move,
                    ignore_action
                )
            )
            .is_success()
        );

        assert_eq!(QUERIES.load(Ordering::Relaxed), 1);
        assert_eq!(LOCKED.load(Ordering::Relaxed), 0);

        assert_eq!(
            super::with_runtime(|runtime| runtime.resolve_task(task))
                .expect("fixture runtime must be active")
                .state(),
            NativeRunState::COMPLETED
        );

        assert!(
            super::with_runtime(|runtime| runtime.resolve_task_retirement(task))
                .expect("fixture runtime must be active")
                .is_success()
        );

        assert!(bray_runtime_task_destruction(task).is_success());
        assert_eq!(LOCKED.load(Ordering::Relaxed), 0);
        assert!(bray_runtime_structured_shutdown().is_success());
    }

    #[test]
    fn terminal_retirement_releases_reports_outside_the_task_table_lock() {
        static RELEASED: AtomicUsize = AtomicUsize::new(0);

        extern "C" fn release(_: usize, _: usize, _: &mut NativeRunOutcome) {
            let unlocked =
                super::super::state::with_runtime(|runtime| runtime.tasks.try_lock().is_ok())
                    .expect("retirement retains the runtime context");

            RELEASED.store(if unlocked { 1 } else { 2 }, Ordering::Relaxed);
        }

        let _isolation = super::super::state::test_runtime_isolation();

        RELEASED.store(0, Ordering::Relaxed);

        assert!(
            bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(1, 1))
                .is_success()
        );

        let task = bray_runtime_task_allocation()
            .task()
            .expect("task admission succeeds");

        assert!(
            start_test_task(
                task,
                protected_frame(8, resume_frame, ignore_completion_move, ignore_action)
            )
            .is_success()
        );

        assert!(bray_runtime_main_thread_lane_drive().is_success());

        assert_eq!(
            bray_runtime_join_registration(task, ignore_wake, 0).state(),
            NativeRunState::COMPLETED
        );

        super::super::state::with_runtime(|runtime| {
            let task = runtime
                .retained_task(task, true)
                .expect("fixture `retained_task` call must succeed");

            let mut admitted = crate::outgoing::OutgoingRecords::admit(1)
                .expect("fixture outgoing records must admit the supplied values");

            let primary = bray_runtime_abi::NativePanicPrimary::new(
                bray_runtime_abi::NativePanicCause::ASSERTION,
                bray_runtime_abi::NativeSourceAnchor::unavailable(),
                bray_runtime_abi::NativePanicMessage::new(0, 0, None, Some(release)),
            );

            let mut report = crate::RuntimePanic::from_native(crate::frame::native_report(primary));

            report.reserve_from(&mut admitted);

            task.task.transfer_cleanup_incident(
                &runtime.cleanup_reports,
                task.task.execution_origin(),
                report,
            );
        })
        .expect("fixture runtime must be active");

        assert!(
            super::super::state::with_runtime(|runtime| runtime.resolve_task_retirement(task))
                .expect("fixture runtime must be active")
                .is_success()
        );

        assert!(bray_runtime_task_destruction(task).is_success());

        assert!(
            super::super::state::with_runtime(|runtime| runtime.report_cleanup_incidents())
                .expect("fixture runtime must be active")
                .is_success()
        );

        assert_eq!(
            RELEASED.load(Ordering::Relaxed),
            1,
            "the release provider must observe an unlocked task table"
        );

        assert!(bray_runtime_structured_shutdown().is_success());
    }

    #[test]
    fn task_start_transfers_each_frame_exactly_once() {
        let _isolation = super::super::state::test_runtime_isolation();

        STARTED_DESTROYED.store(0, Ordering::Relaxed);

        assert!(
            bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(1, 1))
                .is_success()
        );

        let task = bray_runtime_task_allocation()
            .task()
            .expect("fixture `task` call must succeed");

        let frame = protected_frame(8, resume_frame, ignore_completion_move, started_destroy);

        let missing = NativeTaskHandle::new(task.raw() + 1)
            .expect("fixture native task handle must be valid");

        assert_eq!(
            super::with_runtime(|runtime| runtime.start(missing, &frame))
                .expect("fixture runtime must be active"),
            NativeRuntimeStatus::UNKNOWN_TASK
        );

        assert_eq!(STARTED_DESTROYED.load(Ordering::Relaxed), 0);

        assert!(
            super::with_runtime(|runtime| runtime.start(task, &frame))
                .expect("fixture runtime must be active")
                .is_success()
        );

        assert_eq!(
            super::with_runtime(|runtime| runtime.start(task, &frame))
                .expect("fixture runtime must be active"),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert!(bray_runtime_main_thread_lane_drive().is_success());

        assert_eq!(
            bray_runtime_join_registration(task, ignore_wake, 0).state(),
            NativeRunState::COMPLETED
        );

        assert_eq!(STARTED_DESTROYED.load(Ordering::Relaxed), 0);

        assert!(
            super::super::state::with_runtime(|runtime| runtime.resolve_task_retirement(task))
                .expect("fixture runtime must be active")
                .is_success()
        );

        assert!(bray_runtime_task_destruction(task).is_success());
        assert_eq!(STARTED_DESTROYED.load(Ordering::Relaxed), 1);
        assert!(bray_runtime_structured_shutdown().is_success());
    }

    #[test]
    fn failed_root_admission_preserves_the_exact_inactive_owner_for_retry() {
        static DESTROYED: AtomicUsize = AtomicUsize::new(0);

        extern "C-unwind" fn destroy(_: usize) {
            DESTROYED.fetch_add(1, Ordering::Relaxed);
        }

        let _isolation = super::super::state::test_runtime_isolation();
        let configuration = NativeRuntimeConfiguration::new(1, 1);

        assert!(bray_runtime_main_thread_lane_startup(configuration).is_success());

        let frame = protected_frame(8, resume_frame, ignore_completion_move, destroy);
        let failure = crate::outgoing::tests::reject_admission();

        let start =
            bray_runtime_root_execution(NativeInactiveFrameTransfer::new(&frame), configuration);

        assert_eq!(start.status(), NativeRuntimeStatus::RUNTIME_FAILURE);
        assert!(start.root().is_none());
        assert_eq!(DESTROYED.load(Ordering::Relaxed), 0);
        drop(failure);

        let root =
            bray_runtime_root_execution(NativeInactiveFrameTransfer::new(&frame), configuration)
                .root()
                .expect("fixture `root` call must succeed");

        assert_eq!(
            bray_runtime_root_terminal_observation(root).state(),
            NativeRunState::COMPLETED
        );

        assert_eq!(DESTROYED.load(Ordering::Relaxed), 0);
        assert!(bray_runtime_root_completion_resolution(root).is_success());
        assert_eq!(DESTROYED.load(Ordering::Relaxed), 1);
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

        let inactive = frame;

        let failure = crate::outgoing::tests::reject_admission();

        let rejected = bray_runtime_task_allocation();

        assert_eq!(rejected.status(), NativeRuntimeStatus::RUNTIME_FAILURE);
        assert!(rejected.task().is_none());
        assert_eq!(DESTROYED.load(Ordering::Relaxed), 0);

        drop(failure);

        let task = bray_runtime_task_allocation()
            .task()
            .expect("fixture `task` call must succeed");

        assert!(bray_runtime_task_start(task, inactive).is_success());
        assert!(bray_runtime_main_thread_lane_drive().is_success());
        assert_eq!(DESTROYED.load(Ordering::Relaxed), 0);

        assert_eq!(
            bray_runtime_join_registration(task, ignore_wake, 0).state(),
            NativeRunState::COMPLETED
        );

        assert!(
            super::super::state::with_runtime(|runtime| runtime.resolve_task_retirement(task))
                .expect("fixture runtime must be active")
                .is_success()
        );

        assert!(bray_runtime_task_destruction(task).is_success());
        assert_eq!(DESTROYED.load(Ordering::Relaxed), 1);
        assert!(bray_runtime_structured_shutdown().is_success());
    }

    #[test]
    fn frame_contract_failures_are_not_published_as_panics() {
        assert!(
            bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(1, 1))
                .is_success()
        );

        let task = bray_runtime_task_allocation()
            .task()
            .expect("native task storage must allocate");

        assert!(
            start_test_task(
                task,
                protected_frame(
                    8,
                    resume_runtime_failure,
                    ignore_completion_move,
                    ignore_action
                )
            )
            .is_success()
        );

        assert_eq!(
            bray_runtime_main_thread_lane_drive(),
            NativeRuntimeStatus::RUNTIME_FAILURE
        );

        assert_eq!(
            bray_runtime_join_registration(task, ignore_wake, 0).state(),
            NativeRunState::RUNTIME_FAILURE
        );

        assert!(
            super::with_runtime(|runtime| runtime.resolve_task_retirement(task))
                .expect("fixture runtime must be active")
                .is_success()
        );

        assert!(bray_runtime_task_destruction(task).is_success());
        assert!(bray_runtime_structured_shutdown().is_success());
    }

    #[test]
    fn invalid_callback_progress_fails_fast() {
        const CHILD: &str = "BRAY_TEST_INVALID_CALLBACK_PROGRESS";

        if std::env::var_os(CHILD).is_some() {
            assert!(
                bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(1, 1))
                    .is_success()
            );

            let task = bray_runtime_task_allocation()
                .task()
                .expect("native task storage must allocate");

            assert!(
                start_test_task(
                    task,
                    protected_frame(
                        8,
                        resume_unknown_progress,
                        ignore_completion_move,
                        ignore_action
                    )
                )
                .is_success()
            );

            let _ = bray_runtime_main_thread_lane_drive();

            panic!("invalid internal progress must abort at the native boundary");
        }

        crate::test_support::assert_native_abort(
            "native::export::tests::invalid_callback_progress_fails_fast",
            CHILD,
        );
    }

    fn frame() -> NativeInactiveFrame {
        protected_frame(8, resume_frame, ignore_completion_move, destroy_frame)
    }

    fn execute_test_root(
        frame: NativeInactiveFrame,
        configuration: NativeRuntimeConfiguration,
    ) -> super::NativeRootStart {
        let transfer = NativeInactiveFrameTransfer::new(&frame);

        bray_runtime_root_execution(transfer, configuration)
    }

    fn start_test_task(task: NativeTaskHandle, frame: NativeInactiveFrame) -> NativeRuntimeStatus {
        bray_runtime_task_start(task, frame)
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
        let outcome = bray_runtime_synchronous_root_execution(propagate_test_cancellation, 0);

        assert_eq!(outcome.state(), NativeRunState::CANCELLED);
        assert_eq!(outcome.payload(), 0);
    }

    #[test]
    fn foreign_callback_boundary_attaches_threads_for_the_callback_lifetime() {
        let (outcome, attached_after_return) = std::thread::spawn(|| {
            assert!(crate::bootstrap::current_runtime_thread().is_none());

            let outcome =
                bray_runtime_foreign_callback_execution(assert_callback_runtime_thread, 41);

            (
                outcome,
                crate::bootstrap::current_runtime_thread().is_some(),
            )
        })
        .join()
        .unwrap_or_else(|_| panic!("foreign callback test thread must join"));

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_eq!(outcome.payload(), 41);
        assert!(!attached_after_return);
    }

    #[test]
    fn foreign_callback_boundary_reuses_an_attached_runtime_thread() {
        let _thread = crate::bootstrap::RuntimeThreadScope::enter()
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
            crate::bootstrap::main_runtime_thread()
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
    fn inactive_composition_resolves_captures_without_entering_the_body() {
        static BODY_ENTRIES: AtomicUsize = AtomicUsize::new(0);
        static RESUMES: AtomicUsize = AtomicUsize::new(0);
        static RESOLUTIONS: AtomicUsize = AtomicUsize::new(0);
        static DESTRUCTIONS: AtomicUsize = AtomicUsize::new(0);

        extern "C" fn child_state(_: usize, _: u32, operation: u32, _: usize) -> NativeFrameState {
            NativeFrameState::new(
                NativeFrameAffinity::MOVABLE,
                if operation == 0 {
                    NativeLaneRequirements::COMPUTE
                } else {
                    NativeLaneRequirements::NONE
                },
            )
        }

        extern "C-unwind" fn forbidden_body(_: &mut NativeFrameProgress, _: usize) {
            BODY_ENTRIES.fetch_add(1, Ordering::Relaxed);
            panic!("inactive capture cleanup must not enter the source body");
        }

        extern "C-unwind" fn resolve(
            _: &mut NativeFrameProgress,
            failing: usize,
            _: NativeFrameExit,
        ) {
            RESOLUTIONS.fetch_add(1, Ordering::Relaxed);

            if failing != 0 {
                panic!("inactive captured owner failed during cleanup");
            }
        }

        extern "C-unwind" fn destroy(_: usize) {
            DESTRUCTIONS.fetch_add(1, Ordering::Relaxed);
        }

        extern "C-unwind" fn parent(progress: &mut NativeFrameProgress, failing: usize) {
            if RESUMES.fetch_add(1, Ordering::Relaxed) == 0 {
                let child = super::super::fixtures::admit(
                    failing,
                    bray_runtime_abi::NativeActivationDescriptor {
                        storage: bray_runtime_abi::NativeCleanupStorageShape::new([91; 32], 8, 8),
                        header: 0,
                        completion: 0,
                        completion_bytes: 8,
                        completion_align: 8,
                        states: 1,
                        state: child_state,
                        resume: forbidden_body,
                        cancel: forbidden_body,
                        broadcast: ignore_broadcast,
                        resolve,
                        move_completion: ignore_completion_move,
                        destroy,
                    },
                );

                let denial = crate::outgoing::tests::reject_admission();

                super::bray_runtime_awaited_frame_composition(child, 1, 0, 1);

                drop(denial);
                *progress = bray_runtime_suspension_registration(1);
                return;
            }

            let mut outcome = NativeRunOutcome::new(NativeRunState::COMPLETED, 0);

            super::super::fixtures::take_completion(&mut outcome);

            assert_eq!(
                outcome.state(),
                if failing == 0 {
                    NativeRunState::CANCELLED
                } else {
                    NativeRunState::PANICKED
                }
            );

            if failing != 0 {
                assert_eq!(
                    outcome.take_report().consume(false),
                    NativeRuntimeStatus::SUCCESS
                );
            }

            *progress = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 1, 0);
        }

        for failing in [0, 1] {
            RESUMES.store(0, Ordering::Relaxed);
            RESOLUTIONS.store(0, Ordering::Relaxed);
            DESTRUCTIONS.store(0, Ordering::Relaxed);

            let root = execute_test_root(
                protected_frame_with_context(
                    failing,
                    8,
                    movable_blocking_frame_state,
                    parent,
                    ignore_completion_move,
                    ignore_action,
                ),
                NativeRuntimeConfiguration::new(2, 1),
            )
            .root()
            .expect(
                "parent admits capture cleanup without the child's incompatible body requirements",
            );

            assert_eq!(
                bray_runtime_root_terminal_observation(root).state(),
                NativeRunState::COMPLETED
            );

            assert_eq!(BODY_ENTRIES.load(Ordering::Relaxed), 0);
            assert_eq!(RESUMES.load(Ordering::Relaxed), 2);
            assert_eq!(RESOLUTIONS.load(Ordering::Relaxed), 1);
            assert_eq!(DESTRUCTIONS.load(Ordering::Relaxed), 1);

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

    #[test]
    fn conflicting_composition_preserves_the_child_for_independent_start() {
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

        let outcome = bray_runtime_root_terminal_observation(root);

        assert_eq!(outcome.state(), NativeRunState::RUNTIME_FAILURE);
        assert_eq!(CONFLICTING_CHILD_DESTRUCTIONS.load(Ordering::Relaxed), 0);

        assert_eq!(
            bray_runtime_root_completion_resolution(root),
            NativeRuntimeStatus::SUCCESS
        );

        let child = REJECTED_COMPUTE_CHILD
            .lock()
            .expect("fixture mutex must not be poisoned")
            .take()
            .expect("rejection retains the exact child");

        let child = execute_test_root(child, NativeRuntimeConfiguration::new(2, 1))
            .root()
            .expect("independent start selects the child's compute lane");

        assert_eq!(
            bray_runtime_root_terminal_observation(child).state(),
            NativeRunState::COMPLETED
        );

        assert_eq!(CONFLICTING_CHILD_DESTRUCTIONS.load(Ordering::Relaxed), 0);
        assert!(bray_runtime_root_completion_resolution(child).is_success());
        assert_eq!(CONFLICTING_CHILD_DESTRUCTIONS.load(Ordering::Relaxed), 1);

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

    static AWAITED_FAILURE_PHASE: AtomicUsize = AtomicUsize::new(0);
    static AWAITED_REPORT_RELEASES: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn awaited_failures_transfer_original_outcomes_without_further_admission() {
        AWAITED_REPORT_RELEASES.store(0, Ordering::Relaxed);

        for kind in 1..=2 {
            AWAITED_FAILURE_PHASE.store(0, Ordering::Relaxed);

            let root = execute_test_root(
                composed_frame(kind, catch_awaited_failure, ignore_completion_move),
                NativeRuntimeConfiguration::new(1, 1),
            )
            .root()
            .expect("failure-catching root must transfer");

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

            assert_eq!(AWAITED_FAILURE_PHASE.load(Ordering::Relaxed), 2);
            assert_eq!(AWAITED_REPORT_RELEASES.load(Ordering::Relaxed), kind - 1);
        }
    }

    extern "C-unwind" fn catch_awaited_failure(destination: &mut NativeFrameProgress, kind: usize) {
        if AWAITED_FAILURE_PHASE.fetch_add(1, Ordering::Relaxed) == 0 {
            super::super::fixtures::compose(move_failing_child(kind), 1);

            *destination = NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0);
            return;
        }

        let _denied = crate::outgoing::tests::reject_admission();
        let mut outcome = NativeRunOutcome::new(NativeRunState::COMPLETED, 0);

        super::super::fixtures::take_completion(&mut outcome);

        assert_eq!(
            outcome.state(),
            if kind == 1 {
                NativeRunState::CANCELLED
            } else {
                NativeRunState::PANICKED
            }
        );

        if kind == 2 {
            assert_eq!(AWAITED_REPORT_RELEASES.load(Ordering::Relaxed), 0);

            let mut report = crate::RuntimePanic::from_native(outcome.take_report());

            assert_eq!(report.consume(false), NativeRuntimeStatus::SUCCESS);
            assert_eq!(AWAITED_REPORT_RELEASES.load(Ordering::Relaxed), 1);
        }

        *destination = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
    }

    extern "C" fn move_failing_child(kind: usize) -> NativeInactiveFrame {
        composed_frame(kind, resume_failing_child, ignore_completion_move)
    }

    extern "C-unwind" fn resume_failing_child(destination: &mut NativeFrameProgress, kind: usize) {
        *destination = if kind == 1 {
            NativeFrameProgress::new(NativeFrameProgressKind::CANCELLED, 0, 0)
        } else {
            NativeFrameProgress::panicked(crate::frame::native_report(
                bray_runtime_abi::NativePanicPrimary::new(
                    bray_runtime_abi::NativePanicCause::ASSERTION,
                    bray_runtime_abi::NativeSourceAnchor::unavailable(),
                    bray_runtime_abi::NativePanicMessage::new(
                        0,
                        0,
                        None,
                        Some(release_awaited_primary),
                    ),
                ),
            ))
        };
    }

    extern "C" fn release_awaited_primary(_: usize, _: usize, _: &mut NativeRunOutcome) {
        assert_eq!(AWAITED_REPORT_RELEASES.fetch_add(1, Ordering::Relaxed), 0);
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
                composed_frame(1, resume_deep_await, record_deep_completion)
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

        assert!(
            super::with_runtime(|runtime| runtime.resolve_task_retirement(root))
                .expect("fixture runtime must be active")
                .is_success()
        );

        assert!(
            super::with_runtime(|runtime| runtime.resolve_task_retirement(competitor))
                .expect("fixture runtime must be active")
                .is_success()
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

    fn protected_frame(
        alignment: usize,
        resume: extern "C-unwind" fn(&mut NativeFrameProgress, usize),
        move_completion: extern "C-unwind" fn(usize, usize),
        destroy: extern "C-unwind" fn(usize),
    ) -> NativeInactiveFrame {
        protected_frame_with_state(alignment, frame_state, resume, move_completion, destroy)
    }

    fn protected_frame_with_state(
        alignment: usize,
        state: bray_runtime_abi::NativeFrameStateCallback,
        resume: extern "C-unwind" fn(&mut NativeFrameProgress, usize),
        move_completion: extern "C-unwind" fn(usize, usize),
        destroy: extern "C-unwind" fn(usize),
    ) -> NativeInactiveFrame {
        protected_frame_with_context(0, alignment, state, resume, move_completion, destroy)
    }

    fn protected_frame_with_context(
        context: usize,
        alignment: usize,
        state: bray_runtime_abi::NativeFrameStateCallback,
        resume: extern "C-unwind" fn(&mut NativeFrameProgress, usize),
        move_completion: extern "C-unwind" fn(usize, usize),
        destroy: extern "C-unwind" fn(usize),
    ) -> NativeInactiveFrame {
        super::super::fixtures::admit(
            context,
            bray_runtime_abi::NativeActivationDescriptor {
                storage: bray_runtime_abi::NativeCleanupStorageShape::new([7; 32], 8, alignment),
                header: 0,
                completion: 0,
                completion_bytes: 8,
                completion_align: 8,
                states: 2,
                state,
                resume,
                cancel: cancel_frame,
                broadcast: ignore_broadcast,
                resolve: ignore_resolution,
                move_completion,
                destroy,
            },
        )
    }

    extern "C" fn frame_state(_: usize, _: u32, _: u32, _: usize) -> NativeFrameState {
        NativeFrameState::new(
            NativeFrameAffinity::MAIN_THREAD,
            NativeLaneRequirements::MAIN_THREAD,
        )
    }

    extern "C" fn movable_frame_state(_: usize, _: u32, _: u32, _: usize) -> NativeFrameState {
        NativeFrameState::new(NativeFrameAffinity::MOVABLE, NativeLaneRequirements::NONE)
    }

    extern "C" fn movable_blocking_frame_state(
        _: usize,
        _: u32,
        _: u32,
        _: usize,
    ) -> NativeFrameState {
        NativeFrameState::new(
            NativeFrameAffinity::MOVABLE,
            NativeLaneRequirements::BLOCKING,
        )
    }

    extern "C" fn movable_compute_frame_state(
        _: usize,
        _: u32,
        _: u32,
        _: usize,
    ) -> NativeFrameState {
        NativeFrameState::new(
            NativeFrameAffinity::MOVABLE,
            NativeLaneRequirements::COMPUTE,
        )
    }

    extern "C-unwind" fn compose_child_then_panic(_: &mut NativeFrameProgress, _: usize) {
        super::super::fixtures::compose(move_pending_child(0), 1);
        panic!("parent failed before publishing its awaited suspension");
    }

    extern "C-unwind" fn reject_conflicting_child(destination: &mut NativeFrameProgress, _: usize) {
        let child = move_conflicting_child(0);

        let status = super::with_runtime(|runtime| {
            runtime.compose_awaited(
                &child,
                1,
                super::super::fixtures::destination(),
                bray_runtime_abi::NativeFrameComposition::Completion,
            )
        })
        .expect("fixture runtime must be active");

        assert_eq!(status, NativeRuntimeStatus::INVALID_ARGUMENT);
        assert_eq!(CONFLICTING_CHILD_DESTRUCTIONS.load(Ordering::Relaxed), 0);

        assert!(
            REJECTED_COMPUTE_CHILD
                .lock()
                .expect("fixture mutex must not be poisoned")
                .replace(child)
                .is_none()
        );

        *destination = NativeFrameProgress::new(NativeFrameProgressKind::RUNTIME_FAILURE, 0, 0);
    }

    fn assert_awaited_completion() {
        let mut outcome = NativeRunOutcome::new(NativeRunState::COMPLETED, 0);

        super::super::fixtures::take_completion(&mut outcome);

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
    }

    extern "C-unwind" fn await_exact_blocking_child(
        destination: &mut NativeFrameProgress,
        _: usize,
    ) {
        let resumes = EXACT_BLOCKING_PARENT_RESUMES.fetch_add(1, Ordering::Relaxed);

        *destination = if resumes == 0 {
            super::super::fixtures::compose(move_blocking_child(0), 1);

            bray_runtime_suspension_registration(1)
        } else {
            assert_awaited_completion();

            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 17)
        };
    }

    extern "C-unwind" fn await_origin_child(destination: &mut NativeFrameProgress, event: usize) {
        let resumes = ORIGIN_PARENT_RESUMES.fetch_add(1, Ordering::Relaxed);

        *destination = if resumes == 0 {
            super::super::fixtures::compose(move_origin_child(event), 1);

            bray_runtime_suspension_registration(1)
        } else {
            assert_awaited_completion();

            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 1, 23)
        };
    }

    extern "C-unwind" fn reject_then_compose_child(
        destination: &mut NativeFrameProgress,
        _: usize,
    ) {
        let child = move_rejected_child(0);
        let rejection = crate::outgoing::tests::reject_admission();

        let rejected = super::with_runtime(|runtime| {
            runtime.compose_awaited(
                &child,
                u32::MAX,
                super::super::fixtures::destination(),
                bray_runtime_abi::NativeFrameComposition::Completion,
            )
        })
        .expect("fixture runtime must be active");

        assert_eq!(rejected, NativeRuntimeStatus::INVALID_ARGUMENT);
        assert_eq!(REJECTED_CHILD_MOVES.load(Ordering::Relaxed), 1);
        assert_eq!(REJECTED_CHILD_DESTRUCTIONS.load(Ordering::Relaxed), 0);

        let accepted = super::with_runtime(|runtime| {
            runtime.compose_awaited(
                &child,
                1,
                super::super::fixtures::destination(),
                bray_runtime_abi::NativeFrameComposition::Completion,
            )
        })
        .expect("fixture runtime must be active");

        assert_eq!(accepted, NativeRuntimeStatus::SUCCESS);
        drop(rejection);
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

    fn composed_frame(
        depth: usize,
        resume: extern "C-unwind" fn(&mut NativeFrameProgress, usize),
        move_completion: extern "C-unwind" fn(usize, usize),
    ) -> NativeInactiveFrame {
        super::super::fixtures::admit(
            depth,
            bray_runtime_abi::NativeActivationDescriptor {
                storage: bray_runtime_abi::NativeCleanupStorageShape::new([33; 32], 8, 8),
                header: 0,
                completion: 0,
                completion_bytes: 8,
                completion_align: 8,
                states: 2,
                state: frame_state,
                resume,
                cancel: resume,
                broadcast: ignore_broadcast,
                resolve: ignore_resolution,
                move_completion,
                destroy: ignore_action,
            },
        )
    }

    extern "C-unwind" fn resume_deep_await(destination: &mut NativeFrameProgress, depth: usize) {
        let _callback = DeepAwaitCallback::enter();
        let state = &DEEP_AWAIT_STATES[depth - 1];

        *destination = if state.load(Ordering::Relaxed) == 0 && depth < DEEP_AWAIT_DEPTH {
            state.store(1, Ordering::Relaxed);

            super::super::fixtures::compose(move_deep_await_child(depth + 1), 1);

            NativeFrameProgress::new(NativeFrameProgressKind::SUSPENDED, 1, 0)
        } else {
            if depth < DEEP_AWAIT_DEPTH {
                assert_awaited_completion();
            }

            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
        };
    }

    extern "C" fn move_deep_await_child(depth: usize) -> NativeInactiveFrame {
        composed_frame(depth, resume_deep_await, record_deep_completion)
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

    extern "C" fn move_rejected_child(_: usize) -> NativeInactiveFrame {
        REJECTED_CHILD_MOVES.fetch_add(1, Ordering::Relaxed);

        super::super::fixtures::admit(
            0,
            bray_runtime_abi::NativeActivationDescriptor {
                storage: bray_runtime_abi::NativeCleanupStorageShape::new([32; 32], 8, 8),
                header: 0,
                completion: 0,
                completion_bytes: 8,
                completion_align: 8,
                states: 1,
                state: movable_frame_state,
                resume: resume_pending_child,
                cancel: resume_pending_child,
                broadcast: ignore_broadcast,
                resolve: ignore_resolution,
                move_completion: ignore_completion_move,
                destroy: destroy_rejected_child,
            },
        )
    }

    extern "C-unwind" fn destroy_rejected_child(_: usize) {
        REJECTED_CHILD_DESTRUCTIONS.fetch_add(1, Ordering::Relaxed);
    }

    extern "C" fn move_pending_child(_: usize) -> NativeInactiveFrame {
        super::super::fixtures::admit(
            0,
            bray_runtime_abi::NativeActivationDescriptor {
                storage: bray_runtime_abi::NativeCleanupStorageShape::new([31; 32], 8, 8),
                header: 0,
                completion: 0,
                completion_bytes: 8,
                completion_align: 8,
                states: 1,
                state: frame_state,
                resume: resume_pending_child,
                cancel: resume_pending_child,
                broadcast: panic_while_broadcasting_child,
                resolve: ignore_resolution,
                move_completion: ignore_completion_move,
                destroy: destroy_pending_child,
            },
        )
    }

    extern "C-unwind" fn resume_pending_child(destination: &mut NativeFrameProgress, _: usize) {
        PENDING_CHILD_RESUMES.fetch_add(1, Ordering::Relaxed);
        *destination = NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0);
    }

    extern "C-unwind" fn panic_while_broadcasting_child(_: &mut NativeFrameProgress, _: usize) {
        PENDING_CHILD_BROADCASTS.fetch_add(1, Ordering::Relaxed);

        PENDING_CHILD_CLEANUP_THREAD.store(
            crate::bootstrap::current_runtime_thread()
                .unwrap_or_else(|| panic!("cleanup callback must run on a runtime thread"))
                .id()
                .raw() as usize,
            Ordering::Relaxed,
        );

        panic!("suspended child broadcast failed");
    }

    extern "C" fn move_origin_child(event: usize) -> NativeInactiveFrame {
        super::super::fixtures::admit(
            event,
            bray_runtime_abi::NativeActivationDescriptor {
                storage: bray_runtime_abi::NativeCleanupStorageShape::new([31; 32], 8, 8),
                header: 0,
                completion: 0,
                completion_bytes: 8,
                completion_align: 8,
                states: 2,
                state: frame_state,
                resume: wait_for_origin_event,
                cancel: cancel_frame,
                broadcast: ignore_broadcast,
                resolve: ignore_resolution,
                move_completion: ignore_completion_move,
                destroy: ignore_action,
            },
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
                assert_awaited_completion();
            }

            if completed == AWAITED_BLOCKING_CHILD_COUNT {
                return NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 17);
            }

            AWAITED_BLOCKING_COMPLETIONS.store(completed + 1, Ordering::Relaxed);

            super::super::fixtures::compose(move_blocking_child(0), 1);

            bray_runtime_suspension_registration(1)
        })();
    }

    extern "C-unwind" fn await_event_child(destination: &mut NativeFrameProgress, event: usize) {
        let resumes = EVENT_PARENT_RESUMES.fetch_add(1, Ordering::Relaxed);

        *destination = if resumes == 0 {
            super::super::fixtures::compose(move_event_child(event), 1);

            bray_runtime_suspension_registration(1)
        } else {
            assert_awaited_completion();

            NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 19)
        };
    }

    extern "C" fn move_event_child(event: usize) -> NativeInactiveFrame {
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

    extern "C" fn move_blocking_child(_: usize) -> NativeInactiveFrame {
        protected_frame_with_state(
            8,
            movable_blocking_frame_state,
            resume_frame,
            ignore_completion_move,
            ignore_action,
        )
    }

    extern "C" fn move_conflicting_child(_: usize) -> NativeInactiveFrame {
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
            bray_runtime_abi::NativeSourceAnchor::new([0; 32], 0, 0, 1, 0),
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
        assert!(crate::bootstrap::current_runtime_thread().is_some());

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

    extern "C-unwind" fn ignore_broadcast(_: &mut NativeFrameProgress, _: usize) {}

    extern "C-unwind" fn ignore_action(_: usize) {}

    extern "C-unwind" fn panic_broadcast(_: &mut NativeFrameProgress, context: usize) {
        panic_action(context);
    }

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

    extern "C-unwind" fn record_failure_broadcast(_: &mut NativeFrameProgress, _: usize) {
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

    extern "C-unwind" fn started_destroy(_: usize) {
        STARTED_DESTROYED.fetch_add(1, Ordering::Relaxed);
    }
}
