use bray_runtime::native::implementation;
use bray_runtime_abi::{
    NativeExecutionLaneResult, NativeFrameProgress,
    NativeRunResultLayout, NativeRuntimeStatus, NativeTaskHandle,
};

native_adapter! {
    pub extern "C" fn bray_runtime_task_observation_service(
        task: NativeTaskHandle,
        operation: u32,
        output: &mut u64,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_task_observation_service(task, operation, output)
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
    pub extern "C" fn bray_runtime_compatible_lane_selection(
        task: NativeTaskHandle,
        state: u32,
    ) -> NativeExecutionLaneResult {
        implementation::bray_runtime_compatible_lane_selection(task, state)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_activation_callback_admission(
        count: usize,
        destination: &mut bray_runtime_abi::NativeReportRecords,
    ) -> u32 {
        implementation::bray_runtime_activation_callback_admission(count, destination)
    }
}
