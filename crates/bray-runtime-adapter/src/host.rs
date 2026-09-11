use bray_runtime::native::implementation;
use bray_runtime_abi::{
    NativeProductHostDescriptor, NativeProductHostObservation, NativeProductHostOperation,
    NativeRootHandle, NativeRunOutcome, NativeRuntimeStatus, NativeThreadStaticCleanupRegistration,
};

native_adapter! {
    pub extern "C" fn bray_runtime_cleanup_incident_detail_reporting(
        incident: &bray_runtime_abi::NativeCleanupIncident,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_cleanup_incident_detail_reporting(incident)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_cleanup_incident_transfer(
        incident: &bray_runtime_abi::NativeCleanupIncident,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_cleanup_incident_transfer(incident)
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
        capacity: Option<&bray_runtime_abi::NativeCleanupCapacityBinding>,
    ) -> NativeProductHostObservation {
        implementation::bray_runtime_product_host_control(descriptor, operation, capacity)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_provider_retention(
        descriptor: &NativeProductHostDescriptor,
        destination: &mut bray_runtime_abi::NativeProviderRetention,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_provider_retention(descriptor, destination)
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
    pub extern "C" fn bray_runtime_entry_failure_resolution(
        identity: &bray_runtime_abi::NativeTypeIdentity,
        source: &bray_runtime_abi::NativeSourceAnchor,
        value: usize,
        broadcast: Option<&bray_runtime_abi::NativeValueCleanup>,
        lifecycle: Option<&bray_runtime_abi::NativeValueCleanup>,
    ) -> NativeRuntimeStatus {
        implementation::bray_runtime_entry_failure_resolution(
            identity, source, value, broadcast, lifecycle,
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
    pub extern "C" fn bray_runtime_substrate_shutdown() -> NativeRuntimeStatus {
        implementation::bray_runtime_substrate_shutdown()
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_cleanup_capacity_domain_release(
        binding: &mut bray_runtime_abi::NativeCleanupCapacityBinding,
    ) {
        implementation::bray_runtime_cleanup_capacity_domain_release(binding)
    }
}

native_adapter! {
    pub extern "C" fn bray_runtime_cleanup_capacity_domain_formation(
        destination: &mut bray_runtime_abi::NativeCleanupCapacityBinding,
    ) -> bray_runtime_abi::NativeRuntimeStatus {
        bray_runtime::native::implementation::bray_runtime_cleanup_capacity_domain_formation(destination)
    }
}
