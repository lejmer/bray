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

    /// Whether the path stays within its inline value without reaching another owner or protected storage.
    pub fn is_inline(&self) -> bool {
        Self::inline_projections(&self.projections)
    }

    /// Returns whether projections remain within the same inline owner.
    pub fn inline_projections(projections: &[StorageProjection]) -> bool {
        projections.iter().all(|projection| {
            matches!(
                projection,
                StorageProjection::ProductField(_)
                    | StorageProjection::TupleElement(_)
                    | StorageProjection::Element(_)
                    | StorageProjection::ElementFromStart(_)
                    | StorageProjection::ElementFromEnd(_)
                    | StorageProjection::ActiveUnionPayloadField { .. }
                    | StorageProjection::NullableValue
            )
        })
    }

    /// Whether each selector identifies one component independently of evaluation.
    pub fn is_exact(&self) -> bool {
        self.projections
            .iter()
            .all(|projection| projection.is_exact())
    }
}

/// A checked reference access, retaining projections and the typed dereferences between them.
/// It identifies an access route, not a physical storage identity or an authority grant.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StorageReferencePath {
    projections: Arc<[StorageProjection]>,
    borrows: Arc<[(usize, bray_symbols::TypeId)]>,
}

impl StorageReferencePath {
    /// Retains a path prepared by type analysis. Each dereference index precedes that
    /// projection, or follows the final projection when equal to the projection count.
    /// Indices are ordered and may repeat for nested borrow layers.
    pub fn new(
        projections: Arc<[StorageProjection]>,
        borrows: Arc<[(usize, bray_symbols::TypeId)]>,
    ) -> Self {
        Self {
            projections,
            borrows,
        }
    }

    /// The checked component selections between reference dereferences.
    pub fn projections(&self) -> &[StorageProjection] {
        &self.projections
    }

    /// Ordered dereference positions and the type reached by each borrow layer.
    pub fn borrows(&self) -> &[(usize, bray_symbols::TypeId)] {
        &self.borrows
    }
}
