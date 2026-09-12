use bray_bound_tree::{AnyBoundNodeId, StorageAccessId, StorageAccessPurpose, StoragePlan};
use std::collections::BTreeMap;

pub(super) fn invalidating_operation_accesses(
    storage: &StoragePlan,
) -> BTreeMap<AnyBoundNodeId, Box<[StorageAccessId]>> {
    let mut accesses = BTreeMap::<AnyBoundNodeId, Vec<StorageAccessId>>::new();

    for plan in storage
        .access_plans()
        .iter()
        .filter(|plan| access_invalidates_refinements(plan.purpose()))
    {
        accesses.entry(plan.node()).or_default().push(plan.access());
    }

    accesses
        .into_iter()
        .map(|(expression, accesses)| (expression, accesses.into_boxed_slice()))
        .collect()
}

const fn access_invalidates_refinements(purpose: StorageAccessPurpose) -> bool {
    matches!(
        purpose,
        StorageAccessPurpose::Write
            | StorageAccessPurpose::Initialize
            | StorageAccessPurpose::Move
            | StorageAccessPurpose::ValueTransfer
            | StorageAccessPurpose::Borrow(bray_symbols::BorrowKind::Mutable)
            | StorageAccessPurpose::Assignment
    )
}
