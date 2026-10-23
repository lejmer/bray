use bray_bound_tree::{
    AnyBoundNodeId, BorrowCapabilityId, Refinement, StorageAccessId, StorageAccessPlan,
    StorageAccessPurpose, StorageAccessRoot, StorageIdentity, StorageIdentityId,
    StorageRelationship,
};
use bray_symbols::BorrowKind;

use super::super::availability::projection_is_available;
use super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::model::StorageFlowState;

#[derive(Clone)]
pub(super) enum BorrowConflict {
    Unlocated,
    Borrows(Vec<BorrowCapabilityId>),
}

impl<'analysis, C> StorageFlowCollector<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn borrow_conflict(
        &self,
        state: &StorageFlowState,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
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

        let Some(requested) = requested else {
            return None;
        };

        let access = self.operation_access(plan, purpose);

        if requested == BorrowKind::Mutable
            && self.projected_storage_borrow_kind(access) == Some(BorrowKind::Shared)
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

        let Some(authorizing_borrows) = self.borrow_chain(authority) else {
            return Some(BorrowConflict::Unlocated);
        };

        let created_borrow = self.storage.borrow_for_plan(plan);

        let inactive_authorizing_borrows = authorizing_borrows
            .iter()
            .copied()
            .filter(|borrow| {
                Some(*borrow) != created_borrow && !state.active_borrows.contains(borrow)
            })
            .collect::<Vec<_>>();

        if !inactive_authorizing_borrows.is_empty() {
            return Some(BorrowConflict::Borrows(inactive_authorizing_borrows));
        }

        let mut conflicts = Vec::new();

        for active in state.active_borrows.iter().copied() {
            if authorizing_borrows.contains(&active) {
                continue;
            }

            let Some(capability) = self.storage.borrow_capability(active) else {
                return Some(BorrowConflict::Unlocated);
            };

            if requested == BorrowKind::Shared && capability.kind() == BorrowKind::Shared {
                continue;
            }

            if self.storage.relationship(capability.access(), access)
                != StorageRelationship::Disjoint
            {
                conflicts.push(active);
            }
        }

        (!conflicts.is_empty()).then_some(BorrowConflict::Borrows(conflicts))
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
                self.projected_storage_borrow_kind(access) == Some(BorrowKind::Mutable)
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
            || self.projected_storage_borrow_kind(access).is_some()
    }

    pub(super) fn projected_storage_borrow_kind(
        &self,
        access: StorageAccessId,
    ) -> Option<BorrowKind> {
        let Some(access) = self.storage.access(access) else {
            return None;
        };

        let StorageAccessRoot::Storage(storage) = access.root() else {
            return None;
        };

        let Some(root_type) = self.storage.storage_type(storage) else {
            return None;
        };

        for depth in 0..=access.projections().len() {
            let ty = if depth == 0 {
                Some(root_type)
            } else {
                self.storage
                    .access_at(storage, &access.projections()[..depth])
                    .and_then(|prefix| self.storage.access(prefix))
                    .map(|prefix| prefix.reached_type())
            };

            let Some(ty) = ty else {
                continue;
            };

            if depth == access.projections().len() && ty == access.reached_type() {
                continue;
            }

            let data = self.request.semantic_values().type_data(ty);

            if let bray_symbols::TypeData::Borrow { kind, .. } = data.as_ref() {
                return Some(*kind);
            }
        }

        None
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
        let mut capability = self.storage.access(access)?.root().borrow_capability();

        let mut chain = Vec::new();

        while let Some(current) = capability {
            let borrow = self.storage.borrow_capability(current)?;

            chain.push(current);
            capability = borrow.parent();
        }

        Some(chain)
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
                | StorageIdentity::InvocationInput { .. }
                | StorageIdentity::CustomIndexBorrow(_)
                | StorageIdentity::InvocationResult(_)
                | StorageIdentity::ScopedCapability { .. }
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
