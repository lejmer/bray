use bray_runtime::native::implementation;
use bray_runtime_abi::{
    NativePanicCause, NativeProductHostDescriptor, NativeProductHostObservation,
    NativeProductHostOperation, NativeRootHandle, NativeRunOutcome, NativeRuntimeStatus,
    NativeSourceAnchor, NativeStringView, NativeSynchronousRootCallback,
    NativeThreadStaticCleanupRegistration,
};

native_adapter! {
    pub extern "C" fn bray_runtime_thread_attachment_identity_v1() -> u64 {
        implementation::bray_runtime_thread_attachment_identity_v1()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_thread_static_cleanup_registration_v1(
        registration: &NativeThreadStaticCleanupRegistration,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_thread_static_cleanup_registration_v1(registration)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_product_host_control_v1(
        descriptor: &NativeProductHostDescriptor,
        operation: NativeProductHostOperation,
    ) -> NativeProductHostObservation {
        implementation::bray_runtime_product_host_control_v1(descriptor, operation)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_synchronous_root_execution_v1(
        callback: NativeSynchronousRootCallback,
        destination: usize,
    ) -> NativeRunOutcome {
        implementation::bray_runtime_synchronous_root_execution_v1(callback, destination)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_foreign_callback_execution_v1(
        callback: NativeSynchronousRootCallback,
        destination: usize,
    ) -> NativeRunOutcome {
        implementation::bray_runtime_foreign_callback_execution_v1(callback, destination)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_root_terminal_observation_v1(
        root: NativeRootHandle,
    ) -> NativeRunOutcome {
        implementation::bray_runtime_root_terminal_observation_v1(root)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_root_completion_resolution_v1(
        root: NativeRootHandle,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_root_completion_resolution_v1(root)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_panic_reporting_v1(payload: usize) -> NativeRuntimeStatus {
        implementation::bray_runtime_panic_reporting_v1(payload)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_entry_failure_reporting_v1(
        payload: usize,
        size: usize,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_entry_failure_reporting_v1(payload, size)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_panic_report_construction_v1(
        cause: NativePanicCause,
        source: NativeSourceAnchor,
        message: NativeStringView,
    ) -> usize {
        implementation::bray_runtime_panic_report_construction_v1(cause, source, message)
    }
}

native_adapter! {
    pub extern "C-unwind" fn bray_runtime_panic_propagation_v1(payload: usize) -> ! {
        implementation::bray_runtime_panic_propagation_v1(payload)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_cleanup_incident_reporting_v1() -> NativeRuntimeStatus {
        implementation::bray_runtime_cleanup_incident_reporting_v1()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_structured_shutdown_v1() -> NativeRuntimeStatus {
        implementation::bray_runtime_structured_shutdown_v1()
    }
}
