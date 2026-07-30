use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_interface::{
    NativeExecutionLaneResult, NativeFrameProgress, NativeFrameProgressKind,
    NativeProtectedFrame, NativeRunOutcome, NativeRunState,
    NativeRuntimeConfiguration, NativeRuntimeEventCallback, NativeRuntimeStatus,
    NativeTaskAllocation, NativeTaskHandle, NativeWakeCallback,
};

use crate::current_run_cancellation_observable;

use super::state::{
    initialize, runtime_failure, shutdown, with_runtime,
};

macro_rules! native_export {
    ($item:item) => {
        #[expect(
            unsafe_code,
            reason = "the native runtime artifact requires a stable exported ABI symbol"
        )]
        #[unsafe(no_mangle)]
        $item
    };
}

native_export! {
    pub extern "C" fn bray_runtime_root_execution_v1(
        frame: NativeProtectedFrame,
        configuration: NativeRuntimeConfiguration,
    ) -> NativeRunOutcome {
        catch_unwind(AssertUnwindSafe(|| execute_root(frame, configuration)))
            .unwrap_or_else(|_| runtime_failure(NativeRuntimeStatus::PANICKED))
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
        frame: NativeProtectedFrame,
    ) -> NativeRuntimeStatus {
        contain_status(|| {
            with_runtime(|runtime| runtime.start(task, frame))
                .unwrap_or_else(|status| status)
        })
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
        catch_unwind(AssertUnwindSafe(current_run_cancellation_observable))
            .map(u8::from)
            .unwrap_or(0)
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
        contain_status(shutdown)
    }
}

fn contain_status(callback: impl FnOnce() -> NativeRuntimeStatus) -> NativeRuntimeStatus {
    catch_unwind(AssertUnwindSafe(callback))
        .unwrap_or(NativeRuntimeStatus::PANICKED)
}

fn execute_root(
    frame: NativeProtectedFrame,
    configuration: NativeRuntimeConfiguration,
) -> NativeRunOutcome {
    let status = initialize(configuration);

    if !status.is_success() {
        return runtime_failure(status);
    }

    let allocation = with_runtime(|runtime| runtime.allocate())
        .unwrap_or_else(NativeTaskAllocation::failure);

    let Some(task) = allocation.task() else {
        return runtime_failure(allocation.status());
    };

    let status = with_runtime(|runtime| runtime.start(task, frame))
        .unwrap_or_else(|status| status);

    if !status.is_success() {
        return runtime_failure(status);
    }

    loop {
        let outcome = with_runtime(|runtime| runtime.join(task, ignore_root_wake, 0))
            .unwrap_or_else(runtime_failure);

        if outcome.state() != NativeRunState::PENDING {
            return outcome;
        }

        let status = with_runtime(|runtime| runtime.drive_main_thread())
            .unwrap_or_else(|status| status);

        if !status.is_success() {
            return runtime_failure(status);
        }
    }
}

