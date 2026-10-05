use super::identity::{NativeProductIdentity, NativeStaticIdentity};
use super::static_storage::NativeStaticHostCallback;
use crate::NativeProviderReference;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Version of the native product-host descriptor and static-entry records.
pub const PRODUCT_HOST_ABI_VERSION: u32 = 1;
/// Scheduler-independent host services required by the selected native program.
pub const PRODUCT_HOST_SERVICES: u32 = 1;
/// Independent execution services required by the selected native program.
pub const PRODUCT_EXECUTION_SERVICES: u32 = 2;
/// Formation requires a host-owned residency reference for this unloadable image.
pub const PRODUCT_UNLOADABLE: u32 = 4;

/// Supplied service-domain binding and owning provider closure for one image load.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeProductBinding {
    abi_version: u32,
    services: u32,
    domain: usize,
    load_identity: usize,
    provider: NativeProviderReference,
}

impl NativeProductBinding {
    /// Creates one host-supplied binding without acquiring provider ownership.
    pub const fn new(
        load_identity: usize,
        services: u32,
        domain: usize,
        provider: NativeProviderReference,
    ) -> Self {
        Self {
            abi_version: PRODUCT_HOST_ABI_VERSION,
            services,
            domain,
            load_identity,
            provider,
        }
    }

    /// Returns whether supplied compatibility and residency satisfy the product contract.
    pub const fn is_compatible(self, requirements: u32, domain: usize) -> bool {
        self.abi_version == PRODUCT_HOST_ABI_VERSION
            && self.services & !(PRODUCT_HOST_SERVICES | PRODUCT_EXECUTION_SERVICES) == 0
            && self.services & (requirements & !PRODUCT_UNLOADABLE)
                == requirements & !PRODUCT_UNLOADABLE
            && self.domain == domain
            && self.provider.is_valid()
            && (requirements & PRODUCT_UNLOADABLE == 0
                || (self.provider.is_retained()
                    && self.load_identity > isize::MAX as usize
                    && self.load_identity != usize::MAX))
    }

    /// Returns the host's unique load generation. Unloadable hosts allocate from the upper
    /// half of the identity range and never reuse a generation, including after failed formation.
    pub const fn load_identity(self) -> usize {
        self.load_identity
    }

    /// Returns the admitted provider residency operations.
    pub const fn provider(self) -> NativeProviderReference {
        self.provider
    }

    /// Compares the complete supplied binding with the established load binding.
    pub fn same_binding(self, other: Self) -> bool {
        self.abi_version == other.abi_version
            && self.services == other.services
            && self.domain == other.domain
            && self.load_identity == other.load_identity
            && self.provider.same_owner(other.provider)
    }
}

/// Product-host lifecycle state.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeProductHostState(u32);

impl NativeProductHostState {
    /// The descriptor has not been formed by the runtime.
    pub const UNFORMED: Self = Self(0);
    /// New entries and provider roots may be acquired.
    pub const OPEN: Self = Self(1);
    /// New acquisitions are closed while existing obligations drain.
    pub const CLOSING: Self = Self(2);
    /// Static cleanup finished. Residency owners can still delay retirement.
    pub const CLOSED: Self = Self(3);
    /// Descriptor validation or lifecycle execution failed.
    pub const FAILED: Self = Self(4);

    /// Returns the stable integer representation.
    pub const fn code(self) -> u32 {
        self.0
    }
}

/// Result category returned by one product-host control operation.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeProductHostStatus(u32);

impl NativeProductHostStatus {
    /// The requested operation completed.
    pub const SUCCESS: Self = Self(0);
    /// Existing obligations keep closure pending.
    pub const PENDING: Self = Self(1);
    /// The product is already closed to the requested acquisition.
    pub const CLOSED: Self = Self(2);
    /// The descriptor or requested transition is invalid.
    pub const INVALID_ARGUMENT: Self = Self(3);
    /// Cleanup completed with one or more contained incidents.
    pub const INCIDENTS: Self = Self(4);
    /// Runtime infrastructure could not preserve the host contract.
    pub const RUNTIME_FAILURE: Self = Self(5);

    /// Returns the stable integer representation.
    pub const fn code(self) -> u32 {
        self.0
    }
}

/// Operation accepted by the compiler-generated product-host control surface.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeProductHostOperation(u32);

