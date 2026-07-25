use std::sync::Arc;

use bray_symbols::{
    AnonymousCallableParameterSymbolId, BorrowKind, CallableParameterSymbolId,
    LocalBindingSymbolId, PostconditionResultSymbolId, PredicateParameterSymbolId,
    ReceiverParameterSymbolId,
};

use crate::{
    BoundExpressionId, BoundUnitId, BoundUnitKind, StorageAccess, StorageAccessId, StorageIdentity,
    StorageIdentityId, StorageProjection, StorageRelationship,
};

/// A semantic identity that names storage within one bound unit.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageBindingTarget {
    /// An ordinary callable parameter.
    Parameter(CallableParameterSymbolId),
    /// A callable receiver.
    Receiver(ReceiverParameterSymbolId),
    /// An anonymous callable parameter.
    AnonymousParameter(AnonymousCallableParameterSymbolId),
    /// A predicate parameter.
    PredicateParameter(PredicateParameterSymbolId),
    /// A pattern-introduced local binding.
    Local(LocalBindingSymbolId),
    /// A callable result visible to a postcondition.
    PostconditionResult(PostconditionResultSymbolId),
    /// The value produced by the bound unit.
    Result,
}

/// The persistent storage or evaluated access named by a semantic identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageBinding {
    /// A persistent storage origin.
    Identity(StorageIdentityId),
    /// An evaluated access projected from existing storage.
    Access(StorageAccessId),
}

impl StorageBinding {
    pub(super) const fn unit(self) -> BoundUnitId {
        match self {
            Self::Identity(storage) => storage.unit(),
            Self::Access(access) => access.unit(),
        }
    }
}

/// The source-semantic purpose of one evaluated storage access.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum StorageAccessPurpose {
    /// Observe the reached value.
    Read,
    /// Mutate the reached storage.
    Write,
    /// Transfer ownership out of the reached storage.
    Move,
    /// Transfer a value before its copy-or-move behavior has been resolved.
    ValueTransfer,
    /// Establish a borrow capability over the reached storage.
    Borrow(BorrowKind),
    /// Use the reached storage as an assignment destination.
    Assignment,
    /// Select a member substorage.
    Member,
    /// Select one indexed element.
    Index,
    /// Select a slice range.
    Slice,
    /// Apply another typed storage projection.
    Projection,
}

/// One expression occurrence and the exact storage access it evaluates.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageAccessPlan {
    expression: BoundExpressionId,
    purpose: StorageAccessPurpose,
    access: StorageAccessId,
}

impl StorageAccessPlan {
    pub(super) const fn new(
        expression: BoundExpressionId,
        purpose: StorageAccessPurpose,
        access: StorageAccessId,
    ) -> Self {
        Self {
            expression,
            purpose,
            access,
        }
    }

    /// Returns the expression occurrence that evaluates this access.
    pub const fn expression(self) -> BoundExpressionId {
        self.expression
    }

    /// Returns how the expression uses the reached storage.
    pub const fn purpose(self) -> StorageAccessPurpose {
        self.purpose
    }

    /// Returns the evaluated storage-access identity.
    pub const fn access(self) -> StorageAccessId {
        self.access
    }
}

/// Immutable storage identities and occurrence-specific access plans for one bound unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StoragePlan {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    identities: Arc<[StorageIdentity]>,
    accesses: Arc<[StorageAccess]>,
    bindings: Arc<[(StorageBindingTarget, StorageBinding)]>,
    plans: Arc<[StorageAccessPlan]>,
}

impl StoragePlan {
    pub(super) fn new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        identities: Vec<StorageIdentity>,
        accesses: Vec<StorageAccess>,
        bindings: Vec<(StorageBindingTarget, StorageBinding)>,
        plans: Vec<StorageAccessPlan>,
    ) -> Self {
        Self {
            unit,
            kind,
            identities: identities.into(),
            accesses: accesses.into(),
            bindings: bindings.into(),
            plans: plans.into(),
        }
    }

    /// Returns the bound unit described by this plan.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the semantic category of the bound unit.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns storage origins in deterministic allocation order.
    pub fn identities(&self) -> &[StorageIdentity] {
        &self.identities
    }

    /// Returns storage origins with their unit-local identities.
    pub fn identity_entries(
        &self,
    ) -> impl Iterator<Item = (StorageIdentityId, StorageIdentity)> + '_ {
        self.identities
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, identity)| {
                let slot = u32::try_from(index).ok()?;
                let id = StorageIdentityId::from_storage_slot(self.unit, slot);

                Some((id, identity))
            })
    }

    /// Returns evaluated accesses in deterministic evaluation order.
    pub fn accesses(&self) -> &[StorageAccess] {
        &self.accesses
    }

    /// Returns semantic identity relationships in canonical target order.
    pub fn bindings(&self) -> &[(StorageBindingTarget, StorageBinding)] {
        &self.bindings
    }

    /// Returns occurrence-specific access plans in deterministic evaluation order.
    pub fn access_plans(&self) -> &[StorageAccessPlan] {
        &self.plans
    }

    /// Returns one persistent storage identity.
    pub fn identity(&self, id: StorageIdentityId) -> Option<StorageIdentity> {
        self.entry(id.unit(), id.storage_index(), &self.identities)
            .copied()
    }

    /// Returns one evaluated storage access.
    pub fn access(&self, id: StorageAccessId) -> Option<&StorageAccess> {
        self.entry(id.unit(), id.storage_index(), &self.accesses)
    }

    /// Returns the storage or access named by a semantic identity.
    pub fn binding(&self, target: StorageBindingTarget) -> Option<StorageBinding> {
        self.bindings
            .binary_search_by_key(&target, |(target, _)| *target)
            .ok()
            .map(|index| self.bindings[index].1)
    }

    /// Returns every planned use of one expression occurrence.
    pub fn expression_plans(
        &self,
        expression: BoundExpressionId,
    ) -> impl Iterator<Item = StorageAccessPlan> + '_ {
        self.plans
            .iter()
            .copied()
            .filter(move |plan| plan.expression() == expression)
    }

    /// Returns the proven overlap relationship between two evaluated accesses.
    pub fn relationship(
        &self,
        left: StorageAccessId,
        right: StorageAccessId,
    ) -> StorageRelationship {
        let (Some(left), Some(right)) = (self.access(left), self.access(right)) else {
            return StorageRelationship::Error;
        };

        if left.is_recovered() || right.is_recovered() {
            return StorageRelationship::Error;
        }

        let (Some(left_root), Some(right_root)) =
            (storage_root(left.root()), storage_root(right.root()))
        else {
            return StorageRelationship::PotentiallyOverlapping;
        };

        if left_root != right_root {
            return StorageRelationship::Disjoint;
        }

        projection_relationship(left.projections(), right.projections())
    }

    fn entry<'plan, T>(
        &self,
        unit: BoundUnitId,
        index: Option<usize>,
        entries: &'plan [T],
    ) -> Option<&'plan T> {
        if unit != self.unit {
            return None;
        }

        index.and_then(|index| entries.get(index))
    }
}

