use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundExpressionId, BoundPatternId, BoundUnitView, StorageBinding,
    StorageBindingTarget, StoragePlan, StorageScopeOwners,
};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

pub(crate) fn storage_scope_owners<C>(
    request: CheckerUnitView<'_, C>,
) -> Result<StorageScopeOwners, CheckerQueryError>
where
    C: CheckerRequestContext + ?Sized,
{
    Ok(
        StorageScopeOwners::collect(request.unit()).unwrap_or_else(|error| {
            panic!("storage_scope_owners must satisfy its checked construction contract: {error:?}")
        }),
    )
}
pub(crate) fn value_transfer_destinations<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> BTreeMap<bray_bound_tree::BoundExecutionSite, Vec<bray_bound_tree::StorageIdentityId>>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut result = BTreeMap::new();

    for (pattern, source) in request.unit().tree().binding_subjects() {
        let source = request
            .view()
            .expression(source)
            .expect("pattern subject retains its source expression")
            .pattern_subject_site(source);

        extend_pattern_bindings(
            request.view(),
            storage,
            pattern,
            result.entry(source).or_default(),
        );
    }

    result
}

/// A view names existing storage and does not initialize its containing owner.
fn owned_binding(storage: &StoragePlan, target: StorageBindingTarget) -> Option<bray_bound_tree::StorageIdentityId> {
    match storage.binding(target)? {
        StorageBinding::Identity(identity) => Some(identity),
        StorageBinding::Access(_) => None,
    }
}

fn extend_pattern_bindings(
    view: BoundUnitView<'_>,
    storage: &StoragePlan,
    root: BoundPatternId,
    bindings: &mut Vec<bray_bound_tree::StorageIdentityId>,
) {
    let mut pending = vec![root];

    while let Some(pattern) = pending.pop() {
        let id = pattern;

        let Some(pattern) = view.pattern(id) else {
            continue;
        };

        if pattern.kind() == bray_bound_tree::BoundPatternKind::Discard {
            bindings.extend(owned_binding(storage, StorageBindingTarget::PatternDiscard(id)));
        }

        bindings.extend(
            pattern
                .bindings()
                .iter()
                .copied()
                .chain(pattern.entries().iter().filter_map(|entry| entry.binding()))
                .filter_map(|binding| owned_binding(storage, StorageBindingTarget::Local(binding))),
        );

        pending.extend(pattern.children().iter().copied());
    }

    bindings.sort_unstable();
    bindings.dedup();
}

pub(crate) fn pattern_binding_sources<'a>(
    view: BoundUnitView<'a>,
    storage: &'a StoragePlan,
    pattern: BoundPatternId,
) -> impl Iterator<
    Item = (
        StorageBindingTarget,
        BoundExpressionId,
        bray_bound_tree::StorageAccessId,
    ),
> + 'a {
    let bound = view.pattern(pattern).expect("checked pattern must exist");

    let expression = storage
        .node_plans(pattern.into())
        .next()
        .map(|plan| plan.expression());

    let mut plans = storage.node_plans(pattern.into()).filter(|plan| {
        !matches!(
            plan.purpose(),
            bray_bound_tree::StorageAccessPurpose::Projection
                | bray_bound_tree::StorageAccessPurpose::Initialize
        )
    });

    bound
        .bindings()
        .iter()
        .copied()
        .chain(bound.entries().iter().filter_map(|entry| entry.binding()))
        .map(StorageBindingTarget::Local)
        .chain(
            (bound.kind() == bray_bound_tree::BoundPatternKind::Discard)
                .then_some(StorageBindingTarget::PatternDiscard(pattern)),
        )
        .filter_map(move |binding| {
            let (expression, access) = match storage.binding(binding) {
                Some(StorageBinding::Access(access)) => (expression?, access),
                Some(StorageBinding::Identity(_)) => {
                    let plan = plans.next()?;

                    (plan.expression(), plan.access())
                }
                None => return None,
            };

            Some((binding, expression, access))
        })
}
