use std::collections::{BTreeMap, BTreeSet};

use crate::{
    BorrowCapabilityId, BoundUnitId, BoundUnitKind, PlannedBorrowCapability, StorageAccess,
    StorageAccessId, StorageAccessPlan, StorageAccessPurpose, StorageAlternative,
    StorageAlternativeId, StorageBinding, StorageBindingTarget, StorageIdentity, StorageIdentityId,
    StoragePlan,
};
use bray_symbols::TypeId;

/// A contract violation that prevents construction of one storage plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StoragePlanBuildError {
    /// A record or relationship belongs to another bound unit.
    ForeignUnit,
    /// A dense unit-local identity exceeded the supported slot range.
    CapacityExceeded,
    /// A relationship references a storage identity that was not established.
    MissingIdentity,
    /// A relationship references a storage access that was not established.
    MissingAccess,
    /// A relationship references a borrow capability that was not established.
    MissingBorrowCapability,
    /// A semantic identity was associated more than once.
    DuplicateBinding,
    /// A semantic identity was associated with an incompatible storage origin.
    BindingIdentityMismatch,
}

/// Builds one validated immutable storage plan.
pub struct StoragePlanBuilder {
    pub(super) unit: BoundUnitId,
    pub(super) kind: BoundUnitKind,
    pub(super) identities: Vec<StorageIdentity>,
    pub(super) identity_types: BTreeMap<StorageIdentityId, TypeId>,
    pub(super) accesses: Vec<StorageAccess>,
    pub(super) alternatives: Vec<StorageAlternative>,
    pub(super) borrow_capabilities: Vec<PlannedBorrowCapability>,
    pub(super) bindings: BTreeMap<StorageBindingTarget, StorageBinding>,
    pub(super) plans: Vec<StorageAccessPlan>,
    pub(super) planned_accesses: BTreeSet<(
        crate::BoundExpressionId,
        StorageAccessPurpose,
        StorageAccessId,
    )>,
}

impl StoragePlanBuilder {
    /// Starts a storage plan for one exact bound unit.
    pub fn new(unit: BoundUnitId, kind: BoundUnitKind) -> Self {
        Self {
            unit,
            kind,
            identities: Vec::new(),
            identity_types: BTreeMap::new(),
            accesses: Vec::new(),
            alternatives: Vec::new(),
            borrow_capabilities: Vec::new(),
            bindings: BTreeMap::new(),
            plans: Vec::new(),
            planned_accesses: BTreeSet::new(),
        }
    }

    /// Adds one persistent storage origin.
    pub fn push_identity(
        &mut self,
        identity: StorageIdentity,
    ) -> Result<StorageIdentityId, StoragePlanBuildError> {
        if !identity.is_valid_for(self.unit) {
            return Err(StoragePlanBuildError::ForeignUnit);
        }

        let slot = next_slot(self.identities.len())?;
        let id = StorageIdentityId::from_storage_slot(self.unit, slot);

        self.identities.push(identity);

        Ok(id)
    }

    /// Records the checked type stored by one persistent storage origin.
    pub fn set_identity_type(
        &mut self,
        identity: StorageIdentityId,
        ty: TypeId,
    ) -> Result<(), StoragePlanBuildError> {
        if self.identity(identity).is_none() {
            return Err(StoragePlanBuildError::MissingIdentity);
        }

        self.identity_types.insert(identity, ty);

        Ok(())
    }

    /// Adds one planned borrow or reborrow capability.
    pub fn push_borrow_capability(
        &mut self,
        capability: PlannedBorrowCapability,
    ) -> Result<BorrowCapabilityId, StoragePlanBuildError> {
        if capability
            .expression()
            .is_some_and(|expression| expression.unit() != self.unit)
            || capability.access().unit() != self.unit
            || capability
                .parent()
                .is_some_and(|parent| parent.unit() != self.unit)
        {
            return Err(StoragePlanBuildError::ForeignUnit);
        }

        if self.access(capability.access()).is_none() {
            return Err(StoragePlanBuildError::MissingAccess);
        }

        if capability
            .parent()
            .is_some_and(|parent| self.borrow_capability(parent).is_none())
        {
            return Err(StoragePlanBuildError::MissingBorrowCapability);
        }

        let slot = next_slot(self.borrow_capabilities.len())?;
        let id = BorrowCapabilityId::from_storage_slot(self.unit, slot);

        self.borrow_capabilities.push(capability);

        Ok(id)
    }

