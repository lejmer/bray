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
) -> Vec<(ExecutionCondition, ExecutionCondition, ExecutionCondition)> {
    if left.0.witnesses.is_empty() && right.0.witnesses.is_empty() {
        return Vec::new();
    }

    let mut guards = BTreeSet::new();
    let mut left_inputs = BTreeMap::new();
    let mut right_inputs = BTreeMap::new();

    for (state, inputs) in [(left.0, &mut left_inputs), (right.0, &mut right_inputs)] {
        let absent = state
            .assumptions
            .iter()
            .filter_map(|(condition, holds)| condition.nullable_absence(*holds))
            .map(|(subject, absent)| (subject.clone(), std::sync::Arc::clone(absent)))
            .collect::<BTreeMap<_, _>>();

        for (place, joined) in current {
            if let Some(value) = place.value_in(&state.current) {
                collect_observations(&value, joined, inputs, &mut guards, &absent, &mut {
                    ExecutionCondition::WORK_LIMIT
                });
            }
        }

        for (expression, joined) in results {
            if let Some(value) = state.pending_results.get(expression) {
                collect_observations(value, joined, inputs, &mut guards, &absent, &mut {
                    ExecutionCondition::WORK_LIMIT
                });
            }
        }
    }

    let candidates = [left, right]
        .into_iter()
        .flat_map(|(state, values, identities)| {
            state.witnesses.iter().flat_map(move |(owner, witness)| {
                witness.conditions.keys().filter_map(move |condition| {
                    (condition.prove_mixed(&state.trusted_assumptions, &state.assumptions, &mut {
                        ExecutionCondition::WORK_LIMIT
                    }) == Some(true))
                    .then(|| {
                        (
                            owner.with_joined_values(identities, identities),
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

                if let Some(witness) = state.witnesses.get(source.owner_identity()) {
                    for condition in witness.conditions.keys().filter(|condition| {
                        condition.prove_trusted(&state.trusted_assumptions, &state.assumptions)
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

        // An existing guarded claim can already hold on both paths without
        // discovering a new discriminant or adding a redundant outer guard.
        if proved(&condition) {
            retained.push((owner, condition.clone(), condition));
            continue;
        }

        for (parent, guard) in &guards {
            if !owner.is_owned_projection_of(parent) && !parent.is_owned_projection_of(&owner) {
                continue;
            }

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
                retained.push((owner.clone(), condition.clone(), clause));
            }
        }
    }

    retained
}

/// Keep the same proven clause on its complete carrier after identity renaming.
pub(super) fn retain_conditional_guarantees(
    left: &mut ExecutionState,
    right: &mut ExecutionState,
    conditions: &[(ExecutionCondition, ExecutionCondition, ExecutionCondition)],
) {
    for (owner, original, condition) in conditions {
        let mut guarantee = crate::execution_guarantees::ExecutionGuarantee::default();

        for state in [&*left, &*right] {
            let Some(witness) = state.witnesses.get(owner) else {
                continue;
            };

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

        for state in [&mut *left, &mut *right] {
            if let Some(witness) = state.witnesses.get_mut(owner) {
                witness.carry(condition, guarantee.witness, &guarantee.addresses);
            }

            condition
                .clone()
                .assume(true, &mut state.trusted_assumptions);
        }
    }
}

fn collect_observations(
    source: &ExecutionCondition,
    joined: &ExecutionCondition,
    inputs: &mut BTreeMap<ExecutionCondition, ExecutionCondition>,
    guards: &mut BTreeSet<(ExecutionCondition, ExecutionCondition)>,
    absent: &BTreeMap<ExecutionCondition, std::sync::Arc<bray_symbols::ConstantValueData>>,
    budget: &mut usize,
) {
    let Some(remaining) = budget.checked_sub(1) else {
        return;
    };

    *budget = remaining;

    if source == &ExecutionCondition::Unknown || joined == &ExecutionCondition::Unknown {
        return;
    }

    // A snapshot supplies predecessor contents; paths identify the joined owner.
    let joined = joined.owner_identity();
    let owner = joined.clone();

    if source.owner_identity() != &owner
        && !matches!(
            owner,
            ExecutionCondition::Unknown
                | ExecutionCondition::Boolean(_)
                | ExecutionCondition::Literal(_)
        )
    {
        inputs
            .entry(owner.clone())
            .and_modify(|previous| {
                if previous != source {
                    *previous = ExecutionCondition::Unknown;
                }
            })
            .or_insert_with(|| source.clone());
    }

    let absence = match source {
        ExecutionCondition::Literal(value)
            if matches!(
                value.kind(),
                bray_symbols::ConstantValueKind::NullableAbsent
            ) =>
        {
            Some(value)
        }
        _ => absent
            .get(source)
            .or_else(|| absent.get(source.owner_identity())),
    };

    if let Some(absent) = absence {
        insert_guard(
            guards,
            &owner,
            ExecutionCondition::operation(
                BoundOperator::NotEqual,
                vec![
                    joined.clone(),
                    ExecutionCondition::Literal(std::sync::Arc::clone(absent)),
                ],
            ),
        );
    }

    match source {
        ExecutionCondition::Tagged(variant, _) => {
            insert_guard(guards, &owner, joined.clone().matches_variant(*variant))
        }
        ExecutionCondition::Constructed(base, fields) => {
            if let ExecutionCondition::Tagged(variant, _) = base.as_ref() {
                insert_guard(guards, &owner, joined.clone().matches_variant(*variant));
            }

            for (field, value) in fields.iter() {
                if let bray_bound_tree::StorageProjection::ActiveUnionPayloadField {
                    variant, ..
                } = field
                {
                    insert_guard(guards, &owner, joined.clone().matches_variant(*variant));
                }

                collect_observations(
                    value,
                    &joined.clone().project(*field),
                    inputs,
                    guards,
                    absent,
                    budget,
                );
            }
        }
        ExecutionCondition::Present(value) => collect_observations(
            value,
            &joined
                .clone()
                .project(bray_bound_tree::StorageProjection::NullableValue),
            inputs,
            guards,
            absent,
            budget,
        ),
        _ => {}
    }
}

fn insert_guard(
    guards: &mut BTreeSet<(ExecutionCondition, ExecutionCondition)>,
    owner: &ExecutionCondition,
    guard: ExecutionCondition,
) {
    if !matches!(
        guard,
        ExecutionCondition::Boolean(_) | ExecutionCondition::Unknown
    ) {
        guards.insert((owner.clone(), guard.clone()));

        guards.insert((
            owner.clone(),
            ExecutionCondition::operation(BoundOperator::LogicalNot, vec![guard]),
        ));
    }
}
