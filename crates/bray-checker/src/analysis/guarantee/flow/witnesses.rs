use crate::execution_guarantees::ExecutionObservationKind as ObservationKind;
use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundExpressionId, BoundOperator};

use super::super::state::ExecutionState;
use crate::{ExecutionCondition, ExecutionPlace};

/// Propose value-contract facts and prove each against both original predecessors.
pub(super) fn conditional_guarantees(
    left: (
        &ExecutionState,
        &BTreeMap<ExecutionCondition, ExecutionCondition>,
        &BTreeMap<ExecutionCondition, ExecutionCondition>,
    ),
    right: (
        &ExecutionState,
        &BTreeMap<ExecutionCondition, ExecutionCondition>,
        &BTreeMap<ExecutionCondition, ExecutionCondition>,
    ),
    current: &BTreeMap<ExecutionPlace, ExecutionCondition>,
    results: &[(BoundExpressionId, ExecutionCondition)],
    availability: &BTreeSet<(ExecutionCondition, ExecutionCondition)>,
) -> Vec<(ExecutionCondition, ExecutionCondition, ExecutionCondition, bool)> {
    if left.0.witnesses.is_empty() && right.0.witnesses.is_empty() {
        return Vec::new();
    }

    let mut guards = availability.clone();
    let mut inputs = [BTreeMap::new(), BTreeMap::new()];
    let absent = [left.0, right.0].map(absent_observations);

    for (place, joined) in current {
        let source = [left.0, right.0].map(|state| {
            place.value_in(&state.current).unwrap_or(ExecutionCondition::Unknown)
        });

        collect_observations(
            [&source[0], &source[1]], joined, &mut inputs, &mut guards,
            [&absent[0], &absent[1]], &ExecutionCondition::Boolean(true),
            &mut { ExecutionCondition::WORK_LIMIT },
        );
    }

    for (expression, joined) in results {
        let source = [left.0, right.0].map(|state| {
            state.pending_results.get(expression).unwrap_or(&ExecutionCondition::Unknown)
        });

        collect_observations(
            source, joined, &mut inputs, &mut guards,
            [&absent[0], &absent[1]], &ExecutionCondition::Boolean(true),
            &mut { ExecutionCondition::WORK_LIMIT },
        );
    }

    let [left_inputs, right_inputs] = inputs;

    let ordinary = [left, right].map(|(state, values, identities)| {
        state
            .assumptions
            .iter()
            .filter(|(condition, _)| {
                !condition.contains_trusted() && !condition.observes_live(None)
            })
            .map(|(condition, holds)| (condition.with_joined_values(values, identities), *holds))
            .collect::<BTreeSet<_>>()
    });

    // Immutable branch selectors can qualify a moved carrier even when the selector
    // is stored outside it. Each proposed clause still needs both predecessor proofs.
    let path_guards = ordinary[0]
        .iter()
        .filter(|(condition, holds)| ordinary[1].contains(&(condition.clone(), !holds)))
        .flat_map(|(condition, _)| {
            [
                condition.clone(),
                ExecutionCondition::operation(BoundOperator::LogicalNot, vec![condition.clone()]),
            ]
        })
        .collect::<BTreeSet<_>>();

    let candidates = [left, right]
        .into_iter()
        .flat_map(|(state, values, identities)| {
            state
                .witnesses
                .iter()
                .flat_map(move |(owner, witness)| {
                    witness.conditions.keys().filter_map(move |condition| {
                        (!witness.available || condition.prove_mixed(
                            &state.trusted_assumptions,
                            &state.assumptions,
                            &mut { ExecutionCondition::WORK_LIMIT },
                        ) == Some(true))
                        .then(|| {
                            (
                                owner.with_joined_owners(values, identities),
                                condition.with_joined_values(values, identities),
                            )
                        })
                    })
                })
        })
        .collect::<BTreeSet<_>>();

    let mut retained = Vec::new();

    for (owner, condition) in candidates {
        if owner == ExecutionCondition::Unknown || condition == ExecutionCondition::Unknown {
            continue;
        }

        // Each complete carrier supplies its own authority. An ordinary inactive
        // variant needs no trusted evidence, but another carrier cannot lend it.
        let predecessors =
            [(left.0, &left_inputs), (right.0, &right_inputs)].map(|(state, inputs)| {
                let source = owner.with_snapshots(inputs);
                let mut evidence = BTreeSet::new();

                for (_, witness) in state.owned_witnesses([source]) {
                    for condition in witness.conditions.keys().filter(|condition| {
                        !witness.available || condition.prove_trusted(&state.trusted_assumptions, &state.assumptions)
                            == Some(true)
                    }) {
                        condition.clone().assume(true, &mut evidence);
                    }
                }

                (state, inputs, evidence)
            });

        let proved = |clause: &ExecutionCondition| {
            predecessors.iter().all(|(state, inputs, evidence)| {
                clause
                    .with_snapshots(inputs)
                    .prove_trusted(evidence, &state.assumptions)
                    == Some(true)
            })
        };

        // A join preserves validity while the child owns execution. It must not
        // make any unavailable predecessor's authority usable in the caller.
        let available = predecessors.iter().all(|(state, inputs, _)| {
            state.owned_witnesses([owner.with_snapshots(inputs)])
                .all(|(_, witness)| witness.available)
        });

        // An existing guarded claim can already hold on both paths without
        // discovering a new discriminant or adding a redundant outer guard.
        if proved(&condition) {
            retained.push((owner, condition.clone(), condition, available));
            continue;
        }

        for guard in guards
            .iter()
            .filter(|(parent, _)| {
                owner.is_owned_projection_of(parent) || parent.is_owned_projection_of(&owner)
            })
            .map(|(_, guard)| guard)
            .chain(&path_guards)
        {
            let clause = ExecutionCondition::operation(
                BoundOperator::LogicalOr,
                vec![
                    ExecutionCondition::operation(BoundOperator::LogicalNot, vec![guard.clone()]),
                    condition.clone(),
                ],
            );

            // These are actual predecessor snapshots of moved owners, not ordinary
            // equal-content substitutions for borrowed observations.
            if proved(&clause) {
                retained.push((owner.clone(), condition.clone(), clause, available));
            }
        }
    }

    retained
}

