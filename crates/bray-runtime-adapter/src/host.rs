use bray_runtime::native::implementation;
use bray_runtime_abi::{
    NativeProductHostDescriptor, NativeProductHostObservation, NativeProductHostOperation,
    NativeRootHandle, NativeRunOutcome, NativeRuntimeStatus, NativeThreadStaticCleanupRegistration,
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
    pub extern "C" fn bray_runtime_product_host_control(
        descriptor: &NativeProductHostDescriptor,
        operation: NativeProductHostOperation,
    ) -> NativeProductHostObservation {
        implementation::bray_runtime_product_host_control(descriptor, operation)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_thread_attachment_identity(
        descriptor: &'static NativeProductHostDescriptor,
    ) -> u64 {
        implementation::resident_thread_attachment_identity(descriptor)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_thread_static_cleanup_registration(
        registration: &NativeThreadStaticCleanupRegistration,
    ) -> NativeRuntimeStatus {
        implementation::resident_thread_static_cleanup_registration(registration)
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
