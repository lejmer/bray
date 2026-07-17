//! Backend-neutral binary execution and executable-host contracts.

#![forbid(unsafe_code)]

mod contract;
mod identity;
mod role;

pub use contract::{
    ExecutableHostContract, ExecutableHostContractBuildError, ExecutableHostContractBuilder,
    ExecutionCapacityLimits, ExecutionLaneRequirement, RootExecution, RuntimeFeature,
};
pub use identity::{BinarySymbolName, ProtectedAsyncFrameId, RuntimeAbiVersion, RuntimeArtifactId};
pub use role::{RuntimeAbiRole, RuntimeRoleBinding, RuntimeRoleImplementation};
