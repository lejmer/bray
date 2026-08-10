//! Dependency-light semantic model shared by compiler runtime contracts and runtime mechanisms.

#![forbid(unsafe_code)]

mod frame;
mod identity;
mod requirements;
mod version;

pub use frame::{ProtectedFrameStateId, ProtectedFrameStorageId, ProtectedFrameDependencyId, ProtectedFrameAffinity, ProtectedFrameOperation, ProtectedFrameOperations, ProtectedFrameLayout, ProtectedFrameLayoutBuildError, ProtectedFrameStateDescriptor, ProtectedFrameDescriptor, ProtectedFrameDescriptorBuildError};
pub use identity::{ProtectedAsyncFrameId, RuntimeAbiVersion, BinarySymbolName, RuntimeArtifactId, RuntimeIdentity, PanicAbiIdentity};
pub use requirements::{ExecutionLaneRequirement, RuntimeCapability};
pub use version::{ProtectedFrameAbiOperation, ProtectedFrameAbiVersions};
