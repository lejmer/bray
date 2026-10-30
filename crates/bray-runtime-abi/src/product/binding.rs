use crate::{
    NativeProductHostObservation, NativeProviderCallbacks, NativeProviderOwner,
    NativeProviderReference, PRODUCT_HOST_ABI_VERSION,
};

/// Scheduler-independent services admitted by one provider binding.
pub const PROVIDER_HOST_SERVICES: u32 = 1;
/// Execution resources admitted for reachable asynchronous or worker cleanup.
pub const PROVIDER_EXECUTION_SERVICES: u32 = 2;

/// Forms an independently loaded image while its native residency owner is held.
pub type NativeProductProviderFormation = extern "C" fn(
    usize,
    &NativeProductBinding<'_>,
    &mut NativeProductLoadObservation,
) -> crate::NativeRuntimeStatus;
/// Controls a checked external activation through the resident host.
pub type NativeProductProviderControl =
    extern "C" fn(usize, u64, u32, &mut NativeProductLoadObservation) -> crate::NativeRuntimeStatus;
/// Transfers final native residency to a caller outside the retiring image.
pub type NativeProductProviderRetire =
    extern "C" fn(usize, u64, &mut NativeProductRetirement) -> crate::NativeRuntimeStatus;
/// Acquires one root for a producer whose enclosing admitted entry retains the image.
pub type NativeProductProviderRoot =
    extern "C" fn(usize, &mut NativeProviderReference) -> crate::NativeRuntimeStatus;

/// Resumes an already closing load after a native residency owner releases.
pub type NativeProductProviderNotify =
    extern "C" fn(usize, usize, *const NativeProviderCallbacks) -> crate::NativeRuntimeStatus;

/// Image-local formation and root-acquisition operations. Copying their native
/// addresses does not retain the image. callers keep its enclosing residency owner.
#[repr(C)]
#[derive(Debug)]
pub struct NativeProductProviderOperations {
    formation: NativeProductProviderFormation,
    root: NativeProductProviderRoot,
}

impl NativeProductProviderOperations {
    /// Creates the complete compiler-generated operation table.
    pub const fn new(
        formation: NativeProductProviderFormation,
        root: NativeProductProviderRoot,
    ) -> Self {
        Self { formation, root }
    }
}

/// Stable storage and callback domain retained by the resident native host.
/// Every pointed-to owner and the view itself outlive all bound image activations.
#[repr(C)]
#[derive(Debug)]
pub struct NativeProviderDomain {
    host: usize,
    synchronization: usize,
    pool: usize,
    callbacks: Option<&'static NativeProviderCallbacks>,
    consumer: Option<crate::NativePanicReportConsumer>,
    control: Option<NativeProductProviderControl>,
    retire: Option<NativeProductProviderRetire>,
    notify: Option<NativeProductProviderNotify>,
}

impl NativeProviderDomain {
    /// Creates an unpublished destination for the resident domain-access operation.
    pub const fn empty() -> Self {
        Self {
            host: 0,
            synchronization: 0,
            pool: 0,
            callbacks: None,
            consumer: None,
            control: None,
            retire: None,
            notify: None,
        }
    }

    /// Returns whether the resident producer published every required owner.
    pub const fn is_valid(&self) -> bool {
        self.host != 0
            && self.synchronization != 0
            && self.pool != 0
            && self.callbacks.is_some()
            && self.consumer.is_some()
            && self.control.is_some()
            && self.retire.is_some()
            && self.notify.is_some()
    }
}

/// External activation identity. Reload never reuses its resident-assigned epoch.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeProductHandle {
    descriptor: usize,
    epoch: u64,
}

impl NativeProductHandle {
    /// Creates a native handle for an already admitted activation.
    pub const fn new(descriptor: usize, epoch: u64) -> Self {
        Self { descriptor, epoch }
    }

    /// Returns the compiler-generated descriptor address.
    pub const fn descriptor(self) -> usize {
        self.descriptor
    }

    /// Returns the unique activation epoch.
    pub const fn epoch(self) -> u64 {
        self.epoch
    }
}

/// Native input view used only during independent image formation.
/// The caller retains the dependency slice and provider through that invocation.
#[repr(C)]
#[derive(Debug)]
pub struct NativeProductBinding<'dependencies> {
    abi_version: u32,
    services: u32,
    domain: &'static NativeProviderDomain,
    provider: NativeProviderReference,
    dependencies: *const NativeProductHandle,
    dependency_count: usize,
    dependency_lifetime: std::marker::PhantomData<&'dependencies [NativeProductHandle]>,
}

impl<'dependencies> NativeProductBinding<'dependencies> {
    /// Borrows the exact domain, native residency owner and already formed providers.
    pub const fn new(
        domain: &'static NativeProviderDomain,
        services: u32,
        provider: NativeProviderReference,
        dependencies: &'dependencies [NativeProductHandle],
    ) -> Self {
        Self {
            abi_version: PRODUCT_HOST_ABI_VERSION,
            services,
            domain,
            provider,
            dependencies: dependencies.as_ptr(),
            dependency_count: dependencies.len(),
            dependency_lifetime: std::marker::PhantomData,
        }
    }
}

/// Resident result of independent formation or an epoch-checked control operation.
#[repr(C)]
#[derive(Debug)]
pub struct NativeProductLoadObservation {
    observation: NativeProductHostObservation,
    handle: NativeProductHandle,
}

impl NativeProductLoadObservation {
    /// Creates an unpublished native result destination.
    pub const fn empty() -> Self {
        Self {
            observation: NativeProductHostObservation::invalid(),
            handle: NativeProductHandle::new(0, 0),
        }
    }

    /// Returns the lifecycle observation.
    pub const fn observation(&self) -> NativeProductHostObservation {
        self.observation
    }

    /// Returns the activation identity accepted by subsequent native operations.
    pub const fn handle(&self) -> NativeProductHandle {
        self.handle
    }
}

/// Final native residency transferred out of runtime callbacks before image unload.
#[repr(C)]
#[derive(Debug)]
pub struct NativeProductRetirement {
    observation: NativeProductHostObservation,
    provider: NativeProviderOwner,
}

impl NativeProductRetirement {
    /// Creates an unpublished native retirement destination.
    pub const fn empty() -> Self {
        Self {
            observation: NativeProductHostObservation::invalid(),
            provider: NativeProviderOwner::resident(),
        }
    }

    /// Returns the completed or blocked retirement observation.
    pub const fn observation(&self) -> NativeProductHostObservation {
        self.observation
    }

    /// Transfers the final residency owner to the resident caller.
    pub fn into_provider(self) -> NativeProviderOwner {
        self.provider
    }
}
