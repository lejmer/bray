//! Compiler-facing runtime selection, metadata, and executable-host contracts.

#![forbid(unsafe_code)]

mod artifact;
mod binding;
mod compatibility;
mod contract;
mod platform;
mod role;
mod runtime;

pub use artifact::{
    RuntimeArtifact, RuntimeArtifactBuildError, RuntimeArtifactDigest, RuntimeArtifactMetadata,
    RuntimeArtifactMetadataBuildError, RuntimeArtifactMetadataDecodeError,
    RuntimeArtifactMetadataEncodeError,
};
pub use binding::{native_platform_service_role_symbol, native_runtime_role_symbol};
pub use bray_runtime_model::{
    BinarySymbolName, ExecutionLaneRequirement, PanicAbiIdentity, ProtectedAsyncFrameId,
    ProtectedFrameAbiOperation, ProtectedFrameAbiVersions, ProtectedFrameAffinity,
    ProtectedFrameDependencyId, ProtectedFrameDescriptor, ProtectedFrameDescriptorBuildError,
    ProtectedFrameLayout, ProtectedFrameLayoutBuildError, ProtectedFrameOperation,
    ProtectedFrameOperations, ProtectedFrameStateDescriptor, ProtectedFrameStateId,
    ProtectedFrameStorageId, RuntimeAbiVersion, RuntimeArtifactId, RuntimeCapability,
    RuntimeIdentity,
};
pub use compatibility::{RuntimeRequirements, RuntimeRequirementsMergeError};
pub use contract::{
    ExecutableEntryResult, ExecutableHostContract, ExecutableHostContractBuildError,
    ExecutableHostContractBuilder, ExecutableHostEntry, ExecutableHostEntryId,
    ExecutionCapacityLimits, RootExecution,
};
pub use platform::{
    PlatformAbiType, PlatformServiceBinding, PlatformServiceRole, PlatformServiceSignature,
};
pub use role::{
    RuntimeAbiRole, RuntimeRoleBinding, RuntimeRoleContract, RuntimeRoleContractEffect,
    RuntimeRoleImplementation,
};
pub use runtime::{RuntimeCompatibilityError, RuntimeContract, RuntimeContractBuildError};
