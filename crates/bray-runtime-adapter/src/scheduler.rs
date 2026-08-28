use bray_runtime::native::implementation;
use bray_runtime_abi::{
    NativeExecutionLaneResult, NativeFrameProgress, NativeInactiveFrame,
    NativeProtectedFrameTransfer, NativeRootStart, NativeRunOutcome, NativeRunResultLayout,
    NativeRunState, NativeRuntimeConfiguration, NativeRuntimeStatus, NativeTaskAllocation,
    NativeTaskHandle, NativeWakeCallback,
};

type NativeValueCleanupCallback = extern "C-unwind" fn(*mut u8);

native_adapter! {
    pub extern "C" fn bray_runtime_root_execution(
        frame: NativeProtectedFrameTransfer,
        configuration: NativeRuntimeConfiguration,
    ) -> NativeRootStart {
        implementation::bray_runtime_root_execution(frame, configuration)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_main_thread_lane_startup(
        configuration: NativeRuntimeConfiguration,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_main_thread_lane_startup(configuration)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_main_thread_lane_drive() -> NativeRuntimeStatus {
        implementation::bray_runtime_main_thread_lane_drive()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_task_allocation() -> NativeTaskAllocation {
        implementation::bray_runtime_task_allocation()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_task_start(
        task: NativeTaskHandle,
        frame: NativeInactiveFrame,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_task_start(task, frame)
    }
}

native_adapter! {
    pub extern "C-unwind" fn bray_runtime_task_observation_creation(
        task: NativeTaskHandle,
        request_cancellation: u8,
        layout: *const NativeRunResultLayout,
        cancellation: Option<NativeValueCleanupCallback>,
        lifecycle: Option<NativeValueCleanupCallback>,
    ) -> NativeInactiveFrame {
        implementation::bray_runtime_task_observation_creation(
            task,
            request_cancellation,
            layout,
            cancellation,
            lifecycle,
        )
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_task_resolution(
        task: NativeTaskHandle,
        destination: *mut u8,
        layout: *const NativeRunResultLayout,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_task_resolution(task, destination, layout)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_task_destruction(
        task: NativeTaskHandle,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_task_destruction(task)
    }
}

native_adapter! {
    pub extern "C-unwind" fn bray_runtime_awaited_frame_composition(
        frame: NativeInactiveFrame,
    ) {
        implementation::bray_runtime_awaited_frame_composition(frame)
    }
}

native_adapter! {
    pub extern "C-unwind" fn bray_runtime_frame_completion_move() -> usize {
        implementation::bray_runtime_frame_completion_move()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_task_event_creation() -> usize {
        implementation::bray_runtime_task_event_creation()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_task_event_signal(event: usize) -> NativeRuntimeStatus {
        implementation::bray_runtime_task_event_signal(event)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_task_event_destruction(
        event: usize,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_task_event_destruction(event)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_suspension_registration(state: u32) -> NativeFrameProgress {
        implementation::bray_runtime_suspension_registration(state)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_wake(
        task: NativeTaskHandle,
        state: u32,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_wake(task, state)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_join_registration(
        task: NativeTaskHandle,
        callback: NativeWakeCallback,
        context: usize,
    ) -> NativeRunOutcome {
        implementation::bray_runtime_join_registration(task, callback, context)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_terminal_publication(
        state: NativeRunState,
        payload: usize,
    ) -> NativeFrameProgress {
        implementation::bray_runtime_terminal_publication(state, payload)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_compatible_lane_selection(
        task: NativeTaskHandle,
        state: u32,
    ) -> NativeExecutionLaneResult {
        implementation::bray_runtime_compatible_lane_selection(task, state)
    }
}
