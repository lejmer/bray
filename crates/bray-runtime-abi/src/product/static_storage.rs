use super::host::NativeProductHostDescriptor;
use super::host::PRODUCT_HOST_ABI_VERSION;
use super::identity::{NativeStaticIdentity, NativeTypeIdentity};
use crate::{NativeRuntimeStatus, NativeSourceAnchor};

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

/// Compiler-generated callback returning the address of one static instance.
pub type NativeStaticAccessCallback = extern "C" fn() -> usize;

/// Compiler-generated callback cleaning one initialized static instance.
pub type NativeStaticCleanupCallback = extern "C-unwind" fn(&mut crate::NativeRunOutcome);

/// Compiler-generated callback starting finalization into caller-owned storage.
pub type NativeStaticFinalizerStartCallback =
    extern "C-unwind" fn(usize, &mut crate::NativeRunOutcome) -> NativeStaticFinalizerStatus;

/// Compiler-generated callback reporting one owned cleanup incident payload.
pub type NativeCleanupIncidentReportCallback = extern "C-unwind" fn(usize) -> NativeRuntimeStatus;

/// Compiler-generated callback destroying and releasing one owned cleanup incident payload.
/// The destinations retain destruction and backing-release outcomes independently.
pub type NativeCleanupIncidentDestroyCallback =
    extern "C-unwind" fn(usize, &mut crate::NativeRunOutcome, &mut crate::NativeRunOutcome);

/// Owned type-erased finalizer error transferred to its cleanup domain.
#[repr(C)]
#[derive(Debug)]
pub struct NativeCleanupIncident {
    payload: usize,
    type_identity: NativeTypeIdentity,
    source: NativeSourceAnchor,
    report: NativeCleanupIncidentReportCallback,
    destroy: NativeCleanupIncidentDestroyCallback,
    provider: crate::NativeProviderOwner,
}

impl NativeCleanupIncident {
    /// Retains the originating provider through error destruction and backing release.
    pub fn retain_provider(&mut self, provider: crate::NativeProviderOwner) {
        self.provider = provider;
    }

    /// Creates one compiler-generated owned cleanup incident.
    pub const fn new(
        payload: usize,
        type_identity: NativeTypeIdentity,
        source: NativeSourceAnchor,
        report: NativeCleanupIncidentReportCallback,
        destroy: NativeCleanupIncidentDestroyCallback,
    ) -> Self {
        Self {
            payload,
            type_identity,
            source,
            report,
            destroy,
            provider: crate::NativeProviderOwner::resident(),
        }
    }

    /// Returns whether every data field satisfies the native ownership contract.
    pub const fn is_valid(&self) -> bool {
        self.payload != 0 && self.source.is_valid()
    }

    /// Returns the owned payload address.
    pub const fn payload(&self) -> usize {
        self.payload
    }

    /// Returns the concrete Bray type identity.
    pub const fn type_identity(&self) -> NativeTypeIdentity {
        self.type_identity
    }

    /// Returns the finalizer source location when locally available.
    pub const fn source(&self) -> NativeSourceAnchor {
        self.source
    }

    /// Returns the reporting callback that borrows the owned payload.
    pub const fn report(&self) -> NativeCleanupIncidentReportCallback {
        self.report
    }

    /// Returns the callback consuming and releasing the owned payload.
    pub const fn destroy(&self) -> NativeCleanupIncidentDestroyCallback {
        self.destroy
    }
}

/// Compiler-generated callback consuming one completed finalizer result.
pub type NativeStaticFinalizerResolveCallback =
    extern "C-unwind" fn(usize, usize, &mut crate::NativeRunOutcome) -> NativeStaticFinalizerStatus;

/// How one static finalizer reaches completion.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeStaticFinalizerExecution(u32);

impl NativeStaticFinalizerExecution {
    /// The static has no semantic finalizer.
    pub const NONE: Self = Self(0);
    /// Finalization completes during its start callback.
    pub const SYNCHRONOUS: Self = Self(1);
    /// Finalization produces an inactive protected frame.
    pub const ASYNCHRONOUS: Self = Self(2);

    /// Returns whether the value belongs to this ABI version.
    pub const fn is_known(self) -> bool {
        matches!(self.0, 0..=2)
    }

    /// Returns the stable integer representation.
    pub const fn code(self) -> u32 {
        self.0
    }
}

/// Outcome of consuming one completed static finalizer result.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NativeStaticFinalizerStatus(u32);

impl NativeStaticFinalizerStatus {
    /// Finalization completed successfully.
    pub const SUCCESS: Self = Self(0);
    /// Finalization produced a contained recoverable failure.
    pub const INCIDENT: Self = Self(1);

    /// Returns whether the value belongs to this ABI version.
    pub const fn is_known(self) -> bool {
        matches!(self.0, 0 | 1)
    }

    /// Returns the stable integer representation.
    pub const fn code(self) -> u32 {
        self.0
    }
}

