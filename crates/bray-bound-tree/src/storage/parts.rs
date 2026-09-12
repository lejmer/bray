use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{CallableInstanceData, ConstantTermId, TypeId};

use crate::{AsyncCleanupPhases, StorageProjection};

/// A selected unary storage-policy call with its substituted parameter and result types.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageProtocolCall {
    callable: CallableInstanceData,
    callable_type: TypeId,
    parameter: TypeId,
    result: TypeId,
}

impl StorageProtocolCall {
    /// Creates the checked call used to project or release owned storage.
    pub const fn new(
        callable: CallableInstanceData,
        callable_type: TypeId,
        parameter: TypeId,
        result: TypeId,
    ) -> Self {
        Self {
            callable,
            callable_type,
            parameter,
            result,
        }
    }

    /// Returns the selected implementation instance.
    pub const fn callable(self) -> CallableInstanceData {
        self.callable
    }

    /// Returns the selected signature used to verify the unary synchronous Bray call contract.
    pub const fn callable_type(self) -> TypeId {
        self.callable_type
    }

    /// Returns the substituted storage parameter type.
    pub const fn parameter(self) -> TypeId {
        self.parameter
    }

    /// Returns the substituted result type.
    pub const fn result(self) -> TypeId {
        self.result
    }
}

/// A represented component or the element family of a checked fixed array.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageCleanupProjectionKind {
    /// One statically identified represented component.
    Component(StorageProjection),
    /// A represented union payload member whose field identity is hidden from source lookup.
    UnionPayloadElement {
        /// The variant whose payload owns this member.
        variant: bray_symbols::UnionVariantSymbolId,
        /// The member's position in the complete represented payload.
        ordinal: bray_symbols::SymbolOrdinal,
    },
    /// Every element, visited in reverse order after the checked extent is specialized.
    ArrayElements(ConstantTermId),
    /// The target reached through the selected storage policy's mutable-borrow operation.
    OwnedTarget(StorageProtocolCall),
}

impl StorageCleanupProjectionKind {
    /// Returns whether an evaluated projection selects this cleanup component.
    pub fn contains(self, projection: StorageProjection) -> bool {
        match self {
            Self::Component(component) => component == projection,
            Self::UnionPayloadElement { .. } => false,
            Self::OwnedTarget(_) => projection == StorageProjection::OwnedTarget,
            Self::ArrayElements(_) => matches!(
                projection,
                StorageProjection::Element(_)
                    | StorageProjection::ElementFromStart(_)
                    | StorageProjection::ElementFromEnd(_)
            ),
        }
    }
}

impl From<StorageProjection> for StorageCleanupProjectionKind {
    fn from(projection: StorageProjection) -> Self {
        Self::Component(projection)
    }
}

/// One checked projection in a represented-part cleanup path.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageCleanupProjection {
    projection: StorageCleanupProjectionKind,
    source_type: TypeId,
    result_type: TypeId,
}

impl StorageCleanupProjection {
    /// Creates a projection with its independently checked input and result types.
    pub fn new(
        projection: impl Into<StorageCleanupProjectionKind>,
        source_type: TypeId,
        result_type: TypeId,
    ) -> Self {
        Self {
            projection: projection.into(),
            source_type,
            result_type,
        }
    }

    /// Returns the represented storage component.
    pub const fn projection(self) -> StorageCleanupProjectionKind {
        self.projection
    }

    /// Returns the type before this projection.
    pub const fn source_type(self) -> TypeId {
        self.source_type
    }

    /// Returns the type after this projection.
    pub const fn result_type(self) -> TypeId {
        self.result_type
    }

    pub(crate) fn is_valid_for(self, unit: crate::BoundUnitId) -> bool {
        match self.projection() {
            StorageCleanupProjectionKind::Component(component) => component.is_valid_for(unit),
            StorageCleanupProjectionKind::UnionPayloadElement { .. }
            | StorageCleanupProjectionKind::ArrayElements(_)
            | StorageCleanupProjectionKind::OwnedTarget(_) => true,
        }
    }
}

