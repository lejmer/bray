use super::contracts::{
    PlannedBorrowCapability, StorageAccessPlan, StorageAlternative, StorageBinding,
    StorageBindingTarget,
};
use super::relationships::{identity_is_distinct_storage, path_contains, projection_relationship};
use crate::storage::builder::StoragePlanBuilder;
use crate::{
    BorrowCapabilityId, BoundExpressionId, BoundUnitId, BoundUnitKind, StorageAccess,
    StorageAccessId, StorageAlternativeId, StorageIdentity, StorageIdentityId, StorageProjection,
    StorageRelationship,
};
use bray_base::shared_slice;
use bray_symbols::{BorrowKind, TypeId};
use std::collections::BTreeMap;
use std::sync::Arc;

/// Immutable storage identities and occurrence-specific access plans for one bound unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StoragePlan {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    identities: Arc<[StorageIdentity]>,
    identity_types: BTreeMap<StorageIdentityId, TypeId>,
    owned_borrows: BTreeMap<(TypeId, BorrowKind), crate::StorageProtocolCall>,
    accesses: Arc<[StorageAccess]>,
    alternatives: Arc<[StorageAlternative]>,
    resolved_accesses: Arc<[Option<ResolvedStorageAccess>]>,
    borrow_capabilities: Arc<[PlannedBorrowCapability]>,
    bindings: Arc<[(StorageBindingTarget, StorageBinding)]>,
    plans: Arc<[StorageAccessPlan]>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct ResolvedStorageAccess {
    logical_root: StorageIdentityId,
    logical_projections: Arc<[StorageProjection]>,
    paths: Arc<[ResolvedStoragePath]>,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct ResolvedStoragePath {
    pub(super) root: StorageIdentityId,
    pub(super) projections: Arc<[StorageProjection]>,
}

impl StoragePlan {
    pub(in crate::storage) fn new(builder: StoragePlanBuilder) -> Self {
        let StoragePlanBuilder {
            unit,
            kind,
            identities,
            identity_types,
            owned_borrows,
            accesses,
            alternatives,
            borrow_capabilities,
            bindings,
            plans,
            planned_accesses: _,
        } = builder;

        let resolved_accesses = resolve_accesses(
            unit,
            &identities,
            &accesses,
            &alternatives,
            &borrow_capabilities,
        );

        Self {
            unit,
            kind,
            identities: identities.into(),
            identity_types,
            owned_borrows,
            accesses: accesses.into(),
            alternatives: alternatives.into(),
            resolved_accesses: resolved_accesses.into(),
            borrow_capabilities: borrow_capabilities.into(),
            bindings: bindings.into_iter().collect(),
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

    /// Returns whether recovery contributed to any storage identity, access, or borrow.
    pub fn is_recovered(&self) -> bool {
        self.identity_entries()
            .any(|(_, identity)| matches!(identity, StorageIdentity::Error(_)))
            || self
                .access_entries()
                .any(|(_, access)| access.is_recovered())
            || self
                .borrow_capability_entries()
                .any(|(_, capability)| capability.is_recovered())
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

    /// Returns the checked type stored by one persistent storage origin, when recorded.
    pub fn identity_type(&self, identity: StorageIdentityId) -> Option<TypeId> {
        self.identity_types.get(&identity).copied()
    }

    /// Returns the selected policy call for this owned type and borrow kind.
    pub fn owned_borrow(
        &self,
        owner: TypeId,
        kind: BorrowKind,
    ) -> Option<crate::StorageProtocolCall> {
        self.owned_borrows.get(&(owner, kind)).copied()
    }

    /// Returns selected policy borrows independently of exit cleanup requirements.
    pub fn owned_borrows(
        &self,
    ) -> impl Iterator<Item = (TypeId, BorrowKind, crate::StorageProtocolCall)> + '_ {
        self.owned_borrows
            .iter()
            .map(|((owner, kind), call)| (*owner, *kind, *call))
    }

    /// Returns evaluated accesses in deterministic evaluation order.
    pub fn accesses(&self) -> &[StorageAccess] {
        &self.accesses
    }

    /// Returns evaluated accesses with their unit-local identities.
    pub fn access_entries(&self) -> impl Iterator<Item = (StorageAccessId, &StorageAccess)> {
        self.accesses
            .iter()
            .enumerate()
            .filter_map(|(index, access)| {
                let slot = u32::try_from(index).ok()?;
                let id = StorageAccessId::from_storage_slot(self.unit, slot);

                Some((id, access))
            })
    }

    /// Returns branch-dependent aliases in deterministic allocation order.
    pub fn alternatives(&self) -> &[StorageAlternative] {
        &self.alternatives
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

    /// Returns one branch-dependent storage alias.
    pub fn alternative(&self, id: StorageAlternativeId) -> Option<&StorageAlternative> {
        self.entry(id.unit(), id.storage_index(), &self.alternatives)
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

        if left == right {
            return StorageRelationship::Identical;
        }

        // A retained borrow can cover broad source storage while its own fields remain disjoint.
        if left.logical_root == right.logical_root
            && projection_relationship(&left.logical_projections, &right.logical_projections)
                == StorageRelationship::Disjoint
        {
            return StorageRelationship::Disjoint;
        }

        let mut relationship = None;

        for left in left.paths.iter() {
            for right in right.paths.iter() {
                let current = self.path_relationship(left, right);

                relationship = Some(match relationship {
                    None => current,
                    Some(previous) if previous == current => previous,
                    Some(_) => StorageRelationship::PotentiallyOverlapping,
                });
            }
        }

        relationship.unwrap_or(StorageRelationship::Error)
    }

    /// Compares stored values without following the source dependencies of retained borrows.
    pub fn value_relationship(
        &self,
        left: StorageAccessId,
        right: StorageAccessId,
    ) -> StorageRelationship {
        let (Some(left), Some(right)) = (self.resolved_access(left), self.resolved_access(right))
        else {
            return StorageRelationship::Error;
        };

        if left.logical_root != right.logical_root {
            return StorageRelationship::Disjoint;
        }

        projection_relationship(&left.logical_projections, &right.logical_projections)
    }

    /// Returns the persistent storage root reached by one access.
    pub fn root_identity(&self, access: StorageAccessId) -> Option<StorageIdentityId> {
        self.resolved_access(access)
            .map(|access| access.logical_root)
    }

    /// Returns the complete resolved projection path reached by one access.
    pub fn resolved_projections(&self, access: StorageAccessId) -> Option<&[StorageProjection]> {
        self.resolved_access(access)
            .map(|access| access.logical_projections.as_ref())
    }

    /// Returns whether the first access contains the complete second access.
    pub fn access_contains(&self, container: StorageAccessId, contained: StorageAccessId) -> bool {
        let (Some(container), Some(contained)) = (
            self.resolved_access(container),
            self.resolved_access(contained),
        ) else {
            return false;
        };

        if container.logical_root == contained.logical_root {
            return contained
                .logical_projections
                .starts_with(&container.logical_projections);
        }

        contained.paths.iter().all(|contained| {
            container
                .paths
                .iter()
                .any(|container| path_contains(container, contained))
        })
    }

    /// Returns whether an access names its complete persistent storage root.
    pub fn is_root_access(&self, access: StorageAccessId) -> bool {
        self.resolved_access(access)
            .is_some_and(|access| access.logical_projections.is_empty())
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

    fn path_relationship(
        &self,
        left: &ResolvedStoragePath,
        right: &ResolvedStoragePath,
    ) -> StorageRelationship {
        if left.root != right.root {
            return if self.roots_are_proven_disjoint(left.root, right.root) {
                StorageRelationship::Disjoint
            } else {
                StorageRelationship::PotentiallyOverlapping
            };
        }

        projection_relationship(&left.projections, &right.projections)
    }
}

fn resolve_accesses(
    unit: BoundUnitId,
    identities: &[StorageIdentity],
    accesses: &[StorageAccess],
    alternatives: &[StorageAlternative],
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
                resolve_storage_access(unit, access, root, identities, alternatives, &resolved)
            }
            crate::StorageAccessRoot::Recovery(_) => None,
            borrow @ (crate::StorageAccessRoot::Borrow(_)
            | crate::StorageAccessRoot::BorrowedStorage { .. }) => {
                resolve_borrowed_access(unit, access, borrow, capabilities, &resolved)
            }
        };

        resolved.push(current);
    }

    resolved
}

fn resolve_borrowed_access(
    unit: BoundUnitId,
    access: &StorageAccess,
    root: crate::StorageAccessRoot,
    capabilities: &[PlannedBorrowCapability],
    resolved: &[Option<ResolvedStorageAccess>],
) -> Option<ResolvedStorageAccess> {
    let capability = root.borrow_capability()?;
    let retained = root.retained_borrow_storage();

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

    let (logical_root, logical_projections, paths) = match retained {
        None => (
            inherited.logical_root,
            shared_slice(
                inherited
                    .logical_projections
                    .iter()
                    .chain(access.projections())
                    .copied(),
            ),
            shared_slice(inherited.paths.iter().map(|path| ResolvedStoragePath {
                root: path.root,
                projections: shared_slice(
                    path.projections.iter().chain(access.projections()).copied(),
                ),
            })),
        ),
        Some(storage) => (
            storage,
            shared_slice(access.projections().iter().copied()),
            inherited.paths.clone(),
        ),
    };

    Some(ResolvedStorageAccess {
        logical_root,
        logical_projections,
        paths,
    })
}

fn resolve_storage_access(
    unit: BoundUnitId,
    access: &StorageAccess,
    root: StorageIdentityId,
    identities: &[StorageIdentity],
    alternatives: &[StorageAlternative],
    resolved: &[Option<ResolvedStorageAccess>],
) -> Option<ResolvedStorageAccess> {
    let identity = identities.get(root.storage_index()?)?;

    let paths = match identity {
        StorageIdentity::Alternative { alternative, .. } => {
            if alternative.unit() != unit {
                return None;
            }

            let alternative = alternatives.get(alternative.storage_index()?)?;

            let mut paths = Vec::new();

            for branch in alternative.accesses() {
                if branch.unit() != unit {
                    return None;
                }

                let branch = resolved.get(branch.storage_index()?)?.as_ref()?;

                paths.extend(branch.paths.iter().map(|path| ResolvedStoragePath {
                    root: path.root,
                    projections: shared_slice(
                        path.projections.iter().chain(access.projections()).copied(),
                    ),
                }));
            }

            paths
        }
        _ => vec![ResolvedStoragePath {
            root,
            projections: shared_slice(access.projections().iter().copied()),
        }],
    };

    Some(ResolvedStorageAccess {
        logical_root: root,
        logical_projections: shared_slice(access.projections().iter().copied()),
        paths: paths.into(),
    })
}

#[cfg(test)]
mod tests {
    use bray_symbols::SymbolOrdinal;

    use super::StoragePlan;
    use crate::{
        BoundExpressionId, BoundUnitId, BoundUnitKind, StorageAccess, StorageAccessId,
        StorageAccessPurpose, StorageAccessRoot, StorageIdentity, StorageIdentityId,
        StorageProjection, StorageRelationship,
    };

    #[test]
    fn storage_plans_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<StoragePlan>();
    }

    #[test]
    fn value_transfer_accepts_its_checked_copy_or_move_resolution() {
        assert!(
            StorageAccessPurpose::ValueTransfer
                .matches_checked(StorageAccessPurpose::ValueTransfer)
        );

        assert!(StorageAccessPurpose::ValueTransfer.matches_checked(StorageAccessPurpose::Copy));
        assert!(StorageAccessPurpose::ValueTransfer.matches_checked(StorageAccessPurpose::Move));

        assert!(!StorageAccessPurpose::Read.matches_checked(StorageAccessPurpose::Move));
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

        let mut builder = crate::StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);

        let root = builder
            .push_identity(StorageIdentity::Temporary(BoundExpressionId::from_slot(
                unit, 1,
            )))
            .unwrap_or_else(|error| panic!("test storage identity must build: {error:?}"));

        assert_eq!(root, StorageIdentityId::from_slot(unit, 0));

        for access in [first, second, nested] {
            builder
                .push_access(access)
                .unwrap_or_else(|error| panic!("test storage access must build: {error:?}"));
        }

        let plan = builder.finish();

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
            plan.resolved_projections(nested),
            Some(
                [
                    StorageProjection::TupleElement(SymbolOrdinal::new(0)),
                    StorageProjection::Element(BoundExpressionId::from_slot(unit, 0)),
                ]
                .as_slice()
            )
        );

        assert_eq!(
            plan.relationship(first, StorageAccessId::from_slot(BoundUnitId::new(9), 0)),
            StorageRelationship::Error
        );
    }

    #[test]
    fn retained_borrow_fields_are_disjoint_without_separating_their_source() {
        let unit = BoundUnitId::new(6);
        let expression = BoundExpressionId::from_slot(unit, 0);
        let source = crate::test_support::source_anchor();
        let ty = crate::test_support::error_type();
        let mut builder = crate::StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);

        let owner = builder
            .push_identity(StorageIdentity::Temporary(expression))
            .unwrap();

        let retained = builder
            .push_identity(StorageIdentity::Temporary(BoundExpressionId::from_slot(
                unit, 1,
            )))
            .unwrap();

        let owner_access = builder
            .push_access(StorageAccess::new(
                StorageAccessRoot::Storage(owner),
                [],
                ty,
                source,
                false,
            ))
            .unwrap();

        let capability = builder
            .push_borrow_capability(crate::PlannedBorrowCapability::new(
                crate::BorrowCapabilityOrigin::Expression(expression),
                bray_symbols::BorrowKind::Mutable,
                owner_access,
                None,
                source,
                false,
            ))
            .unwrap();

        let root = StorageAccessRoot::BorrowedStorage {
            capability,
            storage: retained,
        };

        let first = builder
            .push_access(StorageAccess::new(
                root,
                [StorageProjection::TupleElement(SymbolOrdinal::new(0))],
                ty,
                source,
                false,
            ))
            .unwrap();

        let second = builder
            .push_access(StorageAccess::new(
                root,
                [StorageProjection::TupleElement(SymbolOrdinal::new(1))],
                ty,
                source,
                false,
            ))
            .unwrap();

        let child = builder
            .push_borrow_capability(crate::PlannedBorrowCapability::new(
                crate::BorrowCapabilityOrigin::Expression(BoundExpressionId::from_slot(unit, 2)),
                bray_symbols::BorrowKind::Shared,
                first,
                Some(capability),
                source,
                false,
            ))
            .unwrap();

        let child_access = builder
            .push_access(StorageAccess::new(
                StorageAccessRoot::Borrow(child),
                [],
                ty,
                source,
                false,
            ))
            .unwrap();

        let plan = builder.finish();

        assert_eq!(
            plan.relationship(first, second),
            StorageRelationship::Disjoint
        );

        assert_eq!(
            plan.relationship(child_access, second),
            StorageRelationship::Disjoint
        );

        assert_ne!(
            plan.relationship(owner_access, first),
            StorageRelationship::Disjoint
        );

        assert_eq!(plan.value_relationship(owner_access, first), StorageRelationship::Disjoint);
        assert_eq!(plan.value_relationship(first, child_access), StorageRelationship::Identical);
        assert_eq!(plan.value_relationship(first, second), StorageRelationship::Disjoint);

        assert_ne!(
            plan.relationship(owner_access, second),
            StorageRelationship::Disjoint
        );

        assert!(!plan.access_contains(first, second));
        assert!(!plan.access_contains(second, child_access));
        assert!(plan.access_contains(first, child_access));
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

        let mut builder = crate::StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);

        for identity in [
            StorageIdentity::Parameter(bray_symbols::CallableParameterSymbolId::from_symbol_id(
                bray_symbols::SymbolId::new(1),
            )),
            StorageIdentity::Result(crate::AnyBoundNodeId::Expression(
                BoundExpressionId::from_slot(unit, 0),
            )),
        ] {
            builder
                .push_identity(identity)
                .unwrap_or_else(|error| panic!("test storage identity must build: {error:?}"));
        }

        for access in [parameter, result] {
            builder
                .push_access(access)
                .unwrap_or_else(|error| panic!("test storage access must build: {error:?}"));
        }

        let plan = builder.finish();

        assert_eq!(
            plan.relationship(
                StorageAccessId::from_slot(unit, 0),
                StorageAccessId::from_slot(unit, 1)
            ),
            StorageRelationship::Disjoint
        );
    }
}