    /// Adds one evaluated storage-access occurrence.
    pub fn push_access(
        &mut self,
        access: StorageAccess,
    ) -> Result<StorageAccessId, StoragePlanBuildError> {
        if !access.is_valid_for(self.unit) {
            return Err(StoragePlanBuildError::ForeignUnit);
        }

        self.validate_access_root(access.root())?;

        let slot = next_slot(self.accesses.len())?;
        let id = StorageAccessId::from_storage_slot(self.unit, slot);

        self.accesses.push(access);

        Ok(id)
    }

    /// Adds one branch-dependent alias after all source accesses are established.
    pub fn push_alternative(
        &mut self,
        pattern: crate::BoundPatternId,
        accesses: impl IntoIterator<Item = StorageAccessId>,
    ) -> Result<StorageAlternativeId, StoragePlanBuildError> {
        if pattern.unit() != self.unit {
            return Err(StoragePlanBuildError::ForeignUnit);
        }

        let accesses = accesses.into_iter().collect::<Vec<_>>();

        if accesses.is_empty() || accesses.iter().any(|access| self.access(*access).is_none()) {
            return Err(StoragePlanBuildError::MissingAccess);
        }

        let slot = next_slot(self.alternatives.len())?;
        let id = StorageAlternativeId::from_storage_slot(self.unit, slot);

        self.alternatives
            .push(StorageAlternative::new(pattern, accesses));

        Ok(id)
    }

    /// Associates a semantic identity with persistent storage or a projected access.
    pub fn bind(
        &mut self,
        target: StorageBindingTarget,
        binding: StorageBinding,
    ) -> Result<(), StoragePlanBuildError> {
        self.validate_binding(binding)?;

        if !self.binding_matches_identity(target, binding) {
            return Err(StoragePlanBuildError::BindingIdentityMismatch);
        }

        if let Some(existing) = self.bindings.get(&target) {
            return if *existing == binding {
                Ok(())
            } else {
                Err(StoragePlanBuildError::DuplicateBinding)
            };
        }

        self.bindings.insert(target, binding);

        Ok(())
    }

    /// Records how one expression occurrence uses an evaluated access.
    pub fn plan_access(
        &mut self,
        expression: crate::BoundExpressionId,
        purpose: StorageAccessPurpose,
        access: StorageAccessId,
    ) -> Result<(), StoragePlanBuildError> {
        if expression.unit() != self.unit || access.unit() != self.unit {
            return Err(StoragePlanBuildError::ForeignUnit);
        }

        if self.access(access).is_none() {
            return Err(StoragePlanBuildError::MissingAccess);
        }

        if !self.planned_accesses.insert((expression, purpose, access)) {
            return Ok(());
        }

        self.plans
            .push(StorageAccessPlan::new(expression, purpose, access));

        Ok(())
    }

    /// Returns the storage or access already associated with a semantic identity.
    pub fn binding(&self, target: StorageBindingTarget) -> Option<StorageBinding> {
        self.bindings.get(&target).copied()
    }

    /// Returns a previously established access.
    pub fn access(&self, id: StorageAccessId) -> Option<&StorageAccess> {
        checked_entry(self.unit, id.unit(), id.storage_index(), &self.accesses)
    }

    /// Returns a previously established storage identity.
    pub fn identity(&self, id: StorageIdentityId) -> Option<&StorageIdentity> {
        checked_entry(self.unit, id.unit(), id.storage_index(), &self.identities)
    }

    /// Returns a previously established borrow capability.
    pub fn borrow_capability(&self, id: BorrowCapabilityId) -> Option<&PlannedBorrowCapability> {
        checked_entry(
            self.unit,
            id.unit(),
            id.storage_index(),
            &self.borrow_capabilities,
        )
    }

    /// Completes the immutable storage plan.
    pub fn finish(self) -> StoragePlan {
        StoragePlan::new(self)
    }