impl NativeProductHostOperation {
    /// Form the descriptor and observe its initial open state.
    pub const FORM: Self = Self(0);
    /// Acquire one admitted product entry.
    pub const ACQUIRE_ENTRY: Self = Self(1);
    /// Release one admitted product entry.
    pub const RELEASE_ENTRY: Self = Self(2);
    /// Acquire one external provider-retention root.
    pub const ACQUIRE_EXTERNAL: Self = Self(3);
    /// Release one external provider-retention root.
    pub const RELEASE_EXTERNAL: Self = Self(4);
    /// Begin closure and finalize once every obligation is quiescent.
    pub const CLOSE: Self = Self(5);
    /// Observe current lifecycle and obligation state.
    pub const OBSERVE: Self = Self(6);
    /// Acquire one exact-thread attachment obligation.
    pub const ACQUIRE_ATTACHMENT: Self = Self(7);
    /// Release one exact-thread attachment obligation.
    pub const RELEASE_ATTACHMENT: Self = Self(8);
    /// Attach the current foreign thread to runtime execution.
    pub const ATTACH_CURRENT_THREAD: Self = Self(9);
    /// Detach the current foreign thread and resolve its exact-thread statics.
    pub const DETACH_CURRENT_THREAD: Self = Self(10);
    /// Close an executable root and resolve its implicit current-thread static obligations.
    /// Explicit foreign-thread attachments must be released by their original owner first.
    pub const FINISH_ROOT: Self = Self(11);
    /// Retire a quiescent closed or failed load before its image is unloaded.
    pub const RETIRE: Self = Self(12);

    /// Returns whether the value belongs to this ABI version.
    pub const fn is_known(self) -> bool {
        matches!(self.0, 0..=12)
    }

    /// Returns the stable integer representation.
    pub const fn code(self) -> u32 {
        self.0
    }
}

/// Compiler-generated descriptor with a frozen supplied binding and one atomic load slot.
#[repr(C)]
#[derive(Debug)]
pub struct NativeProductHostDescriptor {
    abi_version: u32,
    required_services: u32,
    identity: NativeProductIdentity,
    static_entry: NativeStaticHostCallback,
    static_count: usize,
    load: AtomicUsize,
    domain: usize,
    binding: NativeProductBinding,
}

