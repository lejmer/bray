use crate::{
    StorageAccessId, StorageIdentityId, StoragePlan, StorageProjection, StorageRelationship,
};
use std::sync::Arc;

/// A storage slot, distinct from the target of a reference held in that slot.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StoragePath {
    /// The storage instance identified by this path.
    pub root: StorageIdentityId,
    /// The component path within that storage instance.
    pub projections: Arc<[StorageProjection]>,
}

impl StoragePath {
    pub(crate) fn is_valid_for(&self, unit: crate::BoundUnitId) -> bool {
        self.root.unit() == unit
            && self
                .projections
                .iter()
                .all(|projection| projection.is_valid_for(unit))
    }

    /// Whether this logical path is a prefix of the selected subvalue path.
    /// Definite initialization additionally requires an exact selector.
    pub fn contains(&self, other: &Self) -> bool {
        self.root == other.root && other.projections.starts_with(&self.projections)
    }

    /// Compares value slots without following references held in those slots.
    pub fn relationship(&self, other: &Self) -> StorageRelationship {
        if self.root != other.root {
            return StorageRelationship::Disjoint;
        }

        StorageProjection::relationship(&self.projections, &other.projections)
    }

    /// Identifies the whole storage instance.
    pub fn root(root: StorageIdentityId) -> Self {
        Self {
            root,
            projections: Arc::from([]),
        }
    }

    /// Finds a source access with this path, when that path was explicitly evaluated.
    pub fn access_id(&self, storage: &StoragePlan) -> Option<StorageAccessId> {
        storage.access_at(self.root, &self.projections)
    }

    /// Whether each selector identifies one component independently of evaluation.
    pub fn is_exact(&self) -> bool {
        self.projections
            .iter()
            .all(|projection| projection.is_exact())
    }
}
