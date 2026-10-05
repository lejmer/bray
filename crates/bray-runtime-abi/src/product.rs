mod host;
mod identity;
mod static_storage;

pub use host::{
    NativeProductBinding, NativeProductHostDescriptor, NativeProductHostObservation,
    NativeProductHostOperation, NativeProductHostState, NativeProductHostStatus,
    PRODUCT_EXECUTION_SERVICES, PRODUCT_HOST_ABI_VERSION, PRODUCT_HOST_SERVICES,
    PRODUCT_UNLOADABLE,
};
pub use identity::{NativeProductIdentity, NativeStaticIdentity, NativeTypeIdentity};
pub use static_storage::{
    NativeCleanupIncident, NativeCleanupIncidentDestroyCallback,
    NativeCleanupIncidentReportCallback, NativeStaticAccessCallback, NativeStaticCleanupCallback,
    NativeStaticDuration, NativeStaticFinalizer, NativeStaticFinalizerExecution,
    NativeStaticFinalizerResolveCallback, NativeStaticFinalizerStartCallback,
    NativeStaticFinalizerStatus, NativeStaticHostEntry, NativeStaticTransitionCallback,
    NativeThreadStaticCleanupRegistration,
};