/// Keep the same proven clause on its complete carrier after identity renaming.
pub(super) fn retain_conditional_guarantees(
    left: &mut ExecutionState,
    right: &mut ExecutionState,
    conditions: &[(ExecutionCondition, ExecutionCondition, ExecutionCondition, bool)],
) {
    for (owner, original, condition, available) in conditions {
        let mut guarantee = crate::execution_guarantees::ExecutionGuarantee::default();
        let mut dependencies = BTreeSet::new();

        for state in [&*left, &*right] {
            for (_, witness) in state.owned_witnesses([owner.clone()]) {
                dependencies.extend(witness.dependencies.iter().cloned());

                guarantee.witness |= witness
                    .conditions
                    .get(original)
                    .is_some_and(|bound| bound.witness);

                // A predecessor can prove the clause through a differently guarded
                // premise. Preserve every matching address bound on that carrier.
                for source in witness.conditions.values() {
                    for (address, owners) in source
                        .addresses
                        .iter()
                        .filter(|(address, _)| condition.observes_trusted(address))
                    {
                        guarantee
                            .addresses
                            .entry(address.clone())
                            .or_default()
                            .extend(owners.iter().cloned());
                    }
                }
            }
        }

        for state in [&mut *left, &mut *right] {
            let witness = state.witnesses.entry(owner.clone()).or_default();

            if !dependencies.is_subset(&witness.dependencies) {
                std::sync::Arc::make_mut(witness)
                    .dependencies
                    .extend(dependencies.iter().cloned());
            }

            witness.carry(condition, guarantee.witness, &guarantee.addresses);

            if !available {
                std::sync::Arc::make_mut(witness).available = false;
            } else {
                condition
                    .clone()
                    .assume(true, &mut state.trusted_assumptions);
            }
        }
    }
}