impl NativeProductHostDescriptor {
    /// Creates one fresh compiler-generated product-host descriptor.
    pub const fn new(
        identity: NativeProductIdentity,
        static_entry: NativeStaticHostCallback,
        static_count: usize,
    ) -> Self {
        Self {
            abi_version: PRODUCT_HOST_ABI_VERSION,
            required_services: 0,
            identity,
            static_entry,
            static_count,
            load: AtomicUsize::new(0),
            domain: 0,
            binding: NativeProductBinding::new(0, 0, 0, NativeProviderReference::resident()),
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

    /// Supplies the resolved demand and host binding before publishing this descriptor.
    pub const fn with_binding(
        mut self,
        requirements: u32,
        domain: usize,
        binding: NativeProductBinding,
    ) -> Self {
        self.required_services = requirements;
        self.domain = domain;
        self.binding = binding;

        self
    }

    /// Returns the exact scheduler-independent and execution service demand.
    pub const fn required_services(&self) -> u32 {
        self.required_services
    }
    /// Returns the resolved linked service-domain identity.
    pub const fn domain(&self) -> usize {
        self.domain
    }
    /// Returns the externally supplied binding for validation before ownership publication.
    pub const fn binding(&self) -> NativeProductBinding {
        self.binding
    }

    /// Returns the runtime-assigned identity, or zero before formation.
    pub fn load_identity(&self) -> usize {
        self.load.load(Ordering::Acquire)
    }

    /// Installs one identity for this image lifetime without replacing an existing load.
    pub fn assign_load_identity(&self, identity: usize) -> usize {
        assert!(
            identity != 0 && identity != usize::MAX,
            "load identities must be live"
        );

        match self
            .load
            .compare_exchange(0, identity, Ordering::AcqRel, Ordering::Acquire)
        {
            Ok(_) => identity,
            Err(existing) => existing,
        }
    }

    /// Invalidates this load permanently. A newly mapped image has a fresh zero slot.
    pub fn retire_load_identity(&self, identity: usize) -> bool {
        self.load
            .compare_exchange(identity, usize::MAX, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

/// Complete observation returned by every product-host operation.
#[repr(C)]
#[derive(Debug)]
pub struct NativeProductHostObservation {
    status: NativeProductHostStatus,
    state: NativeProductHostState,
    active_entries: usize,
    external_roots: usize,
    thread_attachments: usize,
    initialized_statics: usize,
    cleaned_statics: usize,
    cleanup_incidents: usize,
    last_incident: NativeStaticIdentity,
    retired_provider: crate::NativeProviderOwner,
}

impl NativeProductHostObservation {
    /// Creates one complete product-host observation.
    #[expect(
        clippy::too_many_arguments,
        reason = "the fixed ABI record exposes each independent lifecycle count explicitly"
    )]
    pub const fn new(
        status: NativeProductHostStatus,
        state: NativeProductHostState,
        active_entries: usize,
        external_roots: usize,
        thread_attachments: usize,
        initialized_statics: usize,
        cleaned_statics: usize,
        cleanup_incidents: usize,
        last_incident: NativeStaticIdentity,
    ) -> Self {
        Self {
            status,
            state,
            active_entries,
            external_roots,
            thread_attachments,
            initialized_statics,
            cleaned_statics,
            cleanup_incidents,
            last_incident,
            retired_provider: crate::NativeProviderOwner::resident(),
        }
    }

    /// Transfers the formation owner to the caller after successful retirement.
    /// Dispose the returned observation only after control has returned from the retired image.
    pub fn with_retired_provider(mut self, provider: crate::NativeProviderOwner) -> Self {
        self.retired_provider = provider;

        self
    }

    /// Creates an observation for an invalid operation or descriptor.
    pub const fn invalid() -> Self {
        Self::new(
            NativeProductHostStatus::INVALID_ARGUMENT,
            NativeProductHostState::UNFORMED,
            0,
            0,
            0,
            0,
            0,
            0,
            NativeStaticIdentity::new([0; 32]),
        )
    }

    /// Returns the operation result category.
    pub const fn status(&self) -> NativeProductHostStatus {
        self.status
    }

    /// Returns the current host lifecycle state.
    pub const fn state(&self) -> NativeProductHostState {
        self.state
    }

    /// Returns the number of admitted entries.
    pub const fn active_entries(&self) -> usize {
        self.active_entries
    }

    /// Returns the number of retained external provider roots.
    pub const fn external_roots(&self) -> usize {
        self.external_roots
    }

    /// Returns the number of live exact-thread attachments.
    pub const fn thread_attachments(&self) -> usize {
        self.thread_attachments
    }

    /// Returns the number of initialized product-owned static instances.
    pub const fn initialized_statics(&self) -> usize {
        self.initialized_statics
    }

    /// Returns the number of cleaned product-owned static instances.
    pub const fn cleaned_statics(&self) -> usize {
        self.cleaned_statics
    }

    /// Returns the number of contained cleanup incidents.
    pub const fn cleanup_incidents(&self) -> usize {
        self.cleanup_incidents
    }

    /// Returns the identity of the most recent static cleanup incident.
    pub const fn last_incident(&self) -> NativeStaticIdentity {
        self.last_incident
    }
}

#[cfg(test)]
mod tests {
    use super::{NativeProductBinding, NativeProductHostDescriptor, NativeProductHostObservation};

    #[test]
    fn formation_and_retirement_records_have_explicit_native_layout() {
        assert_abi_layout!(NativeProductBinding, size: 56, align: 8, fields: {
            abi_version: 0, services: 4, domain: 8, load_identity: 16, provider: 24,
        });

        assert_abi_layout!(NativeProductHostDescriptor, size: 128, align: 8, fields: {
            abi_version: 0, required_services: 4, identity: 8, static_entry: 40, static_count: 48,
            load: 56, domain: 64, binding: 72,
        });

        assert_abi_layout!(NativeProductHostObservation, size: 120, align: 8, fields: {
            status: 0, state: 4, active_entries: 8, external_roots: 16, thread_attachments: 24,
            initialized_statics: 32, cleaned_statics: 40, cleanup_incidents: 48, last_incident: 56,
            retired_provider: 88,
        });
    }
}
