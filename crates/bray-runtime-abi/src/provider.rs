use crate::NativeRuntimeStatus;

/// Retains an already admitted provider closure without changing its binding.
pub type NativeProviderRetain = extern "C" fn(usize);
/// Releases one provider reference after the protected callback has returned.
pub type NativeProviderRelease = extern "C" fn(usize);
/// Observes live owning references, including the formation owner.
pub type NativeProviderReferences = extern "C" fn(usize) -> usize;

/// Immutable residency operations supplied by the resident native host.
/// The operations and their table outlive every supplied provider reference.
#[repr(C)]
#[derive(Debug)]
pub struct NativeProviderCallbacks {
    retain: NativeProviderRetain,
    release: NativeProviderRelease,
    references: NativeProviderReferences,
}

impl NativeProviderCallbacks {
    /// Creates the host's infallible, thread-safe residency contract.
    pub const fn new(
        retain: NativeProviderRetain,
        release: NativeProviderRelease,
        references: NativeProviderReferences,
    ) -> Self {
        Self {
            retain,
            release,
            references,
        }
    }
}

/// Borrowed residency operations for one loaded provider and its native dependencies.
/// Copying this view acquires no ownership. Its enclosing native owner keeps the
/// context and immutable callback table alive through every acquisition.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeProviderReference {
    context: usize,
    callbacks: Option<&'static NativeProviderCallbacks>,
}

impl NativeProviderReference {
    /// Creates a reference to code that remains resident for the process lifetime.
    pub const fn resident() -> Self {
        Self {
            context: 0,
            callbacks: None,
        }
    }

    /// Borrows a resident host's operations for one admitted provider closure.
    pub const fn new(context: usize, callbacks: &'static NativeProviderCallbacks) -> Self {
        Self {
            context,
            callbacks: Some(callbacks),
        }
    }

    /// Returns whether the reference has the native resident or retained shape.
    pub const fn is_valid(self) -> bool {
        self.callbacks.is_some() || self.context == 0
    }

    /// Returns whether the reference names an unloadable provider closure.
    pub const fn is_retained(self) -> bool {
        self.callbacks.is_some()
    }

    /// Compares the frozen native owner and operations, rather than its image address.
    pub fn same_owner(self, other: Self) -> bool {
        self.context == other.context
            && match (self.callbacks, other.callbacks) {
                (Some(left), Some(right)) => std::ptr::eq(left, right),
                (None, None) => true,
                _ => false,
            }
    }

    /// Acquires one owner of the admitted closure. Ordinary moves do not acquire again.
    pub fn retain(self) -> Result<NativeProviderOwner, NativeRuntimeStatus> {
        if !self.is_valid() {
            return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
        }

        if let Some(callbacks) = self.callbacks {
            (callbacks.retain)(self.context);
        }

        Ok(NativeProviderOwner { reference: self })
    }

    /// Observes whether formation is the only remaining owning reference.
    /// The product host separately closes admission and establishes cleanup eligibility.
    pub fn can_retire(self) -> bool {
        self.callbacks
            .is_none_or(|callbacks| (callbacks.references)(self.context) == 1)
    }
}

/// Movable residency retained through backing disposal and callback return.
#[repr(C)]
#[derive(Debug)]
pub struct NativeProviderOwner {
    reference: NativeProviderReference,
}

impl NativeProviderOwner {
    /// Creates an owner of process-resident code without acquiring a provider reference.
    pub const fn resident() -> Self {
        Self {
            reference: NativeProviderReference::resident(),
        }
    }

    /// Borrows this owner's already admitted residency contract.
    pub const fn reference(&self) -> NativeProviderReference {
        self.reference
    }

    pub(crate) fn into_reference(mut self) -> NativeProviderReference {
        std::mem::replace(&mut self.reference, NativeProviderReference::resident())
    }

    /// Retains the same provider for a caller guard without rebinding its services.
    pub fn retain(&self) -> Self {
        if let Some(callbacks) = self.reference.callbacks {
            (callbacks.retain)(self.reference.context);
        }

        Self {
            reference: self.reference,
        }
    }
}

