//! Backend-neutral compiler/runtime interface and executable-host contracts.

#![forbid(unsafe_code)]

mod artifact;
mod compatibility;
mod contract;
mod frame;
mod identity;
mod native;
mod platform;
mod role;
mod runtime;

pub use artifact::{
    RuntimeArtifact, RuntimeArtifactBuildError, RuntimeArtifactDigest, RuntimeArtifactMetadata,
    RuntimeArtifactMetadataBuildError, RuntimeArtifactMetadataDecodeError,
    RuntimeArtifactMetadataEncodeError,
};
pub use compatibility::{
    ProtectedFrameAbiOperation, ProtectedFrameAbiVersions, RuntimeRequirements,
    RuntimeRequirementsMergeError,
};
pub use contract::{
    ExecutableEntryResult, ExecutableHostContract, ExecutableHostContractBuildError,
    ExecutableHostContractBuilder, ExecutableHostEntry, ExecutableHostEntryId,
    ExecutionCapacityLimits, ExecutionLaneRequirement, RootExecution, RuntimeCapability,
};
pub use frame::{
    ProtectedFrameAffinity, ProtectedFrameDependencyId, ProtectedFrameDescriptor,
    ProtectedFrameDescriptorBuildError, ProtectedFrameLayout, ProtectedFrameLayoutBuildError,
    ProtectedFrameOperation, ProtectedFrameOperations, ProtectedFrameStateDescriptor,
    ProtectedFrameStateId, ProtectedFrameStorageId,
};
pub use identity::{
    BinarySymbolName, PanicAbiIdentity, ProtectedAsyncFrameId, RuntimeAbiVersion,
    RuntimeArtifactId, RuntimeIdentity,
};
pub use native::{
    AWAITED_FRAME_COMPOSITION_SYMBOL, CHARACTER_FROM_SCALAR_VALUE_SYMBOL,
    CHARACTER_IS_ALPHABETIC_SYMBOL, CHARACTER_IS_NUMERIC_SYMBOL, CHARACTER_IS_WHITESPACE_SYMBOL,
    CHARACTER_SCALAR_VALUE_SYMBOL, CHARACTER_UNICODE_DATA_VERSION, CHARACTER_UTF8_BYTE_SYMBOL,
    CHARACTER_UTF8_LENGTH_SYMBOL, CLEANUP_INCIDENT_REPORTING_SYMBOL,
    COMPATIBLE_LANE_SELECTION_SYMBOL, CURRENT_RUN_CANCELLATION_OBSERVATION_SYMBOL,
    CURRENT_RUN_CANCELLATION_PROPAGATION_SYMBOL, ENTRY_FAILURE_REPORTING_SYMBOL,
    FRAME_COMPLETION_MOVE_SYMBOL, JOIN_REGISTRATION_SYMBOL, MAIN_THREAD_LANE_DRIVE_SYMBOL,
    MAIN_THREAD_LANE_STARTUP_SYMBOL, MEMORY_ALLOCATION_SYMBOL, MEMORY_DEALLOCATION_SYMBOL,
    NativeExecutionLane, NativeExecutionLaneResult, NativeFrameActionCallback, NativeFrameAffinity,
    NativeFrameCancellationCallback, NativeFrameExit, NativeFrameMoveBeforeStartCallback,
    NativeFrameProgress, NativeFrameProgressKind, NativeFrameResolveCallback,
    NativeFrameResumeCallback, NativeFrameState, NativeFrameStateCallback, NativeInactiveFrame,
    NativeLaneRequirements, NativePanicCause, NativeProtectedFrame, NativeProtectedFrameTransfer,
    NativeRootHandle, NativeRootStart, NativeRunOutcome, NativeRunState,
    NativeRuntimeConfiguration, NativeRuntimeEventCallback, NativeRuntimeStatus,
    NativeSourceAnchor, NativeStringView, NativeSynchronousRootCallback, NativeTaskAllocation,
    NativeTaskHandle, NativeWakeCallback, PANIC_PROPAGATION_SYMBOL,
    PANIC_REPORT_CONSTRUCTION_SYMBOL, PANIC_REPORTING_SYMBOL, PLATFORM_CLOCK_MONOTONIC_NOW_SYMBOL,
    PLATFORM_CLOCK_SLEEP_SYMBOL, PLATFORM_CLOCK_WALL_NOW_SYMBOL, PLATFORM_CONTEXT_COPY_SYMBOL,
    PLATFORM_CONTEXT_ENVIRONMENT_KEY_EQUALS_SYMBOL, PLATFORM_CONTEXT_MEASURE_SYMBOL,
    PLATFORM_ENTROPY_FILL_SYMBOL, PLATFORM_STREAM_FLUSH_SYMBOL, PLATFORM_STREAM_LOCK_SYMBOL,
    PLATFORM_STREAM_READ_SYMBOL, PLATFORM_STREAM_UNLOCK_SYMBOL, PLATFORM_STREAM_WRITE_SYMBOL,
    ROOT_CANCELLATION_REQUEST_SYMBOL, ROOT_COMPLETION_RESOLUTION_SYMBOL, ROOT_EXECUTION_SYMBOL,
    ROOT_TERMINAL_OBSERVATION_SYMBOL, RUNTIME_EVENT_SYMBOL, STRING_EQUALS_SYMBOL,
    STRING_FROM_UTF8_SYMBOL, STRING_SCALAR_AT_SYMBOL, STRING_SCALAR_COUNT_SYMBOL,
    STRING_SCALAR_SLICE_SYMBOL, STRUCTURED_SHUTDOWN_SYMBOL, SUSPENSION_REGISTRATION_SYMBOL,
    SYNCHRONOUS_ROOT_EXECUTION_SYMBOL, TASK_ALLOCATION_SYMBOL, TASK_CANCELLATION_REQUEST_SYMBOL,
    TASK_START_SYMBOL, TERMINAL_PUBLICATION_SYMBOL, TEST_ENTRY_SELECTION_SYMBOL, WAKE_SYMBOL,
    native_platform_service_role_symbol, native_runtime_role_symbol,
};
pub use platform::{
    NativePlatformFileMetadata, NativePlatformFileOptions, NativePlatformPath,
    NativePlatformStatus, NativePlatformText, PlatformAbiType, PlatformServiceBinding,
    PlatformServiceRole, PlatformServiceSignature,
};
pub use role::{
    RuntimeAbiRole, RuntimeRoleBinding, RuntimeRoleContract, RuntimeRoleContractEffect,
    RuntimeRoleImplementation,
};
pub use runtime::{RuntimeCompatibilityError, RuntimeContract, RuntimeContractBuildError};
