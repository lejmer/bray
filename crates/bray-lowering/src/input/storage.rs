use std::sync::Arc;

use bray_bound_tree::{
    BoundExpressionId, StorageAccessPlan, StorageFlow, StorageIdentity, StorageIdentityId,
    StorageOperationDecision, StoragePlan,
};

use super::LoweringInput;

pub(super) fn storage_operation_indices(flow: &StorageFlow) -> Arc<[usize]> {
    let mut indices = (0..flow.operations().len()).collect::<Vec<_>>();

    indices.sort_unstable_by_key(|index| (flow.operations()[*index].expression(), *index));

    indices.into()
}

pub(super) fn temporary_storage_indices(
    storage: &StoragePlan,
) -> Arc<[(BoundExpressionId, StorageIdentityId)]> {
    let mut indices = storage
        .identity_entries()
        .filter_map(|(identity, model)| match model {
            StorageIdentity::Temporary(expression) => Some((expression, identity)),
            _ => None,
        })
        .collect::<Vec<_>>();

    // Stable sorting retains allocation order for duplicate temporary origins.
    indices.sort_by_key(|(expression, _)| *expression);

    indices.into()
}

impl LoweringInput<'_> {
    pub(crate) fn storage_operation(
        &self,
        plan: StorageAccessPlan,
    ) -> Option<StorageOperationDecision> {
        let operations = self.storage_flow.operations();
        let expression = plan.expression();

        let start = self
            .storage_operations
            .partition_point(|index| operations[*index].expression() < expression);

        self.storage_operations[start..]
            .iter()
            .map(|index| operations[*index])
            .take_while(|decision| decision.expression() == expression)
            .find(|decision| {
                decision.access() == plan.access()
                    && plan.purpose().matches_checked(decision.purpose())
            })
    }

    pub(crate) fn temporary_storage(
        &self,
        expression: BoundExpressionId,
        ty: bray_symbols::TypeId,
    ) -> Option<StorageIdentityId> {
        let start = self
            .temporary_storage
            .partition_point(|(candidate, _)| *candidate < expression);

        self.temporary_storage[start..]
            .iter()
            .take_while(|(candidate, _)| *candidate == expression)
            .map(|(_, identity)| *identity)
            .find(|identity| self.storage.storage_type(*identity) == Some(ty))
    }
}
