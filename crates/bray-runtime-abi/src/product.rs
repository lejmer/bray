mod binding;
mod descriptor;
mod execution;
mod model;

pub use binding::{
    NativeProductBinding, NativeProductHandle, NativeProductLoadObservation,
    NativeProductProviderControl, NativeProductProviderFormation, NativeProductProviderNotify,
    NativeProductProviderOperations, NativeProductProviderRetire, NativeProductProviderRoot,
    NativeProductRetirement, NativeProviderDomain, PROVIDER_EXECUTION_SERVICES,
    PROVIDER_HOST_SERVICES,
};
pub use descriptor::NativeProductHostDescriptor;
pub use execution::{
    NativeProductCleanupBody, NativeProductCleanupResult, NativeProductExecution,
    NativeProductExecutionFactory,
};
pub use model::{
    NativeCleanupIncident, NativeCleanupIncidentDestroyCallback,
    NativeCleanupIncidentReportCallback, NativeProductHostObservation, NativeProductHostOperation,
    NativeProductHostState, NativeProductHostStatus, NativeProductIdentity,
    NativeStaticAccessCallback, NativeStaticAdmissionCallback, NativeStaticCleanupCallback,
    NativeStaticDuration, NativeStaticFinalizer, NativeStaticFinalizerExecution,
    NativeStaticFinalizerResolveCallback, NativeStaticFinalizerStartCallback,
    NativeStaticFinalizerStatus, NativeStaticHostEntry, NativeStaticIdentity,
    NativeStaticTransitionCallback, NativeThreadStaticCleanupRegistration, NativeTypeIdentity,
    PRODUCT_HOST_ABI_VERSION,
};
