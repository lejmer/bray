use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

use crate::{NativeExecutionServices, NativeHostServices, NativeProductIdentity, NativeProductServiceCallbacks, NativeProductServices, NativeRuntimeStatus, PRODUCT_HOST_ABI_VERSION};
use crate::product::NativeStaticHostCallback;

/// Compiler-generated descriptor with immutable product data and once-published resident identity.
/// Cached service tables remain resident through the image lifetime. Rebinding is forbidden.
#[repr(C)]
#[derive(Debug)]
pub struct NativeProductHostDescriptor {
    abi_version: u32,
    reserved: u32,
    identity: NativeProductIdentity,
    static_entry: NativeStaticHostCallback,
    static_count: usize,
    runtime_instance: AtomicUsize,
    runtime_domain: AtomicUsize,
    runtime_callbacks: AtomicPtr<NativeProductServiceCallbacks>,
    runtime_host: AtomicPtr<NativeHostServices>,
    runtime_execution: AtomicPtr<NativeExecutionServices>,
}

impl NativeProductHostDescriptor {
    /// Creates a product descriptor whose resident identity is initially unbound.
    pub const fn new(
        identity: NativeProductIdentity,
        static_entry: NativeStaticHostCallback,
        static_count: usize,
    ) -> Self {
        Self {
            abi_version: PRODUCT_HOST_ABI_VERSION,
            reserved: 0,
            identity,
            static_entry,
            static_count,
            runtime_instance: AtomicUsize::new(0),
            runtime_domain: AtomicUsize::new(0),
            runtime_callbacks: AtomicPtr::new(std::ptr::null_mut()),
            runtime_host: AtomicPtr::new(std::ptr::null_mut()),
            runtime_execution: AtomicPtr::new(std::ptr::null_mut()),
        }
    }

    /// Returns the record ABI version.
    pub const fn abi_version(&self) -> u32 {
        self.abi_version
    }

    /// Returns the loaded product identity.
    pub const fn identity(&self) -> NativeProductIdentity {
        self.identity
    }

    /// Returns the callback selecting one ordered static entry by ordinal.
    pub const fn static_entry(&self) -> NativeStaticHostCallback {
        self.static_entry
    }

    /// Returns the number of static entries.
    pub const fn static_count(&self) -> usize {
        self.static_count
    }
}

impl NativeProductHostDescriptor {
    /// Publishes one resident identity after validating all callback-provided service data.
    ///
    /// Binding is permanent even if later product formation fails. Retrying formation requires
    /// the same live service domain. The host keeps the resident service image loaded until the
    /// bound provider image is unloaded; releasing a service binding does not end that obligation.
    pub fn bind_services(
        &self,
        services: &NativeProductServices,
        candidate: NonZeroUsize,
    ) -> Result<usize, NativeRuntimeStatus> {
        let invalid = NativeRuntimeStatus::INVALID_ARGUMENT;

        if candidate.get() < 2 {
            return Err(invalid);
        }

        let host = services.host().ok_or(invalid)?;
        let execution = services.execution();

        if execution.is_some_and(|execution| !execution.is_compatible(host)) {
            return Err(invalid);
        }

        let (domain, callbacks) = services.identity().ok_or(invalid)?;

        let execution = execution.map_or(std::ptr::null_mut(), |execution| std::ptr::from_ref(execution).cast_mut());
        let host = std::ptr::from_ref(host).cast_mut();
        let callbacks = callbacks.cast_mut();

        match self.runtime_instance.compare_exchange(0, 1, Ordering::Acquire, Ordering::Acquire) {
            Ok(_) => {
                // Only atomic stores occur while publication is held. No callbacks or failures.
                self.runtime_domain.store(domain, Ordering::Relaxed);
                self.runtime_callbacks.store(callbacks, Ordering::Relaxed);
                self.runtime_host.store(host, Ordering::Relaxed);
                self.runtime_execution.store(execution, Ordering::Relaxed);
                self.runtime_instance.store(candidate.get(), Ordering::Release);

                Ok(candidate.get())
            }
            Err(mut instance) => {
                while instance == 1 {
                    std::hint::spin_loop();
                    instance = self.runtime_instance.load(Ordering::Acquire);
                }

                if self.runtime_domain.load(Ordering::Relaxed) != domain
                    || self.runtime_callbacks.load(Ordering::Relaxed) != callbacks {
                    return Err(invalid);
                }

                Ok(instance)
            }
        }
    }

    /// Returns the published identity only to its owning resident host.
    pub fn runtime_instance(&self, host: &NativeHostServices) -> Option<NonZeroUsize> {
        let instance = self.runtime_instance.load(Ordering::Acquire);

        (instance >= 2 && self.runtime_host.load(Ordering::Relaxed).cast_const() == std::ptr::from_ref(host))
            .then(|| NonZeroUsize::new(instance)).flatten()
    }

    /// Returns the resident host table address after publication, without dereferencing it.
    pub fn runtime_host(&self) -> *const NativeHostServices {
        if self.runtime_instance.load(Ordering::Acquire) < 2 {
            return std::ptr::null();
        }

        self.runtime_host.load(Ordering::Relaxed).cast_const()
    }

    /// Returns the optional resident execution table address after publication.
    pub fn runtime_execution(&self) -> *const NativeExecutionServices {
        if self.runtime_instance.load(Ordering::Acquire) < 2 {
            return std::ptr::null();
        }

        self.runtime_execution.load(Ordering::Relaxed).cast_const()
    }
}

#[cfg(test)]
mod tests {
    use super::NativeProductHostDescriptor;
    use std::mem::{offset_of, size_of};

    #[test]
    fn resident_identity_fields_follow_the_static_descriptor_prefix() {
        let word = size_of::<usize>();
        let instance = offset_of!(NativeProductHostDescriptor, static_count) + word;

        assert_eq!(offset_of!(NativeProductHostDescriptor, runtime_instance), instance);
        assert_eq!(offset_of!(NativeProductHostDescriptor, runtime_domain), instance + word);
        assert_eq!(offset_of!(NativeProductHostDescriptor, runtime_callbacks), instance + 2 * word);
        assert_eq!(offset_of!(NativeProductHostDescriptor, runtime_host), instance + 3 * word);
        assert_eq!(offset_of!(NativeProductHostDescriptor, runtime_execution), instance + 4 * word);
        assert_eq!(size_of::<NativeProductHostDescriptor>(), instance + 5 * word);
    }
}
