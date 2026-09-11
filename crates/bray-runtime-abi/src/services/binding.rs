use std::num::NonZeroUsize;

use crate::{
    NativeCleanupCapacityMetadata, NativeCleanupCapacityMetadataProvider, NativeCleanupStorage,
    NativeExecutionServices, NativeHostServices, NativeProviderRetention, NativeProviderRetirement,
    NativeRuntimeStatus,
};

/// Resident operations and native service tables for one product admission domain.
///
/// Callbacks support every thread without unwinding. The table remains resident
/// independently of the action providers. The binding retains its service context.
#[derive(Debug)]
#[repr(C)]
pub struct NativeProductServiceCallbacks {
    version: u32,
    reserved: u32,
    host: &'static NativeHostServices,
    execution: extern "C" fn(usize) -> Option<&'static NativeExecutionServices>,
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
    register_provider: extern "C" fn(
        usize,
        usize,
        extern "C" fn(usize) -> NativeRuntimeStatus,
        &mut NativeProviderRetirement,
    ) -> NativeRuntimeStatus,
}

impl NativeProductServiceCallbacks {
    /// Creates the current service table. Admission borrows provider metadata only
    /// during the call. Activation transfers prepared backing to an empty output.
    pub const fn new(
        host: &'static NativeHostServices,
        execution: extern "C" fn(usize) -> Option<&'static NativeExecutionServices>,
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
        register_provider: extern "C" fn(
            usize,
            usize,
            extern "C" fn(usize) -> NativeRuntimeStatus,
            &mut NativeProviderRetirement,
        ) -> NativeRuntimeStatus,
    ) -> Self {
        Self {
            version: 1,
            reserved: 0,
            host,
            execution,
            admit,
            activate,
            discharge,
            register_provider,
        }
    }
}

/// Owned binding to resident host operations, optional execution, and cleanup capacity.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct NativeProductServices {
    context: usize,
    callbacks: Option<&'static NativeProductServiceCallbacks>,
    retention: NativeProviderRetention,
}

impl NativeProductServices {
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
        callbacks: &'static NativeProductServiceCallbacks,
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
                    && callbacks.host.version == 1
                    && callbacks.host.reserved == 0
            }
            None => false,
        }
    }

    /// Borrows the resident host table retained by this binding.
    pub const fn host(&self) -> Option<&'static NativeHostServices> {
        match self.callbacks {
            Some(callbacks) if self.is_valid() => Some(callbacks.host),
            _ => None,
        }
    }

    /// Borrows the optional execution component selected during service formation.
    /// The resident getter neither allocates nor calls provider code.
    pub fn execution(&self) -> Option<&'static NativeExecutionServices> {
        let callbacks = self.callbacks.filter(|_| self.is_valid())?;

        (callbacks.execution)(self.context)
    }

    pub(crate) fn identity(&self) -> Option<(usize, *const NativeProductServiceCallbacks)> {
        self.callbacks.filter(|_| self.is_valid()).map(|callbacks| (self.context, std::ptr::from_ref(callbacks)))
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

    /// Registers provider teardown with the resident service before references escape.
    pub fn register_provider(
        &self,
        provider_context: usize,
        teardown: extern "C" fn(usize) -> NativeRuntimeStatus,
        destination: &mut NativeProviderRetirement,
    ) -> NativeRuntimeStatus {
        let Some(callbacks) = self.callbacks.filter(|_| self.is_valid()) else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        if !destination.is_empty() {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        }

        (callbacks.register_provider)(self.context, provider_context, teardown, destination)
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
    use super::{NativeProductServiceCallbacks, NativeProductServices};

    fn assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn product_service_layout_preserves_resident_ownership() {
        let word = std::mem::size_of::<usize>();

        assert_abi_layout!(NativeProductServiceCallbacks, size: 8 + 6 * word, align: word, fields: {
            version: 0,
            reserved: 4,
            host: 8,
            execution: 8 + word,
            admit: 8 + 2 * word,
            activate: 8 + 3 * word,
            discharge: 8 + 4 * word,
            register_provider: 8 + 5 * word,
        });

        assert_abi_layout!(NativeProductServices, size: 4 * word, align: word, fields: {
            context: 0,
            callbacks: word,
            retention: 2 * word,
        });

        assert_send_sync::<NativeProductServiceCallbacks>();
        assert_send_sync::<NativeProductServices>();

        let empty = NativeProductServices::empty();
        assert!(empty.is_empty());
        assert!(!empty.is_valid());
        assert!(!empty.same_domain(&empty.clone()));
    }
}