/// Closed execution and completion contract for one static finalizer.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeStaticFinalizer {
    execution: NativeStaticFinalizerExecution,
    outgoing_capacity: u32,
    result_size: usize,
    result_alignment: usize,
    start: NativeStaticFinalizerStartCallback,
    resolve: NativeStaticFinalizerResolveCallback,
}

impl NativeStaticFinalizer {
    /// Creates one immutable compiler-generated finalizer contract.
    pub const fn new(
        execution: NativeStaticFinalizerExecution,
        outgoing_capacity: u32,
        result_size: usize,
        result_alignment: usize,
        start: NativeStaticFinalizerStartCallback,
        resolve: NativeStaticFinalizerResolveCallback,
    ) -> Self {
        Self {
            execution,
            outgoing_capacity,
            result_size,
            result_alignment,
            start,
            resolve,
        }
    }

    /// Returns the record allowance for all owners in the static value.
    pub const fn outgoing_capacity(self) -> u32 {
        self.outgoing_capacity
    }

    /// Returns how finalization reaches completion.
    pub const fn execution(self) -> NativeStaticFinalizerExecution {
        self.execution
    }

    /// Returns the completed result size in bytes.
    pub const fn result_size(self) -> usize {
        self.result_size
    }

    /// Returns the completed result alignment in bytes.
    pub const fn result_alignment(self) -> usize {
        self.result_alignment
    }

    /// Returns the callback starting finalization.
    pub const fn start(self) -> NativeStaticFinalizerStartCallback {
        self.start
    }

    /// Returns the callback consuming the completed result.
    pub const fn resolve(self) -> NativeStaticFinalizerResolveCallback {
        self.resolve
    }
}

/// Compiler-generated infallible static lifecycle transition callback.
pub type NativeStaticTransitionCallback = extern "C" fn();

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
    prepare: NativeStaticTransitionCallback,
    finalizer: NativeStaticFinalizer,
    destroy: NativeStaticCleanupCallback,
    detach: NativeStaticTransitionCallback,
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
        prepare: NativeStaticTransitionCallback,
        finalizer: NativeStaticFinalizer,
        destroy: NativeStaticCleanupCallback,
        detach: NativeStaticTransitionCallback,
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
            prepare,
            finalizer,
            destroy,
            detach,
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

    /// Returns the callback making storage unavailable for cleanup.
    pub const fn prepare(self) -> NativeStaticTransitionCallback {
        self.prepare
    }

    /// Returns the graceful finalization callback.
    pub const fn finalizer(self) -> NativeStaticFinalizer {
        self.finalizer
    }

    /// Returns the infallible destruction callback.
    pub const fn destroy(self) -> NativeStaticCleanupCallback {
        self.destroy
    }

    /// Returns the callback publishing the cleaned storage state.
    pub const fn detach(self) -> NativeStaticTransitionCallback {
        self.detach
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

/// Exact-thread cleanup registration transferred to the runtime registry.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeThreadStaticCleanupRegistration {
    product: &'static NativeProductHostDescriptor,
    static_identity: NativeStaticIdentity,
    prepare: NativeStaticTransitionCallback,
    finalizer: NativeStaticFinalizer,
    destroy: NativeStaticCleanupCallback,
    detach: NativeStaticTransitionCallback,
}

impl NativeThreadStaticCleanupRegistration {
    /// Creates one exact-thread cleanup registration.
    pub const fn new(
        product: &'static NativeProductHostDescriptor,
        static_identity: NativeStaticIdentity,
        prepare: NativeStaticTransitionCallback,
        finalizer: NativeStaticFinalizer,
        destroy: NativeStaticCleanupCallback,
        detach: NativeStaticTransitionCallback,
    ) -> Self {
        Self {
            product,
            static_identity,
            prepare,
            finalizer,
            destroy,
            detach,
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

    /// Returns the callback making this instance unavailable for cleanup.
    pub const fn prepare(self) -> NativeStaticTransitionCallback {
        self.prepare
    }

    /// Returns the graceful finalization callback.
    pub const fn finalizer(self) -> NativeStaticFinalizer {
        self.finalizer
    }

    /// Returns the infallible destruction callback.
    pub const fn destroy(self) -> NativeStaticCleanupCallback {
        self.destroy
    }

    /// Returns the infallible exact-thread detach callback.
    pub const fn detach(self) -> NativeStaticTransitionCallback {
        self.detach
    }
}

#[cfg(test)]
mod tests {
    use super::super::host::{NativeProductHostDescriptor, NativeProductHostObservation};
    use super::{
        NativeCleanupIncident, NativeStaticFinalizer, NativeStaticHostEntry,
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

        assert_eq!(
            std::mem::align_of::<NativeCleanupIncident>(),
            std::mem::align_of::<usize>()
        );

        assert_eq!(
            std::mem::align_of::<NativeStaticFinalizer>(),
            std::mem::align_of::<usize>()
        );
    }

    #[test]
    fn typed_incidents_retain_their_native_provider_record() {
        assert_abi_layout!(NativeCleanupIncident, size: 144, align: 8, fields: {
            payload: 0, type_identity: 8, source: 40, report: 96, destroy: 104, provider: 112,
        });
    }
}
