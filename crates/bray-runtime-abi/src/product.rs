/// Version of the native product-host descriptor and static-entry records.
pub const PRODUCT_HOST_ABI_VERSION: u32 = 1;

/// Stable runtime symbol controlling one compiler-generated product host.
pub const PRODUCT_HOST_CONTROL_RUNTIME_SYMBOL: &str = "bray_runtime_product_host_control_v1";

/// Stable identity of one loaded product instance.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeProductIdentity([u8; 32]);

impl NativeProductIdentity {
    /// Creates an identity from its exact compiler-generated digest.
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the exact identity digest.
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Stable identity of one closed static instance.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeStaticIdentity([u8; 32]);

impl NativeStaticIdentity {
    /// Creates an identity from its exact compiler-generated digest.
    pub const fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the exact identity digest.
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Native storage owner encoded by one static-host entry.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeStaticDuration(u32);

impl NativeStaticDuration {
    /// Storage owned by one loaded product instance.
    pub const PRODUCT: Self = Self(0);
    /// Storage owned by one exact thread attachment.
    pub const EXACT_THREAD: Self = Self(1);

    /// Returns whether the value belongs to this ABI version.
    pub const fn is_known(self) -> bool {
        matches!(self.0, 0 | 1)
    }

    /// Returns the stable integer representation.
    pub const fn code(self) -> u32 {
        self.0
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
    /// Cleanup finished and the product may be unloaded.
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

    /// Returns whether the value belongs to this ABI version.
    pub const fn is_known(self) -> bool {
        matches!(self.0, 0..=10)
    }

    /// Returns the stable integer representation.
    pub const fn code(self) -> u32 {
        self.0
    }
}

/// Compiler-generated callback returning the address of one static instance.
pub type NativeStaticAccessCallback = extern "C" fn() -> usize;

/// Compiler-generated callback cleaning one initialized static instance.
pub type NativeStaticCleanupCallback = extern "C-unwind" fn();

/// Compiler-generated callback returning one dependency identity by ordinal.
pub type NativeStaticDependencyCallback = extern "C" fn(usize) -> NativeStaticIdentity;

/// Compiler-generated callback returning one static-host entry by ordinal.
pub type NativeStaticHostCallback = extern "C" fn(usize) -> NativeStaticHostEntry;

/// Typed contribution for one demanded static realization.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeStaticHostEntry {
    abi_version: u32,
    duration: NativeStaticDuration,
    identity: NativeStaticIdentity,
    order: u64,
    storage: usize,
    access: NativeStaticAccessCallback,
    cleanup: NativeStaticCleanupCallback,
    dependency: NativeStaticDependencyCallback,
    dependency_count: usize,
}

impl NativeStaticHostEntry {
    /// Creates one immutable compiler-generated static-host entry.
    #[expect(
        clippy::too_many_arguments,
        reason = "the fixed ABI record keeps every native contribution field explicit"
    )]
    pub const fn new(
        duration: NativeStaticDuration,
        identity: NativeStaticIdentity,
        order: u64,
        storage: usize,
        access: NativeStaticAccessCallback,
        cleanup: NativeStaticCleanupCallback,
        dependency: NativeStaticDependencyCallback,
        dependency_count: usize,
    ) -> Self {
        Self {
            abi_version: PRODUCT_HOST_ABI_VERSION,
            duration,
            identity,
            order,
            storage,
            access,
            cleanup,
            dependency,
            dependency_count,
        }
    }

    /// Returns the record ABI version.
    pub const fn abi_version(self) -> u32 {
        self.abi_version
    }

    /// Returns the storage owner category.
    pub const fn duration(self) -> NativeStaticDuration {
        self.duration
    }

    /// Returns the exact static instance identity.
    pub const fn identity(self) -> NativeStaticIdentity {
        self.identity
    }

    /// Returns the deterministic cleanup ordinal.
    pub const fn order(self) -> u64 {
        self.order
    }

    /// Returns the native storage address for product-owned entries.
    pub const fn storage(self) -> usize {
        self.storage
    }

    /// Returns the materialization and access callback.
    pub const fn access(self) -> NativeStaticAccessCallback {
        self.access
    }

    /// Returns the cleanup callback.
    pub const fn cleanup(self) -> NativeStaticCleanupCallback {
        self.cleanup
    }

