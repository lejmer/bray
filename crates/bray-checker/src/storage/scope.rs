use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundBlockItem, BoundExpression, BoundExpressionId, BoundPatternId, BoundUnitView,
    StorageBinding, StorageBindingTarget, StorageIdentityId, StoragePlan, StorageScopeBuildError,
    StorageScopeOwners,
};

use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext,
    CheckerStorageFlowFailure, CheckerUnitView,
};

pub(crate) fn storage_scope_owners<C>(
    request: CheckerUnitView<'_, C>,
) -> Result<StorageScopeOwners, CheckerQueryError>
where
    C: CheckerRequestContext + ?Sized,
{
    StorageScopeOwners::collect(request.unit()).map_err(|error| {
        CheckerQueryError::Infrastructure(CheckerInfrastructureError::StorageFlow(match error {
            StorageScopeBuildError::UnbalancedScopes { open_scope } => {
                CheckerStorageFlowFailure::UnbalancedScopes { open_scope }
            }
            StorageScopeBuildError::MissingPattern { pattern } => {
                CheckerStorageFlowFailure::MissingPattern { pattern }
            }
        }))
    })
}
pub(crate) fn local_initialization_bindings<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> BTreeMap<BoundExpressionId, Vec<StorageBinding>>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut result = BTreeMap::new();

    for (_, block) in request.unit().tree().blocks() {
        for item in block.items() {
            let BoundBlockItem::LocalBinding(binding) = item else {
                continue;
            };

            let bindings = binding
                .bindings()
                .iter()
                .filter_map(|binding| storage.binding(StorageBindingTarget::Local(*binding)))
                .collect::<Vec<_>>();

            if !bindings.is_empty() {
                result.insert(binding.initializer(), bindings);
            }
        }
    }

    result
}

pub(crate) fn value_transfer_bindings<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> BTreeMap<BoundExpressionId, Vec<StorageBinding>>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut result = local_initialization_bindings(request, storage);

    for (id, expression) in request.unit().tree().expressions() {
        match expression {
            BoundExpression::Assignment(assignment)
                if assignment.operator().binary_operator().is_none() =>
            {
                if let [_, value] = assignment.operands() {
                    result.entry(*value).or_default().extend(
                        storage
                            .expression_plans(id)
                            .filter(|plan| {
                                plan.purpose() == bray_bound_tree::StorageAccessPurpose::Assignment
                            })
                            .map(|plan| StorageBinding::Access(plan.access())),
                    );
                }
            }
            BoundExpression::Match(expression) => {
                let bindings = result.entry(expression.subject()).or_default();

                for arm in expression.arms() {
                    extend_pattern_bindings(request.view(), storage, arm.pattern(), bindings);
                }
            }
            BoundExpression::Structured(test)
                if test.kind()
                    == bray_bound_tree::BoundStructuredExpressionKind::PatternBinding =>
            {
                if let Some(subject) = test.operands().first() {
                    let bindings = result.entry(*subject).or_default();

                    for pattern in test.patterns() {
                        extend_pattern_bindings(request.view(), storage, *pattern, bindings);
                    }
                }
            }
            _ => {}
        }
    }

    result
}

fn extend_pattern_bindings(
    view: BoundUnitView<'_>,
    storage: &StoragePlan,
    root: BoundPatternId,
    bindings: &mut Vec<StorageBinding>,
) {
    let mut pending = vec![root];

    while let Some(pattern) = pending.pop() {
        let Some(pattern) = view.pattern(pattern) else {
            continue;
        };

        bindings.extend(
            pattern
                .bindings()
                .iter()
                .filter_map(|binding| storage.binding(StorageBindingTarget::Local(*binding))),
        );

        pending.extend(pattern.children().iter().copied());
    }
}

pub(crate) fn local_initialization_destinations<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> BTreeMap<BoundExpressionId, StorageIdentityId>
where
    C: CheckerRequestContext + ?Sized,
{
    local_initialization_bindings(request, storage)
        .into_iter()
        .filter_map(|(initializer, bindings)| match bindings.as_slice() {
            [StorageBinding::Identity(destination)] => Some((initializer, *destination)),
            [StorageBinding::Access(_)] | [] | [_, _, ..] => None,
        })
        .collect()
}