fn storage_root(root: crate::StorageAccessRoot) -> Option<StorageIdentityId> {
    match root {
        crate::StorageAccessRoot::Storage(storage)
        | crate::StorageAccessRoot::OwnedIndirection { storage, .. } => Some(storage),
        crate::StorageAccessRoot::Borrow(_) | crate::StorageAccessRoot::Recovery(_) => None,
    }
}

fn projection_relationship(
    left: &[StorageProjection],
    right: &[StorageProjection],
) -> StorageRelationship {
    for (left, right) in left.iter().zip(right) {
        if left == right {
            continue;
        }

        return if projections_are_disjoint(*left, *right) {
            StorageRelationship::Disjoint
        } else {
            StorageRelationship::PotentiallyOverlapping
        };
    }

    if left == right {
        StorageRelationship::Identical
    } else {
        StorageRelationship::PotentiallyOverlapping
    }
}

fn projections_are_disjoint(left: StorageProjection, right: StorageProjection) -> bool {
    match (left, right) {
        (StorageProjection::ProductField(left), StorageProjection::ProductField(right)) => {
            left != right
        }
        (StorageProjection::TupleElement(left), StorageProjection::TupleElement(right))
        | (StorageProjection::ElementFromStart(left), StorageProjection::ElementFromStart(right))
        | (StorageProjection::ElementFromEnd(left), StorageProjection::ElementFromEnd(right)) => {
            left != right
        }
        (
            StorageProjection::ActiveUnionPayloadField {
                variant: left_variant,
                field: left_field,
            },
            StorageProjection::ActiveUnionPayloadField {
                variant: right_variant,
                field: right_field,
            },
        ) => left_variant != right_variant || left_field != right_field,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use bray_symbols::SymbolOrdinal;

    use super::StoragePlan;
    use crate::{
        BoundExpressionId, BoundUnitId, BoundUnitKind, StorageAccess, StorageAccessId,
        StorageAccessRoot, StorageIdentity, StorageIdentityId, StorageProjection,
        StorageRelationship,
    };

    #[test]
    fn storage_plans_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<StoragePlan>();
    }

    #[test]
    fn storage_relationships_distinguish_disjoint_and_overlapping_substorage() {
        let unit = BoundUnitId::new(4);
        let root = StorageIdentityId::from_slot(unit, 0);
        let source = crate::test_support::source_anchor();
        let ty = crate::test_support::error_type();

        let first = StorageAccess::new(
            StorageAccessRoot::Storage(root),
            [StorageProjection::TupleElement(SymbolOrdinal::new(0))],
            ty,
            source,
            false,
        );

        let second = StorageAccess::new(
            StorageAccessRoot::Storage(root),
            [StorageProjection::TupleElement(SymbolOrdinal::new(1))],
            ty,
            source,
            false,
        );

        let nested = StorageAccess::new(
            StorageAccessRoot::Storage(root),
            [
                StorageProjection::TupleElement(SymbolOrdinal::new(0)),
                StorageProjection::Element(BoundExpressionId::from_slot(unit, 0)),
            ],
            ty,
            source,
            false,
        );

        let plan = StoragePlan::new(
            unit,
            BoundUnitKind::CallableBody,
            [StorageIdentity::Temporary(BoundExpressionId::from_slot(
                unit, 1,
            ))]
            .into(),
            vec![first, second, nested],
            Vec::new(),
            Vec::new(),
        );

        let first = StorageAccessId::from_slot(unit, 0);
        let second = StorageAccessId::from_slot(unit, 1);
        let nested = StorageAccessId::from_slot(unit, 2);

        assert_eq!(
            plan.relationship(first, second),
            StorageRelationship::Disjoint
        );

        assert_eq!(
            plan.relationship(first, nested),
            StorageRelationship::PotentiallyOverlapping
        );

        assert_eq!(
            plan.relationship(first, StorageAccessId::from_slot(BoundUnitId::new(9), 0)),
            StorageRelationship::Error
        );
    }
}
