use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{
    AnonymousCallableParameterSymbolId, BorrowKind, CallableParameterSymbolId,
    ReceiverParameterSymbolId, StructFieldSymbolId, SymbolOrdinal, TypeId,
    UnionPayloadFieldSymbolId, UnionVariantSymbolId,
};

use crate::identity::define_unit_scoped_id;
use crate::{
    AnyBoundNodeId, BoundDependencyContractId, BoundExpressionId, BoundNodeOrigin,
    BoundSourceAnchor, BoundUnitId,
};

define_unit_scoped_id!(
    StorageIdentityId,
    "Identifies one exact or symbolic storage origin in a checked semantic unit."
);

macro_rules! impl_storage_id {
    ($id:ident) => {
        impl $id {
            pub(super) const fn from_storage_slot(unit: BoundUnitId, slot: u32) -> Self {
                Self { unit, slot }
            }

            pub(super) fn storage_index(self) -> Option<usize> {
                usize::try_from(self.slot).ok()
            }
        }
    };
}

impl_storage_id!(StorageIdentityId);
impl_storage_id!(StorageAccessId);
impl_storage_id!(BorrowCapabilityId);
define_unit_scoped_id!(
    StorageAccessId,
    "Identifies one evaluated storage-access occurrence in a checked semantic unit."
);
define_unit_scoped_id!(
    BorrowCapabilityId,
    "Identifies one semantic borrow capability created in a checked semantic unit."
);

/// One exact or symbolic storage origin and its semantic provenance.
///
/// A storage identity is deliberately distinct from the symbol or bound node that introduced it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageIdentity {
    /// Owned storage introduced by a local binding or pattern.
    LocalOwned(AnyBoundNodeId),
    /// Storage supplied through an owned or borrowed callable parameter.
    Parameter(CallableParameterSymbolId),
    /// Storage supplied through a callable receiver.
    Receiver(ReceiverParameterSymbolId),
    /// Storage supplied through an anonymous callable parameter.
    AnonymousParameter(AnonymousCallableParameterSymbolId),
    /// Storage identified by a compilation-wide storage-bearing declaration.
    Surface(super::SurfaceStorageSymbol),
    /// Source-correlated temporary storage.
    Temporary(BoundExpressionId),
    /// Storage created by an allocation operation.
    Allocation(BoundExpressionId),
    /// Storage introduced by a compiler-required operation.
    CompilerCreated(BoundNodeOrigin),
    /// Conservative storage used while recovering from an earlier error.
    Error(BoundSourceAnchor),
}

impl StorageIdentity {
    pub(super) fn is_valid_for(self, unit: BoundUnitId) -> bool {
        match self {
            Self::LocalOwned(node) => node.unit() == unit,
            Self::Temporary(expression) | Self::Allocation(expression) => expression.unit() == unit,
            Self::Parameter(_)
            | Self::Receiver(_)
            | Self::Surface(_)
            | Self::CompilerCreated(_)
            | Self::Error(_) => true,
            Self::AnonymousParameter(parameter) => parameter.region().raw() == unit.raw(),
        }
    }
}

/// The root from which one storage-access occurrence is evaluated.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageAccessRoot {
    /// Direct access to a unit-local storage origin.
    Storage(StorageIdentityId),
    /// Access derived through an active borrow capability.
    Borrow(BorrowCapabilityId),
    /// Access to storage owned through a value's indirection layer.
    OwnedIndirection {
        /// The evaluated owner value occurrence.
        owner: BoundExpressionId,
        /// The persistent storage identity owned by that value.
        storage: StorageIdentityId,
    },
    /// Conservative access through recovery storage.
    Recovery(StorageIdentityId),
}

impl StorageAccessRoot {
    pub(super) fn is_valid_for(self, unit: BoundUnitId) -> bool {
        match self {
            Self::Storage(storage) | Self::Recovery(storage) => storage.unit() == unit,
            Self::Borrow(capability) => capability.unit() == unit,
            Self::OwnedIndirection { owner, storage } => {
                owner.unit() == unit && storage.unit() == unit
            }
        }
    }
}