    /// Returns the callback selecting one dependency identity by ordinal.
    pub const fn dependency(self) -> NativeStaticDependencyCallback {
        self.dependency
    }

    /// Returns the number of dependency identities.
    pub const fn dependency_count(self) -> usize {
        self.dependency_count
    }
}

/// Immutable compiler-generated descriptor for one loaded product host.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeProductHostDescriptor {
    abi_version: u32,
    reserved: u32,
    identity: NativeProductIdentity,
    static_entry: NativeStaticHostCallback,
    static_count: usize,
}

impl NativeProductHostDescriptor {
    /// Creates one immutable compiler-generated product-host descriptor.
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
        }
    }

    /// Returns the record ABI version.
    pub const fn abi_version(self) -> u32 {
        self.abi_version
    }

    /// Returns the loaded product identity.
    pub const fn identity(self) -> NativeProductIdentity {
        self.identity
    }

    /// Returns the callback selecting one ordered static entry by ordinal.
    pub const fn static_entry(self) -> NativeStaticHostCallback {
        self.static_entry
    }

    /// Returns the number of static entries.
    pub const fn static_count(self) -> usize {
        self.static_count
    }
}

/// Exact-thread cleanup registration transferred to the runtime registry.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeThreadStaticCleanupRegistration {
    product: &'static NativeProductHostDescriptor,
    static_identity: NativeStaticIdentity,
    callback: NativeStaticCleanupCallback,
}

impl NativeThreadStaticCleanupRegistration {
    /// Creates one exact-thread cleanup registration.
    pub const fn new(
        product: &'static NativeProductHostDescriptor,
        static_identity: NativeStaticIdentity,
        callback: NativeStaticCleanupCallback,
    ) -> Self {
        Self {
            product,
            static_identity,
            callback,
        }
    }

    /// Returns the compiler-generated product descriptor.
    pub const fn product(self) -> &'static NativeProductHostDescriptor {
        self.product
    }

    /// Returns the exact static instance identity.
    pub const fn static_identity(self) -> NativeStaticIdentity {
        self.static_identity
    }

    /// Returns the exact-thread cleanup callback.
    pub const fn callback(self) -> NativeStaticCleanupCallback {
        self.callback
    }
}

/// Complete observation returned by every product-host operation.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
        }
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
    pub const fn status(self) -> NativeProductHostStatus {
        self.status
    }

    /// Returns the current host lifecycle state.
    pub const fn state(self) -> NativeProductHostState {
        self.state
    }

    /// Returns the number of admitted entries.
    pub const fn active_entries(self) -> usize {
        self.active_entries
    }

    /// Returns the number of retained external provider roots.
    pub const fn external_roots(self) -> usize {
        self.external_roots
    }

    /// Returns the number of live exact-thread attachments.
    pub const fn thread_attachments(self) -> usize {
        self.thread_attachments
    }

    /// Returns the number of initialized product-owned static instances.
    pub const fn initialized_statics(self) -> usize {
        self.initialized_statics
    }

    /// Returns the number of cleaned product-owned static instances.
    pub const fn cleaned_statics(self) -> usize {
        self.cleaned_statics
    }

    /// Returns the number of contained cleanup incidents.
    pub const fn cleanup_incidents(self) -> usize {
        self.cleanup_incidents
    }

    /// Returns the identity of the most recent static cleanup incident.
    pub const fn last_incident(self) -> NativeStaticIdentity {
        self.last_incident
    }
}

#[cfg(test)]
mod tests {
    use super::{
        NativeProductHostDescriptor, NativeProductHostObservation, NativeStaticHostEntry,
        NativeThreadStaticCleanupRegistration,
    };

    #[test]
    fn product_host_records_keep_native_pointer_alignment() {
        assert_eq!(
            std::mem::align_of::<NativeProductHostDescriptor>(),
            std::mem::align_of::<usize>()
        );

        assert_eq!(
            std::mem::align_of::<NativeStaticHostEntry>(),
            std::mem::align_of::<usize>()
        );

        assert_eq!(
            std::mem::align_of::<NativeThreadStaticCleanupRegistration>(),
            std::mem::align_of::<usize>()
        );

        assert_eq!(
            std::mem::align_of::<NativeProductHostObservation>(),
            std::mem::align_of::<usize>()
        );
    }
}
