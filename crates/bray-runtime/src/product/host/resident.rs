use bray_runtime_abi::{
    NativeProductHostObservation, NativeProductHostOperation, NativeProductHostState,
    NativeProductHostStatus, NativeRuntimeStatus, NativeStaticIdentity,
};

#[expect(
    unsafe_code,
    reason = "the resident Bray provider owns product state and admitted static metadata"
)]
unsafe extern "C" {
    safe fn bray_runtime_product_host_control(
        product: usize,
        operation: NativeProductHostOperation,
        output: &mut NativeProductHostObservation,
    ) -> NativeRuntimeStatus;
    safe fn bray_runtime_product_execution_resource(
        product: usize,
        output: &mut usize,
    ) -> NativeRuntimeStatus;
    safe fn bray_runtime_product_cleanup_incident(
        product: usize,
        identity: &NativeStaticIdentity,
        count: usize,
    ) -> NativeRuntimeStatus;
    safe fn bray_runtime_product_thread_static_order(
        product: usize,
        identity: &NativeStaticIdentity,
        execution: u32,
        output: &mut u64,
    ) -> NativeRuntimeStatus;
}

pub(in crate::product) fn observe(
    product: usize,
    operation: NativeProductHostOperation,
) -> NativeProductHostObservation {
    let mut observation = host_failure(NativeRuntimeStatus::RUNTIME_FAILURE);

    let status = bray_runtime_product_host_control(product, operation, &mut observation);

    if !status.is_success() {
        return host_failure(status);
    }

    observation
}

pub(super) fn execution_runtime(product: usize) -> Option<crate::native::RetainedRuntime> {
    let mut resource = 0;

    assert!(
        bray_runtime_product_execution_resource(product, &mut resource).is_success(),
        "exact-thread cleanup retains its admitted execution service"
    );

    super::super::execution::runtime(resource)
}

pub(super) fn static_order(
    product: usize,
    identity: NativeStaticIdentity,
    execution: u32,
) -> Result<u64, NativeRuntimeStatus> {
    let mut order = 0;

    let status =
        bray_runtime_product_thread_static_order(product, &identity, execution, &mut order);

    status.is_success().then_some(order).ok_or(status)
}

pub(super) fn report_incidents(product: usize, identity: NativeStaticIdentity, count: usize) {
    assert!(
        bray_runtime_product_cleanup_incident(product, &identity, count).is_success(),
        "drained exact-thread incidents retain the common product owner"
    );
}

pub(in crate::product) fn observation_with_status(
    product: usize,
    status: NativeProductHostStatus,
) -> NativeProductHostObservation {
    let observed = observe(product, NativeProductHostOperation::OBSERVE);

    NativeProductHostObservation::new(
        status,
        observed.state(),
        observed.active_entries(),
        observed.external_roots(),
        observed.thread_attachments(),
        observed.initialized_statics(),
        observed.cleaned_statics(),
        observed.cleanup_incidents(),
        observed.last_incident(),
    )
}

fn host_failure(status: NativeRuntimeStatus) -> NativeProductHostObservation {
    NativeProductHostObservation::new(
        match status {
            NativeRuntimeStatus::IDENTITY_EXHAUSTED => NativeProductHostStatus::IDENTITY_EXHAUSTED,
            NativeRuntimeStatus::OUTGOING_STORAGE_UNAVAILABLE => {
                NativeProductHostStatus::OUTGOING_STORAGE_UNAVAILABLE
            }
            _ => NativeProductHostStatus::RUNTIME_FAILURE,
        },
        NativeProductHostState::FAILED,
        0,
        0,
        0,
        0,
        0,
        0,
        NativeStaticIdentity::new([0; 32]),
    )
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::{NativeProductHostStatus, NativeRuntimeStatus};

    #[test]
    fn transport_failures_preserve_specific_admission_causes() {
        for (native, expected) in [
            (
                NativeRuntimeStatus::IDENTITY_EXHAUSTED,
                NativeProductHostStatus::IDENTITY_EXHAUSTED,
            ),
            (
                NativeRuntimeStatus::OUTGOING_STORAGE_UNAVAILABLE,
                NativeProductHostStatus::OUTGOING_STORAGE_UNAVAILABLE,
            ),
            (
                NativeRuntimeStatus::RUNTIME_FAILURE,
                NativeProductHostStatus::RUNTIME_FAILURE,
            ),
        ] {
            assert_eq!(super::host_failure(native).status(), expected);
        }
    }
}
