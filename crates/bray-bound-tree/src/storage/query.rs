use bray_symbols::TypeId;

use super::{StorageAccess, StorageAccessId, StorageIdentityId, StoragePlan};

impl StoragePlan {
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
