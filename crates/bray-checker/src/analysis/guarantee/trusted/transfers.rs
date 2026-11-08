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
                    super::super::deferred::possible(&candidate.selection, &state.assumptions)
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
                            || witness.is_owning(&entry.trusted_assumptions, &entry.assumptions)
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

                    transfers.push((occurrence, condition));

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

    !potential.is_empty() && !parent.conditions.iter().any(|(condition, guarantee)| {
        guarantee.witness
            && condition.prove(&entry.assumptions, &mut { crate::ExecutionCondition::WORK_LIMIT }) != Some(true)
            && condition.prove_trusted(&potential, &entry.assumptions) == Some(true)
    })
}

pub(super) fn check_witness_transfers<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    domain: &super::super::flow::ExecutionFlowDomain<'_, '_, C>,
    state: &ExecutionState,
    operation: crate::analysis::model::AnalysisOperationKind,
    current_access: &impl Fn(bray_bound_tree::StorageAccessPlan) -> Option<bray_bound_tree::StoragePath>,
    transfers: &mut BTreeSet<BoundExpressionId>,
) {
    for plan in operation.storage_plans(storage).filter(|plan| {
        matches!(
            plan.purpose(),
            bray_bound_tree::StorageAccessPurpose::Copy
                | bray_bound_tree::StorageAccessPurpose::ValueTransfer
                | bray_bound_tree::StorageAccessPurpose::Move
        )
    }) {
        let path = current_access(plan);

        let value = if storage.access_plan_role(plan) == bray_bound_tree::StorageAccessPlanRole::SourceMaterialization {
            path.as_ref().map(|path| domain.current_access_value(state, path))
                .unwrap_or(crate::ExecutionCondition::Unknown)
        } else {
            domain.storage_value(state, plan.expression(), plan.access())
        };

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

        let is_witness = state.owns_witness(&value);
        let partial_witness = path.as_ref().is_some_and(|path| separates_carrier(domain, state, path));

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
    path: &bray_bound_tree::StoragePath,
) -> bool {
    for index in 0..path.projections.len() {
        let container = bray_bound_tree::StoragePath {
            root: path.root,
            projections: path.projections[..index].into(),
        };

        let value = domain.current_access_value(state, &container);

        // Presence extraction retains the complete carrier. Other selectors
        // cross its checked current storage boundary, including through loans.
        if state.directly_carries_witness(&reached_container(&value))
            && path.projections[index..].iter().any(|projection|
                *projection != bray_bound_tree::StorageProjection::NullableValue)
        {
            return true;
        }
    }

    false
}

// A field projection crosses borrow layers into reached storage. The loan's
// own contract does not make that loan the owner of the reached aggregate.
fn reached_container(value: &crate::ExecutionCondition) -> crate::ExecutionCondition {
    value.clone().into_referent()
}
