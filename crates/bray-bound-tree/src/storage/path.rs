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

    /// Whether this path contains the other slot without crossing an owned allocation boundary.
    pub fn contains_storage(&self, other: &Self) -> bool {
        self.contains(other)
            && StorageProjection::remains_in_allocation(&other.projections[self.projections.len()..])
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
                    | StorageProjection::AnyElement
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

/// The declared storage policy of one owning projection in a checked access route.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageReferenceOwner {
    /// Storage selected by an ordinary owning indirection.
    Indirection,
    /// Buffer backing reached through its declared pointer field.
    Buffer {
        /// The declared owning descriptor type.
        ty: bray_symbols::TypeId,
        /// The field containing the current allocation address.
        pointer: bray_symbols::StructFieldSymbolId,
        /// The declared allocation element type.
        element: bray_symbols::TypeId,
    },
}

/// A checked access route, including typed borrow and owned-storage loads.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageReferencePath {
    projections: Arc<[StorageProjection]>,
    borrows: Arc<[(usize, bray_symbols::TypeId)]>,
    owners: Arc<[(usize, StorageReferenceOwner)]>,
}

impl StorageReferencePath {
    pub(crate) fn is_valid_for(&self, unit: crate::BoundUnitId) -> bool {
        self.projections
            .iter()
            .all(|projection| projection.is_valid_for(unit))
    }

    /// Retains a path prepared by type analysis. Each dereference index precedes that
    /// projection, or follows the final projection when equal to the projection count.
    /// Indices are ordered and may repeat for nested borrow layers.
    pub fn new(
        projections: Arc<[StorageProjection]>,
        borrows: Arc<[(usize, bray_symbols::TypeId)]>,
        owners: Arc<[(usize, StorageReferenceOwner)]>,
    ) -> Self {
        assert!(projections.iter().enumerate()
            .filter_map(|(index, projection)| (*projection == StorageProjection::OwnedTarget).then_some(index))
            .eq(owners.iter().map(|(index, _)| *index)),
            "checked reference path retains every owned-storage load");

        Self {
            projections,
            borrows,
            owners,
        }
    }

    /// The checked component selections between reference dereferences.
    pub fn projections(&self) -> &[StorageProjection] {
        &self.projections
    }

    /// Ordered reference-load positions and the type reached by each load.
    pub fn borrows(&self) -> &[(usize, bray_symbols::TypeId)] {
        &self.borrows
    }

    /// The declared policy for each owning projection, prepared before flow analysis.
    pub fn owners(&self) -> &[(usize, StorageReferenceOwner)] {
        &self.owners
    }

    /// Retains the checked metadata before a selected reference load.
    pub fn prefix(&self, projections: usize, borrows: usize) -> Self {
        Self::new(self.projections[..projections].into(), self.borrows[..borrows].into(),
            self.owners.iter().copied().take_while(|(index, _)| *index < projections).collect())
    }

    /// Removes formal wrapping without moving any load into the caller's removed prefix.
    pub fn strip_prefix(&self, prefix: &[StorageProjection]) -> Option<Self> {
        Some(Self::new(self.projections.strip_prefix(prefix)?.into(),
            self.borrows.iter().map(|(index, ty)| Some((index.checked_sub(prefix.len())?, *ty))).collect::<Option<Vec<_>>>()?.into(),
            self.owners.iter().map(|(index, owner)| Some((index.checked_sub(prefix.len())?, *owner))).collect::<Option<Vec<_>>>()?.into()))
    }

    /// Appends checked terminal borrow loads while retaining the component route.
    pub fn with_terminal_borrows(&self, targets: &[bray_symbols::TypeId]) -> Self {
        if targets.is_empty() {
            return self.clone();
        }

        Self {
            projections: Arc::clone(&self.projections),
            borrows: self.borrows.iter().copied()
                .chain(targets.iter().map(|ty| (self.projections.len(), *ty)))
                .collect::<Vec<_>>().into(),
            owners: Arc::clone(&self.owners),
        }
    }

    /// Prepends the checked borrow loads performed before this value's access route.
    pub fn with_initial_borrows(&self, targets: &[bray_symbols::TypeId]) -> Self {
        Self {
            projections: Arc::clone(&self.projections),
            borrows: targets.iter().map(|ty| (0, *ty)).chain(self.borrows.iter().copied()).collect(),
            owners: Arc::clone(&self.owners),
        }
    }

    /// Whether this load follows an explicit raw address rather than a language borrow.
    pub fn is_address_load(&self, step: usize) -> bool {
        let (index, _) = self.borrows.get(step).expect("reference load belongs to its prepared route");

        self.projections.get(*index) == Some(&StorageProjection::AddressTarget)
            && self.borrows.get(step + 1).is_none_or(|(next, _)| next != index)
    }
}
