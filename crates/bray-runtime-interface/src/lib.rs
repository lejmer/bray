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
    RuntimeArtifact, RuntimeArtifactBuildError, RuntimeArtifactDigest,
    RuntimeArtifactMetadata, RuntimeArtifactMetadataBuildError,
    RuntimeArtifactMetadataDecodeError, RuntimeArtifactMetadataEncodeError,
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
    ProtectedFrameDescriptorBuildError, ProtectedFrameLayout,
    ProtectedFrameLayoutBuildError, ProtectedFrameOperation,
    ProtectedFrameOperations, ProtectedFrameStateDescriptor, ProtectedFrameStateId,
    ProtectedFrameStorageId,
};
pub use identity::{
    BinarySymbolName, PanicAbiIdentity, ProtectedAsyncFrameId, RuntimeAbiVersion,
    RuntimeArtifactId, RuntimeIdentity,
};
pub use native::{
    COMPATIBLE_LANE_SELECTION_SYMBOL, CURRENT_RUN_CANCELLATION_OBSERVATION_SYMBOL,
    JOIN_REGISTRATION_SYMBOL, MAIN_THREAD_LANE_DRIVE_SYMBOL,
    MAIN_THREAD_LANE_STARTUP_SYMBOL, NativeExecutionLane,
    NativeExecutionLaneResult, NativeFrameActionCallback, NativeFrameAffinity,
    NativeFrameExit, NativeFrameProgress, NativeFrameProgressKind,
    NativeFrameResolveCallback, NativeFrameResumeCallback, NativeFrameState,
    NativeFrameStateCallback, NativeLaneRequirements, NativeProtectedFrame,
    NativeRunOutcome, NativeRunState, NativeRuntimeConfiguration,
    NativeRuntimeEventCallback, NativeRuntimeStatus, NativeTaskAllocation,
    NativeTaskHandle, NativeWakeCallback, RUNTIME_EVENT_SYMBOL,
    ROOT_EXECUTION_SYMBOL, STRUCTURED_SHUTDOWN_SYMBOL, SUSPENSION_REGISTRATION_SYMBOL,
    TASK_ALLOCATION_SYMBOL, TASK_CANCELLATION_REQUEST_SYMBOL,
    TASK_START_SYMBOL, TERMINAL_PUBLICATION_SYMBOL, WAKE_SYMBOL,
};
pub use role::{
    RuntimeAbiRole, RuntimeRoleBinding, RuntimeRoleContract, RuntimeRoleContractEffect,
    RuntimeRoleImplementation,
};
pub use runtime::{
    RuntimeCompatibilityError, RuntimeContract, RuntimeContractBuildError,
};
