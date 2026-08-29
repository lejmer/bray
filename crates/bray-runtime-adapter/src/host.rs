use bray_runtime::native::implementation;
use bray_runtime_abi::{
    NativePanicCause, NativeProductHostDescriptor, NativeProductHostObservation,
    NativeProductHostOperation, NativeRootHandle, NativeRunOutcome, NativeRuntimeStatus,
    NativeSourceAnchor, NativeStringView, NativeSynchronousRootCallback,
    NativeThreadStaticCleanupRegistration,
};

native_adapter! {
    pub extern "C" fn bray_runtime_initialization(
        worker_capacity: usize,
        timer_capacity: usize,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_initialization(worker_capacity, timer_capacity)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_thread_attachment_identity(
        descriptor: &'static NativeProductHostDescriptor,
    ) -> u64 {
        implementation::bray_runtime_thread_attachment_identity(descriptor)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_thread_static_cleanup_registration(
        registration: &NativeThreadStaticCleanupRegistration,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_thread_static_cleanup_registration(registration)
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
    pub extern "C" fn bray_runtime_synchronous_root_execution(
        callback: NativeSynchronousRootCallback,
        destination: usize,
    ) -> NativeRunOutcome {
        implementation::bray_runtime_synchronous_root_execution(callback, destination)
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
    pub extern "C" fn bray_runtime_panic_reporting(payload: usize) -> NativeRuntimeStatus {
        implementation::bray_runtime_panic_reporting(payload)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_entry_failure_reporting(
        payload: usize,
        size: usize,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_entry_failure_reporting(payload, size)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_panic_report_construction(
        cause: NativePanicCause,
        source_present: u32,
        source_identity: u32,
        source_start: u32,
        source_end: u32,
        source_version: u64,
        message_data: *const u8,
        message_length: usize,
    ) -> usize {
        implementation::bray_runtime_panic_report_construction(
            cause,
            source_present,
            source_identity,
            source_start,
            source_end,
            source_version,
            message_data,
            message_length,
        )
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_panic_propagation(payload: usize) -> ! {
        implementation::bray_runtime_panic_propagation(payload)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_cleanup_incident_reporting() -> NativeRuntimeStatus {
        implementation::bray_runtime_cleanup_incident_reporting()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_structured_shutdown() -> NativeRuntimeStatus {
        implementation::bray_runtime_structured_shutdown()
    }
}
