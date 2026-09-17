use bray_runtime::native::implementation;
use bray_runtime_abi::{NativeRootHandle, NativeRuntimeStatus, NativeTaskHandle};

fn host() -> &'static bray_runtime_abi::NativeHostServices {
    implementation::resident_host_services()
}

fn execution() -> &'static bray_runtime_abi::NativeExecutionServices {
    implementation::resident_execution_services()
}

native_adapter! {
    pub extern "C" fn bray_runtime_cleanup_shield_enter() {
        (host().cleanup_shield_enter)()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_cleanup_shield_leave() {
        (host().cleanup_shield_leave)()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_root_cancellation_request(
        root: NativeRootHandle,
    ) -> NativeRuntimeStatus {
        (execution().root_cancellation_request)(root)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_task_cancellation_request(
        task: NativeTaskHandle,
    ) -> NativeRuntimeStatus {
        (execution().task_cancellation_request)(task)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_current_run_cancellation_observation() -> u8 {
        (host().current_run_cancellation_observation)()
    }
}

native_adapter! {
    pub extern "C-unwind" fn bray_runtime_current_run_cancellation_propagation() -> ! {
        (host().current_run_cancellation_propagation)()
    }
}
