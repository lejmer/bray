//! Stable native ABI primitives shared by generated code and linked runtime components.

#![forbid(unsafe_code)]

#[cfg(test)]
#[macro_use]
mod layout;
mod catalog;
mod platform;
mod process;
mod product;
mod run_result;
mod runtime;
mod signature;
pub mod symbols;
mod temporal;

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
    NativeProductIdentity, NativeStaticAccessCallback, NativeStaticCleanupCallback,
    NativeStaticDuration, NativeStaticFinalizer, NativeStaticFinalizerExecution,
    NativeStaticFinalizerResolveCallback, NativeStaticFinalizerStartCallback,
    NativeStaticFinalizerStatus, NativeStaticHostEntry, NativeStaticIdentity,
    NativeStaticTransitionCallback, NativeThreadStaticCleanupRegistration, NativeTypeIdentity,
    PRODUCT_HOST_ABI_VERSION,
};
pub use run_result::NativeRunResultLayout;
pub use runtime::{
    MAX_PERFORMANCE_OBSERVATION_RECORDS, MEMORY_ALLOCATION_OBSERVATION_SYMBOL,
    MEMORY_COPY_OBSERVATION_SYMBOL, MEMORY_OBSERVATION_BEGIN_SYMBOL, NativeBrayCallOutcome,
    NativeExecutionLane, NativeExecutionLaneResult, NativeFrameActionCallback, NativeFrameAffinity,
    NativeFrameCancellationCallback, NativeFrameCompletionMoveCallback, NativeFrameExit,
    NativeFrameMoveBeforeStartCallback, NativeFrameProgress, NativeFrameProgressKind,
    NativeFrameResolveCallback, NativeFrameResumeCallback, NativeFrameState,
    NativeFrameStateCallback, NativeInactiveFrame, NativeLaneRequirements, NativePanicCause,
    NativeProtectedFrame, NativeProtectedFrameTransfer, NativeRootHandle, NativeRootStart,
    NativeRunOutcome, NativeRunState, NativeRuntimeConfiguration, NativeRuntimeEventCallback,
    NativeRuntimeStatus, NativeSourceAnchor, NativeStringView, NativeSynchronousRootCallback,
    NativeTaskAllocation, NativeTaskHandle, NativeThreadCancellationCallback,
    NativeThreadOperationCallback, NativeWakeCallback, PERFORMANCE_INTERVAL_BEGIN_SYMBOL,
    PERFORMANCE_INTERVAL_END_SYMBOL, PERFORMANCE_OBSERVATION_HEADER,
    PERFORMANCE_OBSERVATION_PATH_ENVIRONMENT,
};
pub use temporal::{
    NativePlatformDateTime, NativePlatformTemporalObservation, NativePlatformTemporalResolution,
    NativePlatformTemporalValue,
};

pub use signature::{PlatformAbiType, PlatformAbiValue, platform_signature_matches};
