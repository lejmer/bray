use bray_runtime::native::implementation;
use bray_runtime_abi::{NativeRootHandle, NativeRuntimeStatus, NativeTaskHandle};

native_adapter! {
    pub extern "C" fn bray_runtime_cleanup_shield_enter() {
        implementation::bray_runtime_cleanup_shield_enter()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_cleanup_shield_leave() {
        implementation::bray_runtime_cleanup_shield_leave()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_root_cancellation_request(
        root: NativeRootHandle,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_root_cancellation_request(root)
    }
}

native_adapter! {
    pub extern "C-unwind" fn bray_runtime_awaited_frame_cancellation_request() {
        implementation::bray_runtime_awaited_frame_cancellation_request()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_task_cancellation_request(
        task: NativeTaskHandle,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_task_cancellation_request(task)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_current_run_cancellation_observation() -> u8 {
        implementation::bray_runtime_current_run_cancellation_observation()
    }
}

native_adapter! {
    pub extern "C-unwind" fn bray_runtime_current_run_cancellation_propagation() -> ! {
        implementation::bray_runtime_current_run_cancellation_propagation()
    }
}
