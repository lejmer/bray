use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, AsyncStorageCleanupRequirement, BoundCallResult, BoundCallableTarget,
    BoundExpression, BoundExpressionId, BoundPatternKind, BoundUnit, CheckedSemanticSelections,
    SemanticSelection, StorageAccessPurpose, StorageCleanupType, StorageIdentity,
    StorageIdentityId, StoragePlan,
};
use bray_symbols::BorrowKind;

/// Proves capture-free cleanup for stable owners of ordinary, unstarted async calls.
pub(super) fn cleanup_free_futures(
    unit: &BoundUnit,
    storage: &StoragePlan,
    selections: &CheckedSemanticSelections,
    cleanup_types: &[StorageCleanupType],
) -> BTreeSet<StorageIdentityId> {
    let cleanup_free = cleanup_types
        .iter()
        .filter(|shape| shape.cleanup() == AsyncStorageCleanupRequirement::None)
        .map(StorageCleanupType::ty)
        .collect::<BTreeSet<_>>();

    let calls = selections
        .entries()
        .iter()
        .filter_map(|entry| {
            let SemanticSelection::Call(call) = entry.selection() else {
                return None;
            };

            // An indirect target does not identify whether it invokes a runtime hook. Such hooks can
            // own a terminal result before their observation future starts. Bray lambdas are capture-free.
            (matches!(call.resolution().result(), BoundCallResult::LazyFuture(_))
                && matches!(
                    call.resolution().target(),
                    BoundCallableTarget::Declaration(_) | BoundCallableTarget::Anonymous(_)
                )
                && call.implementation_hook().is_none()
                && call.receiver().is_none_or(|receiver| match receiver.mode() {
                    bray_symbols::ReceiverMode::Shared | bray_symbols::ReceiverMode::Mutable => true,
                    bray_symbols::ReceiverMode::Consuming | bray_symbols::ReceiverMode::ConsumingMutable => {
                        cleanup_free.contains(&receiver.target_type())
                    }
                })
                // Receiver target types describe the referent. Borrowed receivers capture only
                // their capability, while explicit input types already include their borrow layer.
                && call.input_types().skip(usize::from(call.receiver().is_some()))
                    .all(|ty| cleanup_free.contains(&ty)))
            .then_some((entry.expression(), call.resolution().result().ty()))
        })
        .collect::<BTreeMap<_, _>>();

    let future_types = calls.values().copied().collect::<BTreeSet<_>>();
    let mut producers = stable_producers(unit, storage);

    producers.retain(|identity, _| {
        storage
            .storage_type(*identity)
            .is_some_and(|ty| future_types.contains(&ty))
    });

    let sources = storage
        .access_plans()
        .iter()
        .filter(|plan| {
            matches!(
                plan.purpose(),
                StorageAccessPurpose::Read
                    | StorageAccessPurpose::Copy
                    | StorageAccessPurpose::Move
                    | StorageAccessPurpose::ValueTransfer
            )
        })
        .filter_map(|plan| {
            storage
                .is_root_access(plan.access())
                .then(|| {
                    storage
                        .root_identity(plan.access())
                        .map(|identity| (plan.expression(), identity))
                })
                .flatten()
        })
        .collect::<BTreeMap<_, _>>();

    let mut proven = BTreeSet::new();
    let mut visited = BTreeSet::new();

    for identity in producers.keys().copied() {
        let mut current = identity;
        let mut chain = BTreeSet::new();

        while chain.insert(current) {
            let Some(expression) = producers.get(&current).copied() else {
                break;
            };

            if proven.contains(&current) || calls.contains_key(&expression) {
                proven.extend(chain.iter().copied());
                break;
            }

            if visited.contains(&current) {
                break;
            }

            if !matches!(
                unit.tree().expression(expression),
                Some(BoundExpression::Name(_) | BoundExpression::PatternReference(_))
            ) {
                break;
            }

            let Some(source) = sources.get(&expression).copied() else {
                break;
            };

            current = source;
        }

        visited.extend(chain);
    }

    proven
}

fn stable_producers(
    unit: &BoundUnit,
    storage: &StoragePlan,
) -> BTreeMap<StorageIdentityId, BoundExpressionId> {
    let initializers = unit.collect_local_initializers();

    let mut producers = storage
        .identity_entries()
        .filter_map(|(identity, origin)| match origin {
            StorageIdentity::Temporary(expression) => Some((identity, expression)),
            StorageIdentity::LocalOwned(AnyBoundNodeId::Pattern(pattern)) => {
                let pattern = unit.tree().pattern(pattern)?;

                if pattern.kind() != BoundPatternKind::Binding {
                    return None;
                }

                let [binding] = pattern.bindings() else {
                    return None;
                };

                initializers
                    .get(&(*binding).into())
                    .map(|expression| (identity, *expression))
            }
            _ => None,
        })
        .collect::<BTreeMap<_, _>>();

    // A whole-unit certificate must remain valid through every mutation and exposed mutable alias.
    for plan in storage.access_plans().iter().filter(|plan| {
        matches!(
            plan.purpose(),
            StorageAccessPurpose::Write
                | StorageAccessPurpose::Assignment
                | StorageAccessPurpose::Borrow(BorrowKind::Mutable)
        )
    }) {
        if let Some(identity) = storage.root_identity(plan.access()) {
            producers.remove(&identity);
        }
    }

    producers
}
