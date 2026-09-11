//! Movable panic primary data and provider-owned immutable message bytes.

use crate::{NativePanicCause, NativeProviderRetention, NativeRuntimeStatus, NativeSourceAnchor};

/// Immutable bytes retained with their provider, optionally owning releasable backing.
/// Native callers move this descriptor. Copying its fields would duplicate ownership.
#[derive(Debug)]
#[repr(C)]
pub struct NativePanicMessage {
    address: usize,
    length: usize,
    context: usize,
    copy: Option<extern "C" fn(usize, usize, usize, *mut u8, usize) -> NativeRuntimeStatus>,
    release: Option<extern "C" fn(usize, usize, usize)>,
    _retention: NativeProviderRetention,
}

impl NativePanicMessage {
    /// Creates an initialized empty message without provider ownership.
    pub const fn empty() -> Self {
        Self {
            address: 0,
            length: 0,
            context: 0,
            copy: None,
            release: None,
            _retention: NativeProviderRetention::empty(),
        }
    }

    /// Adopts immutable bytes whose lifetime is covered by the supplied provider reference.
    /// The provider must keep the bytes and copy callback valid until this owner is dropped.
    /// Callbacks must support any thread and must not unwind. The copy callback writes exactly
    /// the requested range into the supplied destination without retaining that destination.
    pub const fn retained(
        address: usize,
        length: usize,
        context: usize,
        copy: extern "C" fn(usize, usize, usize, *mut u8, usize) -> NativeRuntimeStatus,
        retention: NativeProviderRetention,
    ) -> Self {
        Self {
            address,
            length,
            context,
            copy: Some(copy),
            release: None,
            _retention: retention,
        }
    }

    /// Adopts one exclusively owned immutable backing allocation and its provider reference.
    /// The release callback consumes the backing exactly once, before provider retention ends.
    /// The byte lifetime, copy contract, and callback requirements match [`Self::retained`].
    pub const fn owned(
        address: usize,
        length: usize,
        context: usize,
        copy: extern "C" fn(usize, usize, usize, *mut u8, usize) -> NativeRuntimeStatus,
        release: extern "C" fn(usize, usize, usize),
        retention: NativeProviderRetention,
    ) -> Self {
        Self {
            address,
            length,
            context,
            copy: Some(copy),
            release: Some(release),
            _retention: retention,
        }
    }

    /// Returns the byte length of the immutable message.
    pub const fn len(&self) -> usize {
        self.length
    }

    /// Returns whether the message contains no bytes.
    pub const fn is_empty(&self) -> bool {
        self.length == 0
    }

    /// Copies one bounded range without allocating. Provider failures retain their exact status.
    pub fn copy_to(&self, offset: usize, destination: &mut [u8]) -> NativeRuntimeStatus {
        if offset > self.length || destination.len() > self.length - offset {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        if destination.is_empty() {
            return NativeRuntimeStatus::SUCCESS;
        }

        let Some(copy) = self.copy else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        copy(
            self.context,
            self.address,
            offset,
            destination.as_mut_ptr(),
            destination.len(),
        )
    }
}

impl Drop for NativePanicMessage {
    fn drop(&mut self) {
        if let Some(release) = self.release {
            release(self.context, self.address, self.length);
        }
    }
}

/// Inline primary data that moves with a report header or into a stable outgoing record.
#[derive(Debug)]
#[repr(C)]
pub struct NativePanicPrimary {
    cause: NativePanicCause,
    source: NativeSourceAnchor,
    message: NativePanicMessage,
}

impl NativePanicPrimary {
    /// Adopts a message with its structured cause and exact source occurrence.
    pub const fn new(
        cause: NativePanicCause,
        source: NativeSourceAnchor,
        message: NativePanicMessage,
    ) -> Self {
        Self {
            cause,
            source,
            message,
        }
    }

    /// Returns the structured primary cause.
    pub const fn cause(&self) -> NativePanicCause {
        self.cause
    }

    /// Returns the source occurrence of the primary failure.
    pub const fn source(&self) -> NativeSourceAnchor {
        self.source
    }

    /// Borrows the owned immutable message.
    pub const fn message(&self) -> &NativePanicMessage {
        &self.message
    }
}

#[cfg(test)]
mod tests {
    use super::{NativePanicMessage, NativePanicPrimary};
    use crate::{
        NativePanicCause, NativeProviderRetention, NativeProviderRetentionCallbacks,
        NativeRuntimeStatus, NativeSourceAnchor,
    };
    use std::num::NonZeroUsize;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static PHASE: AtomicUsize = AtomicUsize::new(0);
    static COPIES: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn retain(_: usize) {
        panic!("moving a message does not retain again");
    }
    extern "C" fn release_provider(context: usize) {
        assert_eq!(context, 7);
        assert_eq!(PHASE.swap(2, Ordering::SeqCst), 1);
    }
    extern "C" fn release_backing(context: usize, address: usize, length: usize) {
        assert_eq!((context, address, length), (9, 11, 3));
        assert_eq!(PHASE.swap(1, Ordering::SeqCst), 0);
    }
    extern "C" fn copy(
        context: usize,
        address: usize,
        offset: usize,
        destination: *mut u8,
        length: usize,
    ) -> NativeRuntimeStatus {
        assert_eq!((context, address, offset, length), (9, 11, 1, 2));
        assert!(!destination.is_null());
        COPIES.fetch_add(1, Ordering::SeqCst);

        NativeRuntimeStatus::RUNTIME_FAILURE
    }

    #[test]
    fn moved_primary_releases_backing_before_provider_and_checks_copy_bounds() {
        static CALLBACKS: NativeProviderRetentionCallbacks =
            NativeProviderRetentionCallbacks::new(retain, release_provider);

        PHASE.store(0, Ordering::SeqCst);
        COPIES.store(0, Ordering::SeqCst);
        let retention = NativeProviderRetention::new(NonZeroUsize::new(7).unwrap(), &CALLBACKS);
        let message = NativePanicMessage::owned(11, 3, 9, copy, release_backing, retention);

        let primary = NativePanicPrimary::new(
            NativePanicCause::MESSAGE,
            NativeSourceAnchor::unavailable(),
            message,
        );

        let moved = primary;

        assert_eq!(
            moved.message().copy_to(1, &mut [0; 2]),
            NativeRuntimeStatus::RUNTIME_FAILURE
        );

        assert_eq!(
            moved.message().copy_to(2, &mut [0; 2]),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            moved.message().copy_to(usize::MAX, &mut []),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            moved.message().copy_to(3, &mut []),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(COPIES.load(Ordering::SeqCst), 1);
        assert_eq!(PHASE.load(Ordering::SeqCst), 0);
        drop(moved);
        assert_eq!(PHASE.load(Ordering::SeqCst), 2);

        assert_eq!(
            NativePanicMessage::empty().copy_to(0, &mut []),
            NativeRuntimeStatus::SUCCESS
        );
    }

    #[test]
    fn message_and_primary_have_movable_native_layouts() {
        fn assert_send_sync<T: Send + Sync>() {}
        let word = std::mem::size_of::<usize>();

        assert_abi_layout!(NativePanicMessage, size: 7 * word, align: std::mem::align_of::<usize>(), fields: {
            address: 0, length: word, context: 2 * word, copy: 3 * word,
            release: 4 * word, _retention: 5 * word,
        });

        assert_abi_layout!(NativePanicPrimary, size: 32 + 7 * word, align: 8, fields: {
            cause: 0, source: 8, message: 32,
        });

        assert_send_sync::<NativePanicMessage>();
        assert_send_sync::<NativePanicPrimary>();
    }
}
