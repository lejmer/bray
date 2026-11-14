use std::sync::Arc;

use bray_bound_tree::{
    BoundExpressionId, StorageIdentityId, StoragePlan,
};

use super::LoweringInput;

pub(super) fn temporary_storage_indices(
    storage: &StoragePlan,
) -> Arc<[(BoundExpressionId, StorageIdentityId)]> {
    storage.value_slots().collect::<Vec<_>>().into()
}

impl LoweringInput<'_> {
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
