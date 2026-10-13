use std::collections::BTreeMap;

use bray_bound_tree::{BoundExecutionSite, StorageIdentityId, StoragePlan};

pub(super) fn index_storage_roots(
    storage: &StoragePlan,
) -> BTreeMap<StorageIdentityId, Vec<BoundExecutionSite>> {
    let mut accesses_by_root = BTreeMap::<_, Vec<_>>::new();

    for plan in storage.access_plans() {
        let Some(root) = storage.root_identity(plan.access()) else {
            continue;
        };

        if matches!(storage.identity(root), Some(bray_bound_tree::StorageIdentity::ScopedCapability { expression, .. }) if plan.occurrence() == expression.into())
        {
            continue;
        }

        accesses_by_root
            .entry(root)
            .or_default()
            .push(plan.occurrence());
    }

    accesses_by_root
}
