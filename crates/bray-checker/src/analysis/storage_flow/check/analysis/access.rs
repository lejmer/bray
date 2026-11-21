use bray_bound_tree::{
    AnyBoundNodeId, BorrowCapabilityId, Refinement, StorageAccessId, StorageAccessPlan,
    StorageAccessPurpose, StorageAccessRoot, StorageIdentity, StorageIdentityId,
    StorageRelationship,
};
use bray_symbols::BorrowKind;

use super::super::availability::projection_is_available;
use super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{ReachedAccess, borrow_chain_from};
use crate::analysis::storage_flow::model::StorageFlowState;

#[derive(Clone)]
pub(super) enum BorrowConflict {
    Unlocated,
    Borrows(Vec<BorrowCapabilityId>),
}

pub(super) fn overlapping_borrow_conflict<'target>(
    storage: &bray_bound_tree::StoragePlan,
    state: &StorageFlowState,
    access: StorageAccessId,
    targets: Option<(impl Iterator<Item = &'target bray_bound_tree::StorageTarget> + Clone, bool)>,
    requested: BorrowKind,
    authorizing_borrows: &[BorrowCapabilityId],
) -> Option<BorrowConflict> {
    let mut conflicts = Vec::new();

    for active in state.active_borrows.iter().copied() {
        if authorizing_borrows.contains(&active) {
            continue;
        }

        let Some(capability) = storage.borrow_capability(active) else {
            return Some(BorrowConflict::Unlocated);
        };

        if requested == BorrowKind::Shared && capability.kind() == BorrowKind::Shared {
            continue;
        }

        let reached = targets.clone().map(|(targets, located)| state.memory.borrow_relationship(active, targets, located));

        let overlaps = match reached {
            Some(StorageRelationship::Disjoint) => false,
            Some(StorageRelationship::Identical) => true,
            // Physical regions can be wider than a checked language loan. The
            // plan still supplies separation between checked capability paths.
            _ => storage.relationship(capability.access(), access) != StorageRelationship::Disjoint,
        };

        if overlaps {
            trace_borrow_overlap(storage, state, access, active, targets.clone().map(|(targets, _)| targets), reached, authorizing_borrows);
            conflicts.push(active);
        }
    }

    (!conflicts.is_empty()).then_some(BorrowConflict::Borrows(conflicts))
}

fn trace_borrow_overlap<'target>(
    storage: &bray_bound_tree::StoragePlan,
    state: &StorageFlowState,
    access: StorageAccessId,
    loan: BorrowCapabilityId,
    targets: Option<impl Iterator<Item = &'target bray_bound_tree::StorageTarget>>,
    reached_overlap: Option<StorageRelationship>,
    authority: &[BorrowCapabilityId],
) {
    type Pair = (StorageAccessId, BorrowCapabilityId);

    static SEEN: std::sync::OnceLock<Option<std::sync::Mutex<std::collections::BTreeSet<Pair>>>> =
        std::sync::OnceLock::new();

    let Some(seen) = SEEN.get_or_init(|| std::env::var_os("BRA_694_TRACE_BORROWS").map(|_| Default::default())) else {
        return;
    };

    if !seen.lock().expect("borrow trace lock").insert((access, loan)) {
        return;
    }

    let capability = storage.borrow_capability(loan).expect("active borrow exists");

    let describe = |access| {
        (storage.access(access).map(|access| (access.root(), access.source(), access.reached_type())),
            storage.logical_path(access),
            storage.root_identity(access).and_then(|root| storage.identity(root)).map(|identity| identity.kind_name()))
    };

    eprintln!("bra-694 borrow unit={} kind={:?} access={access:?} plan={:?} reached={:?} loan={loan:?} capability={capability:?} loan_plan={:?} regions={:?} structural={:?} overlap={reached_overlap:?} authority={authority:?}",
        storage.unit().raw(), storage.kind(), describe(access), targets.map(|targets| targets.collect::<Vec<_>>()),
        describe(capability.access()), state.memory.borrow_regions(loan), storage.relationship(capability.access(), access));
}

