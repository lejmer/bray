use bray_symbols::TypeId;

use super::{StorageAccess, StorageAccessId, StorageIdentityId, StoragePlan};

impl StoragePlan {
    /// Returns the first evaluated access that names one complete persistent storage root.
    pub fn root_access(&self, identity: StorageIdentityId) -> Option<StorageAccessId> {
        self.access_at(identity, &[])
    }

    /// Returns the owned receiver-first input slot staged for one selected call.
    pub fn invocation_input(
        &self,
        expression: crate::BoundExpressionId,
        ordinal: bray_symbols::SymbolOrdinal,
    ) -> Option<StorageAccessId> {
        let binding = super::StorageBindingTarget::InvocationInput { expression, ordinal };
        let super::StorageBinding::Identity(identity) = self.binding(binding)? else {
            panic!("staged call input has an owned storage identity: {binding:?}");
        };

        Some(self.root_access(identity)
            .expect("staged call input retains its complete root access"))
    }

    /// Returns the complete result slot for an implicit invocation.
    pub fn invocation_result(&self, occurrence: crate::BoundExecutionSite) -> Option<StorageAccessId> {
        let super::StorageBinding::Identity(identity) =
            self.binding(super::StorageBindingTarget::InvocationResult(occurrence))? else {
            panic!("invocation result binding owns its result slot: {occurrence:?}");
        };
        Some(self.root_access(identity).expect("invocation result has its complete root access"))
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
