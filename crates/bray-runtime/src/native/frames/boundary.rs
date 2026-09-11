use bray_runtime_abi::{NativeFrameMetadata, NativeRuntimeStatus};

use super::cleanup::activate_cleanup;
use super::registry::{admit, release};

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

/// Releases a resolved context and its remaining unused activation reservations.
pub extern "C" fn bray_runtime_frame_storage_release(context: usize) {
    release(context);
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
    use crate::test_support::with_allocation_failure;

    #[test]
    fn missing_native_frame_metadata_is_rejected() {
        let _isolation = test_runtime_isolation();

        with_allocation_failure(|| {
            assert_eq!(super::bray_runtime_frame_storage_activation(None), 0);
            assert_eq!(super::bray_runtime_frame_storage_admission(None), 0);
        });

        let state = registry().lock().unwrap();
        assert!(state.frames.is_empty());
        assert!(state.cleanup_capacity.is_empty());
    }
}
