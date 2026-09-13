use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{
    StaticReferenceSelection, TypeId, UnionPayloadFieldSymbolId, UnionVariantSymbolId,
};

use crate::{MirFieldReference, MirOperand, MirSourceAnchor, MirStorageId};

/// Semantic role of one MIR storage allocation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirStorageKind {
    /// Caller-supplied parameter storage at its semantic ABI position.
    Parameter(u32),
    /// Caller-owned argument storage borrowed by a runtime-default provider.
    BorrowedParameter(u32),
    /// Source-correlated local storage.
    Local,
    /// Compiler-created temporary storage.
    Temporary,
    /// Storage for the unit's returned value.
    Return,
    /// Inactive protected-frame storage that may still be moved.
    InactiveFrame,
    /// Stable storage of the currently executing protected frame.
    CurrentFrame,
    /// Stable storage owned by the currently executing task.
    CurrentTask,
    /// Stable storage owned by one child task control record.
    ChildTask,
    /// One open or closed Bray-owned static instance selected by checked semantics.
    Static(StaticReferenceSelection),
    /// One provider-owned native static address selected by checked semantics.
    NativeStatic(StaticReferenceSelection),
}

/// One typed storage allocation owned by a MIR unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirStorage {
    source: MirSourceAnchor,
    kind: MirStorageKind,
    ty: TypeId,
}

impl MirStorage {
    pub(crate) const fn new(source: MirSourceAnchor, kind: MirStorageKind, ty: TypeId) -> Self {
        Self { source, kind, ty }
    }

    /// Returns the source construct associated with this storage.
    pub const fn source(&self) -> &MirSourceAnchor {
        &self.source
    }

    /// Returns the storage's semantic role.
    pub const fn kind(&self) -> &MirStorageKind {
        &self.kind
    }

    /// Returns the storage's checked type.
    pub const fn ty(&self) -> TypeId {
        self.ty
    }
}

/// One typed projection from a storage place.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirProjectionKind {
    /// Dereference a pointer or reference.
    Dereference,
    /// Select a declared field or payload field.
    Field(MirFieldReference),
    /// Select a tuple element by ordinal.
    TupleField(u32),
    /// Select a fixed element counted from the start.
    ElementFromStart(u32),
    /// Select a fixed element counted from the end.
    ElementFromEnd(u32),
    /// Select an element using a checked index value.
    Index(MirOperand),
    /// Select a range using optional checked bounds.
    Slice {
        /// Inclusive lower bound.
        start: Option<MirOperand>,
        /// Exclusive upper bound.
        end: Option<MirOperand>,
    },
    /// Select the payload of a checked union variant.
    Variant(UnionVariantSymbolId),
    /// Select a field from the checked active union payload.
    ActiveUnionPayloadField {
        /// The variant proven active for this projection.
        variant: UnionVariantSymbolId,
        /// The selected payload field.
        field: UnionPayloadFieldSymbolId,
    },
    /// Select a represented member from the checked active union payload without a visible field name.
    ActiveUnionPayloadElement {
        /// The variant proven active for this projection.
        variant: UnionVariantSymbolId,
        /// The member's position in the complete represented payload.
        ordinal: bray_symbols::SymbolOrdinal,
    },
    /// Select the present contents of nullable storage.
    NullableValue,
    /// Select the storage-policy value represented by owned indirection.
    OwnedStorage,
}

/// One projection step with its checked input and result types.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirProjection {
    kind: MirProjectionKind,
    source_type: TypeId,
    result_type: TypeId,
}

impl MirProjection {
    /// Creates a typed projection step.
    pub const fn new(kind: MirProjectionKind, source_type: TypeId, result_type: TypeId) -> Self {
        Self {
            kind,
            source_type,
            result_type,
        }
    }

    /// Returns the projection operation.
    pub const fn kind(&self) -> &MirProjectionKind {
        &self.kind
    }

    /// Returns the checked type before this projection.
    pub const fn source_type(&self) -> TypeId {
        self.source_type
    }

    /// Returns the checked type after this projection.
    pub const fn result_type(&self) -> TypeId {
        self.result_type
    }
}

/// A typed addressable storage location and its projections.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirPlace {
    storage: MirStorageId,
    projections: Arc<[MirProjection]>,
    ty: TypeId,
}

impl MirPlace {
    /// Creates a place from its root storage, source-order projections, and resulting type.
    pub fn new(
        storage: MirStorageId,
        projections: impl IntoIterator<Item = MirProjection>,
        ty: TypeId,
    ) -> Self {
        Self {
            storage,
            projections: shared_slice(projections),
            ty,
        }
    }

    /// Returns a new place extending this checked path by one typed projection.
    pub fn project(&self, kind: MirProjectionKind, ty: TypeId) -> Self {
        // A projected place owns its path independently of the retained parent place.
        let projections = self
            .projections
            .iter()
            .cloned()
            .chain([MirProjection::new(kind, self.ty, ty)]);

        Self::new(self.storage, projections, ty)
    }

    /// Returns the root storage allocation.
    pub const fn storage(&self) -> MirStorageId {
        self.storage
    }

    /// Returns projections in evaluation order.
    pub fn projections(&self) -> &[MirProjection] {
        &self.projections
    }

    /// Returns the checked type after applying every projection.
    pub const fn ty(&self) -> TypeId {
        self.ty
    }
}

#[cfg(test)]
mod tests {
    use super::{MirPlace, MirProjectionKind};
    use crate::{MirStorageId, MirUnitId};
    use bray_symbols::{SemanticValueStore, TypeData};

    #[test]
    fn extending_a_place_preserves_the_parent_and_each_projection_type() {
        let values = SemanticValueStore::try_new().unwrap();
        let leaf = values.intern_type(TypeData::tuple([])).unwrap();
        let inner = values.intern_type(TypeData::tuple([leaf])).unwrap();
        let outer = values.intern_type(TypeData::tuple([inner])).unwrap();
        let storage = MirStorageId::from_slot(MirUnitId::new(4), 2);

        let parent =
            MirPlace::new(storage, [], outer).project(MirProjectionKind::TupleField(0), inner);

        let child = parent.project(MirProjectionKind::TupleField(0), leaf);

        assert_eq!(parent.ty(), inner);
        assert_eq!(parent.projections().len(), 1);
        assert_eq!(child.storage(), storage);
        assert_eq!(child.ty(), leaf);
        assert_eq!(child.projections().len(), 2);
        assert_eq!(child.projections()[0], parent.projections()[0]);
        assert_eq!(child.projections()[1].source_type(), inner);
        assert_eq!(child.projections()[1].result_type(), leaf);
    }
}