/// A complete represented subvalue whose cleanup is guarded independently of its siblings.
///
/// Paths are relative to the owning storage identity. Their order is the type-defined cleanup
/// order, including represented members that have no evaluated source access occurrence.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageCleanupPart {
    projections: Arc<[StorageCleanupProjection]>,
    phases: AsyncCleanupPhases,
    release: Option<StorageProtocolCall>,
}

impl StorageCleanupPart {
    /// Creates one checked leaf of represented-part cleanup.
    pub fn new(
        projections: impl IntoIterator<Item = StorageCleanupProjection>,
        phases: AsyncCleanupPhases,
    ) -> Self {
        Self {
            projections: shared_slice(projections),
            phases,
            release: None,
        }
    }

    /// Creates the final storage-policy release after the owner's surviving target parts.
    pub fn release_storage(
        projections: impl IntoIterator<Item = StorageCleanupProjection>,
        release: StorageProtocolCall,
    ) -> Self {
        Self {
            projections: shared_slice(projections),
            phases: AsyncCleanupPhases::Lifecycle,
            release: Some(release),
        }
    }

    /// Returns the selected release call when this leaf resolves the storage policy itself.
    pub const fn release(&self) -> Option<StorageProtocolCall> {
        self.release
    }

    /// Returns the checked path from the storage root to this complete subvalue.
    pub fn projections(&self) -> &[StorageCleanupProjection] {
        &self.projections
    }

    /// Returns the cleanup phases required by this subvalue.
    pub const fn phases(&self) -> AsyncCleanupPhases {
        self.phases
    }

    /// Returns whether moving one concrete storage path removes this entire cleanup part.
    /// Moving one array element does not remove its whole element family.
    pub fn is_fully_moved_by(&self, path: &[StorageProjection]) -> bool {
        path.len() <= self.projections.len()
            && path.iter().zip(self.projections.iter()).all(|(moved, part)| {
                match part.projection() {
                    StorageCleanupProjectionKind::Component(component) => *moved == component,
                    StorageCleanupProjectionKind::OwnedTarget(_) => {
                        *moved == StorageProjection::OwnedTarget
                    }
                    StorageCleanupProjectionKind::ArrayElements(_)
                    | StorageCleanupProjectionKind::UnionPayloadElement { .. } => false,
                }
            })
    }

    pub(crate) fn is_valid_for(&self, unit: crate::BoundUnitId) -> bool {
        self.projections
            .iter()
            .all(|projection| projection.is_valid_for(unit))
    }
}

#[cfg(test)]
mod tests {
    use super::{StorageCleanupPart, StorageCleanupProjection, StorageCleanupProjectionKind};
    use crate::{AsyncCleanupPhases, StorageProjection};
    use bray_symbols::{ConstantTermData, SymbolOrdinal};

    #[test]
    fn complete_part_moves_distinguish_ancestors_siblings_and_array_elements() {
        let store = crate::test_support::semantic_values();
        let ty = crate::test_support::error_type_in(&store);
        let first = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let second = StorageProjection::TupleElement(SymbolOrdinal::new(1));

        let part = StorageCleanupPart::new(
            [StorageCleanupProjection::new(first, ty, ty)],
            AsyncCleanupPhases::Lifecycle,
        );

        assert!(part.is_fully_moved_by(&[]));
        assert!(part.is_fully_moved_by(&[first]));
        assert!(!part.is_fully_moved_by(&[second]));
        assert!(!part.is_fully_moved_by(&[first, second]));

        let length = store
            .intern_constant_term(ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
            .unwrap();

        let array = StorageCleanupPart::new(
            [StorageCleanupProjection::new(
                StorageCleanupProjectionKind::ArrayElements(length),
                ty,
                ty,
            )],
            AsyncCleanupPhases::Lifecycle,
        );

        assert!(array.is_fully_moved_by(&[]));

        assert!(!array.is_fully_moved_by(&[StorageProjection::ElementFromStart(
            SymbolOrdinal::new(0),
        )]));
    }
}
