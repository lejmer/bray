//! Ownership of prepared cleanup backing across native providers.

use std::num::NonZeroUsize;

use crate::NativeProviderRetention;

/// Prepared backing transferred from a cleanup allowance to an activation or incident.
///
/// The defining provider releases its own allocation. Native callers must move this
/// descriptor rather than copying its fields. Its address is never a borrowed pointer
/// into the descriptor itself.
#[derive(Debug)]
#[repr(C)]
pub struct NativeCleanupStorage {
    address: usize,
    context: usize,
    release: Option<extern "C" fn(usize, usize)>,
    retention: NativeProviderRetention,
}

impl NativeCleanupStorage {
    /// Creates an initialized output destination without an allocation or provider reference.
    pub const fn empty() -> Self {
        Self {
            address: 0,
            context: 0,
            release: None,
            retention: NativeProviderRetention::empty(),
        }
    }

    /// Adopts one prepared allocation and its already acquired provider reference.
    ///
    /// The callback consumes the allocation identified by context and address. It must
    /// support any thread, must not unwind, and remains valid through the retained provider.
    pub const fn owned(
        address: NonZeroUsize,
        context: usize,
        release: extern "C" fn(usize, usize),
        retention: NativeProviderRetention,
    ) -> Self {
        Self {
            address: address.get(),
            context,
            release: Some(release),
            retention,
        }
    }

    /// Returns the stable backing address, or no address for an empty output destination.
    pub const fn address(&self) -> Option<NonZeroUsize> {
        NonZeroUsize::new(self.address)
    }

    /// Returns whether this destination owns no prepared backing.
    pub const fn is_empty(&self) -> bool {
        self.release.is_none()
    }
}

impl Drop for NativeCleanupStorage {
    fn drop(&mut self) {
        if let Some(release) = self.release {
            release(self.context, self.address);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::NativeCleanupStorage;

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn cleanup_backing_has_a_movable_native_ownership_layout() {
        let word = std::mem::size_of::<usize>();

        assert_abi_layout!(NativeCleanupStorage, size: 5 * word, align: word, fields: {
            address: 0,
            context: word,
            release: 2 * word,
            retention: 3 * word,
        });

        assert_send_sync::<NativeCleanupStorage>();
        let empty = NativeCleanupStorage::empty();
        assert!(empty.is_empty());
        assert_eq!(empty.address(), None);
    }
}
