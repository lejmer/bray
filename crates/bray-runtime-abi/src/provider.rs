use crate::NativeRuntimeStatus;

/// Retains an already admitted provider closure without changing its binding.
pub type NativeProviderRetain = extern "C" fn(usize);
/// Releases one provider reference after the protected callback has returned.
pub type NativeProviderRelease = extern "C" fn(usize);
/// Observes live owning references, including the formation owner.
pub type NativeProviderReferences = extern "C" fn(usize) -> usize;

/// Host-owned residency operations for a loaded provider and its native dependencies.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeProviderReference {
    context: usize,
    retain: Option<NativeProviderRetain>,
    release: Option<NativeProviderRelease>,
    references: Option<NativeProviderReferences>,
}

impl NativeProviderReference {
    /// Creates a reference to code that remains resident for the process lifetime.
    pub const fn resident() -> Self {
        Self {
            context: 0,
            retain: None,
            release: None,
            references: None,
        }
    }

    /// Creates the native host's infallible residency contract.
    /// The operations must be safe from every thread that can dispose transferred ownership.
    /// Their code and context belong to the native host, outside the unloadable closure.
    /// Count only admitted owning references, including formation. The caller also keeps its
    /// image handle borrowed through each native call and disposes retirement results after return.
    pub const fn new(
        context: usize,
        retain: NativeProviderRetain,
        release: NativeProviderRelease,
        references: NativeProviderReferences,
    ) -> Self {
        Self {
            context,
            retain: Some(retain),
            release: Some(release),
            references: Some(references),
        }
    }

    /// Returns whether all residency operations are supplied together.
    pub const fn is_valid(self) -> bool {
        self.retain.is_some() == self.release.is_some()
            && self.retain.is_some() == self.references.is_some()
            && (self.retain.is_some() || self.context == 0)
    }

    /// Returns whether the reference is to an unloadable provider closure.
    pub const fn is_retained(self) -> bool {
        self.retain.is_some() && self.release.is_some()
    }

    /// Compares the admitted provider owner rather than just its image address.
    pub fn same_owner(self, other: Self) -> bool {
        self.context == other.context
            && match (self.retain, other.retain) {
                (Some(left), Some(right)) => std::ptr::fn_addr_eq(left, right),
                (None, None) => true,
                _ => false,
            }
            && match (self.release, other.release) {
                (Some(left), Some(right)) => std::ptr::fn_addr_eq(left, right),
                (None, None) => true,
                _ => false,
            }
            && match (self.references, other.references) {
                (Some(left), Some(right)) => std::ptr::fn_addr_eq(left, right),
                (None, None) => true,
                _ => false,
            }
    }

    /// Acquires one owner of the admitted closure. Ordinary moves do not call this operation.
    pub fn retain(self) -> Result<NativeProviderOwner, NativeRuntimeStatus> {
        if !self.is_valid() {
            return Err(NativeRuntimeStatus::INVALID_ARGUMENT);
        }

        if let Some(retain) = self.retain {
            retain(self.context);
        }

        Ok(NativeProviderOwner { reference: self })
    }

    /// Returns whether only formation retains the closure, after new ownership admission closes.
    pub fn can_retire(self) -> bool {
        self.references
            .is_none_or(|references| references(self.context) == 1)
    }
}

/// Movable provider residency retained through backing disposal and callback return.
#[repr(C)]
#[derive(Debug)]
pub struct NativeProviderOwner {
    reference: NativeProviderReference,
}

impl NativeProviderOwner {
    /// Adopts one reference already acquired by the native owner.
    pub const fn adopt(reference: NativeProviderReference) -> Self {
        Self { reference }
    }

    /// Creates an owner of process-resident code without acquiring an unloadable provider.
    pub const fn resident() -> Self {
        Self {
            reference: NativeProviderReference::resident(),
        }
    }

    /// Borrows the already admitted residency contract.
    pub const fn reference(&self) -> NativeProviderReference {
        self.reference
    }

    /// Retains the same provider for a callback guard without selecting or rebinding services.
    pub fn retain(&self) -> Self {
        if let Some(retain) = self.reference.retain {
            retain(self.reference.context);
        }

        Self {
            reference: self.reference,
        }
    }
}

impl Drop for NativeProviderOwner {
    fn drop(&mut self) {
        if let Some(release) = self.reference.release.take() {
            release(self.reference.context);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NativeProviderOwner, NativeProviderReference};

    #[test]
    fn provider_ownership_keeps_the_host_callback_record_layout() {
        assert_abi_layout!(NativeProviderReference, size: 32, align: 8, fields: {
            context: 0, retain: 8, release: 16, references: 24,
        });

        assert_abi_layout!(NativeProviderOwner, size: 32, align: 8, fields: { reference: 0 });
    }
}
