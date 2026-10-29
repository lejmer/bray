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
    pub extern "C" fn bray_runtime_panic_propagation(report: &mut bray_runtime_abi::NativePanicReport) -> ! {
        implementation::bray_runtime_panic_propagation(report)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_cleanup_incident_reporting() -> NativeRuntimeStatus {
        implementation::bray_runtime_cleanup_incident_reporting()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_substrate_shutdown() -> NativeRuntimeStatus {
        implementation::bray_runtime_substrate_shutdown()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_complex_multiply(
        width: u32,
        left: &bray_runtime_abi::NativeComplexBits,
        right: &bray_runtime_abi::NativeComplexBits,
        output: &mut bray_runtime_abi::NativeComplexBits,
    ) {
        implementation::bray_runtime_complex_multiply(width, left, right, output)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_complex_divide(
        width: u32,
        left: &bray_runtime_abi::NativeComplexBits,
        right: &bray_runtime_abi::NativeComplexBits,
        output: &mut bray_runtime_abi::NativeComplexBits,
    ) {
        implementation::bray_runtime_complex_divide(width, left, right, output)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_complex_power(
        width: u32,
        left: &bray_runtime_abi::NativeComplexBits,
        right: &bray_runtime_abi::NativeComplexBits,
        output: &mut bray_runtime_abi::NativeComplexBits,
    ) {
        implementation::bray_runtime_complex_power(width, left, right, output)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_real_power(
        width: u32,
        left: &bray_runtime_abi::NativeRealBits,
        right: &bray_runtime_abi::NativeRealBits,
        output: &mut bray_runtime_abi::NativeRealBits,
    ) {
        implementation::bray_runtime_real_power(width, left, right, output)
    }
}
