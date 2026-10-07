use crate::{CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{
    AnyBoundNodeId, CheckedSemanticSelections, SemanticSelection, StorageAccessId,
    StorageAccessPurpose, StoragePlan, StorageRelationship,
};
use bray_symbols::{ExecutionProperty, TypeData};
use std::collections::{BTreeMap, BTreeSet};

pub(super) enum StorageInvalidation {
    All,
    Accesses(Vec<StorageAccessId>),
}

impl StorageInvalidation {
    pub(super) fn invalidates(&self, access: StorageAccessId, storage: &StoragePlan) -> bool {
        match self {
            Self::All => true,
            Self::Accesses(mutations) => mutations.iter().any(|mutation| {
                storage.relationship(access, *mutation) != StorageRelationship::Disjoint
            }),
        }
    }
}

pub(super) fn invalidating_operation_accesses<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
    copied_types: &BTreeSet<bray_symbols::TypeId>,
) -> BTreeMap<AnyBoundNodeId, StorageInvalidation> {
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
            .entry(plan.node())
            .or_insert_with(|| StorageInvalidation::Accesses(Vec::new()));

        if let StorageInvalidation::Accesses(changed) = invalidation {
            changed.push(plan.access());
        }
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
            || call.implementation_hook() == Some(bray_compiler_known::ImplementationHook::RawPointerReinterpret)
        {
            continue;
        }

        // Opaque calls may carry mutation authority inside aggregates or raw pointers.
        // Without a purity certificate or a mutation summary, require fresh observations.
        accesses.insert(expression.into(), StorageInvalidation::All);
    }

    for invalidation in accesses.values_mut() {
        let StorageInvalidation::Accesses(changed) = invalidation else {
            continue;
        };

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
            *invalidation = StorageInvalidation::All;

            continue;
        }

        changed.sort_unstable();
        changed.dedup();
    }

    accesses
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
