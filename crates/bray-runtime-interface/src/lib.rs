//! Backend-neutral compiler/runtime interface and executable-host contracts.

#![forbid(unsafe_code)]

mod artifact;
mod compatibility;
mod contract;
mod frame;
mod identity;
mod native;
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
    ExecutableHostContractBuilder, ExecutionCapacityLimits, ExecutionLaneRequirement,
    RootExecution, RuntimeCapability,
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
    AWAITED_FRAME_COMPOSITION_SYMBOL, CLEANUP_INCIDENT_REPORTING_SYMBOL,
    COMPATIBLE_LANE_SELECTION_SYMBOL, CURRENT_RUN_CANCELLATION_OBSERVATION_SYMBOL,
    ENTRY_FAILURE_REPORTING_SYMBOL, FRAME_COMPLETION_MOVE_SYMBOL, JOIN_REGISTRATION_SYMBOL,
    MAIN_THREAD_LANE_DRIVE_SYMBOL, MAIN_THREAD_LANE_STARTUP_SYMBOL, NativeExecutionLane,
    NativeExecutionLaneResult, NativeFrameActionCallback, NativeFrameAffinity,
    NativeFrameCancellationCallback, NativeFrameExit, NativeFrameMoveBeforeStartCallback,
    NativeFrameProgress, NativeFrameProgressKind, NativeFrameResolveCallback,
    NativeFrameResumeCallback, NativeFrameState, NativeFrameStateCallback, NativeInactiveFrame,
    NativeLaneRequirements, NativeProtectedFrame, NativeProtectedFrameTransfer, NativeRootHandle,
    NativeRootStart, NativeRunOutcome, NativeRunState, NativeRuntimeConfiguration,
    NativeRuntimeEventCallback, NativeRuntimeStatus, NativeStringView,
    NativeSynchronousRootCallback, NativeTaskAllocation, NativeTaskHandle, NativeWakeCallback,
    PANIC_PROPAGATION_SYMBOL, PANIC_REPORT_CONSTRUCTION_SYMBOL, PANIC_REPORTING_SYMBOL,
    ROOT_CANCELLATION_REQUEST_SYMBOL, ROOT_COMPLETION_RESOLUTION_SYMBOL, ROOT_EXECUTION_SYMBOL,
    ROOT_TERMINAL_OBSERVATION_SYMBOL, RUNTIME_EVENT_SYMBOL, STRUCTURED_SHUTDOWN_SYMBOL,
    SUSPENSION_REGISTRATION_SYMBOL, SYNCHRONOUS_ROOT_EXECUTION_SYMBOL, TASK_ALLOCATION_SYMBOL,
    TASK_CANCELLATION_REQUEST_SYMBOL, TASK_START_SYMBOL, TERMINAL_PUBLICATION_SYMBOL, WAKE_SYMBOL,
};
pub use role::{
    RuntimeAbiRole, RuntimeRoleBinding, RuntimeRoleContract, RuntimeRoleContractEffect,
    RuntimeRoleImplementation,
};
pub use runtime::{RuntimeCompatibilityError, RuntimeContract, RuntimeContractBuildError};
