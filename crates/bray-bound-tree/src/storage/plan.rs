use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{
    AnonymousCallableParameterSymbolId, BorrowKind, CallableParameterSymbolId,
    LocalBindingSymbolId, PostconditionResultSymbolId, PredicateParameterSymbolId,
    ReceiverParameterSymbolId,
};

use crate::{
    BorrowCapabilityId, BoundExpressionId, BoundSourceAnchor, BoundUnitId, BoundUnitKind,
    StorageAccess, StorageAccessId, StorageIdentity, StorageIdentityId, StorageProjection,
    StorageRelationship,
};

/// The semantic construct that establishes a borrow capability.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BorrowCapabilityOrigin {
    /// A borrow or reborrow expression evaluated inside the unit.
    Expression(BoundExpressionId),
    /// A borrow supplied by the caller when the unit begins.
    Entry(StorageBindingTarget),
}

/// One borrow capability established by an evaluated storage access.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PlannedBorrowCapability {
    origin: BorrowCapabilityOrigin,
    kind: BorrowKind,
    access: StorageAccessId,
    parent: Option<BorrowCapabilityId>,
    source: BoundSourceAnchor,
    is_recovered: bool,
}

impl PlannedBorrowCapability {
    /// Creates one planned borrow or reborrow capability.
    pub const fn new(
        origin: BorrowCapabilityOrigin,
        kind: BorrowKind,
        access: StorageAccessId,
        parent: Option<BorrowCapabilityId>,
        source: BoundSourceAnchor,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            kind,
            access,
            parent,
            source,
            is_recovered,
        }
    }

    /// Returns the semantic construct that establishes the capability.
    pub const fn origin(self) -> BorrowCapabilityOrigin {
        self.origin
    }

    /// Returns the expression that establishes the capability, when evaluated in the unit.
    pub const fn expression(self) -> Option<BoundExpressionId> {
        match self.origin {
            BorrowCapabilityOrigin::Expression(expression) => Some(expression),
            BorrowCapabilityOrigin::Entry(_) => None,
        }
    }

    /// Returns the entry binding that supplies the capability, when any.
    pub const fn entry_binding(self) -> Option<StorageBindingTarget> {
        match self.origin {
            BorrowCapabilityOrigin::Expression(_) => None,
            BorrowCapabilityOrigin::Entry(target) => Some(target),
        }
    }

    /// Returns the shared or mutable borrow category.
    pub const fn kind(self) -> BorrowKind {
        self.kind
    }

    /// Returns the storage access from which the capability is derived.
    pub const fn access(self) -> StorageAccessId {
        self.access
    }

    /// Returns the parent capability when this is a reborrow.
    pub const fn parent(self) -> Option<BorrowCapabilityId> {
        self.parent
    }

    /// Returns the construct that establishes the capability.
    pub const fn source(self) -> BoundSourceAnchor {
        self.source
    }

    /// Returns whether recovery affected this capability.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
    }
}

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
    /// Establish an initialized value in newly produced storage.
    Initialize,
    /// Evaluate a destination that requires mutation authority.
    Write,
    /// Transfer ownership out of the reached storage.
    Move,
    /// Copy the reached value without transferring ownership.
    Copy,
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
    resolved_accesses: Arc<[Option<ResolvedStorageAccess>]>,
    borrow_capabilities: Arc<[PlannedBorrowCapability]>,
    bindings: Arc<[(StorageBindingTarget, StorageBinding)]>,
    plans: Arc<[StorageAccessPlan]>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedStorageAccess {
    root: StorageIdentityId,
    projections: Arc<[StorageProjection]>,
}

impl StoragePlan {
    pub(super) fn new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        identities: Vec<StorageIdentity>,
        accesses: Vec<StorageAccess>,
        borrow_capabilities: Vec<PlannedBorrowCapability>,
        bindings: Vec<(StorageBindingTarget, StorageBinding)>,
        plans: Vec<StorageAccessPlan>,
    ) -> Self {
        let resolved_accesses = resolve_accesses(unit, &accesses, &borrow_capabilities);

        Self {
            unit,
            kind,
            identities: identities.into(),
            accesses: accesses.into(),
            resolved_accesses: resolved_accesses.into(),
            borrow_capabilities: borrow_capabilities.into(),
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

    /// Returns planned borrow capabilities in deterministic allocation order.
    pub fn borrow_capabilities(&self) -> &[PlannedBorrowCapability] {
        &self.borrow_capabilities
    }

    /// Returns planned borrow capabilities with their unit-local identities.
    pub fn borrow_capability_entries(
        &self,
    ) -> impl Iterator<Item = (BorrowCapabilityId, PlannedBorrowCapability)> + '_ {
        self.borrow_capabilities
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, capability)| {
                let slot = u32::try_from(index).ok()?;
                let id = BorrowCapabilityId::from_storage_slot(self.unit, slot);

                Some((id, capability))
            })
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

    /// Returns one planned borrow capability.
    pub fn borrow_capability(&self, id: BorrowCapabilityId) -> Option<PlannedBorrowCapability> {
        self.entry(id.unit(), id.storage_index(), &self.borrow_capabilities)
            .copied()
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
        let (Some(left), Some(right)) = (self.resolved_access(left), self.resolved_access(right))
        else {
            return StorageRelationship::Error;
        };

        if left.root != right.root {
            return if self.roots_are_proven_disjoint(left.root, right.root) {
                StorageRelationship::Disjoint
            } else {
                StorageRelationship::PotentiallyOverlapping
            };
        }

        projection_relationship(&left.projections, &right.projections)
    }

    /// Returns the persistent storage root reached by one access.
    pub fn root_identity(&self, access: StorageAccessId) -> Option<StorageIdentityId> {
        self.resolved_access(access).map(|access| access.root)
    }

    /// Returns whether the first access contains the complete second access.
    pub fn access_contains(&self, container: StorageAccessId, contained: StorageAccessId) -> bool {
        let (Some(container), Some(contained)) = (
            self.resolved_access(container),
            self.resolved_access(contained),
        ) else {
            return false;
        };

        container.root == contained.root
            && container.projections.len() <= contained.projections.len()
            && container
                .projections
                .iter()
                .zip(contained.projections.iter())
                .all(|(container, contained)| container == contained)
    }

    /// Returns whether an access names its complete persistent storage root.
    pub fn is_root_access(&self, access: StorageAccessId) -> bool {
        self.resolved_access(access)
            .is_some_and(|access| access.projections.is_empty())
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

    fn resolved_access(&self, access: StorageAccessId) -> Option<&ResolvedStorageAccess> {
        self.entry(
            access.unit(),
            access.storage_index(),
            &self.resolved_accesses,
        )?
        .as_ref()
    }

    fn roots_are_proven_disjoint(&self, left: StorageIdentityId, right: StorageIdentityId) -> bool {
        let (Some(left), Some(right)) = (self.identity(left), self.identity(right)) else {
            return false;
        };

        identity_is_distinct_storage(left) || identity_is_distinct_storage(right)
    }
}

