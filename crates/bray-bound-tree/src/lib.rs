//! Source-correlated semantic trees after name binding.

#![forbid(unsafe_code)]

mod dependency;
mod identity;
mod node;
mod origin;
mod storage;
#[cfg(test)]
mod test_support;
mod unit;

pub use dependency::{
    BoundDependencyContract, BoundDependencyContractId, BoundDependencyGuard,
    BoundDependencyRequirement, BoundDependencyRequirementKind, BoundDependencySubject,
    DependencyContractInstantiationContext, DependencyContractInstantiationError,
    GuardedBoundDependencyRequirement, LifecycleObligationId, ScopedCapabilityId,
};
pub use node::{
    AnyBoundNodeId, BoundBlockId, BoundCallableBodyId, BoundExpressionId, BoundNodeKind,
    BoundPatternId, ExactBoundNodeId,
};
pub use origin::{
    BoundNodeOrdinal, BoundNodeOrigin, BoundSynthesisRole, SynthesizedBoundNodeOrigin,
};
pub use storage::{
    BorrowCapability, BorrowCapabilityId, StorageAccess, StorageAccessId, StorageAccessRoot,
    StorageIdentity, StorageIdentityId, StorageProjection, StorageRelationship,
};
pub use unit::{
    AnonymousCallableUnitKey, BoundSourceAnchor, BoundUnitId, BoundUnitKey, BoundUnitKeyData,
    BoundUnitKind, DeclaredBoundUnitKey,
};
