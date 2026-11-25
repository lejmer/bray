use bray_symbols::TypeId;

use super::{StorageAccess, StorageAccessId, StorageIdentityId, StoragePlan};

impl StoragePlan {
    /// Whether the access reaches storage first produced by this evaluated occurrence.
    /// Borrows of such storage become available with the result, after invocation inputs.
    pub fn access_is_produced_at(
        &self,
        access: StorageAccessId,
        occurrence: crate::BoundExecutionSite,
    ) -> bool {
        self.root_identity(access)
            .and_then(|root| self.identity(root))
            .and_then(|identity| identity.definition_occurrence())
            == Some(occurrence)
    }

    /// Returns the first evaluated access that names one complete persistent storage root.
    pub fn root_access(&self, identity: StorageIdentityId) -> Option<StorageAccessId> {
        self.access_at(identity, &[])
    }

    /// Returns the sole checked input access of an implicit scope or iteration call.
    pub fn implicit_input_plan(&self, occurrence: crate::BoundExecutionSite) -> Option<crate::StorageAccessPlan> {
        match occurrence {
            crate::BoundExecutionSite::Iterate(_)
            | crate::BoundExecutionSite::Next(_)
            | crate::BoundExecutionSite::ScopedEnter(_)
            | crate::BoundExecutionSite::ScopedExit(_) => self.occurrence_plans(occurrence)
                .find(|plan| plan.purpose() != crate::StorageAccessPurpose::Initialize),
            _ => None,
        }
    }

    /// Returns the owned receiver-first input slot staged for one selected call.
    pub fn invocation_input(
        &self,
        expression: crate::BoundExpressionId,
        ordinal: bray_symbols::SymbolOrdinal,
    ) -> Option<StorageAccessId> {
        let binding = super::StorageBindingTarget::InvocationInput {
            expression,
            ordinal,
        };

        let super::StorageBinding::Identity(identity) = self.binding(binding)? else {
            panic!("staged call input has an owned storage identity: {binding:?}");
        };

        Some(
            self.root_access(identity)
                .expect("staged call input retains its complete root access"),
        )
    }

    /// Returns a separately planned invocation result, including a custom indexer's returned borrow.
    pub fn invocation_result(
        &self,
        occurrence: crate::BoundExecutionSite,
    ) -> Option<StorageAccessId> {
        let super::StorageBinding::Identity(identity) =
            self.binding(super::StorageBindingTarget::InvocationResult(occurrence))?
        else {
            panic!("invocation result binding owns its result slot: {occurrence:?}");
        };

        Some(
            self.root_access(identity)
                .expect("invocation result has its complete root access"),
        )
    }

    /// Returns the payload initialized by a fallible with expression's success yield.
    pub fn scoped_result_payload(&self, expression: crate::BoundExpressionId) -> Option<StorageAccessId> {
        let super::StorageBinding::Access(access) = self.binding(super::StorageBindingTarget::ScopedResultPayload(expression))? else {
            unreachable!("scoped success payload has a checked projected access");
        };

        Some(access)
    }

    /// Returns the storage initialized by the checked return, yield, or break transfer.
    pub fn control_transfer_destination(&self, expression: crate::BoundExpressionId) -> Option<StorageAccessId> {
        self.scoped_result_payload(expression).or_else(|| {
            self.node_plans(expression.into())
                .find(|plan| plan.purpose() == crate::StorageAccessPurpose::Initialize)
                .map(|plan| plan.access())
        })
    }

    /// Returns the checked type stored by one persistent storage origin.
    pub fn storage_type(&self, identity: StorageIdentityId) -> Option<TypeId> {
        self.identity_type(identity).or_else(|| {
            self.root_access(identity)
                .and_then(|access| self.access(access))
                .map(StorageAccess::reached_type)
        })
    }
}
