use bray_runtime::native::implementation;
use bray_runtime_abi::{
    NativeExecutionLaneResult, NativeFrameProgress, NativeInactiveFrame,
    NativeProtectedFrameTransfer, NativeRootStart, NativeRunOutcome, NativeRunState,
    NativeRuntimeConfiguration, NativeRuntimeStatus, NativeTaskAllocation, NativeTaskHandle,
    NativeWakeCallback,
};

native_adapter! {
    pub extern "C" fn bray_runtime_root_execution_v1(
        frame: NativeProtectedFrameTransfer,
        configuration: NativeRuntimeConfiguration,
    ) -> NativeRootStart {
        implementation::bray_runtime_root_execution_v1(frame, configuration)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_main_thread_lane_startup_v1(
        configuration: NativeRuntimeConfiguration,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_main_thread_lane_startup_v1(configuration)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_main_thread_lane_drive_v1() -> NativeRuntimeStatus {
        implementation::bray_runtime_main_thread_lane_drive_v1()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_task_allocation_v1() -> NativeTaskAllocation {
        implementation::bray_runtime_task_allocation_v1()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_task_start_v1(
        task: NativeTaskHandle,
        frame: NativeProtectedFrameTransfer,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_task_start_v1(task, frame)
    }
}

native_adapter! {
    pub extern "C-unwind" fn bray_runtime_awaited_frame_composition_v1(
        frame: NativeInactiveFrame,
    ) {
        implementation::bray_runtime_awaited_frame_composition_v1(frame)
    }
}

native_adapter! {
    pub extern "C-unwind" fn bray_runtime_frame_completion_move_v1() -> usize {
        implementation::bray_runtime_frame_completion_move_v1()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_suspension_registration_v1(state: u32) -> NativeFrameProgress {
        implementation::bray_runtime_suspension_registration_v1(state)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_wake_v1(
        task: NativeTaskHandle,
        state: u32,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_wake_v1(task, state)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_join_registration_v1(
        task: NativeTaskHandle,
        callback: NativeWakeCallback,
        context: usize,
    ) -> NativeRunOutcome {
        implementation::bray_runtime_join_registration_v1(task, callback, context)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_terminal_publication_v1(
        state: NativeRunState,
        payload: usize,
    ) -> NativeFrameProgress {
        implementation::bray_runtime_terminal_publication_v1(state, payload)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_compatible_lane_selection_v1(
        task: NativeTaskHandle,
        state: u32,
    ) -> NativeExecutionLaneResult {
        implementation::bray_runtime_compatible_lane_selection_v1(task, state)
    }
}