fn resolve_accesses(
    unit: BoundUnitId,
    accesses: &[StorageAccess],
    capabilities: &[PlannedBorrowCapability],
) -> Vec<Option<ResolvedStorageAccess>> {
    let mut resolved = Vec::with_capacity(accesses.len());

    for access in accesses {
        if access.is_recovered() {
            resolved.push(None);

            continue;
        }

        let current = match access.root() {
            crate::StorageAccessRoot::Storage(root)
            | crate::StorageAccessRoot::OwnedIndirection { storage: root, .. } => {
                Some(ResolvedStorageAccess {
                    root,
                    projections: shared_slice(access.projections().iter().copied()),
                })
            }
            crate::StorageAccessRoot::Recovery(_) => None,
            crate::StorageAccessRoot::Borrow(capability) => {
                resolve_borrowed_access(unit, access, capability, capabilities, &resolved)
            }
        };

        resolved.push(current);
    }

    resolved
}

fn resolve_borrowed_access(
    unit: BoundUnitId,
    access: &StorageAccess,
    capability: BorrowCapabilityId,
    capabilities: &[PlannedBorrowCapability],
    resolved: &[Option<ResolvedStorageAccess>],
) -> Option<ResolvedStorageAccess> {
    if capability.unit() != unit {
        return None;
    }

    let capability = capabilities.get(capability.storage_index()?)?;

    if capability.is_recovered() || capability.access().unit() != unit {
        return None;
    }

    let inherited = resolved
        .get(capability.access().storage_index()?)?
        .as_ref()?;

    let projections = inherited
        .projections
        .iter()
        .chain(access.projections())
        .copied();

    Some(ResolvedStorageAccess {
        root: inherited.root,
        projections: shared_slice(projections),
    })
}

const fn identity_is_distinct_storage(identity: StorageIdentity) -> bool {
    matches!(
        identity,
        StorageIdentity::LocalOwned(_)
            | StorageIdentity::Result(_)
            | StorageIdentity::Temporary(_)
            | StorageIdentity::Allocation(_)
            | StorageIdentity::CompilerCreated(_)
    )
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

        assert!(plan.access_contains(first, nested));
        assert!(!plan.access_contains(nested, first));
        assert!(!plan.access_contains(first, second));

        assert_eq!(
            plan.relationship(first, StorageAccessId::from_slot(BoundUnitId::new(9), 0)),
            StorageRelationship::Error
        );
    }

    #[test]
    fn unique_storage_is_disjoint_from_symbolic_storage() {
        let unit = BoundUnitId::new(5);
        let source = crate::test_support::source_anchor();
        let ty = crate::test_support::error_type();

        let parameter = StorageAccess::new(
            StorageAccessRoot::Storage(StorageIdentityId::from_slot(unit, 0)),
            [],
            ty,
            source,
            false,
        );

        let result = StorageAccess::new(
            StorageAccessRoot::Storage(StorageIdentityId::from_slot(unit, 1)),
            [],
            ty,
            source,
            false,
        );

        let plan = StoragePlan::new(
            unit,
            BoundUnitKind::CallableBody,
            vec![
                StorageIdentity::Parameter(
                    bray_symbols::CallableParameterSymbolId::from_symbol_id(
                        bray_symbols::SymbolId::new(1),
                    ),
                ),
                StorageIdentity::Result(crate::AnyBoundNodeId::Expression(
                    BoundExpressionId::from_slot(unit, 0),
                )),
            ],
            vec![parameter, result],
            Vec::new(),
            Vec::new(),
            Vec::new(),
        );

        assert_eq!(
            plan.relationship(
                StorageAccessId::from_slot(unit, 0),
                StorageAccessId::from_slot(unit, 1)
            ),
            StorageRelationship::Disjoint
        );
    }
}
