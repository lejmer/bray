use crate::{CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{
    AnyBoundNodeId, CheckedSemanticSelections, SemanticSelection, StorageAccessId,
    StorageAccessPurpose, StoragePlan,
};
use bray_symbols::{ExecutionProperty, TypeData};
use std::collections::BTreeMap;

pub(super) fn invalidating_operation_accesses<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
) -> BTreeMap<AnyBoundNodeId, Box<[StorageAccessId]>> {
    let mut accesses = BTreeMap::<AnyBoundNodeId, Vec<StorageAccessId>>::new();

    for plan in storage.access_plans().iter().filter(|plan| {
        if plan.purpose() == StorageAccessPurpose::ValueTransfer
            && storage.access(plan.access()).is_some_and(|access| {
                matches!(
                    request
                        .semantic_values()
                        .type_data(access.reached_type())
                        .as_ref(),
                    TypeData::Borrow { .. }
                )
            })
        {
            return false;
        }

        access_invalidates_refinements(plan.purpose())
    }) {
        accesses.entry(plan.node()).or_default().push(plan.access());
    }

    for (expression, _) in request.unit().tree().expressions() {
        let Some(SemanticSelection::Call(call)) = selections.expression(expression) else {
            continue;
        };

        if call
            .phase_behaviors()
            .invocation()
            .execution_properties()
            .contains(&ExecutionProperty::Pure)
        {
            continue;
        }

        // Opaque calls may carry mutation authority inside aggregates or raw pointers.
        // Without a purity certificate or a mutation summary, require fresh observations.
        accesses
            .entry(expression.into())
            .or_default()
            .extend(storage.access_entries().map(|(access, _)| access));
    }

    for changed in accesses.values_mut() {
        let aliases_storage = changed.iter().any(|access| {
            storage.root_identity(*access).is_some_and(|identity| {
                matches!(
                    storage.identity(identity),
                    Some(bray_bound_tree::StorageIdentity::LocalOwned(_))
                ) && storage.identity_type(identity).is_none_or(|ty| {
                    matches!(
                        request.semantic_values().type_data(ty).as_ref(),
                        TypeData::Borrow { .. }
                    )
                })
            })
        });

        if aliases_storage {
            // Local borrows can be reassigned or returned by calls. Until their referent
            // sets are available here, a write through one may affect any observation.
            changed.extend(storage.access_entries().map(|(access, _)| access));
        }

        changed.sort_unstable();
        changed.dedup();
    }

    accesses
        .into_iter()
        .map(|(expression, accesses)| (expression, accesses.into_boxed_slice()))
        .collect()
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
