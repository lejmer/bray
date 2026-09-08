use bray_runtime::native::implementation;
use bray_runtime_abi::{
    NativeExecutionLaneResult, NativeFrameProgress, NativeInactiveFrame, NativeRootStart,
    NativeRunOutcome, NativeRunResultLayout, NativeRunState, NativeRuntimeConfiguration,
    NativeRuntimeStatus, NativeTaskAllocation, NativeTaskHandle, NativeWakeCallback,
};

native_adapter! {
    pub extern "C" fn bray_runtime_root_execution(
        frame: NativeInactiveFrame,
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
    pub extern "C" fn bray_runtime_task_completion_borrow(
        task: NativeTaskHandle,
        destination: Option<&mut usize>,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_task_completion_borrow(task, destination)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_task_completion_borrow_release(task: NativeTaskHandle) -> NativeRuntimeStatus {
        implementation::bray_runtime_task_completion_borrow_release(task)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_task_resolution(
        task: NativeTaskHandle,
        destination: *mut u8,
        layout: Option<&NativeRunResultLayout>,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_task_resolution(task, destination, layout)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_task_destruction(
        task: NativeTaskHandle,
        cleanup: Option<&'static bray_runtime_abi::NativeTaskTerminalCleanup>,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_task_destruction(task, cleanup)
    }
}

native_adapter! {
    pub extern "C-unwind" fn bray_runtime_awaited_frame_composition(
        frame: NativeInactiveFrame,
        entry: u8,
    ) {
        implementation::bray_runtime_awaited_frame_composition(frame, entry)
    }
}

native_adapter! {
    pub extern "C-unwind" fn bray_runtime_inactive_capture_destruction(frame: NativeInactiveFrame) -> usize {
        implementation::bray_runtime_inactive_capture_destruction(frame)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_awaited_frame_resolution(
        destination: *mut u8,
        layout: &NativeRunResultLayout,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_awaited_frame_resolution(destination, layout)
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
