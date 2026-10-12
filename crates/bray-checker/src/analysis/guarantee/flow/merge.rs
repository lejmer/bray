use std::collections::BTreeMap;

use crate::execution_guarantees::ExecutionCondition;

use super::super::super::id::AnalysisBlockId;
use super::super::state::ExecutionState;

pub(in crate::analysis) fn merge(
    target: &mut Option<ExecutionState>,
    incoming: &Option<ExecutionState>,
    block: Option<AnalysisBlockId>,
) -> bool {
    let Some(incoming) = incoming else {
        return false;
    };

    let Some(target) = target else {
        // The first reachable predecessor supplies an independently owned flow environment.
        *target = Some(incoming.clone());

        return true;
    };

    let mut changed = false;
    let mut incoming_joined = std::borrow::Cow::Borrowed(incoming);

    if let Some(block) = block {
        let mut left = BTreeMap::new();
        let mut right = BTreeMap::new();
        let mut left_identities = BTreeMap::new();
        let mut right_identities = BTreeMap::new();
        let mut places = Vec::new();
        let mut joins = false;
        let mut joined_facts = Vec::new();

        let observed = target
            .current
            .keys()
            .chain(incoming.current.keys())
            .collect::<std::collections::BTreeSet<_>>();

        for place in observed {
            let value = place
                .value_in(&target.current)
                .unwrap_or(ExecutionCondition::Unknown);

            let other = place
                .value_in(&incoming.current)
                .unwrap_or(ExecutionCondition::Unknown);

            let joined = if value == other {
                value.clone()
            } else if value == ExecutionCondition::Unknown || other == ExecutionCondition::Unknown {
                ExecutionCondition::Unknown
            } else {
                ExecutionCondition::Joined(
                    block.unit(),
                    u32::try_from(block.to_index().expect("flow block has an index"))
                        .expect("flow block index fits"),
                    place.clone(),
                )
            };

            if value != other && joined != ExecutionCondition::Unknown {
                let left_truth =
                    value.prove(&target.assumptions, &mut { ExecutionCondition::WORK_LIMIT });

                let right_truth = other.prove(&incoming.assumptions, &mut {
                    ExecutionCondition::WORK_LIMIT
                });

                if let (Some(left), Some(right)) = (left_truth, right_truth)
                    && left == right
                {
                    joined_facts.push((joined.clone(), left));
                }
            }

            joins |= target.current.get(place) != Some(&joined)
                || incoming.current.get(place) != Some(&joined);

            places.push((place.clone(), joined.clone()));

            for (map, identities, value) in [
                (&mut left, &mut left_identities, &value),
                (&mut right, &mut right_identities, &other),
            ] {
                // Literal payloads are shared by unrelated places and contract constants.
                // Joining their slots must not rename the constant itself.
                if *value == joined
                    || matches!(
                        value,
                        ExecutionCondition::Literal(_)
                            | ExecutionCondition::Boolean(_)
                            | ExecutionCondition::Unknown
                    )
                {
                    continue;
                }

                // Contents and containing identity have different substitution domains.
                insert_replacement(map, value, &joined);

                for identity in [value, value.owner_identity()] {
                    if *identity != joined {
                        insert_replacement(identities, identity, &joined);
                    }
                }
            }
        }

        if joins {
            target.join_values(&left, &left_identities);

            let incoming = incoming_joined.to_mut();

            incoming.join_values(&right, &right_identities);

            for (condition, holds) in joined_facts {
                condition.clone().assume(holds, &mut target.assumptions);
                condition.assume(holds, &mut incoming.assumptions);
            }

            for (place, value) in places {
                changed |= target.current.get(&place) != Some(&value);
                target.assign(place.clone(), value.clone());
                incoming.assign(place, value);
            }
        }
    }

    let incoming = &incoming_joined;
    let dependencies = target.completion_dependencies.len();

    target
        .completion_dependencies
        .extend(incoming.completion_dependencies.iter().copied());

    changed |= dependencies != target.completion_dependencies.len();

    for (expression, evidence) in &incoming.entries {
        match target.entries.entry(*expression) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(evidence.clone());
                changed = true;
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                changed |= entry.get_mut().intersect(evidence);
            }
        }
    }

    let before = target.assumptions.len();

    let boundaries_before = target.trust_boundaries.len();

    target
        .trust_boundaries
        .retain(|boundary| incoming.trust_boundaries.contains(boundary));

    changed |= boundaries_before != target.trust_boundaries.len();

    target
        .assumptions
        .retain(|condition| incoming.assumptions.contains(condition));

    changed |= before != target.assumptions.len();

    // Proven truth is intersected below; retain every possible carrier obligation.
    for (carrier, conditions) in &incoming.witness_carriers {
        let retained = target.witness_carriers.entry(carrier.clone()).or_default();
        let before = retained.len();

        retained.extend(conditions.iter().cloned());
        changed |= before != retained.len();
    }

    let before = target.trusted_assumptions.len();

    target
        .trusted_assumptions
        .retain(|condition| incoming.trusted_assumptions.contains(condition));

    changed |= before != target.trusted_assumptions.len();

    // Conflicting provenance cannot become an unbound allocation proof at a merge.
    for (condition, owner) in &mut target.allocation_owners {
        if incoming.allocation_owners.get(condition) != Some(owner)
            && *owner != ExecutionCondition::Unknown
        {
            *owner = ExecutionCondition::Unknown;
            changed = true;
        }
    }

    for condition in incoming.allocation_owners.keys() {
        if let std::collections::btree_map::Entry::Vacant(entry) =
            target.allocation_owners.entry(condition.clone())
        {
            entry.insert(ExecutionCondition::Unknown);
            changed = true;
        }
    }

    for (carrier, dependencies) in &incoming.witness_dependencies {
        let retained = target
            .witness_dependencies
            .entry(carrier.clone())
            .or_default();

        let before = retained.len();

        retained.extend(dependencies.iter().cloned());
        changed |= before != retained.len();
    }

    changed |= merge_values(&mut target.current, &incoming.current);
    changed |= merge_values(&mut target.expressions, &incoming.expressions);
    changed |= merge_values(&mut target.pending_results, &incoming.pending_results);

    if target.result != incoming.result && target.result != ExecutionCondition::Unknown {
        target.result = ExecutionCondition::Unknown;
        changed = true;
    }

    changed
}

fn merge_values<K: Ord + Clone>(
    target: &mut BTreeMap<K, ExecutionCondition>,
    incoming: &BTreeMap<K, ExecutionCondition>,
) -> bool {
    let mut changed = false;

    for (key, value) in target.iter_mut() {
        if incoming.get(key) != Some(value) && *value != ExecutionCondition::Unknown {
            *value = ExecutionCondition::Unknown;
            changed = true;
        }
    }

    for key in incoming.keys() {
        // Join keys are small identities or share immutable field paths.
        if let std::collections::btree_map::Entry::Vacant(entry) = target.entry(key.clone()) {
            entry.insert(ExecutionCondition::Unknown);
            changed = true;
        }
    }

    changed
}

fn insert_replacement(
    replacements: &mut BTreeMap<ExecutionCondition, ExecutionCondition>,
    value: &ExecutionCondition,
    joined: &ExecutionCondition,
) {
    // A value shared by differently merged aliases has no unique replacement.
    replacements
        .entry(value.clone())
        .and_modify(|prior| {
            if prior != joined {
                *prior = ExecutionCondition::Unknown;
            }
        })
        .or_insert_with(|| joined.clone());
}
