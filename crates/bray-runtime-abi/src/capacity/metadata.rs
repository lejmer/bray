use crate::{NativeCleanupStorage, NativeRuntimeStatus};

/// Supplies one action descriptor by ordinal during admission only.
pub type NativeCleanupCapacityMetadataProvider =
    extern "C" fn(usize) -> Option<&'static NativeCleanupCapacityMetadata>;

/// Provider-owned description of one cleanup action's prepared storage.
#[derive(Debug)]
#[repr(C)]
pub struct NativeCleanupCapacityMetadata {
    identity: [u8; 32],
    prepare: extern "C" fn(&mut NativeCleanupStorage) -> NativeRuntimeStatus,
}

impl NativeCleanupCapacityMetadata {
    /// Creates a descriptor whose callback prepares backing in an empty destination.
    ///
    /// The callback must not unwind. Prepared storage retains its defining provider.
    /// Admission must not retain a borrowed descriptor after its provider is released.
    pub const fn new(
        identity: [u8; 32],
        prepare: extern "C" fn(&mut NativeCleanupStorage) -> NativeRuntimeStatus,
    ) -> Self {
        Self { identity, prepare }
    }

    /// Returns the stable action identity across providers.
    pub const fn identity(&self) -> &[u8; 32] {
        &self.identity
    }

    /// Prepares backing without replacing an existing owned allocation.
    pub fn prepare(&self, destination: &mut NativeCleanupStorage) -> NativeRuntimeStatus {
        if !destination.is_empty() {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        (self.prepare)(destination)
    }
}

#[cfg(test)]
mod tests {
    use super::NativeCleanupCapacityMetadata;

    #[test]
    fn cleanup_metadata_has_the_native_abi_layout() {
        let word = std::mem::size_of::<usize>();

        assert_eq!(
            std::mem::size_of::<Option<super::NativeCleanupCapacityMetadataProvider>>(),
            word
        );

        assert_eq!(
            std::mem::align_of::<Option<super::NativeCleanupCapacityMetadataProvider>>(),
            std::mem::align_of::<usize>()
        );

        assert_abi_layout!(NativeCleanupCapacityMetadata, size: 32 + word, align: word, fields: {
            identity: 0,
            prepare: 32,
        });

        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<NativeCleanupCapacityMetadata>();
    }
}
