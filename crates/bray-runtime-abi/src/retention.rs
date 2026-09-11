//! Owned references that keep native providers available during retirement.

use std::num::NonZeroUsize;

/// Immutable callbacks supplied by a resident retention service.
///
/// The service and its callbacks must remain available until every retained reference
/// is released. Both callbacks must support calls from any thread without unwinding.
#[derive(Debug)]
#[repr(C)]
pub struct NativeProviderRetentionCallbacks {
    retain: extern "C" fn(usize),
    release: extern "C" fn(usize),
}

impl NativeProviderRetentionCallbacks {
    /// Creates the resident service callbacks for acquiring and releasing one reference.
    ///
    /// Retaining a live context must succeed. Releasing its last reference may retire
    /// the provider but must not invalidate the resident callback service itself.
    pub const fn new(retain: extern "C" fn(usize), release: extern "C" fn(usize)) -> Self {
        Self { retain, release }
    }
}

/// One owned provider reference, or an initialized empty output destination.
///
/// Native callers must move this value rather than copying or rebinding its fields.
#[derive(Debug)]
#[repr(C)]
pub struct NativeProviderRetention {
    context: usize,
    callbacks: Option<&'static NativeProviderRetentionCallbacks>,
}

impl NativeProviderRetention {
    /// Adopts one already retained reference to the context without retaining it again.
    ///
    /// The context must belong to this callback service and remain valid until its
    /// last reference is released. The callback service must have a stable lifetime
    /// independent of the provider it retains.
    pub const fn new(
        context: NonZeroUsize,
        callbacks: &'static NativeProviderRetentionCallbacks,
    ) -> Self {
        Self {
            context: context.get(),
            callbacks: Some(callbacks),
        }
    }

    /// Creates an empty owner suitable for an initialized native output destination.
    pub const fn empty() -> Self {
        Self {
            context: 0,
            callbacks: None,
        }
    }

    /// Returns whether this value owns no provider reference.
    pub const fn is_empty(&self) -> bool {
        self.callbacks.is_none()
    }
}

impl Clone for NativeProviderRetention {
    fn clone(&self) -> Self {
        if let Some(callbacks) = self.callbacks {
            (callbacks.retain)(self.context);
        }

        Self {
            context: self.context,
            callbacks: self.callbacks,
        }
    }
}

impl Drop for NativeProviderRetention {
    fn drop(&mut self) {
        if let Some(callbacks) = self.callbacks {
            (callbacks.release)(self.context);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NativeProviderRetention, NativeProviderRetentionCallbacks};

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn provider_retention_has_the_native_abi_layout() {
        let word = std::mem::size_of::<usize>();
        let alignment = std::mem::align_of::<usize>();

        assert_abi_layout!(NativeProviderRetention, size: 2 * word, align: alignment, fields: {
            context: 0,
            callbacks: word,
        });

        assert_abi_layout!(NativeProviderRetentionCallbacks, size: 2 * word, align: alignment, fields: {
            retain: 0,
            release: word,
        });

        assert_send_sync::<NativeProviderRetention>();
    }
}
