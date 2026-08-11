use bray_bound_tree::{
    AnyBoundNodeId, BorrowCapabilityId, RefinementFact, StorageAccessId, StorageAccessPlan,
    StorageAccessPurpose, StorageAccessRoot, StorageIdentity, StorageIdentityId, StorageProjection,
    StorageRelationship,
};
use bray_symbols::BorrowKind;

use super::super::availability::projection_is_available;
use super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::model::StorageFlowState;

impl<'analysis, C> StorageFlowCollector<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn has_borrow_conflict(
        &self,
        state: &StorageFlowState,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
    ) -> bool {
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
            return false;
        };

        let access = self.operation_access(plan, purpose);

        if let Some(kind) = self.projected_storage_borrow_kind(access) {
            return requested == BorrowKind::Mutable && kind == BorrowKind::Shared;
        }

        let authority = match purpose {
            StorageAccessPurpose::Borrow(_) => plan.access(),
            _ => access,
        };

        let Some(authorizing_borrows) = self.borrow_chain(authority) else {
            return true;
        };

        let created_borrow = self.input.borrow(plan);

        if authorizing_borrows
            .iter()
            .any(|borrow| Some(*borrow) != created_borrow && !state.active_borrows.contains(borrow))
        {
            return true;
        }

        state.active_borrows.iter().copied().any(|active| {
            if authorizing_borrows.contains(&active) {
                return false;
            }

            let Some(capability) = self.storage.borrow_capability(active) else {
                return true;
            };

            if requested == BorrowKind::Shared && capability.kind() == BorrowKind::Shared {
                return false;
            }

            self.storage.relationship(capability.access(), access) != StorageRelationship::Disjoint
        })
    }

    pub(super) fn has_mutation_authority(&self, access: StorageAccessId) -> bool {
        let Some(storage_access) = self.storage.access(access) else {
            return false;
        };

        match storage_access.root() {
            StorageAccessRoot::Borrow(_) => self.borrow_chain(access).is_some_and(|borrows| {
                !borrows.is_empty()
                    && borrows.iter().all(|borrow| {
                        self.storage
                            .borrow_capability(*borrow)
                            .is_some_and(|borrow| borrow.kind() == BorrowKind::Mutable)
                    })
            }),
            StorageAccessRoot::Recovery(_) => false,
            StorageAccessRoot::Storage(storage)
            | StorageAccessRoot::OwnedIndirection { storage, .. } => {
                self.projected_storage_borrow_kind(access) == Some(BorrowKind::Mutable)
                    || self.owned_storage_is_mutable(storage)
            }
        }
    }

    pub(super) fn fields_allow_mutation(&self, access: StorageAccessId) -> bool {
        let Some(access) = self.storage.access(access) else {
            return false;
        };

        access
            .projections()
            .iter()
            .all(|projection| match projection {
                StorageProjection::ProductField(field) => self
                    .request
                    .symbols()
                    .struct_field(*field)
                    .is_some_and(bray_symbols::StructFieldSymbol::allows_mutation),
                StorageProjection::ActiveUnionPayloadField { field, .. } => self
                    .request
                    .symbols()
                    .union_payload_field(*field)
                    .is_some_and(bray_symbols::UnionPayloadFieldSymbol::allows_mutation),
                StorageProjection::TupleElement(_)
                | StorageProjection::ElementFromStart(_)
                | StorageProjection::ElementFromEnd(_)
                | StorageProjection::Element(_)
                | StorageProjection::SliceRange { .. }
                | StorageProjection::NullableValue
                | StorageProjection::OwnedTarget => true,
            })
    }

    pub(super) fn operation_access(
        &self,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
    ) -> StorageAccessId {
        match purpose {
            StorageAccessPurpose::Borrow(_) => self
                .input
                .borrow(plan)
                .and_then(|borrow| self.storage.borrow_capability(borrow))
                .map(|borrow| borrow.access())
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
        self.storage.access(access).is_some_and(|record| {
            matches!(record.root(), StorageAccessRoot::Borrow(_))
                || self.projected_storage_borrow_kind(access).is_some()
        })
    }

    fn projected_storage_borrow_kind(&self, access: StorageAccessId) -> Option<BorrowKind> {
        let access = self.storage.access(access)?;

        let StorageAccessRoot::Storage(storage) = access.root() else {
            return None;
        };

        let root_type = self.storage.storage_type(storage)?;

        if access.projections().is_empty() && access.reached_type() == root_type {
            return None;
        }

        self.request
            .semantic_values()
            .type_data(root_type)
            .ok()
            .and_then(|data| match data.as_ref() {
                bray_symbols::TypeData::Borrow { kind, .. } => Some(*kind),
                _ => None,
            })
    }

    pub(super) fn type_is_borrow(&self, ty: bray_symbols::TypeId) -> bool {
        self.request
            .semantic_values()
            .type_data(ty)
            .is_ok_and(|data| matches!(data.as_ref(), bray_symbols::TypeData::Borrow { .. }))
    }

    pub(super) fn refinements_allow_access(
        &self,
        access: StorageAccessId,
        refinements: &[RefinementFact],
    ) -> bool {
        let Some(storage_access) = self.storage.access(access) else {
            return false;
        };

        for projection in storage_access.projections() {
            if !projection_is_available(self.storage, access, *projection, refinements) {
                return false;
            }
        }

        true
    }

    pub(super) fn borrow_chain(&self, access: StorageAccessId) -> Option<Vec<BorrowCapabilityId>> {
        let mut capability = match self.storage.access(access)?.root() {
            StorageAccessRoot::Borrow(capability) => Some(capability),
            StorageAccessRoot::Storage(_)
            | StorageAccessRoot::OwnedIndirection { .. }
            | StorageAccessRoot::Recovery(_) => None,
        };

        let mut chain = Vec::new();

        while let Some(current) = capability {
            let borrow = self.storage.borrow_capability(current)?;

            chain.push(current);
            capability = borrow.parent();
        }

        Some(chain)
    }

    pub(super) fn mutation_authority_access(
        &self,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
    ) -> Option<StorageAccessId> {
        match purpose {
            StorageAccessPurpose::Write | StorageAccessPurpose::Assignment => Some(plan.access()),
            StorageAccessPurpose::Borrow(BorrowKind::Mutable) => self
                .input
                .borrow(plan)
                .and_then(|borrow| self.storage.borrow_capability(borrow))
                .map(|borrow| borrow.access()),
            StorageAccessPurpose::Read
            | StorageAccessPurpose::Initialize
            | StorageAccessPurpose::Copy
            | StorageAccessPurpose::Move
            | StorageAccessPurpose::Borrow(BorrowKind::Shared)
            | StorageAccessPurpose::ValueTransfer
            | StorageAccessPurpose::Member
            | StorageAccessPurpose::Index
            | StorageAccessPurpose::Slice
            | StorageAccessPurpose::Projection => None,
        }
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
                | StorageIdentity::CustomIndexBorrow(_)
                | StorageIdentity::IterationCursor(_)
                | StorageIdentity::IterationElement(_)
                | StorageIdentity::Allocation(_)
                | StorageIdentity::CompilerCreated(_),
            ) => true,
            Some(StorageIdentity::Parameter(_) | StorageIdentity::Receiver(_)) => {
                self.input.storage_is_mutable(storage)
            }
            Some(
                StorageIdentity::LocalOwned(_)
                | StorageIdentity::AnonymousParameter(_)
                | StorageIdentity::PredicateParameter(_)
                | StorageIdentity::PostconditionResult(_)
                | StorageIdentity::Error(_),
            )
            | None => false,
        }
    }
}
