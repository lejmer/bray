//! Stable native ABI primitives shared by generated code and linked runtime components.

#![forbid(unsafe_code)]

#[cfg(test)]
#[macro_use]
mod layout;
mod activation;
mod catalog;
mod panic;
mod platform;
mod process;
mod product;
mod product_execution;
mod records;
mod run_result;
mod runtime;
mod signature;
mod storage;
pub mod symbols;
mod temporal;

pub use activation::{NativeActivationDescriptor, NativeActivationExecution};
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
    NativeCleanupIncidentReportCallback, NativeProductHostDescriptor, NativeProductHostObservation,
    NativeProductHostOperation, NativeProductHostState, NativeProductHostStatus,
    NativeProductIdentity, NativeStaticAccessCallback, NativeStaticAdmissionCallback,
    NativeStaticCleanupCallback, NativeStaticDuration, NativeStaticFinalizer,
    NativeStaticFinalizerExecution, NativeStaticFinalizerResolveCallback,
    NativeStaticFinalizerStartCallback, NativeStaticFinalizerStatus, NativeStaticHostEntry,
    NativeStaticIdentity, NativeStaticTransitionCallback, NativeThreadStaticCleanupRegistration,
    NativeTypeIdentity, PRODUCT_HOST_ABI_VERSION,
};
pub use product_execution::{
    NativeProductCleanupBody, NativeProductCleanupResult, NativeProductExecution,
    NativeProductExecutionFactory,
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
pub use temporal::{
    NativePlatformDateTime, NativePlatformTemporalObservation, NativePlatformTemporalResolution,
    NativePlatformTemporalValue,
};

pub use signature::{PlatformAbiType, PlatformAbiValue, platform_signature_matches};

pub use storage::{
    NativeCleanupStorageBundle, NativeCleanupStorageLease, NativeCleanupStorageShape,
};