pub(in crate::analysis::guarantee) fn owned_alternatives(
    left: (&ExecutionState, &ExecutionCondition),
    right: (&ExecutionState, &ExecutionCondition),
    joined: &ExecutionCondition,
) -> bray_symbols::DependencyOwnedResult<bray_bound_tree::StorageProjection, ExecutionCondition> {
    let mut guards = BTreeSet::new();
    let mut inputs = [BTreeMap::new(), BTreeMap::new()];
    let absent = [left.0, right.0].map(absent_observations);

    collect_observations(
        [left.1, right.1], joined, &mut inputs, &mut guards,
        [&absent[0], &absent[1]], &ExecutionCondition::Boolean(true),
        &mut { ExecutionCondition::WORK_LIMIT },
    );

    bray_symbols::DependencyOwnedResult::choice([(left, right.0), (right, left.0)].into_iter().zip(inputs).map(
        |(((state, source), other), inputs)| {
            let mut result = state.owned_result(source);

            for (condition, holds) in state.assumptions.difference(&other.assumptions) {
                if matches!(condition, ExecutionCondition::Input(place) if place.is_callable_input()) {
                    let condition = if *holds { condition.clone() } else { ExecutionCondition::operation(BoundOperator::LogicalNot, vec![condition.clone()]) };

                    result = bray_symbols::DependencyOwnedResult::case(condition, result);
                }
            }

            for (_, guard) in &guards {
                if guard
                    .with_snapshots(&inputs)
                    .prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                    == Some(true)
                {
                    result = bray_symbols::DependencyOwnedResult::case(guard.clone(), result);
                }
            }

            result
        },
    ))
}

fn absent_observations(state: &ExecutionState) -> BTreeMap<ExecutionCondition, std::sync::Arc<bray_symbols::ConstantValueData>> {
    state.assumptions.iter()
        .filter_map(|(condition, holds)| condition.nullable_absence(*holds))
        .map(|(subject, absent)| (subject.clone(), std::sync::Arc::clone(absent)))
        .collect()
}

fn collect_observations(
    sources: [&ExecutionCondition; 2],
    joined: &ExecutionCondition,
    inputs: &mut [BTreeMap<ExecutionCondition, ExecutionCondition>; 2],
    guards: &mut BTreeSet<(ExecutionCondition, ExecutionCondition)>,
    absent: [&BTreeMap<ExecutionCondition, std::sync::Arc<bray_symbols::ConstantValueData>>; 2],
    active: &ExecutionCondition,
    budget: &mut usize,
) {
    let Some(remaining) = budget.checked_sub(1) else { return; };

    *budget = remaining;

    if joined == &ExecutionCondition::Unknown { return; }

    let Some((role, owner)) = joined.ownership_identity() else { return; };

    let owner = owner.clone();

    for (source, inputs) in sources.into_iter().zip(inputs.iter_mut()) {
        if source.ownership_identity() != Some((role, &owner))
            && !matches!(owner, ExecutionCondition::Unknown | ExecutionCondition::Boolean(_) | ExecutionCondition::Literal(_)) {
            inputs.entry(owner.clone()).and_modify(|previous| {
                if previous != source { *previous = ExecutionCondition::Unknown; }
            }).or_insert_with(|| source.clone());
        }
    }

    if role == ObservationKind::Reference {
        let sources = sources.map(|source| source.clone().into_referent());

        collect_reference_data([&sources[0], &sources[1]], &joined.clone().into_referent(), inputs, budget);
        return;
    }

    if let ExecutionCondition::Observation { contents, kind: ObservationKind::Owned | ObservationKind::Reference, .. } = joined {
        let sources = sources.map(|source| match source {
            ExecutionCondition::Observation { contents, kind: ObservationKind::Owned | ObservationKind::Reference, .. } => contents.as_ref(),
            value => value,
        });

        collect_observations(sources, contents, inputs, guards, absent, active, budget);
        return;
    }

    // Payload authority is conditional on every enclosing active case. A test
    // of an inner tag alone has no meaning on an inactive outer payload.
    let presence = sources.into_iter().zip(absent).find_map(|(source, absent)| {
        match source {
            ExecutionCondition::Literal(value) if matches!(value.kind(), bray_symbols::ConstantValueKind::NullableAbsent) => Some(value),
            _ => absent.get(source).or_else(|| absent.get(source.owner_identity())),
        }
    }).map(|absent| ExecutionCondition::operation(
        BoundOperator::LogicalAnd,
        vec![active.clone(), ExecutionCondition::operation(
            BoundOperator::NotEqual,
            vec![joined.clone(), ExecutionCondition::Literal(std::sync::Arc::clone(absent))],
        )],
    ));

    if let Some(presence) = &presence { insert_guard(guards, &owner, presence.clone()); }

    let mut fields = BTreeSet::new();

    for source in sources {
        match source {
            ExecutionCondition::Tagged(variant, _) => insert_guard(
                guards, &owner, ExecutionCondition::operation(
                    BoundOperator::LogicalAnd, vec![active.clone(), joined.clone().matches_variant(*variant)],
                ),
            ),
            ExecutionCondition::Constructed(base, components) => {
                if let ExecutionCondition::Tagged(variant, _) = base.as_ref() {
                    insert_guard(guards, &owner, ExecutionCondition::operation(
                        BoundOperator::LogicalAnd, vec![active.clone(), joined.clone().matches_variant(*variant)],
                    ));
                }

                fields.extend(components.iter().map(|(field, _)| *field));
            }
            ExecutionCondition::Present(_) => { fields.insert(bray_bound_tree::StorageProjection::NullableValue); }
            _ => {}
        }
    }

    for field in fields {
        let active = match field {
            bray_bound_tree::StorageProjection::ActiveUnionPayloadField { variant, .. } => {
                ExecutionCondition::operation(
                    BoundOperator::LogicalAnd, vec![active.clone(), joined.clone().matches_variant(variant)],
                )
            }
            bray_bound_tree::StorageProjection::NullableValue => presence.clone().unwrap_or_else(|| active.clone()),
            _ => active.clone(),
        };

        if matches!(field, bray_bound_tree::StorageProjection::ActiveUnionPayloadField { .. }) {
            insert_guard(guards, &owner, active.clone());
        }

        let values = sources.map(|source| source.clone().project(field));

        collect_observations(
            [&values[0], &values[1]], &joined.clone().project(field), inputs, guards, absent,
            &active, budget,
        );
    }
}

