use bray_runtime::native::implementation;
use bray_runtime_abi::{
    NativeProductExecution, NativeRootHandle, NativeRunOutcome, NativeRuntimeStatus,
};

native_adapter! {
    pub extern "C" fn bray_runtime_substrate_initialization(
        worker_capacity: usize,
        timer_capacity: usize,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_substrate_initialization(worker_capacity, timer_capacity)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_product_execution_provider(
        asynchronous: u32,
        output: &mut std::mem::MaybeUninit<NativeProductExecution>,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_product_execution_provider(asynchronous, output)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_root_terminal_observation(
        root: NativeRootHandle,
    ) -> NativeRunOutcome {
        implementation::bray_runtime_root_terminal_observation(root)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_root_completion_resolution(
        root: NativeRootHandle,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_root_completion_resolution(root)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_cleanup_incident_reporting() -> NativeRuntimeStatus {
        implementation::bray_runtime_cleanup_incident_reporting()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_execution_shutdown() -> NativeRuntimeStatus {
        implementation::bray_runtime_execution_shutdown()
    }
}
