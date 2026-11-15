use bray_runtime_abi::{
    NativeEventCallback, NativeEventReference, NativeRuntimeStatus, NativeTaskReference,
};

// The Source provider initializes these opaque owners. Each call borrows an
// actual retained owner or validates an external identity at the resident host.
#[expect(
    unsafe_code,
    reason = "the Bray provider owns event storage and transitions"
)]
unsafe extern "C" {
    #[link_name = "bray_runtime_event_creation"]
    pub(crate) safe fn creation(runtime: u64, output: &mut NativeEventReference) -> usize;
    #[link_name = "bray_runtime_event_acquisition"]
    pub(crate) safe fn acquisition(
        runtime: u64,
        identity: usize,
        output: &mut NativeEventReference,
    ) -> u32;
    #[link_name = "bray_runtime_event_release"]
    pub(crate) safe fn release(reference: &NativeEventReference);
    #[link_name = "bray_runtime_event_observation"]
    pub(super) safe fn observation(reference: &NativeEventReference, generation: &mut u64) -> u32;
    #[link_name = "bray_runtime_event_registration"]
    pub(super) safe fn registration(
        reference: &NativeEventReference,
        observed: u64,
        parent: &NativeTaskReference,
        descriptor: &NativeEventCallback,
    ) -> NativeRuntimeStatus;
    #[link_name = "bray_runtime_event_readiness"]
    pub(super) safe fn readiness(reference: &NativeTaskReference) -> u32;
    #[link_name = "bray_runtime_event_withdrawal"]
    pub(super) safe fn withdrawal(reference: &NativeTaskReference) -> u32;
    #[link_name = "bray_runtime_event_signal"]
    pub(crate) safe fn signal(identity: usize) -> NativeRuntimeStatus;
    #[link_name = "bray_runtime_event_destruction"]
    pub(crate) safe fn destruction(runtime: u64, identity: usize) -> NativeRuntimeStatus;
    #[link_name = "bray_runtime_event_clear"]
    pub(crate) safe fn clear(runtime: u64);
}
