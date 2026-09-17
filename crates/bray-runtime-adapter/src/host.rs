use bray_runtime::native::implementation;
use bray_runtime_abi::{
    NativeProductHostDescriptor, NativeProductHostObservation, NativeProductHostOperation,
    NativeRootHandle, NativeRunOutcome, NativeRuntimeStatus, NativeSynchronousRootCallback,
    NativeThreadStaticCleanupRegistration,
};

fn host() -> &'static bray_runtime_abi::NativeHostServices {
    implementation::resident_host_services()
}

fn execution() -> &'static bray_runtime_abi::NativeExecutionServices {
    implementation::resident_execution_services()
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
    pub extern "C" fn bray_runtime_synchronous_root_execution(
        callback: NativeSynchronousRootCallback,
        context: usize,
    ) -> NativeRunOutcome {
        (host().synchronous_root_execution)(callback, context)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_product_host_control(
        descriptor: &NativeProductHostDescriptor,
        operation: NativeProductHostOperation,
    ) -> NativeProductHostObservation {
        (host().product_host_control)(descriptor, operation)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_thread_attachment_identity(
        descriptor: &'static NativeProductHostDescriptor,
    ) -> u64 {
        (host().thread_attachment_identity)(descriptor)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_thread_static_cleanup_registration(
        registration: &NativeThreadStaticCleanupRegistration,
    ) -> NativeRuntimeStatus {
        (host().thread_static_cleanup_registration)(registration)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_root_terminal_observation(
        root: NativeRootHandle,
    ) -> NativeRunOutcome {
        (execution().root_terminal_observation)(root)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_root_completion_resolution(
        root: NativeRootHandle,
    ) -> NativeRuntimeStatus {
        (execution().root_completion_resolution)(root)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_entry_failure_reporting(
        payload: usize,
        size: usize,
    ) -> NativeRuntimeStatus {
        (host().entry_failure_reporting)(payload, size)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_panic_propagation(report: &mut bray_runtime_abi::NativePanicReport) -> ! {
        (host().panic_propagation)(report)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_cleanup_incident_reporting() -> NativeRuntimeStatus {
        (host().cleanup_incident_reporting)()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_substrate_shutdown() -> NativeRuntimeStatus {
        implementation::bray_runtime_substrate_shutdown()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_panic_reporting(report: &mut bray_runtime_abi::NativePanicReport) -> NativeRuntimeStatus {
        (host().panic_reporting)(report)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_panic_report_destruction(report: &mut bray_runtime_abi::NativePanicReport) -> NativeRuntimeStatus {
        (host().panic_report_destruction)(report)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_outgoing_admission(count: usize, outcome: &mut NativeRunOutcome) {
        (host().outgoing_admission)(count, outcome);
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_outgoing_discharge(count: usize) {
        (host().outgoing_discharge)(count);
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_outgoing_activation() -> usize {
        (host().outgoing_activation)()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_outgoing_retirement(record: usize, outcome: &mut NativeRunOutcome) {
        (host().outgoing_retirement)(record, outcome);
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_panic_report_suppression(primary: &mut bray_runtime_abi::NativePanicReport, incident: &mut bray_runtime_abi::NativePanicReport) -> bray_runtime_abi::NativePanicReport {
        (host().panic_report_suppression)(primary, incident)
    }
}
