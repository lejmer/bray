//! Stable native ABI primitives shared by generated code and linked runtime components.

#![forbid(unsafe_code)]

#[cfg(test)]
#[macro_use]
mod layout;
mod activation;
mod catalog;
mod numeric;
mod panic;
mod platform;
mod process;
mod product;
mod provider;
mod records;
mod run_result;
mod runtime;
mod signature;
mod storage;
pub mod symbols;
mod task;
mod temporal;

pub use task::{NativeTaskObservationGlue, NativeValueCleanupFactory};

pub use activation::{NativeActivationDescriptor, NativeActivationExecution};
pub use numeric::{NativeComplexBits, NativeRealBits};
pub use panic::{
    NativePanicCause, NativePanicMessage, NativePanicMessageCopy, NativePanicMessageRelease,
    NativePanicPrimary, NativePanicReport, NativePanicReportConsumer, NativeSourceAnchor,
};
pub use platform::{
    NativePlatformEnvironmentEntry, NativePlatformEnvironmentList, NativePlatformFileMetadata,
    NativePlatformFileOptions, NativePlatformPath, NativePlatformSpanList, NativePlatformStatus,
    NativePlatformText,
};
pub use process::{NativePlatformChildRequest, NativePlatformExitStatus};
pub use product::{
    NativeCleanupIncident, NativeCleanupIncidentDestroyCallback,
    NativeCleanupIncidentReportCallback, NativeProductBinding, NativeProductCleanupBody,
    NativeProductCleanupResult, NativeProductExecution, NativeProductExecutionFactory,
    NativeProductHandle, NativeProductHostDescriptor, NativeProductHostObservation,
    NativeProductHostOperation, NativeProductHostState, NativeProductHostStatus,
    NativeProductIdentity, NativeProductLoadObservation, NativeProductProviderControl,
    NativeProductProviderFormation, NativeProductProviderNotify, NativeProductProviderOperations,
    NativeProductProviderRetire, NativeProductProviderRoot, NativeProductRetirement,
    NativeProviderDomain, NativeStaticAccessCallback, NativeStaticAdmissionCallback,
    NativeStaticCleanupCallback, NativeStaticDuration, NativeStaticFinalizer,
    NativeStaticFinalizerExecution, NativeStaticFinalizerResolveCallback,
    NativeStaticFinalizerStartCallback, NativeStaticFinalizerStatus, NativeStaticHostEntry,
    NativeStaticIdentity, NativeStaticTransitionCallback, NativeThreadStaticCleanupRegistration,
    NativeTypeIdentity, PRODUCT_HOST_ABI_VERSION, PROVIDER_EXECUTION_SERVICES,
    PROVIDER_HOST_SERVICES,
};
pub use provider::{
    NativeProviderCallbacks, NativeProviderOwner, NativeProviderReference,
    NativeProviderReferences, NativeProviderRelease, NativeProviderRetain,
};
pub use records::{NativeReportRecords, NativeReportSegment};
pub use run_result::NativeRunResultLayout;
pub use runtime::{
    MAX_PERFORMANCE_OBSERVATION_RECORDS, NativeExecutionLane, NativeExecutionLaneResult,
    NativeFrameActionCallback, NativeFrameAffinity, NativeFrameCancellationCallback,
    NativeFrameCompletionMoveCallback, NativeFrameComposition, NativeFrameExit,
    NativeFrameProgress, NativeFrameProgressKind, NativeFrameResolveCallback,
    NativeFrameResumeCallback, NativeFrameState, NativeFrameStateCallback, NativeInactiveFrame,
    NativeInactiveFrameTransfer, NativeLaneRequirements, NativeRootHandle, NativeRootStart,
    NativeRunOutcome, NativeRunState, NativeRuntimeConfiguration, NativeRuntimeEventCallback,
    NativeRuntimeStatus, NativeStringView, NativeSynchronousRootCallback, NativeTaskAllocation,
    NativeTaskHandle, NativeThreadCancellationCallback, NativeThreadOperationCallback,
    NativeValueCleanupCallback, NativeWakeCallback, PERFORMANCE_OBSERVATION_HEADER,
    PERFORMANCE_OBSERVATION_PATH_ENVIRONMENT,
};
pub use symbols::{
    MEMORY_ALLOCATION_OBSERVATION_SYMBOL, MEMORY_COPY_OBSERVATION_SYMBOL,
    MEMORY_OBSERVATION_BEGIN_SYMBOL, PERFORMANCE_INTERVAL_BEGIN_SYMBOL,
    PERFORMANCE_INTERVAL_END_SYMBOL,
};
pub use task::{NativeTaskDispatchServices, NativeTaskReference, NativeTaskSnapshot};
pub use temporal::{
    NativePlatformDateTime, NativePlatformTemporalObservation, NativePlatformTemporalResolution,
    NativePlatformTemporalValue,
};

pub use signature::{PlatformAbiType, PlatformAbiValue, platform_signature_matches};

pub use storage::{
    NativeCleanupStorageBundle, NativeCleanupStorageLease, NativeCleanupStorageShape,
};
