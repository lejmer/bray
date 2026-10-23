use std::alloc::Layout;

use bray_runtime_abi::{NativeCleanupStorageLease, NativeCleanupStorageShape, NativeRuntimeStatus};

/// A byte lease retained by an unfinished native consumer of the Bray backing service.
/// Its consumer resolves initialized values before releasing it.
pub(crate) struct Backing {
    lease: NativeCleanupStorageLease,
    address: usize,
}

impl Backing {
    pub(crate) fn admit(size: usize, alignment: usize) -> Result<Self, NativeRuntimeStatus> {
        Layout::from_size_align(size, alignment)
            .map_err(|_| NativeRuntimeStatus::INVALID_ARGUMENT)?;

        if size == 0 {
            return Ok(Self {
                lease: NativeCleanupStorageLease::empty(),
                address: alignment,
            });
        }

        Self::admit_shape(NativeCleanupStorageShape::new([0; 32], size, alignment))
    }

    pub(crate) fn admit_shape(
        descriptor: NativeCleanupStorageShape,
    ) -> Result<Self, NativeRuntimeStatus> {
        let mut lease = NativeCleanupStorageLease::empty();

        let status =
            crate::report_provider::bray_runtime_cleanup_storage_admission(&descriptor, &mut lease);

        if status != NativeRuntimeStatus::SUCCESS.code() {
            return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
        }

        let pointer =
            crate::report_provider::bray_runtime_cleanup_storage_activation(&lease, &descriptor);

        assert!(
            !pointer.is_null(),
            "newly admitted cleanup storage must activate with its exact descriptor"
        );

        Ok(Self {
            lease,
            address: pointer.expose_provenance(),
        })
    }

    /// Transfers the exact lease to the generated incident's final release callback.
    pub(crate) fn transfer(mut self) -> NativeCleanupStorageLease {
        std::mem::replace(&mut self.lease, NativeCleanupStorageLease::empty())
    }

    pub(crate) const fn lease(&self) -> NativeCleanupStorageLease {
        self.lease
    }

    pub(crate) const fn address(&self) -> usize {
        self.address
    }
}

impl Drop for Backing {
    fn drop(&mut self) {
        if self.lease == NativeCleanupStorageLease::empty() {
            return;
        }

        let status = crate::report_provider::bray_runtime_cleanup_storage_release(&mut self.lease);

        assert_eq!(
            status,
            NativeRuntimeStatus::SUCCESS.code(),
            "owned cleanup storage must release exactly once"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::Backing;
    use bray_runtime_abi::NativeRuntimeStatus;

    #[test]
    fn backing_honors_zero_large_and_overaligned_layouts() {
        for (size, alignment) in [(0, 1), (1, 4096), (65537, 256)] {
            let backing = Backing::admit(size, alignment).expect("layout must be admitted");

            assert_ne!(backing.address(), 0);
            assert_eq!(backing.address() % alignment, 0);
        }
    }

    #[test]
    fn invalid_layouts_are_rejected_before_acquiring_storage() {
        for (size, alignment) in [(1, 0), (1, 3), (usize::MAX, 1)] {
            assert!(matches!(
                Backing::admit(size, alignment),
                Err(NativeRuntimeStatus::INVALID_ARGUMENT)
            ));
        }
    }
}
