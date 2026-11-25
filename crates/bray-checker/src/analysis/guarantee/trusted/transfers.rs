use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use crate::{CheckerRequestContext, CheckerUnitView};
use crate::execution_guarantees::ExecutionAssumptions;
use bray_bound_tree::{BoundExpressionId, StoragePlan};

pub(super) struct WitnessTransferFailure {
    pub(super) kind: bray_diagnostics::DiagnosticWitnessTransferKind,
    pub(super) carrier: crate::ExecutionCondition,
    pub(super) guarantee: crate::ExecutionCondition,
}

pub(super) fn copied_witness(state: &ExecutionState, value: crate::ExecutionCondition) -> WitnessTransferFailure {
    let (carrier, witness) = state.owned_witnesses([value]).find(|(carrier, _)|
        state.directly_carries_witness(carrier))
        .expect("a rejected copy retains its live owning carrier");

    let guarantee = witness.owning_conditions(state.proof()).next()
        .expect("a rejected copy retains its live owning condition").clone();

    WitnessTransferFailure {
        kind: bray_diagnostics::DiagnosticWitnessTransferKind::Copy,
        carrier: carrier.clone(), guarantee,
    }
}

/// Capture potential partial extractions before consumption retires their carrier evidence.
pub(super) fn collect_returned_witness_transfers<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>,
    state: &ExecutionState,
    operation: crate::analysis::model::AnalysisOperationKind,
    transfers: &mut Vec<(
        bray_bound_tree::BoundExecutionSite,
        Option<crate::ExecutionCondition>,
        WitnessTransferFailure,
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

    let mut frame = match domain.request.view().expression(expression) {
        Some(bray_bound_tree::BoundExpression::Await(_)) =>
            state.admitted_receiver(occurrence).deferred_value().cloned(),
        _ => None,
    };

    if matches!(domain.semantics.selections().expression(expression), Some(bray_bound_tree::SemanticSelection::Call(call)) if call.implementation_hook() == Some(bray_compiler_known::ImplementationHook::FutureStart))
    {
        // Starting a task activates its body even when the task is later cancelled or disposed.
        let value = match operation {
            AnalysisOperationKind::Bound(_) => domain.started_future(state, expression)
                .unwrap_or_else(crate::ExecutionCondition::unknown),
            _ => domain.value(state, expression),
        };

        frame = value.deferred_value().cloned();
    }

    if let Some(frame) = frame {
        for (source, candidate) in &frame.producers {
            if !super::super::deferred::possible(&candidate.selection, &state.ordinary_proof()) { continue; }

            collect_entry_transfers(domain, state, occurrence, source.local(), &candidate.entry, transfers);
        }
    } else if let Some(contract) = domain.dependencies.calls().get(&occurrence)
        && contract.deferred_effects().is_none()
        && let Some(entry) = state.entries.get(&occurrence)
    {
        collect_entry_transfers(domain, state, occurrence, Some(occurrence), entry, transfers);
    }
}