impl Drop for NativeProviderOwner {
    fn drop(&mut self) {
        if let Some(callbacks) = self.reference.callbacks.take() {
            (callbacks.release)(self.reference.context);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    use super::{NativeProviderCallbacks, NativeProviderOwner, NativeProviderReference};
    use crate::{
        NativePanicCause, NativePanicMessage, NativePanicPrimary, NativePanicReport,
        NativeReportRecords, NativeRuntimeStatus, NativeSourceAnchor,
    };

    static CALLBACK_TEST: Mutex<()> = Mutex::new(());
    static REFERENCES: AtomicUsize = AtomicUsize::new(0);
    static LOCALS_RETURNED: AtomicBool = AtomicBool::new(false);
    static RELEASE_AFTER_LOCALS: AtomicBool = AtomicBool::new(false);
    static HELD_THROUGH_CALLBACK: AtomicBool = AtomicBool::new(false);

    extern "C" fn retain(_: usize) {
        REFERENCES.fetch_add(1, Ordering::SeqCst);
    }

    extern "C" fn release(_: usize) {
        if REFERENCES.fetch_sub(1, Ordering::SeqCst) == 1 {
            RELEASE_AFTER_LOCALS.store(LOCALS_RETURNED.load(Ordering::SeqCst), Ordering::SeqCst);
        }
    }

    extern "C" fn references(_: usize) -> usize {
        REFERENCES.load(Ordering::SeqCst)
    }

    static CALLBACKS: NativeProviderCallbacks =
        NativeProviderCallbacks::new(retain, release, references);

    struct CallbackLocal;

    impl Drop for CallbackLocal {
        fn drop(&mut self) {
            HELD_THROUGH_CALLBACK.store(REFERENCES.load(Ordering::SeqCst) == 1, Ordering::SeqCst);
            LOCALS_RETURNED.store(true, Ordering::SeqCst);
        }
    }

    extern "C" fn consume(report: &mut NativePanicReport, _: u32) -> NativeRuntimeStatus {
        let _local = CallbackLocal;

        drop(report.take_parts());

        NativeRuntimeStatus::SUCCESS
    }

    #[test]
    fn provider_reference_shares_its_resident_callback_table() {
        assert_abi_layout!(NativeProviderCallbacks, size: 24, align: 8, fields: {
            retain: 0, release: 8, references: 16,
        });

        assert_abi_layout!(NativeProviderReference, size: 16, align: 8, fields: {
            context: 0, callbacks: 8,
        });

        assert_abi_layout!(NativeProviderOwner, size: 16, align: 8, fields: { reference: 0 });
    }

    #[test]
    fn native_caller_retains_primary_and_chain_only_providers_through_callback_locals() {
        let _isolation = CALLBACK_TEST
            .lock()
            .expect("provider callback test owns its native fixture");

        for primary in [true, false] {
            REFERENCES.store(0, Ordering::SeqCst);
            LOCALS_RETURNED.store(false, Ordering::SeqCst);
            RELEASE_AFTER_LOCALS.store(false, Ordering::SeqCst);
            HELD_THROUGH_CALLBACK.store(false, Ordering::SeqCst);

            let mut report = if primary {
                NativePanicReport::new(
                    NativePanicPrimary::new(
                        NativePanicCause::MESSAGE,
                        NativeSourceAnchor::unavailable(),
                        NativePanicMessage::empty(),
                    ),
                    consume,
                )
            } else {
                NativePanicReport::from_records(NativeReportRecords::default(), consume)
            };

            report.retain_provider(
                NativeProviderReference::new(0, &CALLBACKS)
                    .retain()
                    .expect("fixture supplies a valid native owner"),
            );

            assert_eq!(REFERENCES.load(Ordering::SeqCst), 1);
            assert!(report.consume(false).is_success());
            assert!(HELD_THROUGH_CALLBACK.load(Ordering::SeqCst));
            assert!(RELEASE_AFTER_LOCALS.load(Ordering::SeqCst));
            assert_eq!(REFERENCES.load(Ordering::SeqCst), 0);
        }
    }
}
