use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{TypeId, UnionVariantSymbolId};

use crate::{MirFieldReference, MirSourceAnchor, MirStorageId, MirValueId};

/// Semantic role of one MIR storage allocation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirStorageKind {
    /// Caller-supplied parameter storage.
    Parameter,
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
    pub const fn kind(&self) -> MirStorageKind {
        self.kind
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
    /// Select an element using a checked index value.
    Index(MirValueId),
    /// Select a range using optional checked bounds.
    Slice {
        /// Inclusive lower bound.
        start: Option<MirValueId>,
        /// Exclusive upper bound.
        end: Option<MirValueId>,
    },
    /// Select the payload of a checked union variant.
    Variant(UnionVariantSymbolId),
}

/// One projection step and its checked resulting type.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirProjection {
    kind: MirProjectionKind,
    result_type: TypeId,
}

impl MirProjection {
    /// Creates a projection from its operation and checked result type.
    pub const fn new(kind: MirProjectionKind, result_type: TypeId) -> Self {
        Self { kind, result_type }
    }

    /// Returns the projection operation.
    pub const fn kind(&self) -> &MirProjectionKind {
        &self.kind
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
