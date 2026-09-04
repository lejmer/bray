use bray_symbols::TypeId;

use super::{StorageAccess, StorageAccessId, StorageIdentityId, StoragePlan, StorageProjection};

impl StoragePlan {
    /// Returns the first evaluated access that names one logical storage path.
    pub fn access_at(
        &self,
        identity: StorageIdentityId,
        projections: &[StorageProjection],
    ) -> Option<StorageAccessId> {
        self.access_entries().find_map(|(access, _)| {
            (self.root_identity(access) == Some(identity)
                && self.resolved_projections(access) == Some(projections))
            .then_some(access)
        })
    }

    /// Returns the first evaluated access that names one complete persistent storage root.
    pub fn root_access(&self, identity: StorageIdentityId) -> Option<StorageAccessId> {
        self.access_at(identity, &[])
    }

    /// Returns the checked type stored by one persistent storage origin.
    pub fn storage_type(&self, identity: StorageIdentityId) -> Option<TypeId> {
        self.identity_type(identity).or_else(|| {
            self.root_access(identity)
                .and_then(|access| self.access(access))
                .map(StorageAccess::reached_type)
        })
    }
}
