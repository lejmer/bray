use std::num::NonZeroUsize;

use crate::{NativeCleanupStorage, NativeProviderRetention, NativeRuntimeStatus};

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

/// Resident service callbacks for one cleanup capacity domain.
///
/// Callbacks support every thread without unwinding. The table remains resident
/// independently of the action providers. The binding retains its service context.
#[derive(Debug)]
#[repr(C)]
pub struct NativeCleanupCapacityCallbacks {
    version: u32,
    reserved: u32,
    admit: extern "C" fn(
        usize,
        usize,
        Option<NativeCleanupCapacityMetadataProvider>,
    ) -> NativeRuntimeStatus,
    activate: extern "C" fn(
        usize,
        &NativeCleanupCapacityMetadata,
        &mut NativeCleanupStorage,
    ) -> NativeRuntimeStatus,
    discharge: extern "C" fn(usize, &NativeCleanupCapacityMetadata) -> NativeRuntimeStatus,
}

impl NativeCleanupCapacityCallbacks {
    /// Creates the current service table. Admission borrows provider metadata only
    /// during the call. Activation transfers prepared backing to an empty output.
    pub const fn new(
        admit: extern "C" fn(
            usize,
            usize,
            Option<NativeCleanupCapacityMetadataProvider>,
        ) -> NativeRuntimeStatus,
        activate: extern "C" fn(
            usize,
            &NativeCleanupCapacityMetadata,
            &mut NativeCleanupStorage,
        ) -> NativeRuntimeStatus,
        discharge: extern "C" fn(usize, &NativeCleanupCapacityMetadata) -> NativeRuntimeStatus,
    ) -> Self {
        Self {
            version: 1,
            reserved: 0,
            admit,
            activate,
            discharge,
        }
    }
}

/// Owned reference to a cleanup capacity domain and its resident service.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct NativeCleanupCapacityBinding {
    context: usize,
    callbacks: Option<&'static NativeCleanupCapacityCallbacks>,
    retention: NativeProviderRetention,
}

impl NativeCleanupCapacityBinding {
    /// Creates an initialized empty binding without acquiring a reference.
    pub const fn empty() -> Self {
        Self {
            context: 0,
            callbacks: None,
            retention: NativeProviderRetention::empty(),
        }
    }

    /// Adopts an already retained service owner that keeps this context alive.
    /// The owner must be nonempty and belong to the supplied service context.
    pub const fn new(
        context: NonZeroUsize,
        callbacks: &'static NativeCleanupCapacityCallbacks,
        retention: NativeProviderRetention,
    ) -> Self {
        Self {
            context: context.get(),
            callbacks: Some(callbacks),
            retention,
        }
    }

    /// Returns whether no service is bound.
    pub const fn is_empty(&self) -> bool {
        self.context == 0 && self.callbacks.is_none() && self.retention.is_empty()
    }

    /// Checks the current service contract and retained context.
    pub const fn is_valid(&self) -> bool {
        match self.callbacks {
            Some(callbacks) => {
                self.context != 0
                    && !self.retention.is_empty()
                    && callbacks.version == 1
                    && callbacks.reserved == 0
            }
            None => false,
        }
    }

    /// Compares domain identity without invoking either service.
    pub fn same_domain(&self, other: &Self) -> bool {
        match (self.callbacks, other.callbacks) {
            (Some(left), Some(right)) => self.context == other.context && std::ptr::eq(left, right),
            _ => false,
        }
    }

    /// Admits the requested action multiplicity before owner construction.
    pub fn admit(
        &self,
        count: usize,
        provider: Option<NativeCleanupCapacityMetadataProvider>,
    ) -> NativeRuntimeStatus {
        let Some(callbacks) = self.callbacks.filter(|_| self.is_valid()) else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        (callbacks.admit)(self.context, count, provider)
    }

    /// Transfers prepared action backing into an initialized empty destination.
    pub fn activate(
        &self,
        metadata: &NativeCleanupCapacityMetadata,
        destination: &mut NativeCleanupStorage,
    ) -> NativeRuntimeStatus {
        let Some(callbacks) = self.callbacks.filter(|_| self.is_valid()) else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        if !destination.is_empty() {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        (callbacks.activate)(self.context, metadata, destination)
    }

    /// Discharges one owner's unused allowance for the action.
    pub fn discharge(&self, metadata: &NativeCleanupCapacityMetadata) -> NativeRuntimeStatus {
        let Some(callbacks) = self.callbacks.filter(|_| self.is_valid()) else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        (callbacks.discharge)(self.context, metadata)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NativeCleanupCapacityBinding, NativeCleanupCapacityCallbacks, NativeCleanupCapacityMetadata,
    };

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn capacity_service_layout_preserves_resident_and_provider_ownership() {
        let word = std::mem::size_of::<usize>();

        assert_abi_layout!(NativeCleanupCapacityMetadata, size: 32 + word, align: word, fields: {
            identity: 0,
            prepare: 32,
        });

        assert_abi_layout!(NativeCleanupCapacityCallbacks, size: 8 + 3 * word, align: word, fields: {
            version: 0,
            reserved: 4,
            admit: 8,
            activate: 8 + word,
            discharge: 8 + 2 * word,
        });

        assert_abi_layout!(NativeCleanupCapacityBinding, size: 4 * word, align: word, fields: {
            context: 0,
            callbacks: word,
            retention: 2 * word,
        });

        assert_send_sync::<NativeCleanupCapacityMetadata>();
        assert_send_sync::<NativeCleanupCapacityCallbacks>();
        assert_send_sync::<NativeCleanupCapacityBinding>();

        let empty = NativeCleanupCapacityBinding::empty();
        assert!(empty.is_empty());
        assert!(!empty.is_valid());
        assert!(!empty.same_domain(&empty.clone()));
    }
}
