use bray_runtime::native::implementation;
use bray_runtime_abi::{NativeRootHandle, NativeRuntimeStatus, NativeTaskHandle};

native_adapter! {
    pub extern "C" fn bray_runtime_root_cancellation_request_v1(
        root: NativeRootHandle,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_root_cancellation_request_v1(root)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_task_cancellation_request_v1(
        task: NativeTaskHandle,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_task_cancellation_request_v1(task)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_current_run_cancellation_observation_v1() -> u8 {
        implementation::bray_runtime_current_run_cancellation_observation_v1()
    }
}

native_adapter! {
    pub extern "C-unwind" fn bray_runtime_current_run_cancellation_propagation_v1() -> ! {
        implementation::bray_runtime_current_run_cancellation_propagation_v1()
    }
}
