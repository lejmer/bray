use bray_runtime::native::implementation;
use bray_runtime_abi::{
    NativeProductHostDescriptor, NativeProductHostObservation, NativeProductHostOperation,
    NativeRootHandle, NativeRunOutcome, NativeRuntimeStatus, NativeThreadStaticCleanupRegistration,
};

native_adapter! {
    pub extern "C" fn bray_runtime_substrate_static_outcome_reporting(outcome: &mut NativeRunOutcome) {
        implementation::bray_runtime_substrate_static_outcome_reporting(outcome)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_substrate_initialization(
        worker_capacity: usize,
        timer_capacity: usize,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_substrate_initialization(worker_capacity, timer_capacity)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_substrate_product_host_control(
        descriptor: &NativeProductHostDescriptor,
        operation: NativeProductHostOperation,
    ) -> NativeProductHostObservation {
        implementation::bray_runtime_product_host_control(descriptor, operation)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_substrate_thread_attachment_identity(
        descriptor: &'static NativeProductHostDescriptor,
    ) -> u64 {
        implementation::bray_runtime_substrate_thread_attachment_identity(descriptor)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_substrate_thread_static_cleanup_registration(
        registration: &NativeThreadStaticCleanupRegistration,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_substrate_thread_static_cleanup_registration(registration)
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
    pub extern "C" fn bray_runtime_entry_failure_reporting(
        payload: usize,
        size: usize,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_entry_failure_reporting(payload, size)
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
    pub extern "C" fn bray_runtime_panic_reporting(report: &mut bray_runtime_abi::NativePanicReport) -> NativeRuntimeStatus {
        implementation::bray_runtime_panic_reporting(report)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_panic_report_destruction(report: &mut bray_runtime_abi::NativePanicReport) -> NativeRuntimeStatus {
        implementation::bray_runtime_panic_report_destruction(report)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_outgoing_admission(count: usize, outcome: &mut NativeRunOutcome) {
        implementation::bray_runtime_outgoing_admission(count, outcome);
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_outgoing_discharge(count: usize) {
        implementation::bray_runtime_outgoing_discharge(count);
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_outgoing_activation() -> usize {
        implementation::bray_runtime_outgoing_activation()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_outgoing_retirement(record: usize, outcome: &mut NativeRunOutcome) {
        implementation::bray_runtime_outgoing_retirement(record, outcome);
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_panic_report_suppression(primary: &mut bray_runtime_abi::NativePanicReport, incident: &mut bray_runtime_abi::NativePanicReport) -> bray_runtime_abi::NativePanicReport {
        implementation::bray_runtime_panic_report_suppression(primary, incident)
    }
}