fn collect_entry_transfers<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>, state: &ExecutionState,
    occurrence: bray_bound_tree::BoundExecutionSite, source: Option<bray_bound_tree::BoundExecutionSite>,
    entry: &crate::ExecutionCallEvidence,
    transfers: &mut Vec<(bray_bound_tree::BoundExecutionSite, Option<crate::ExecutionCondition>, WitnessTransferFailure)>,
) {
    if entry.witness_inputs.is_empty() { return; }

    let input_count = entry.positional_arguments().count();
    let admission = || domain.proofs.proof(&entry.trusted_assumptions, &entry.assumptions);

    let bind = |term| {
        let formal = crate::execution_condition_from_type_term(domain.request.semantic_values(), term, input_count)
            .expect("checked ownership condition belongs to its selected input scope");

        entry.bind_condition(&formal, &crate::ExecutionCondition::result())
    };

    let assumptions = state.assumptions.joined_with(&entry.assumptions);
    let proof = state.proofs.ordinary_proof(&assumptions);

    for row in entry.outputs.reachable_rows() {
        let row_guard = row.guard.bind(entry, &crate::ExecutionCondition::result());

        if !proof.can_assume(&row_guard, true) { continue; }

        let ownership = &row.ownership;
        let result = row.writes.get(&bray_bound_tree::StorageValueDestination::Result);

        let decode_input = |term| {
            let decoded = crate::execution_condition_from_type_term(
                domain.request.semantic_values(), term, input_count,
            ).expect("checked owned input decodes in the same semantic store");

            let crate::ExecutionConditionKind::Input(input) = decoded.kind() else {
                unreachable!("checked ownership source contains only input projections");
            };

            // The transfer walk retains the formal path after its decoded condition is released.
            input.clone()
        };

        for (transfer, cases) in ownership.guarded_transfers() {
            let input = decode_input(transfer.input());
            let mut enclosing = None;

            let root = crate::ExecutionPlace {
                root: input.root.clone(),
                projections: std::sync::Arc::new([]),
            };

            let mut value = root
                .value_in(&entry.arguments)
                .unwrap_or(crate::ExecutionCondition::unknown());

            for (index, projection) in input.projections.iter().enumerate() {
                let container = reached_container(&value);
                let witness = entry.witness_inputs.get(container.owner_identity());

                if let Some(witness) = witness
                    && let Some(guarantee) = witness.owning_conditions(admission()).next()
                    && input.projections[index..].iter().any(|projection| {
                        *projection != bray_symbols::ConstantProjectionKind::NullableValue
                    })
                    && !{
                        // Only moves that occur together with this transfer can enclose it.
                        let complete = enclosing.get_or_insert_with(|| {
                            let conditions = cases.iter().copied().copied().chain(transfer.condition()).collect::<Vec<_>>();

                            ownership.transfers_with(transfer, &conditions).into_iter()
                                .map(|transfer| (decode_input(transfer.input()), transfer)).collect::<Vec<_>>()
                        });

                        complete.iter().any(|(owner, returned)| {
                            // A child annotation within the same complete returned carrier is not separation.
                            owner.contains(&input) && owner.projections.len() <= index
                                && transfer.output().strip_prefix(returned.output())
                                    == Some(&input.projections[owner.projections.len()..])
                        })
                    }
                    && !independent_result_owner(domain, source, entry, &row.carriers, transfer,
                        result, witness)
                {
                    let condition =
                        cases
                            .into_iter()
                            .copied()
                            .chain(transfer.condition())
                            .map(&bind)
                            .chain(std::iter::once(row_guard.clone()))
                            .reduce(|left, right| {
                                crate::ExecutionCondition::operation(
                                    bray_bound_tree::BoundOperator::LogicalAnd,
                                    vec![left, right],
                                )
                            });

                    if condition.as_ref().is_some_and(|condition| !proof.can_assume(condition, true)) {
                        break;
                    }

                    transfers.push((occurrence, condition, WitnessTransferFailure {
                        kind: bray_diagnostics::DiagnosticWitnessTransferKind::Separation,
                        carrier: container,
                        guarantee: guarantee.clone(),
                    }));

                    break;
                }

                value = value.component(*projection);
            }
        }
    }
}