/// One checked component of an evaluated storage access path.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageProjection {
    /// A named product field.
    ProductField(StructFieldSymbolId),
    /// A tuple element by stable ordinal.
    TupleElement(SymbolOrdinal),
    /// A field of the checked active union payload.
    ActiveUnionPayloadField {
        /// The variant proven active for this projection.
        variant: UnionVariantSymbolId,
        /// The selected payload field.
        field: UnionPayloadFieldSymbolId,
    },
    /// An array or slice element selected by a checked expression occurrence.
    Element(BoundExpressionId),
    /// A slice range selected by checked optional bound expressions.
    SliceRange {
        /// The inclusive lower-bound expression, when supplied.
        start: Option<BoundExpressionId>,
        /// The exclusive upper-bound expression, when supplied.
        end: Option<BoundExpressionId>,
    },
    /// The present contents of a nullable storage value.
    NullableValue,
    /// The storage owned through an indirection layer.
    OwnedTarget,
}

impl StorageProjection {
    fn is_valid_for(self, unit: BoundUnitId) -> bool {
        match self {
            Self::Element(selector) => selector.unit() == unit,
            Self::SliceRange { start, end } => {
                optional_expression_is_valid_for(start, unit)
                    && optional_expression_is_valid_for(end, unit)
            }
            Self::ProductField(_)
            | Self::TupleElement(_)
            | Self::ActiveUnionPayloadField { .. }
            | Self::NullableValue
            | Self::OwnedTarget => true,
        }
    }
}

fn optional_expression_is_valid_for(
    expression: Option<BoundExpressionId>,
    unit: BoundUnitId,
) -> bool {
    match expression {
        Some(expression) => expression.unit() == unit,
        None => true,
    }
}

/// One evaluated occurrence of a storage access path.
///
/// Repeated evaluation of an identical path creates distinct [`StorageAccessId`] values because
/// selector values can change between occurrences.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageAccess {
    root: StorageAccessRoot,
    projections: Arc<[StorageProjection]>,
    reached_type: TypeId,
    source: BoundSourceAnchor,
    is_recovered: bool,
}

impl StorageAccess {
    /// Creates one evaluated storage access occurrence.
    pub fn new(
        root: StorageAccessRoot,
        projections: impl IntoIterator<Item = StorageProjection>,
        reached_type: TypeId,
        source: BoundSourceAnchor,
        is_recovered: bool,
    ) -> Self {
        Self {
            root,
            projections: shared_slice(projections),
            reached_type,
            source,
            is_recovered,
        }
    }

    /// Returns the evaluated root of this access occurrence.
    pub const fn root(&self) -> StorageAccessRoot {
        self.root
    }

    /// Returns the ordered checked projection path.
    pub fn projections(&self) -> &[StorageProjection] {
        &self.projections
    }

    /// Returns the type reached after every projection.
    pub const fn reached_type(&self) -> TypeId {
        self.reached_type
    }

    /// Returns the source construct that evaluated this access.
    pub const fn source(&self) -> BoundSourceAnchor {
        self.source
    }

    /// Returns whether checking recovered this access after an earlier error.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }

    pub(super) fn is_valid_for(&self, unit: BoundUnitId) -> bool {
        self.root.is_valid_for(unit)
            && self
                .projections
                .iter()
                .all(|projection| projection.is_valid_for(unit))
    }
}

/// The immutable semantic origin of one borrow capability.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BorrowCapability {
    kind: BorrowKind,
    access: StorageAccessId,
    source: BoundSourceAnchor,
    dependency_contract: BoundDependencyContractId,
    derived_from: Option<BorrowCapabilityId>,
}

impl BorrowCapability {
    /// Creates a borrow capability established over one evaluated storage access.
    pub const fn new(
        kind: BorrowKind,
        access: StorageAccessId,
        source: BoundSourceAnchor,
        dependency_contract: BoundDependencyContractId,
        derived_from: Option<BorrowCapabilityId>,
    ) -> Self {
        Self {
            kind,
            access,
            source,
            dependency_contract,
            derived_from,
        }
    }

    /// Returns whether this capability permits shared observation or exclusive mutation.
    pub const fn kind(self) -> BorrowKind {
        self.kind
    }

    /// Returns the evaluated storage access reached by the borrow operation.
    pub const fn access(self) -> StorageAccessId {
        self.access
    }

    /// Returns the source construct that established this capability.
    pub const fn source(self) -> BoundSourceAnchor {
        self.source
    }

    /// Returns the dependency contract carried by this capability.
    pub const fn dependency_contract(self) -> BoundDependencyContractId {
        self.dependency_contract
    }

    /// Returns the parent capability when this capability is a reborrow.
    pub const fn derived_from(self) -> Option<BorrowCapabilityId> {
        self.derived_from
    }

    pub(super) fn is_valid_for(self, unit: BoundUnitId) -> bool {
        self.access.unit() == unit
            && self.dependency_contract.unit() == unit
            && match self.derived_from {
                Some(capability) => capability.unit() == unit,
                None => true,
            }
    }
}

