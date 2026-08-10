//! Dependency-light semantic model shared by compiler runtime contracts and runtime mechanisms.

#![forbid(unsafe_code)]

mod frame;
mod identity;
mod requirements;
mod version;

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
pub use requirements::{ExecutionLaneRequirement, RuntimeCapability};
pub use version::{ProtectedFrameAbiOperation, ProtectedFrameAbiVersions};
