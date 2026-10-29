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

pub(crate) fn drain_product_thread_statics(product: usize) -> NativeProductHostObservation {
    observe(product, NativeProductHostOperation::DRAIN_THREAD_STATICS)
}
