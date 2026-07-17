mod builder;
mod facts;
mod model;
mod requirements;
mod support;

pub use builder::{CheckedStorageFactsBuildError, CheckedStorageFactsBuilder};
pub use facts::{
    CheckedStorageFacts, LocalStorageFact, StorageAccessFact, StorageAccessOccurrence,
    StorageParameter, StorageParameterFact, StorageReferent, SurfaceStorageFact,
    SurfaceStorageSymbol,
};
pub use model::{
    BorrowCapability, BorrowCapabilityId, StorageAccess, StorageAccessId, StorageAccessRoot,
    StorageIdentity, StorageIdentityId, StorageProjection, StorageRelationship,
};
