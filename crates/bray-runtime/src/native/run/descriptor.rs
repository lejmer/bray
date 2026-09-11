use bray_runtime_abi::NativeRuntimeStatus;

/// Builds the admitted execution contract for sequential host cleanup activations.
pub(in crate::native) fn cleanup_descriptor(
    metadata: impl IntoIterator<Item = bray_runtime_abi::NativeFrameMetadataCallback>,
) -> Result<bray_runtime_model::ProtectedFrameDescriptor, NativeRuntimeStatus> {
    use bray_runtime_model::{
        ProtectedAsyncFrameId, ProtectedFrameAbiVersions, ProtectedFrameAffinity,
        ProtectedFrameDescriptor, ProtectedFrameLayout, ProtectedFrameStateDescriptor,
        ProtectedFrameStateId, RuntimeAbiVersion,
    };

    let mut states = Vec::new();

    crate::allocation::reserve_vec_entries(&mut states, 1)
        .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

    states.push(ProtectedFrameStateDescriptor::new(
        ProtectedFrameStateId::new(0),
        [],
        [],
        [],
        ProtectedFrameAffinity::OriginThread,
    ));

    for metadata in metadata {
        let descriptor = super::super::frame::NativeFrame::checked_descriptor(
            metadata().ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?,
        )?;

        for state in descriptor.states() {
            if states.iter().any(|existing| {
                existing.affinity() == state.affinity()
                    && existing.lane_requirements() == state.lane_requirements()
            }) {
                continue;
            }

            let next =
                u32::try_from(states.len()).map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

            crate::allocation::reserve_vec_entries(&mut states, 1)
                .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

            states.push(ProtectedFrameStateDescriptor::new(
                ProtectedFrameStateId::new(next),
                state.lane_requirements().iter().copied(),
                [],
                [],
                state.affinity(),
            ));
        }
    }

    let alignment =
        std::num::NonZeroUsize::new(std::mem::align_of::<triomphe::Arc<super::NativeRun>>())
            .ok_or(NativeRuntimeStatus::RUNTIME_FAILURE)?;

    let layout = ProtectedFrameLayout::try_new(
        std::mem::size_of::<triomphe::Arc<super::NativeRun>>(),
        alignment,
    )
    .map_err(|_| NativeRuntimeStatus::RUNTIME_FAILURE)?;

    let version = RuntimeAbiVersion::CURRENT;

    // This runtime-owned driver contract never identifies generated storage or participates in pooling.
    ProtectedFrameDescriptor::try_new(
        ProtectedAsyncFrameId::new(*b"bray.static.cleanup.v1\0\0\0\0\0\0\0\0\0\0"),
        version,
        ProtectedFrameAbiVersions::uniform(version),
        layout,
        layout,
        states,
    )
    .map_err(|_| NativeRuntimeStatus::INVALID_ARGUMENT)
}