pub(super) fn independent_result_owner<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>,
    invocation: Option<bray_bound_tree::BoundExecutionSite>,
    entry: &crate::ExecutionCallEvidence, carriers: &[bray_symbols::DependencyWitnessCarrier<bray_bound_tree::StorageProjection>],
    transfer: &bray_symbols::DependencyOwnedTransfer,
    result: Option<&crate::ExecutionCondition>,
    parent: &crate::execution_guarantees::ExecutionWitness,
) -> bool {
    let ordinary = crate::TrustedCallContract::default();

    let declared = domain.request.trusted_contracts()
        .and_then(|contracts| contracts.calls.get(&invocation?)).unwrap_or(&ordinary);

    // A possible new carrier cannot justify moving a component of a complete owner.
    let subjects = domain.possible_completion_carriers(entry, carriers, declared);

    let path = crate::storage::owned_storage_projections(domain.dependencies, transfer.output());
    let output = path.iter().copied().fold(crate::ExecutionCondition::result(), |value, projection| value.project(projection));

    if !subjects.iter().any(|(_, subject)| subject.observes_completion_result()
        && output.is_owned_projection_of(subject)) {
        return false;
    }

    let Some(result) = result else { return false; };

    let mut potential = ExecutionAssumptions::default();

    for (guarantee, subject) in &subjects {
        if !subject.observes_completion_result() { continue; }

        let guarantee = entry.bind_condition(guarantee, result);

        // Ignore ordinary guards when checking which trusted authority could
        // escape. A later branch can establish a guard after this transfer.
        for claim in guarantee.observations().filter(|claim| matches!(claim.kind(), crate::ExecutionConditionKind::Trusted(_))) {
            claim.clone().assume(true, &mut potential);
        }
    }

    let proof = domain.proofs.proof(&potential, &entry.assumptions);

    !potential.is_empty() && !parent.conditions.iter().any(|(condition, guarantee)| {
        guarantee.witness
            && condition.prove_ordinary(&entry.assumptions, Some(&domain.proofs)) != Some(true)
            && proof.prove_trusted(condition) == Some(true)
    })
}

pub(super) fn check_witness_transfers<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    domain: &super::super::flow::ExecutionFlowDomain<'_, '_, C>,
    state: &ExecutionState,
    plans: impl Iterator<Item = bray_bound_tree::StorageAccessPlan>,
    current_access: &impl Fn(bray_bound_tree::StorageAccessPlan) -> Option<bray_bound_tree::StoragePath>,
    mut record: impl FnMut(BoundExpressionId, WitnessTransferFailure),
) -> std::collections::BTreeSet<BoundExpressionId> {
    let mut transferred = std::collections::BTreeSet::new();

    for plan in plans.filter(|plan| plan.purpose().transfers_value()) {
        let path = current_access(plan);

        let materializes = storage.access_plan_role(plan) == bray_bound_tree::StorageAccessPlanRole::SourceMaterialization;

        let value = if materializes {
            path.as_ref().map(|path| domain.current_access_value(state, path))
                .unwrap_or(crate::ExecutionCondition::unknown())
        } else {
            domain.storage_value(state, plan.expression(), plan.access())
        };

        let is_witness = state.owns_witness(&value);
        let partial_witness = path.as_ref().and_then(|path| separated_carrier(domain, state, path));

        if !is_witness && partial_witness.is_none() {
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

        if is_witness && !copies {
            transferred.insert(plan.expression());
        }

        let failure = if is_witness && copies && materializes {
            copied_witness(state, value)
        } else if !is_witness && !copies && let Some(carrier) = partial_witness {
            let guarantee = state.witnesses.get(carrier.owner_identity())
                .and_then(|witness| witness.owning_conditions(state.proof()).next())
                .expect("a rejected transfer retains its live owning condition").clone();

            WitnessTransferFailure { kind: bray_diagnostics::DiagnosticWitnessTransferKind::Separation,
                carrier, guarantee }
        } else {
            continue;
        };

        record(plan.expression(), failure);
    }

    transferred
}

fn separated_carrier<C: CheckerRequestContext + ?Sized>(
    domain: &ExecutionFlowDomain<'_, '_, C>,
    state: &ExecutionState,
    path: &bray_bound_tree::StoragePath,
) -> Option<crate::ExecutionCondition> {
    for index in 0..path.projections.len() {
        let container = bray_bound_tree::StoragePath {
            root: path.root,
            projections: path.projections[..index].into(),
        };

        let value = domain.current_access_value(state, &container);

        // Presence extraction retains the complete carrier. Other selectors
        // cross its checked current storage boundary, including through loans.
        let carrier = reached_container(&value);

        if state.directly_carries_witness(&carrier)
            && path.projections[index..].iter().any(|projection|
                *projection != bray_bound_tree::StorageProjection::NullableValue)
        {
            return Some(carrier);
        }
    }

    None
}

// A field projection crosses borrow layers into reached storage. The loan's
// own contract does not make that loan the owner of the reached aggregate.
fn reached_container(value: &crate::ExecutionCondition) -> crate::ExecutionCondition {
    value.clone().into_referent()
}
