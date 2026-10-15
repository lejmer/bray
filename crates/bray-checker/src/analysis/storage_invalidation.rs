use crate::{CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{
    BoundExecutionSite, CheckedExpressionSemantics, SelectedCall, SemanticSelection,
    StorageAccessPurpose, StorageInvalidation, StoragePlan,
};
use bray_symbols::{ExecutionProperty, TypeData};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn invalidating_operation_accesses<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expressions: &CheckedExpressionSemantics,
    storage: &StoragePlan,
    memory: &bray_bound_tree::CheckedMemoryOperations,
    copied_types: &BTreeSet<bray_symbols::TypeId>,
) -> BTreeMap<BoundExecutionSite, StorageInvalidation> {
    let selections = expressions.selections();
    let mut accesses = BTreeMap::new();

    for plan in storage.access_plans().iter().filter(|plan| {
        if plan.purpose() == StorageAccessPurpose::ValueTransfer
            && storage.access(plan.access()).is_some_and(|access| {
                matches!(
                    request
                        .semantic_values()
                        .type_data(access.reached_type())
                        .as_ref(),
                    TypeData::Borrow { .. }
                ) || copied_types.contains(&access.reached_type())
            })
        {
            return false;
        }

        access_invalidates_refinements(plan.purpose())
    }) {
        let invalidation = accesses
            .entry(plan.occurrence())
            .or_insert_with(|| StorageInvalidation::Paths(Vec::new()));

        if let StorageInvalidation::Paths(changed) = invalidation {
            if let Some(path) = storage.logical_path(plan.access()) {
                changed.push(path.clone());
            } else {
                *invalidation = StorageInvalidation::unknown();
            }
        }
    }

    for (expression, node) in request.unit().tree().expressions() {
        if let Some(SemanticSelection::ScopedUse(scoped)) = selections.expression(expression) {
            for occurrence in [
                BoundExecutionSite::ScopedEnter(expression),
                BoundExecutionSite::ScopedExit(expression),
            ] {
                let signature = scoped.invocation(occurrence).1;

                let data = request
                    .semantic_values()
                    .type_data(signature.callable_type());

                let TypeData::Callable(callable) = data.as_ref() else {
                    panic!("scoped invocation retains its selected callable type");
                };

                if !callable
                    .phase_behaviors()
                    .invocation()
                    .execution_properties()
                    .contains(&ExecutionProperty::Pure)
                {
                    accesses.insert(occurrence, StorageInvalidation::unknown());
                }
            }

            continue;
        }

        let Some(SemanticSelection::Call(call)) = selections.expression(expression) else {
            continue;
        };

        let constant = matches!(node, bray_bound_tree::BoundExpression::Call(bound)
            if expressions.types().expression(bound.callee()).is_some_and(|entry|
                matches!(request.semantic_values().type_data(entry.ty()).as_ref(), TypeData::Callable(callable)
                    if callable.constness() == bray_symbols::CallableConstness::Constant)));

        if constant
            || call_preserves_storage(
                call,
                memory
                    .operation(expression)
                    .map(|operation| operation.kind()),
            )
            || !matches!(
                call.resolution().result(),
                bray_bound_tree::BoundCallResult::Immediate(_)
            )
        {
            accesses.remove(&expression.into());
            continue;
        }

        // Opaque calls may carry mutation authority inside aggregates or raw pointers.
        // Without a purity certificate or a mutation summary, require fresh observations.
        accesses.insert(expression.into(), StorageInvalidation::unknown());
    }

    for invalidation in accesses.values_mut() {
        let StorageInvalidation::Paths(changed) = invalidation else {
            continue;
        };

        let aliases_storage = changed.iter().any(|path| {
            matches!(
                storage.identity(path.root),
                Some(bray_bound_tree::StorageIdentity::LocalOwned(_))
            ) && storage.identity_type(path.root).is_none_or(|ty| {
                matches!(
                    request.semantic_values().type_data(ty).as_ref(),
                    TypeData::Borrow { .. }
                )
            })
        });

        if aliases_storage {
            // Local borrows can be reassigned or returned by calls. Until their referent
            // sets are available here, a write through one may affect any observation.
            *invalidation = StorageInvalidation::unknown();

            continue;
        }

        changed.sort_unstable();
        changed.dedup();
    }

    accesses
}

pub(in crate::analysis) fn call_preserves_storage(
    call: &SelectedCall,
    operation: Option<bray_bound_tree::CheckedMemoryOperationKind>,
) -> bool {
    if let Some(operation) = operation {
        return crate::memory::execution::storage_effect(operation)
            == crate::memory::execution::StorageEffect::None;
    }

    call.phase_behaviors()
        .invocation()
        .execution_properties()
        .contains(&ExecutionProperty::Pure)
        || matches!(
            call.implementation_hook(),
            Some(
                bray_compiler_known::ImplementationHook::SequenceLength
                    | bray_compiler_known::ImplementationHook::SequenceIsEmpty
                    | bray_compiler_known::ImplementationHook::NumericTruncate
            )
        )
}

const fn access_invalidates_refinements(purpose: StorageAccessPurpose) -> bool {
    // Write checks the destination while evaluating its address. Assignment invalidates
    // the old value after the right-hand side and replacement cleanup have run.
    matches!(
        purpose,
        StorageAccessPurpose::Initialize
            | StorageAccessPurpose::Move
            | StorageAccessPurpose::ValueTransfer
            | StorageAccessPurpose::Assignment
    )
}
