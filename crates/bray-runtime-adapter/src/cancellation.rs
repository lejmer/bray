use bray_runtime::native::implementation;
use bray_runtime_abi::{NativeRootHandle, NativeRuntimeStatus, NativeTaskHandle};

native_adapter! {
    pub extern "C" fn bray_runtime_root_cancellation_request(
        root: NativeRootHandle,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_root_cancellation_request(root)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_task_cancellation_request(
        task: NativeTaskHandle,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_task_cancellation_request(task)
    }
}
