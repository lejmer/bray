use std::collections::BTreeMap;

use crate::execution_guarantees::ExecutionCondition;

use super::super::super::id::AnalysisBlockId;
use super::super::state::ExecutionState;

pub(super) fn merge(
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
        let mut places = Vec::new();

        for (place, value) in &target.current {
            let Some(other) = incoming.current.get(place) else {
                continue;
            };

            if value == other
                || *value == ExecutionCondition::Unknown
                || *other == ExecutionCondition::Unknown
            {
                continue;
            }

            let joined = ExecutionCondition::Joined(
                block.unit(),
                u32::try_from(block.to_index().expect("flow block has an index"))
                    .expect("flow block index fits"),
                place.clone(),
            );

            places.push((place.clone(), joined.clone()));

            for (map, value) in [(&mut left, value), (&mut right, other)] {
                // Literal payloads are shared by unrelated places and contract constants.
                // Joining their slots must not rename the constant itself.
                if *value == joined
                    || matches!(
                        value,
                        ExecutionCondition::Literal(_) | ExecutionCondition::Boolean(_)
                    )
                {
                    continue;
                }

                // A value shared by differently merged aliases has no unique replacement.
                map.entry(value.clone())
                    .and_modify(|prior| {
                        if *prior != joined {
                            *prior = ExecutionCondition::Unknown;
                        }
                    })
                    .or_insert_with(|| joined.clone());
            }
        }

        if !places.is_empty() {
            target.join_values(&left);

            let incoming = incoming_joined.to_mut();

            incoming.join_values(&right);

            for (place, value) in places {
                changed |= target.current.get(&place) != Some(&value);
                target.current.insert(place.clone(), value.clone());
                incoming.current.insert(place, value);
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

    let before = target.witness_carriers.len();

    target
        .witness_carriers
        .retain(|value| incoming.witness_carriers.contains(value));

    changed |= before != target.witness_carriers.len();

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
