use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use crate::{CheckerRequestContext, CheckerUnitView};
use crate::execution_guarantees::ExecutionProof;
use bray_bound_tree::{BoundExpressionId, StoragePlan};
use std::collections::BTreeSet;

pub(super) struct WitnessTransferFailure {
    pub(super) kind: bray_diagnostics::DiagnosticWitnessTransferKind,
    pub(super) carrier: crate::ExecutionCondition,
    pub(super) guarantee: crate::ExecutionCondition,
}

pub(super) fn copied_witness(state: &ExecutionState, value: crate::ExecutionCondition) -> WitnessTransferFailure {
    let (carrier, witness) = state.owned_witnesses([value]).find(|(carrier, _)|
        state.directly_carries_witness(carrier))
        .expect("a rejected copy retains its live owning carrier");

    let guarantee = witness.owning_conditions(&state.trusted_assumptions, &state.assumptions).next()
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

    let mut deferred = domain
        .awaited_invocations(state, expression)
        .collect::<Vec<_>>();

    if matches!(domain.semantics.selections().expression(expression), Some(bray_bound_tree::SemanticSelection::Call(call)) if call.implementation_hook() == Some(bray_compiler_known::ImplementationHook::FutureStart))
    {
        // Starting a task activates its body even when the task is later cancelled or disposed.
        let frame = match operation {
            AnalysisOperationKind::Bound(_) => {
                let Some(bray_bound_tree::SemanticSelection::Call(call)) =
                    domain.semantics.selections().expression(expression) else {
                    unreachable!("selected FutureStart retains its call");
                };

                // Before production the existing receiver owns this frame. At
                // Completion the actual Task result owns the rebound frame.
                domain.receiver_value(state, expression, call)
            }
            _ => domain.value(state, expression),
        };

        deferred.extend(
            state
                .deferred_entries(&frame)
                .into_iter()
                .flat_map(|value| value.producers.iter())
                .filter(|(_, candidate)| {
                    super::super::deferred::possible(&candidate.selection, &state.proofs.ordinary_proof(&state.assumptions))
                })
                .map(|(source, candidate)| (*source, &candidate.entry)),
        );
    }

    let entries = if deferred.is_empty() {
        state
            .entries
            .get(&occurrence)
            .map(|entry| (occurrence, entry))
            .into_iter()
            .collect::<Vec<_>>()
    } else {
        deferred
            .into_iter()
            .map(|(source, entry)| (source, entry.as_ref()))
            .collect()
    };

    for (invocation, entry) in entries {
        let Some(contract) = domain.dependencies.calls().get(&invocation) else {
            continue;
        };

        if invocation == occurrence && contract.deferred_effects().is_some() {
            continue;
        }

        for (transfer, cases) in contract.owned_result().guarded_transfers() {
            let crate::ExecutionCondition::Input(input) =
                crate::execution_condition_from_type_term(
                    domain.request.semantic_values(),
                    transfer.input(),
                    contract.input_count(),
                )
                .expect("checked owned input decodes in the same semantic store")
            else {
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
                let container = reached_container(&value);
                let witness = state.witnesses.get(container.owner_identity());

                if witness.is_some_and(|witness| {
                        state.directly_carries_witness(&container)
                            || witness.owning_conditions(&entry.trusted_assumptions, &entry.assumptions).next().is_some()
                    })
                    && input.projections[index..].iter().any(|projection| {
                        *projection != bray_symbols::ConstantProjectionKind::NullableValue
                    })
                    && !independent_result_owner(domain, invocation, entry, transfer,
                        &input, witness.expect("a partial witness retains its owning contract"))
                {
                    let condition =
                        cases
                            .into_iter()
                            .copied()
                            .chain(transfer.condition())
                            .map(|condition| {
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
                            })
                            .reduce(|left, right| {
                                crate::ExecutionCondition::operation(
                                    bray_bound_tree::BoundOperator::LogicalAnd,
                                    vec![left, right],
                                )
                            });

                    let witness = witness.expect("a rejected extraction retains its carrier contract");

                    let guarantee = witness.owning_conditions(&entry.trusted_assumptions, &entry.assumptions).next()
                        .or_else(|| witness.owning_conditions(&state.trusted_assumptions, &state.assumptions).next())
                        .expect("a rejected extraction retains its live owning condition").clone();

                    transfers.push((occurrence, condition, WitnessTransferFailure {
                        kind: bray_diagnostics::DiagnosticWitnessTransferKind::Separation,
                        carrier: container,
                        guarantee,
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
    invocation: bray_bound_tree::BoundExecutionSite,
    entry: &crate::ExecutionCallEvidence,
    transfer: &bray_symbols::DependencyOwnedTransfer,
    input: &crate::ExecutionPlace,
    parent: &crate::execution_guarantees::ExecutionWitness,
) -> bool {
    let capture = |condition: &crate::ExecutionCondition| condition.capture_entry(
        &|place| place.value_in(&entry.arguments).unwrap_or(crate::ExecutionCondition::Unknown),
        &entry.trusted_assumptions, &entry.assumptions,
        &mut { crate::ExecutionCondition::WORK_LIMIT },
    );

    let ordinary = crate::TrustedCallContract::default();

    let declared = domain.request.trusted_contracts()
        .and_then(|contracts| contracts.calls.get(&invocation)).unwrap_or(&ordinary);

    // Use the same checked carrier selection as completion. An independently
    // introduced result owner may receive a component, but cannot transport the
    // parent's authority through that component.
    let (_, subjects) = domain.completion_guarantees(invocation, declared, &capture);

    let path = crate::storage::owned_storage_projections(domain.dependencies, transfer.output());
    let output = path.iter().copied().fold(crate::ExecutionCondition::Result, |value, projection| value.project(projection));

    if !subjects.iter().any(|(_, subject)| subject.observes_completion_result()
        && output.is_owned_projection_of(subject)) {
        return false;
    }

    let Some(source) = input.value_in(&entry.arguments) else { return false; };

    let result = crate::ExecutionCondition::Result.with_component(transfer.output(), source, Some(&path));
    let mut potential = BTreeSet::new();

    for (guarantee, subject) in &subjects {
        if !subject.observes_completion_result() { continue; }

        let guarantee = guarantee.substitute(
            &|place| place.value_in(&entry.arguments).unwrap_or(crate::ExecutionCondition::Unknown),
            &result, &mut { crate::ExecutionCondition::WORK_LIMIT },
        );

        // Ignore ordinary guards when checking which trusted authority could
        // escape. A later branch can establish a guard after this transfer.
        for claim in guarantee.observations().filter(|claim| matches!(claim, crate::ExecutionCondition::Trusted(_))) {
            claim.clone().assume(true, &mut potential);
        }
    }

    let proof = ExecutionProof::new(&potential, &entry.assumptions);

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
    transfers: &mut std::collections::BTreeMap<BoundExpressionId, WitnessTransferFailure>,
) {
    for plan in plans.filter(|plan| {
        matches!(
            plan.purpose(),
            bray_bound_tree::StorageAccessPurpose::Copy
                | bray_bound_tree::StorageAccessPurpose::ValueTransfer
                | bray_bound_tree::StorageAccessPurpose::Move
        )
    }) {
        let path = current_access(plan);

        let materializes = storage.access_plan_role(plan) == bray_bound_tree::StorageAccessPlanRole::SourceMaterialization;

        let value = if materializes {
            path.as_ref().map(|path| domain.current_access_value(state, path))
                .unwrap_or(crate::ExecutionCondition::Unknown)
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

        let failure = if is_witness && copies && materializes {
            copied_witness(state, value)
        } else if !copies && let Some(carrier) = partial_witness {
            let guarantee = state.witnesses.get(carrier.owner_identity())
                .and_then(|witness| witness.owning_conditions(&state.trusted_assumptions, &state.assumptions).next())
                .expect("a rejected transfer retains its live owning condition").clone();

            WitnessTransferFailure { kind: bray_diagnostics::DiagnosticWitnessTransferKind::Separation,
                carrier, guarantee }
        } else {
            continue;
        };

        transfers.entry(plan.expression()).or_insert(failure);
    }
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
