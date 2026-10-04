use std::sync::Arc;

use bray_bound_tree::{
    BoundExpressionId, StorageAccessPlan, StorageFlow, StorageIdentity, StorageIdentityId,
    StorageOperationDecision, StoragePlan,
};

use super::LoweringInput;

pub(super) fn storage_plan_indices(storage: &StoragePlan) -> Arc<[usize]> {
    let mut indices = (0..storage.access_plans().len()).collect::<Vec<_>>();

    // Original positions preserve evaluation-order first matches within each expression.
    indices.sort_unstable_by_key(|index| (storage.access_plans()[*index].expression(), *index));

    indices.into()
}

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
    pub(crate) fn expression_storage_plans(
        &self,
        expression: BoundExpressionId,
    ) -> impl Iterator<Item = StorageAccessPlan> + '_ {
        let plans = self.storage.access_plans();

        let start = self
            .storage_plans
            .partition_point(|index| plans[*index].expression() < expression);

        self.storage_plans[start..]
            .iter()
            .map(|index| plans[*index])
            .take_while(move |plan| plan.expression() == expression)
    }

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
