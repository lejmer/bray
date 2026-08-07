use std::collections::BTreeMap;

use bray_bound_tree::{BoundExpressionId, StorageIdentityId, StoragePlan};
use bray_symbols::TypeId;

pub(super) fn index_storage_roots(
    storage: &StoragePlan,
) -> (
    BTreeMap<StorageIdentityId, Vec<BoundExpressionId>>,
    BTreeMap<StorageIdentityId, TypeId>,
) {
    let mut accesses_by_root = BTreeMap::<_, Vec<_>>::new();

    let mut types_by_root = storage
        .identity_entries()
        .filter_map(|(identity, _)| storage.identity_type(identity).map(|ty| (identity, ty)))
        .collect::<BTreeMap<_, _>>();

    for plan in storage.access_plans() {
        let Some(root) = storage.root_identity(plan.access()) else {
            continue;
        };

        accesses_by_root
            .entry(root)
            .or_default()
            .push(plan.expression());
    }

    for (access, model) in storage.access_entries() {
        let Some(root) = storage.root_identity(access) else {
            continue;
        };

        if storage
            .resolved_projections(access)
            .is_some_and(<[_]>::is_empty)
        {
            types_by_root.entry(root).or_insert(model.reached_type());
        }
    }

    (accesses_by_root, types_by_root)
}