    fn validate_access_root(
        &self,
        root: crate::StorageAccessRoot,
    ) -> Result<(), StoragePlanBuildError> {
        match root {
            crate::StorageAccessRoot::Storage(storage)
            | crate::StorageAccessRoot::Recovery(storage)
            | crate::StorageAccessRoot::OwnedIndirection { storage, .. } => {
                if self.identity(storage).is_none() {
                    return Err(StoragePlanBuildError::MissingIdentity);
                }
            }
            crate::StorageAccessRoot::Borrow(capability) => {
                if self.borrow_capability(capability).is_none() {
                    return Err(StoragePlanBuildError::MissingBorrowCapability);
                }
            }
        }

        Ok(())
    }

    fn validate_binding(&self, binding: StorageBinding) -> Result<(), StoragePlanBuildError> {
        if binding.unit() != self.unit {
            return Err(StoragePlanBuildError::ForeignUnit);
        }

        match binding {
            StorageBinding::Identity(storage) if self.identity(storage).is_none() => {
                Err(StoragePlanBuildError::MissingIdentity)
            }
            StorageBinding::Access(access) if self.access(access).is_none() => {
                Err(StoragePlanBuildError::MissingAccess)
            }
            StorageBinding::Identity(_) | StorageBinding::Access(_) => Ok(()),
        }
    }

    fn binding_matches_identity(
        &self,
        target: StorageBindingTarget,
        binding: StorageBinding,
    ) -> bool {
        let StorageBinding::Identity(storage) = binding else {
            return matches!(
                target,
                StorageBindingTarget::Parameter(_)
                    | StorageBindingTarget::Receiver(_)
                    | StorageBindingTarget::AnonymousParameter(_)
                    | StorageBindingTarget::PredicateParameter(_)
                    | StorageBindingTarget::Local(_)
            );
        };

        match (target, self.identity(storage).copied()) {
            (
                StorageBindingTarget::Parameter(expected),
                Some(StorageIdentity::Parameter(actual)),
            ) => expected == actual,
            (StorageBindingTarget::Receiver(expected), Some(StorageIdentity::Receiver(actual))) => {
                expected == actual
            }
            (
                StorageBindingTarget::AnonymousParameter(expected),
                Some(StorageIdentity::AnonymousParameter(actual)),
            ) => expected == actual,
            (
                StorageBindingTarget::PredicateParameter(expected),
                Some(StorageIdentity::PredicateParameter(actual)),
            ) => expected == actual,
            (
                StorageBindingTarget::Local(_),
                Some(
                    StorageIdentity::LocalOwned(_)
                    | StorageIdentity::Alternative { .. }
                    | StorageIdentity::Error(_),
                ),
            )
            | (StorageBindingTarget::Result, Some(StorageIdentity::Result(_))) => true,
            (
                StorageBindingTarget::PostconditionResult(expected),
                Some(StorageIdentity::PostconditionResult(actual)),
            ) => expected == actual,
            _ => false,
        }
    }
}

fn checked_entry<T>(
    expected_unit: BoundUnitId,
    actual_unit: BoundUnitId,
    index: Option<usize>,
    entries: &[T],
) -> Option<&T> {
    if actual_unit != expected_unit {
        return None;
    }

    index.and_then(|index| entries.get(index))
}

fn next_slot(length: usize) -> Result<u32, StoragePlanBuildError> {
    u32::try_from(length).map_err(|_| StoragePlanBuildError::CapacityExceeded)
}

#[cfg(test)]
mod tests {
    use bray_symbols::{CallableParameterSymbolId, SymbolId};

    use super::{StoragePlanBuildError, StoragePlanBuilder};
    use crate::{
        BoundUnitId, BoundUnitKind, StorageBinding, StorageBindingTarget, StorageIdentity,
    };

    #[test]
    fn builders_reject_incompatible_identity_relationships() {
        let unit = BoundUnitId::new(4);
        let parameter = CallableParameterSymbolId::from_symbol_id(SymbolId::new(2));
        let mut builder = StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);

        let storage = match builder.push_identity(StorageIdentity::Parameter(parameter)) {
            Ok(storage) => storage,
            Err(error) => panic!("test parameter storage must build: {error:?}"),
        };

        assert_eq!(
            builder.bind(
                StorageBindingTarget::Result,
                StorageBinding::Identity(storage),
            ),
            Err(StoragePlanBuildError::BindingIdentityMismatch)
        );
    }
}
