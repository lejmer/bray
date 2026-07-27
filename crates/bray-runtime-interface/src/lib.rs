//! Backend-neutral compiler/runtime interface and executable-host contracts.

#![forbid(unsafe_code)]

mod compatibility;
mod contract;
mod identity;
mod role;
mod runtime;

pub use compatibility::{
    ProtectedFrameAbiOperation, ProtectedFrameAbiVersions, RuntimeRequirements,
    RuntimeRequirementsMergeError,
};
pub use contract::{
    ExecutableHostContract, ExecutableHostContractBuildError, ExecutableHostContractBuilder,
    ExecutionCapacityLimits, ExecutionLaneRequirement, RootExecution, RuntimeCapability,
};
pub use identity::{
    BinarySymbolName, PanicAbiIdentity, ProtectedAsyncFrameId, RuntimeAbiVersion,
    RuntimeArtifactId, RuntimeIdentity,
};
pub use role::{
    RuntimeAbiRole, RuntimeRoleBinding, RuntimeRoleContract, RuntimeRoleContractEffect,
    RuntimeRoleImplementation,
};
pub use runtime::{
    RuntimeCompatibilityError, RuntimeContract, RuntimeContractBuildError,
};
