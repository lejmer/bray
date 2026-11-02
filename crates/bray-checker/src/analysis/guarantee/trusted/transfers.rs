use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use crate::{CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{BoundExpressionId, StoragePlan};
use std::collections::BTreeSet;

/// Capture potential partial extractions before consumption retires their carrier evidence.
pub(super) fn collect_returned_witness_transfers<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>,
    state: &ExecutionState,
    operation: crate::analysis::model::AnalysisOperationKind,
    transfers: &mut Vec<(
        bray_bound_tree::BoundExecutionSite,
        Option<crate::ExecutionCondition>,
    )>,
) {
    use crate::analysis::model::{AnalysisCallPhase, AnalysisOperationKind};

    if !matches!(
        operation,
        AnalysisOperationKind::Call {
            phase: AnalysisCallPhase::Completion,
            ..
        } | AnalysisOperationKind::Bound(_)
    ) {
        return;
    }

    let occurrence = operation.occurrence();

    let Some(expression) = occurrence.expression() else {
        return;
    };

    let invocation = domain
        .awaited_invocation(state, expression)
        .unwrap_or(occurrence);

    let Some(contract) = domain.dependencies.calls().get(&invocation) else {
        return;
    };

    // Creating an inactive frame owns its inputs. Driving it performs the extraction.
    if invocation == occurrence && contract.deferred_effects().is_some() {
        return;
    }

    let Some(entry) = state.entries.get(&invocation) else {
        return;
    };

    for transfer in contract.owned_transfers() {
        let crate::ExecutionCondition::Input(input) = crate::execution_condition_from_type_term(
            domain.request.semantic_values(),
            transfer.input(),
            contract.input_count(),
        )
        .expect("checked owned input decodes in the same semantic store") else {
            unreachable!("checked ownership source contains only input projections");
        };

        let root = crate::ExecutionPlace {
            root: input.root.clone(),
            projections: std::sync::Arc::new([]),
        };

        let mut value = root
            .value_in(&entry.arguments)
            .unwrap_or(crate::ExecutionCondition::Unknown);

        for (index, projection) in input.projections.iter().enumerate() {
            if state.directly_carries_witness(&value)
                && input.projections[index..].iter().any(|projection| {
                    *projection != bray_symbols::ConstantProjectionKind::NullableValue
                })
            {
                let condition = transfer.condition().map(|condition| {
                    crate::execution_condition_from_type_term(
                        domain.request.semantic_values(),
                        condition,
                        contract.input_count(),
                    )
                    .expect("checked ownership condition decodes in the same semantic store")
                    .substitute(
                        &|place| {
                            place
                                .value_in(&entry.arguments)
                                .unwrap_or(crate::ExecutionCondition::Unknown)
                        },
                        &crate::ExecutionCondition::Result,
                        &mut { crate::ExecutionCondition::WORK_LIMIT },
                    )
                });

                transfers.push((occurrence, condition));

                break;
            }

            value = value.component(*projection);
        }
    }
}

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
                            | bray_bound_tree::StorageIdentity::InvocationInput { .. }
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
