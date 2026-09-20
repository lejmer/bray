//! Compiler-facing runtime selection, metadata, and executable-host contracts.

#![forbid(unsafe_code)]

mod artifact;
mod binding;
mod compatibility;
mod component_dependency;
mod component_validation;
mod contract;
mod family;
mod platform;
mod role;
mod role_artifact;
mod runtime;
mod signature;

pub use artifact::{
    RuntimeArtifact, RuntimeArtifactBuildError, RuntimeArtifactComponent,
    RuntimeArtifactComponentMetadata, RuntimeArtifactDigest, RuntimeArtifactMetadata,
    RuntimeArtifactMetadataBuildError, RuntimeArtifactMetadataDecodeError,
    RuntimeArtifactMetadataEncodeError, RuntimeArtifactPurpose, RuntimeArtifactSelection,
    RuntimeArtifactSelectionError,
};
pub use binding::SourceRoleBinding;
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
    ExecutionCapacityLimits, RootExecution, selected_runtime_role_symbol,
};
pub use family::PlatformServiceFamily;
pub use platform::{
    PlatformAbiType, PlatformServiceBinding, PlatformServiceRole, PlatformServiceSignature,
};
pub use role::{
    RuntimeAbiRole, RuntimeRoleBinding, RuntimeRoleContract,
    RuntimeRoleContractEffect, RuntimeRoleImplementation, RuntimeRoleSourceBinding,
    RuntimeServiceClass,
};
pub use runtime::{RuntimeCompatibilityError, RuntimeContract, RuntimeContractBuildError};

pub use role_artifact::RuntimeRoleArtifact;

pub use signature::{RuntimeAbiType, RuntimeNativeSignature};

pub use bray_runtime_abi::runtime_role_catalog;
