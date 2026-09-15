mod builder;
mod cleanup;
mod flow;
mod liveness;
mod model;
mod parts;
mod plan;
mod query;
mod replacement;
mod retention;
mod scope;
mod shape;

pub use builder::{StoragePlanBuildError, StoragePlanBuilder};
pub use flow::{
    StorageExitDecision, StorageExitPoint, StorageFlow, StorageFlowBuildError,
    StorageOperationDecision, StorageOperationStatus, StorageSuspensionState,
};
pub use liveness::{
    LastUse, LiveAcrossScope, LiveAcrossSuspension, Liveness, LivenessBuildError, OwnerRetention,
};
pub use model::{
    BorrowCapabilityId, StorageAccess, StorageAccessId, StorageAccessRoot, StorageAlternativeId,
    StorageIdentity, StorageIdentityId, StorageProjection, StorageRelationship,
};
pub use parts::{
    StorageCleanupPart, StorageCleanupProjection, StorageCleanupProjectionKind, StorageProtocolCall,
};
pub use plan::{
    BorrowCapabilityOrigin, PlannedBorrowCapability, StorageAccessPlan, StorageAccessPurpose,
    StorageAlternative, StorageBinding, StorageBindingTarget, StoragePlan,
};
pub use replacement::{
    StorageReplacementDecision, StorageReplacementPlan, StorageReplacementState,
};
pub use scope::{
    StorageScopeBuildError, StorageScopeOwners, storage_expression_republishes_destructor_receiver,
    storage_identity_is_destructor_receiver, storage_identity_transfers_at_unit_exit,
};
pub use shape::StorageCleanupType;
