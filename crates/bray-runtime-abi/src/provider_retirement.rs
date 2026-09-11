use std::num::NonZeroUsize;

use crate::{NativeProviderRetention, NativeRuntimeStatus};

/// Resident observation published only after provider teardown has returned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct NativeProviderRetirementObservation {
    references: usize,
    status: NativeRuntimeStatus,
    complete: bool,
}

impl NativeProviderRetirementObservation {
    /// Records outstanding references and the exact teardown result.
    /// Completion may be published only after the teardown callback returns.
    pub const fn new(references: usize, status: NativeRuntimeStatus, complete: bool) -> Self {
        Self {
            references,
            status,
            complete,
        }
    }

    /// Returns the outstanding provider references.
    pub const fn references(&self) -> usize {
        self.references
    }

    /// Returns the precise teardown status.
    pub const fn status(&self) -> NativeRuntimeStatus {
        self.status
    }

    /// Returns whether provider teardown has returned successfully.
    pub const fn is_complete(&self) -> bool {
        self.complete
    }
}

/// Resident callbacks that outlive every registration and provider reference callback.
/// All callbacks support any thread and must not unwind. Acquire and observe do not
/// allocate, invoke provider code, or release owned objects. They may be called while
/// the provider host is locked. Begin and release may call providers only outside service locks.
#[derive(Debug)]
#[repr(C)]
pub struct NativeProviderRetirementCallbacks {
    acquire: extern "C" fn(usize, &mut NativeProviderRetention) -> NativeRuntimeStatus,
    begin: extern "C" fn(usize) -> NativeRuntimeStatus,
    observe: extern "C" fn(usize) -> NativeProviderRetirementObservation,
    release: extern "C" fn(usize),
}

impl NativeProviderRetirementCallbacks {
    /// Creates callbacks for retaining a provider, starting retirement, observing
    /// teardown completion, and releasing the owned registration.
    pub const fn new(
        acquire: extern "C" fn(usize, &mut NativeProviderRetention) -> NativeRuntimeStatus,
        begin: extern "C" fn(usize) -> NativeRuntimeStatus,
        observe: extern "C" fn(usize) -> NativeProviderRetirementObservation,
        release: extern "C" fn(usize),
    ) -> Self {
        Self {
            acquire,
            begin,
            observe,
            release,
        }
    }
}

/// Owned provider registration in a resident retirement service.
/// Native callers move this value rather than copying its fields.
#[derive(Debug)]
#[repr(C)]
pub struct NativeProviderRetirement {
    context: usize,
    callbacks: Option<&'static NativeProviderRetirementCallbacks>,
}

impl NativeProviderRetirement {
    /// Creates an initialized empty registration destination.
    pub const fn empty() -> Self {
        Self {
            context: 0,
            callbacks: None,
        }
    }

    /// Adopts one registration whose service remains resident through every callback.
    pub const fn new(
        context: NonZeroUsize,
        callbacks: &'static NativeProviderRetirementCallbacks,
    ) -> Self {
        Self {
            context: context.get(),
            callbacks: Some(callbacks),
        }
    }

    /// Returns whether no registration is owned.
    pub const fn is_empty(&self) -> bool {
        self.callbacks.is_none()
    }

    /// Acquires a provider reference into an initialized empty destination.
    pub fn acquire(&self, destination: &mut NativeProviderRetention) -> NativeRuntimeStatus {
        let Some(callbacks) = self.callbacks else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        if self.context == 0 || !destination.is_empty() {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        (callbacks.acquire)(self.context, destination)
    }

    /// Requests teardown once existing references and references derived from them have drained.
    pub fn begin(&self) -> NativeRuntimeStatus {
        let Some(callbacks) = self.callbacks.filter(|_| self.context != 0) else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        (callbacks.begin)(self.context)
    }

    /// Observes resident completion without executing provider code.
    pub fn observe(&self) -> NativeProviderRetirementObservation {
        let Some(callbacks) = self.callbacks.filter(|_| self.context != 0) else {
            return NativeProviderRetirementObservation::new(
                0,
                NativeRuntimeStatus::INVALID_ARGUMENT,
                false,
            );
        };

        (callbacks.observe)(self.context)
    }
}

impl Drop for NativeProviderRetirement {
    fn drop(&mut self) {
        if let Some(callbacks) = self.callbacks {
            (callbacks.release)(self.context);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NativeProviderRetirement, NativeProviderRetirementCallbacks,
        NativeProviderRetirementObservation,
    };

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn resident_retirement_layout_preserves_registration_ownership() {
        let word = std::mem::size_of::<usize>();

        assert_abi_layout!(NativeProviderRetirement, size: 2 * word, align: word, fields: {
            context: 0,
            callbacks: word,
        });

        assert_abi_layout!(NativeProviderRetirementCallbacks, size: 4 * word, align: word, fields: {
            acquire: 0,
            begin: word,
            observe: 2 * word,
            release: 3 * word,
        });

        assert_abi_layout!(NativeProviderRetirementObservation, size: word + 8, align: word, fields: {
            references: 0,
            status: word,
            complete: word + 4,
        });

        assert_send_sync::<NativeProviderRetirement>();
        assert_send_sync::<NativeProviderRetirementCallbacks>();
        assert_send_sync::<NativeProviderRetirementObservation>();
        let empty = NativeProviderRetirement::empty();
        assert!(empty.is_empty());
        assert!(!empty.observe().is_complete());
        assert_eq!(empty.begin(), crate::NativeRuntimeStatus::INVALID_ARGUMENT);
    }
}
