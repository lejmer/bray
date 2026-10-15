use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use crate::{CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{BoundExpressionId, StoragePlan};
use std::collections::BTreeSet;

pub(super) fn check_witness_transfers<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    domain: &super::super::flow::ExecutionFlowDomain<'_, '_, C>,
    state: &ExecutionState,
    occurrence: bray_bound_tree::BoundExecutionSite,
    transfers: &mut BTreeSet<BoundExpressionId>,
) {
    for plan in storage.occurrence_plans(occurrence).filter(|plan| {
        matches!(
            plan.purpose(),
            bray_bound_tree::StorageAccessPurpose::Copy
                | bray_bound_tree::StorageAccessPurpose::ValueTransfer
                | bray_bound_tree::StorageAccessPurpose::Move
        )
    }) {
        let value = domain.storage_value(state, plan.expression(), plan.access());

        let fresh = plan.purpose() == bray_bound_tree::StorageAccessPurpose::ValueTransfer
            && storage
                .root_identity(plan.access())
                .and_then(|identity| storage.identity(identity))
                .is_some_and(|identity| {
                    matches!(
                        identity,
                        bray_bound_tree::StorageIdentity::Temporary(_)
                            | bray_bound_tree::StorageIdentity::Result(_)
                            | bray_bound_tree::StorageIdentity::Allocation(_)
                    )
                });

        let (is_witness, mut partial_witness) = state.witness_projection(&value);

        partial_witness |= separates_carrier(domain, state, plan.access());

        if !is_witness && !partial_witness {
            continue;
        }

        let ty = storage
            .access(plan.access())
            .expect("checked access plan references existing storage")
            .reached_type();

        if matches!(
            request.semantic_values().type_data(ty).as_ref(),
            bray_symbols::TypeData::Borrow { .. }
        ) {
            continue;
        }

        let copies = storage.effective_purpose(plan) == bray_bound_tree::StorageAccessPurpose::Copy;

        if (is_witness && copies && !fresh) || (partial_witness && !copies) {
            transfers.insert(plan.expression());
        }
    }
}

fn separates_carrier<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>,
    state: &ExecutionState,
    access: bray_bound_tree::StorageAccessId,
) -> bool {
    let Some(path) = domain.storage.resolved_projections(access) else {
        return false;
    };

    if path.is_empty() {
        return false;
    }

    let root = domain
        .storage
        .root_identity(access)
        .expect("resolved access retains its root");

    let mut value = domain.storage_identity_value(state, root);

    for (index, projection) in path.iter().enumerate() {
        // Unwrapping presence moves the complete contained carrier. Other ownership
        // projections must not detach any component of a directly carrying value.
        if state.directly_carries_witness(&value)
            && path[index..]
                .iter()
                .any(|projection| *projection != bray_bound_tree::StorageProjection::NullableValue)
        {
            return true;
        }

        value = value.project(*projection);
    }

    false
}
