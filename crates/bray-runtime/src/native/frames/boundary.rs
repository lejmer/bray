use bray_runtime_abi::{NativeFrameMetadata, NativeFrameMetadataProvider, NativeRuntimeStatus};

use super::cleanup::{activate_cleanup, admit_cleanup, discharge_cleanup};
use super::registry::{admit, release};
use crate::native::export::contain_status;

/// Reserves a generated context before any captures transfer into it.
pub extern "C" fn bray_runtime_frame_storage_admission(
    metadata: Option<&NativeFrameMetadata>,
) -> usize {
    contain_address(metadata, admit)
}

/// Activates secured cleanup storage without performing a new allocation.
pub extern "C" fn bray_runtime_frame_storage_activation(
    metadata: Option<&NativeFrameMetadata>,
) -> usize {
    contain_address(metadata, |metadata| activate_cleanup(&metadata))
}

/// Releases a resolved context and all remaining unused task reservations.
pub extern "C" fn bray_runtime_frame_storage_release(context: usize) {
    release(context);
}

/// Secures an entire compiler-generated cleanup bundle before its owner is established.
pub extern "C" fn bray_runtime_cleanup_capacity_admission(
    count: usize,
    metadata: Option<NativeFrameMetadataProvider>,
) -> NativeRuntimeStatus {
    contain_status(|| {
        if count == 0 {
            return NativeRuntimeStatus::SUCCESS;
        }

        let Some(metadata) = metadata else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        admit_cleanup((0..count).map(|index| {
            metadata(index)
                .copied()
                .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)
        }))
        .err()
        .unwrap_or(NativeRuntimeStatus::SUCCESS)
    })
}

/// Retires one bundle credit after its owner is resolved, including statically omitted phases.
pub extern "C" fn bray_runtime_cleanup_capacity_discharge(
    metadata: Option<&NativeFrameMetadata>,
) -> NativeRuntimeStatus {
    contain_status(|| {
        let Some(metadata) = metadata else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        discharge_cleanup(metadata)
            .err()
            .unwrap_or(NativeRuntimeStatus::SUCCESS)
    })
}

fn contain_address(
    metadata: Option<&NativeFrameMetadata>,
    operation: fn(NativeFrameMetadata) -> Result<usize, NativeRuntimeStatus>,
) -> usize {
    std::panic::catch_unwind(|| {
        metadata
            .and_then(|metadata| operation(*metadata).ok())
            .unwrap_or(0)
    })
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use crate::native::frames::registry::registry;
    use crate::native::state::test_runtime_isolation;
    use crate::test_support::{native_origin_frame_state, with_allocation_failure};
    use bray_runtime_abi::{NativeFrameMetadata, NativeRuntimeStatus};

    static METADATA: [NativeFrameMetadata; 2] = [
        NativeFrameMetadata::new([192; 32], 2, 256, 64, 0, 1, native_origin_frame_state),
        NativeFrameMetadata::new([193; 32], 2, 512, 64, 0, 1, native_origin_frame_state),
    ];

    extern "C" fn metadata(index: usize) -> Option<&'static NativeFrameMetadata> {
        METADATA.get(index)
    }

    #[test]
    fn native_bundle_admission_preserves_existing_owners_when_a_provider_is_incomplete() {
        let _isolation = test_runtime_isolation();
        let ordinary = super::bray_runtime_frame_storage_admission(Some(&METADATA[0]));
        assert_ne!(ordinary, 0);

        assert_eq!(
            super::bray_runtime_cleanup_capacity_admission(3, Some(metadata)),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        {
            let state = registry().lock().unwrap();
            assert_eq!(state.frames.len(), 1);
            assert!(state.cleanup_capacity.is_empty());
        }

        with_allocation_failure(|| {
            assert_eq!(
                super::bray_runtime_frame_storage_activation(Some(&METADATA[0])),
                0
            );

            super::bray_runtime_frame_storage_release(ordinary);
        });

        assert!(registry().lock().unwrap().frames.is_empty());
    }

    #[test]
    fn native_activation_and_discharge_need_no_further_allocation() {
        let _isolation = test_runtime_isolation();

        assert_eq!(
            super::bray_runtime_cleanup_capacity_admission(2, Some(metadata)),
            NativeRuntimeStatus::SUCCESS
        );

        with_allocation_failure(|| {
            assert_eq!(
                super::bray_runtime_cleanup_capacity_admission(2, Some(metadata)),
                NativeRuntimeStatus::ALLOCATION_FAILURE
            );

            let active = super::bray_runtime_frame_storage_activation(Some(&METADATA[0]));
            assert_ne!(active, 0);

            for metadata in &METADATA {
                assert_eq!(
                    super::bray_runtime_cleanup_capacity_discharge(Some(metadata)),
                    NativeRuntimeStatus::SUCCESS
                );
            }

            super::bray_runtime_frame_storage_release(active);
        });

        let state = registry().lock().unwrap();
        assert!(state.frames.is_empty());
        assert!(state.cleanup_capacity.is_empty());
    }

    #[test]
    fn empty_and_missing_native_metadata_are_distinct_contracts() {
        let _isolation = test_runtime_isolation();

        with_allocation_failure(|| {
            assert_eq!(
                super::bray_runtime_cleanup_capacity_admission(0, None),
                NativeRuntimeStatus::SUCCESS
            );

            assert_eq!(
                super::bray_runtime_cleanup_capacity_admission(1, None),
                NativeRuntimeStatus::INVALID_ARGUMENT
            );

            assert_eq!(
                super::bray_runtime_cleanup_capacity_discharge(None),
                NativeRuntimeStatus::INVALID_ARGUMENT
            );

            assert_eq!(super::bray_runtime_frame_storage_activation(None), 0);
            assert_eq!(super::bray_runtime_frame_storage_admission(None), 0);
        });

        let state = registry().lock().unwrap();
        assert!(state.frames.is_empty());
        assert!(state.cleanup_capacity.is_empty());
    }
}