impl<'analysis, C> StorageFlowCollector<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn prepare_formed_borrow(
        &self,
        state: &StorageFlowState,
        plan: StorageAccessPlan,
        loan: BorrowCapabilityId,
        reached: &mut Option<ReachedAccess>,
    ) -> Option<bray_symbols::TypeId> {
        let capability = self.storage.borrow_capability(loan).expect("checked borrow has its capability");

        if capability.occurrence() != Some(plan.occurrence()) {
            return None;
        }

        let source = self.storage.access(capability.access()).expect("checked loan has a source").reached_type();
        let output = self.storage.access(plan.access()).expect("checked loan has its produced type").reached_type();
        let values = self.request.semantic_values();

        let referent = match (values.type_data(source).as_ref(), values.type_data(output).as_ref()) {
            (bray_symbols::TypeData::Borrow { target: source, .. }, bray_symbols::TypeData::Borrow { target: output, .. })
                if source == output => Some(*source),
            _ => None,
        };

        if let Some(mut access) = reached.take() {
            access.loan = access.loan.or(capability.parent());

            if referent.is_some() {
                access = state.memory.reborrow_access(access);
            }

            access.loan = access.loan.filter(|parent| parent.unit() == self.storage.unit());

            // A descriptor produced at the same site can still carry that site's
            // previous capability. Its existing parent can authorize a fresh loan.
            if access.loan == Some(loan) {
                access.loan = state.memory.borrow_ancestors(loan).and_then(|ancestors| ancestors.first().copied());
            }

            *reached = Some(access);
        }

        Some(referent.unwrap_or(source))
    }

    pub(super) fn borrow_conflict(
        &self,
        state: &StorageFlowState,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
        reached: Option<&ReachedAccess>,
    ) -> Option<BorrowConflict> {
        let requested = match purpose {
            StorageAccessPurpose::Borrow(kind) => Some(kind),
            StorageAccessPurpose::Write
            | StorageAccessPurpose::Move
            | StorageAccessPurpose::Assignment => Some(BorrowKind::Mutable),
            StorageAccessPurpose::Read
            | StorageAccessPurpose::Initialize
            | StorageAccessPurpose::Copy
            | StorageAccessPurpose::ValueTransfer => Some(BorrowKind::Shared),
            StorageAccessPurpose::Member
            | StorageAccessPurpose::Index
            | StorageAccessPurpose::Slice
            | StorageAccessPurpose::Projection => None,
        };

        let requested = requested?;

        let access = self.operation_access(plan, purpose);

        if requested == BorrowKind::Mutable
            && self.storage.projected_borrow_kind(access) == Some(BorrowKind::Shared)
        {
            let origin = self
                .storage
                .access(access)
                .and_then(|access| access.root().borrow_capability());

            return Some(origin.map_or(BorrowConflict::Unlocated, |borrow| {
                BorrowConflict::Borrows(vec![borrow])
            }));
        }

        let authority = match purpose {
            StorageAccessPurpose::Borrow(_) => plan.access(),
            _ => access,
        };

        let planned = self.storage.access(authority)?.root().borrow_capability();
        let actual = reached.and_then(|access| access.loan).filter(|loan| loan.unit() == self.storage.unit());
        let created_borrow = self.storage.borrow_for_plan(plan);

        let authority = match (planned, actual) {
            (Some(planned), Some(actual)) if created_borrow != Some(planned)
                && state.memory.borrow_ancestors(planned).is_some_and(|ancestors| ancestors.contains(&actual)) =>
                Some(planned),
            (_, Some(actual)) => Some(actual),
            (planned, None) => planned,
        };

        let Some(authorizing_borrows) = borrow_chain_from(self.storage, Some(&state.memory), authority) else {
            return Some(BorrowConflict::Unlocated);
        };

        let inactive_authorizing_borrows = authorizing_borrows
            .iter()
            .copied()
            .filter(|borrow| {
                Some(*borrow) != created_borrow && !state.active_borrows.contains(borrow)
            })
            .collect::<Vec<_>>();

        if !inactive_authorizing_borrows.is_empty() {
            if std::env::var_os("BRA_694_TRACE_BORROWS").is_some() {
                eprintln!(
                    "inactive borrow {plan:?} checked={planned:?} reached={actual:?} authority={authorizing_borrows:?} inactive={inactive_authorizing_borrows:?} active={:?}",
                    state.active_borrows
                );
            }

            return Some(BorrowConflict::Borrows(inactive_authorizing_borrows));
        }

        let targets = reached.map(|access| &access.address).filter(|address| !address.is_unknown() && !address.is_displaced())
            .map(|address| (address.targets.iter(), address.has_values()));

        overlapping_borrow_conflict(self.storage, state, access, targets, requested, &authorizing_borrows)
    }

    pub(super) fn has_mutation_authority(&self, access: StorageAccessId) -> bool {
        let Some(storage_access) = self.storage.access(access) else {
            return false;
        };

        if storage_access.root().borrow_capability().is_some() {
            return self.borrow_chain(access).is_some_and(|borrows| {
                !borrows.is_empty()
                    && borrows.iter().all(|borrow| {
                        self.storage
                            .borrow_capability(*borrow)
                            .is_some_and(|borrow| borrow.kind() == BorrowKind::Mutable)
                    })
            });
        }

        match storage_access.root() {
            StorageAccessRoot::Recovery(_) => false,
            StorageAccessRoot::Storage(storage)
            | StorageAccessRoot::OwnedIndirection { storage, .. } => {
                self.storage.projected_borrow_kind(access) == Some(BorrowKind::Mutable)
                    || self.owned_storage_is_mutable(storage)
            }
            StorageAccessRoot::Borrow(_) | StorageAccessRoot::BorrowedStorage { .. } => false,
        }
    }

    pub(super) fn operation_access(
        &self,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
    ) -> StorageAccessId {
        match purpose {
            StorageAccessPurpose::Borrow(_) => self
                .input
                .borrowed_access(plan, self.storage)
                .unwrap_or_else(|| plan.access()),
            StorageAccessPurpose::Read
            | StorageAccessPurpose::Write
            | StorageAccessPurpose::Initialize
            | StorageAccessPurpose::Assignment
            | StorageAccessPurpose::Copy
            | StorageAccessPurpose::Move
            | StorageAccessPurpose::ValueTransfer
            | StorageAccessPurpose::Member
            | StorageAccessPurpose::Index
            | StorageAccessPurpose::Slice
            | StorageAccessPurpose::Projection => plan.access(),
        }
    }

    pub(super) fn retained_borrow_value(
        &self,
        state: &StorageFlowState,
        access: StorageAccessId,
    ) -> Option<BorrowCapabilityId> {
        let access = self.storage.access(access)?;

        if !access.projections().is_empty()
            || !self.type_is_borrow(access.reached_type())
        {
            return None;
        }

        let borrow = access.root().borrow_capability()?;

        (state.definitely_active_borrows.contains(&borrow) && state.memory.has_borrow(borrow)).then_some(borrow)
    }

    pub(super) fn access_uses_borrow(&self, access: StorageAccessId) -> bool {
        let Some(record) = self.storage.access(access) else {
            return false;
        };

        self.storage
            .root_identity(access)
            .and_then(|root| self.storage.identity(root))
            .is_some_and(|identity| {
                identity.borrows_input_storage(self.request.unit().key().kind())
            })
            || record.root().borrow_capability().is_some()
            || self.storage.projected_borrow_kind(access).is_some()
    }

    pub(super) fn type_is_borrow(&self, ty: bray_symbols::TypeId) -> bool {
        let data = self.request.semantic_values().type_data(ty);

        matches!(data.as_ref(), bray_symbols::TypeData::Borrow { .. })
    }

    pub(super) fn refinements_allow_access<'refinements>(
        &self,
        state: &StorageFlowState,
        access: StorageAccessId,
        refinements: impl Iterator<Item = &'refinements Refinement> + Clone,
    ) -> bool {
        let Some(projections) = self.storage.resolved_projections(access) else {
            return false;
        };

        for (depth, projection) in projections.iter().enumerate() {
            if !projection_is_available(
                self.storage,
                access,
                *projection,
                depth,
                refinements.clone(),
            ) && !self.borrow_establishes_projection(state, access, depth)
            {
                return false;
            }
        }

        true
    }

    fn borrow_establishes_projection(
        &self,
        state: &StorageFlowState,
        access: StorageAccessId,
        depth: usize,
    ) -> bool {
        self.borrow_chain(access).is_some_and(|chain| {
            chain.into_iter().any(|borrow| {
                if !state.definitely_active_borrows.contains(&borrow) {
                    return false;
                }

                let capability = self
                    .storage
                    .borrow_capability(borrow)
                    .expect("a checked borrow chain retains its capabilities");

                // The reached payload cannot outlive or replace an enclosing
                // nullable or union while its checked borrow remains live.
                self.storage.access_contains(capability.access(), access)
                    && self
                        .storage
                        .resolved_projections(capability.access())
                        .is_some_and(|path| depth < path.len())
            })
        })
    }

    pub(super) fn borrow_chain(&self, access: StorageAccessId) -> Option<Vec<BorrowCapabilityId>> {
        borrow_chain_from(self.storage, None, self.storage.access(access)?.root().borrow_capability())
    }

    pub(super) fn owned_storage_is_mutable(&self, storage: StorageIdentityId) -> bool {
        match self.storage.identity(storage) {
            Some(StorageIdentity::LocalOwned(AnyBoundNodeId::Pattern(pattern))) => self
                .request
                .view()
                .pattern(pattern)
                .is_some_and(bray_bound_tree::BoundPattern::is_mutable),
            Some(StorageIdentity::Alternative { pattern, .. }) => self
                .request
                .view()
                .pattern(pattern)
                .is_some_and(bray_bound_tree::BoundPattern::is_mutable),
            Some(
                StorageIdentity::Result(_)
                | StorageIdentity::Temporary(_)
                | StorageIdentity::Evaluated(_)
                | StorageIdentity::InvocationInput { .. }
                | StorageIdentity::CustomIndexBorrow(_)
                | StorageIdentity::InvocationResult(_)
                | StorageIdentity::ScopedCapability { .. }
                | StorageIdentity::ScopedResultReplacement(_)
                | StorageIdentity::Allocation(_)
                | StorageIdentity::CompilerCreated(_),
            ) => true,
            Some(
                StorageIdentity::Parameter(_)
                | StorageIdentity::Receiver(_)
                | StorageIdentity::AnonymousParameter(_),
            ) => self.input.storage_is_mutable(storage),
            Some(
                StorageIdentity::Static(_)
                | StorageIdentity::LocalOwned(_)
                | StorageIdentity::PredicateParameter(_)
                | StorageIdentity::ContractParameter(_)
                | StorageIdentity::PostconditionResult(_)
                | StorageIdentity::Error(_),
            )
            | None => false,
        }
    }
}