/// A checker-proven relationship between two evaluated storage accesses.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageRelationship {
    /// Both accesses are proven to reach the same storage or substorage.
    Identical,
    /// The accesses are proven not to overlap.
    Disjoint,
    /// The accesses can overlap because disjointness was not proven.
    PotentiallyOverlapping,
    /// Recovery prevented a meaningful relationship proof.
    Error,
}

impl BoundUnitId {
    /// Returns a storage identity only when it belongs to this unit.
    pub fn checked_storage_identity(self, id: StorageIdentityId) -> Option<StorageIdentityId> {
        (id.unit() == self).then_some(id)
    }

    /// Returns a storage-access occurrence only when it belongs to this unit.
    pub fn checked_storage_access(self, id: StorageAccessId) -> Option<StorageAccessId> {
        (id.unit() == self).then_some(id)
    }

    /// Returns a borrow capability only when it belongs to this unit.
    pub fn checked_borrow_capability(self, id: BorrowCapabilityId) -> Option<BorrowCapabilityId> {
        (id.unit() == self).then_some(id)
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::BorrowKind;

    use super::{
        BorrowCapability, BorrowCapabilityId, StorageAccess, StorageAccessId, StorageAccessRoot,
        StorageIdentity, StorageIdentityId, StorageProjection, StorageRelationship,
    };
    use crate::test_support::{error_type, source_anchor};
    use crate::{BoundDependencyContractId, BoundExpressionId, BoundUnitId};

    #[test]
    fn storage_identity_is_distinct_from_provenance_and_access_occurrences() {
        let unit = BoundUnitId::new(4);
        let expression = BoundExpressionId::from_slot(unit, 2);
        let identity = StorageIdentityId::from_slot(unit, 2);

        let first_access = StorageAccessId::from_slot(unit, 2);
        let second_access = StorageAccessId::from_slot(unit, 3);

        assert!(StorageIdentity::Temporary(expression).is_valid_for(unit));
        assert_ne!(first_access, second_access);
        assert_eq!(identity.unit(), expression.unit());
    }

    #[test]
    fn storage_access_retains_dynamic_selector_occurrence() {
        let unit = BoundUnitId::new(1);
        let selector = BoundExpressionId::from_slot(unit, 8);
        let root = StorageAccessRoot::Storage(StorageIdentityId::from_slot(unit, 0));

        let ty = error_type();
        let source = source_anchor();

        let access = StorageAccess::new(
            root,
            [StorageProjection::Element(selector)],
            ty,
            source,
            false,
        );

        assert_eq!(access.root(), root);

        assert_eq!(
            access.projections(),
            &[StorageProjection::Element(selector)]
        );

        assert_eq!(access.reached_type(), ty);
        assert_eq!(access.source(), source);

        assert!(!access.is_recovered());
        assert!(access.is_valid_for(unit));
        assert!(!access.is_valid_for(BoundUnitId::new(2)));
    }

    #[test]
    fn borrow_identity_is_distinct_from_activity_and_reborrow_origin() {
        let unit = BoundUnitId::new(5);
        let parent = BorrowCapabilityId::from_slot(unit, 0);
        let child_id = BorrowCapabilityId::from_slot(unit, 1);
        let contract = BoundDependencyContractId::from_slot(unit, 0);

        let child = BorrowCapability::new(
            BorrowKind::Mutable,
            StorageAccessId::from_slot(unit, 3),
            source_anchor(),
            contract,
            Some(parent),
        );

        assert_ne!(parent, child_id);
        assert_eq!(child.derived_from(), Some(parent));
        assert_eq!(child.kind(), BorrowKind::Mutable);
        assert!(child.is_valid_for(unit));
    }

    #[test]
    fn relationship_requires_an_explicit_conservative_outcome() {
        let outcomes = [
            StorageRelationship::Identical,
            StorageRelationship::Disjoint,
            StorageRelationship::PotentiallyOverlapping,
            StorageRelationship::Error,
        ];

        assert_eq!(outcomes.len(), 4);

        assert_ne!(
            StorageRelationship::Disjoint,
            StorageRelationship::PotentiallyOverlapping
        );
    }

    #[test]
    fn checked_accessors_reject_foreign_unit_ids() {
        let unit = BoundUnitId::new(12);
        let local = StorageAccessId::from_slot(unit, 0);
        let foreign = StorageAccessId::from_slot(BoundUnitId::new(13), 0);

        assert_eq!(unit.checked_storage_access(local), Some(local));
        assert_eq!(unit.checked_storage_access(foreign), None);
    }
}
