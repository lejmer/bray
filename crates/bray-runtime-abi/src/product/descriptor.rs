use super::model::NativeStaticHostCallback;
use crate::{NativeProductIdentity, PRODUCT_HOST_ABI_VERSION};

/// Immutable compiler-generated descriptor for one loaded product host.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct NativeProductHostDescriptor {
    abi_version: u32,
    asynchronous: u32,
    identity: NativeProductIdentity,
    static_entry: NativeStaticHostCallback,
    static_count: usize,
    execution: Option<crate::NativeProductExecutionFactory>,
    activation: usize,
    provider: usize,
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
            asynchronous: 0,
            identity,
            static_entry,
            static_count,
            execution: None,
            activation: 0,
            provider: 0,
        }
    }

    /// Selects the execution-resource provider required by async or exact-thread consumers.
    pub const fn with_execution(
        mut self,
        factory: crate::NativeProductExecutionFactory,
        asynchronous: bool,
    ) -> Self {
        self.execution = Some(factory);
        self.asynchronous = asynchronous as u32;

        self
    }

    /// Selects independent-load formation and the image's atomic activation slot.
    /// The enclosing native image owner retains both objects through retirement.
    pub fn with_provider_load(
        mut self,
        activation: *const std::sync::atomic::AtomicUsize,
        provider: *const crate::NativeProductProviderOperations,
    ) -> Self {
        self.activation = activation.addr();
        self.provider = provider.addr();

        self
    }

    /// Returns whether this image must bind a resident provider before entry.
    pub const fn is_independent(self) -> bool {
        self.activation != 0
    }

    /// Returns the demanded execution-resource factory, if the product requires one.
    pub const fn execution(self) -> Option<crate::NativeProductExecutionFactory> {
        self.execution
    }

    /// Returns whether formation must acquire a service capable of driving asynchronous cleanup.
    pub const fn asynchronous(self) -> bool {
        self.asynchronous != 0
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
