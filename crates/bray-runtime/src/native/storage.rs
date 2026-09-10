use std::alloc::Layout;

use bray_runtime_abi::NativeRuntimeStatus;

/// Owns stable, zero-initialized bytes for compiler-described native frames and results.
pub(super) struct NativeStorage {
    storage: Vec<u8>,
    offset: usize,
}

impl NativeStorage {
    pub(super) fn new(size: usize, alignment: usize) -> Result<Self, NativeRuntimeStatus> {
        let layout = Layout::from_size_align(size.max(1), alignment)
            .map_err(|_| NativeRuntimeStatus::INVALID_ARGUMENT)?;

        let length = layout
            .size()
            .checked_add(layout.align() - 1)
            .filter(|length| isize::try_from(*length).is_ok())
            .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

        let mut storage = Vec::new();

        if size != 0 {
            #[cfg(test)]
            if crate::test_support::allocation_should_fail() {
                return Err(NativeRuntimeStatus::ALLOCATION_FAILURE);
            }

            storage
                .try_reserve_exact(length)
                .map_err(|_| NativeRuntimeStatus::ALLOCATION_FAILURE)?;

            storage.resize(length, 0);
        }

        // A byte allocation has unit stride, so every valid power-of-two alignment can
        // be reached within the extra alignment - 1 bytes reserved above. Empty results use an
        // aligned dangling address: their transfer touches no bytes and needs no allocation.
        let offset = storage.as_ptr().align_offset(layout.align());

        Ok(Self { storage, offset })
    }

    pub(super) fn pointer(&self) -> *mut u8 {
        self.storage.as_ptr().wrapping_add(self.offset).cast_mut()
    }

    pub(super) fn address(&self) -> usize {
        self.pointer().addr()
    }
}

#[cfg(test)]
mod tests {
    use super::NativeStorage;
    use bray_runtime_abi::NativeRuntimeStatus;

    #[test]
    fn result_allocations_preserve_alignment_zeroing_and_address_across_moves() {
        for size in [0, 1, 15, 16, 17, 1024] {
            for alignment in [1, 2, 8, 16, 32, 64, 4096] {
                let storage = NativeStorage::new(size, alignment).unwrap();
                let address = storage.address();

                assert_ne!(address, 0);
                assert_eq!(address % alignment, 0);

                if size == 0 {
                    assert_eq!(storage.storage.capacity(), 0);
                } else {
                    assert!(storage.offset + size <= storage.storage.len());
                }

                assert!(storage.storage.iter().all(|byte| *byte == 0));

                let moved = Box::new(storage);

                assert_eq!(moved.address(), address);
            }
        }
    }

    #[test]
    fn empty_completion_storage_remains_available_when_allocations_fail() {
        crate::test_support::with_allocation_failure(|| {
            for alignment in [1, 8, 64, 4096] {
                let storage = NativeStorage::new(0, alignment).unwrap();

                assert_ne!(storage.address(), 0);
                assert_eq!(storage.address() % alignment, 0);
                assert_eq!(storage.storage.capacity(), 0);
            }
        });
    }

    #[test]
    fn invalid_result_layouts_fail_before_allocation() {
        for (size, alignment) in [(1, 0), (1, 3), (usize::MAX, 1), (usize::MAX, 4096)] {
            assert_eq!(
                NativeStorage::new(size, alignment).err(),
                Some(NativeRuntimeStatus::INVALID_ARGUMENT)
            );
        }
    }
}