fn collect_reference_data(
    sources: [&ExecutionCondition; 2],
    joined: &ExecutionCondition,
    inputs: &mut [BTreeMap<ExecutionCondition, ExecutionCondition>; 2],
    budget: &mut usize,
) {
    let Some(remaining) = budget.checked_sub(1) else { return; };

    *budget = remaining;

    if let ExecutionCondition::Observation { contents, kind: ObservationKind::Owned, .. } = joined {
        let sources = sources.map(|source| match source {
            ExecutionCondition::Observation { contents, kind: ObservationKind::Owned, .. } => contents.as_ref(),
            value => value,
        });

        collect_reference_data(sources, contents, inputs, budget);
        return;
    }

    if matches!(joined, ExecutionCondition::Unknown | ExecutionCondition::Literal(_) | ExecutionCondition::Boolean(_)) {
        return;
    }

    // These substitutions instantiate an actual predecessor's reached data
    // during proof. They do not create an ownership mapping or another origin.
    for (source, inputs) in sources.into_iter().zip(inputs.iter_mut()) {
        if source != joined {
            inputs.entry(joined.clone()).and_modify(|previous| {
                if previous != source { *previous = ExecutionCondition::Unknown; }
            }).or_insert_with(|| source.clone());
        }
    }

    if let ExecutionCondition::Constructed(_, fields) = joined {
        for (field, value) in fields.iter() {
            let sources = sources.map(|source| source.clone().project(*field));

            collect_reference_data([&sources[0], &sources[1]], value, inputs, budget);
        }
    }
}

fn insert_guard(
    guards: &mut BTreeSet<(ExecutionCondition, ExecutionCondition)>,
    owner: &ExecutionCondition,
    guard: ExecutionCondition,
) {
    if !matches!(guard, ExecutionCondition::Boolean(_) | ExecutionCondition::Unknown) {
        guards.insert((owner.clone(), guard.clone()));
        guards.insert((owner.clone(), ExecutionCondition::operation(BoundOperator::LogicalNot, vec![guard])));
    }
}
