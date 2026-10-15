use std::alloc::Layout;

use bray_runtime_abi::NativeRuntimeStatus;

/// An allocation retained by an unfinished native consumer of the Bray backing service.
/// It owns bytes only. Its consumer resolves initialized values before releasing it.
pub(crate) struct Backing {
    address: usize,
    layout: Layout,
}

impl Backing {
    pub(crate) fn admit(size: usize, alignment: usize) -> Result<Self, NativeRuntimeStatus> {
        let layout = Layout::from_size_align(size, alignment)
            .map_err(|_| NativeRuntimeStatus::INVALID_ARGUMENT)?;

        if size == 0 {
            return Ok(Self {
                address: alignment,
                layout,
            });
        }

        let pointer =
            crate::report_provider::bray_runtime_cleanup_storage_admission(size, alignment);

        if pointer.is_null() {
            return Err(NativeRuntimeStatus::RUNTIME_FAILURE);
        }

        // Native callbacks pass addresses across the C ABI. Retain the allocation owner
        // separately so exposing the address cannot transfer or duplicate its release duty.
        Ok(Self {
            address: pointer.expose_provenance(),
            layout,
        })
    }

    /// Transfers release responsibility to a generated incident destructor.
    pub(crate) fn transfer(self) {
        std::mem::forget(self);
    }

    pub(crate) const fn address(&self) -> usize {
        self.address
    }

    pub(crate) fn pointer(&self) -> *mut u8 {
        std::ptr::with_exposed_provenance_mut(self.address)
    }
}

impl Drop for Backing {
    fn drop(&mut self) {
        if self.layout.size() == 0 {
            return;
        }

        crate::report_provider::bray_runtime_cleanup_storage_release(
            self.pointer(),
            self.layout.size(),
            self.layout.align(),
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