extern "C" fn ignore_root_wake(_: usize) {}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use bray_runtime_interface::{
        NativeFrameAffinity, NativeFrameExit, NativeFrameProgress,
        NativeFrameProgressKind, NativeFrameState, NativeLaneRequirements,
        NativeProtectedFrame, NativeRunState, NativeRuntimeConfiguration,
        NativeRuntimeStatus,
    };

    use super::{
        bray_runtime_main_thread_lane_drive_v1,
        bray_runtime_main_thread_lane_startup_v1,
        bray_runtime_root_execution_v1,
        bray_runtime_structured_shutdown_v1,
        bray_runtime_join_registration_v1,
        bray_runtime_task_allocation_v1, bray_runtime_task_start_v1,
    };

    static DESTROYED: AtomicUsize = AtomicUsize::new(0);
    static ROOT_DESTROYED: AtomicUsize = AtomicUsize::new(0);
    static ROOT_COMPLETION_DESTINATION: AtomicUsize = AtomicUsize::new(0);
    static REJECTED_DESTROYED: AtomicUsize = AtomicUsize::new(0);
    static STARTED_DESTROYED: AtomicUsize = AtomicUsize::new(0);

    #[test]
    fn root_execution_moves_completion_before_frame_destruction() {
        ROOT_DESTROYED.store(0, Ordering::Relaxed);
        ROOT_COMPLETION_DESTINATION.store(0, Ordering::Relaxed);

        let outcome = bray_runtime_root_execution_v1(
            protected_frame(
                8,
                resume_frame,
                record_root_completion_destination,
                root_destroy,
            ),
            NativeRuntimeConfiguration::new(2, 1),
        );

        assert_eq!(outcome.state(), NativeRunState::COMPLETED);
        assert_ne!(outcome.payload(), 0);

        assert_eq!(
            outcome.payload(),
            ROOT_COMPLETION_DESTINATION.load(Ordering::Relaxed)
        );

        assert_eq!(ROOT_DESTROYED.load(Ordering::Relaxed), 1);

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
    fn native_runtime_boundary_starts_executes_and_shuts_down() {
        assert_eq!(
            bray_runtime_main_thread_lane_startup_v1(
                NativeRuntimeConfiguration::new(8, 8)
            ),
            NativeRuntimeStatus::SUCCESS
        );

        let allocation = bray_runtime_task_allocation_v1();

        let Some(task) = allocation.task() else {
            panic!("native frame must allocate");
        };

        assert_eq!(
            bray_runtime_task_start_v1(task, frame()),
            NativeRuntimeStatus::SUCCESS
        );

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
            bray_runtime_main_thread_lane_startup_v1(
                NativeRuntimeConfiguration::new(0, 1)
            ),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            bray_runtime_main_thread_lane_startup_v1(
                NativeRuntimeConfiguration::new(1, 1)
            ),
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
            bray_runtime_main_thread_lane_startup_v1(
                NativeRuntimeConfiguration::new(1, 1)
            ),
            NativeRuntimeStatus::SUCCESS
        );

        let allocation = bray_runtime_task_allocation_v1();

        let Some(task) = allocation.task() else {
            panic!("native task storage must allocate");
        };

        assert_eq!(
            bray_runtime_task_start_v1(
                task,
                protected_frame(
                    0,
                    resume_frame,
                    ignore_completion_move,
                    rejected_destroy,
                )
            ),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(REJECTED_DESTROYED.load(Ordering::Relaxed), 1);

        assert_eq!(
            bray_runtime_task_start_v1(
                task,
                protected_frame(
                    8,
                    resume_frame,
                    ignore_completion_move,
                    started_destroy,
                )
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
            bray_runtime_main_thread_lane_startup_v1(
                NativeRuntimeConfiguration::new(2, 1)
            ),
            NativeRuntimeStatus::SUCCESS
        );

        for resume in [
            resume_runtime_failure as extern "C" fn(usize, u8) -> NativeFrameProgress,
            resume_unknown_progress,
        ] {
            let allocation = bray_runtime_task_allocation_v1();

            let Some(task) = allocation.task() else {
                panic!("native task storage must allocate");
            };

            assert_eq!(
                bray_runtime_task_start_v1(
                    task,
                    protected_frame(
                        8,
                        resume,
                        ignore_completion_move,
                        ignore_action,
                    )
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
        protected_frame(
            8,
            resume_frame,
            ignore_completion_move,
            destroy_frame,
        )
    }

    fn protected_frame(
        alignment: usize,
        resume: extern "C" fn(usize, u8) -> NativeFrameProgress,
        move_completion: extern "C" fn(usize, usize),
        destroy: extern "C" fn(usize),
    ) -> NativeProtectedFrame {
        NativeProtectedFrame::new(
            0,
            [7; 32],
            1,
            8,
            alignment,
            8,
            8,
            frame_state,
            resume,
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

    extern "C" fn resume_frame(_: usize, _: u8) -> NativeFrameProgress {
        NativeFrameProgress::new(
            NativeFrameProgressKind::COMPLETED,
            0,
            17,
        )
    }

    extern "C" fn resume_runtime_failure(_: usize, _: u8) -> NativeFrameProgress {
        NativeFrameProgress::new(
            NativeFrameProgressKind::RUNTIME_FAILURE,
            0,
            0,
        )
    }

    extern "C" fn resume_unknown_progress(_: usize, _: u8) -> NativeFrameProgress {
        NativeFrameProgress::new(
            NativeFrameProgressKind::from_code(u32::MAX),
            0,
            0,
        )
    }

    extern "C" fn ignore_action(_: usize) {}

    extern "C" fn ignore_wake(_: usize) {}

    extern "C" fn ignore_resolution(_: usize, _: NativeFrameExit) {}

    extern "C" fn destroy_frame(_: usize) {
        DESTROYED.fetch_add(1, Ordering::Relaxed);
    }

    extern "C" fn record_root_completion_destination(
        _: usize,
        destination: usize,
    ) {
        ROOT_COMPLETION_DESTINATION.store(destination, Ordering::Relaxed);
    }

    extern "C" fn ignore_completion_move(_: usize, _: usize) {}

    extern "C" fn root_destroy(_: usize) {
        ROOT_DESTROYED.fetch_add(1, Ordering::Relaxed);
    }

    extern "C" fn rejected_destroy(_: usize) {
        REJECTED_DESTROYED.fetch_add(1, Ordering::Relaxed);
    }

    extern "C" fn started_destroy(_: usize) {
        STARTED_DESTROYED.fetch_add(1, Ordering::Relaxed);
    }
}
