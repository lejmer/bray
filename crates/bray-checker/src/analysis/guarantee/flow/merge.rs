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

    let (incoming_joined, mut changed) = match block {
        Some(block) => join_observations(target, incoming, block),
        None => (std::borrow::Cow::Borrowed(incoming), false),
    };

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

fn join_observations<'a>(
    target: &mut ExecutionState,
    incoming: &'a ExecutionState,
    block: AnalysisBlockId,
) -> (std::borrow::Cow<'a, ExecutionState>, bool) {
    let mut changed = false;
    let mut incoming_joined = std::borrow::Cow::Borrowed(incoming);

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
        } else if value.owner_identity() == other.owner_identity()
            && value.owner_identity() != &ExecutionCondition::Unknown
        {
            join_components(&value, &other)
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
    }

    // Assemble the same complete snapshots that field assignment will expose before
    // renaming predicates. A joined field must not change its parent's meaning later.
    let mut joined_values = places.iter().cloned().collect::<BTreeMap<_, _>>();

    for (place, value) in &places {
        super::super::state::update_parent_observations(&mut joined_values, place, value);
    }

    for (place, joined) in &joined_values {
        let value = place
            .value_in(&target.current)
            .unwrap_or(ExecutionCondition::Unknown);

        let other = place
            .value_in(&incoming.current)
            .unwrap_or(ExecutionCondition::Unknown);

        for (map, identities, value) in [
            (&mut left, &mut left_identities, &value),
            (&mut right, &mut right_identities, &other),
        ] {
            // Rename the complete current observation only for an established owner.
            // Both predecessors must still establish the predicate after renaming.
            // Snapshots share immutable components, so borrowed keys need no tree copy.
            if value != joined
                && value.owner_identity().observation_path().is_some()
                && joined.owner_identity().observation_path().is_some()
            {
                insert_replacement(map, &value.clone().borrowed(), &joined.clone().borrowed());
            }

            // Literal payloads are shared by unrelated places and contract constants.
            // Joining their slots must not rename the constant itself.
            if value == joined
                || value == joined.owner_identity()
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
            insert_replacement(map, value, joined);

            for identity in [value, value.owner_identity()] {
                if identity != joined.owner_identity() {
                    insert_replacement(identities, identity, joined.owner_identity());
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

        for (place, value) in joined_values {
            changed |= target.current.get(&place) != Some(&value);
            target.current.insert(place.clone(), value.clone());
            incoming.current.insert(place, value);
        }
    }

    (incoming_joined, changed)
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

fn join_components(left: &ExecutionCondition, right: &ExecutionCondition) -> ExecutionCondition {
    common_components(left, right, &mut { ExecutionCondition::WORK_LIMIT })
}

fn common_components(
    left: &ExecutionCondition,
    right: &ExecutionCondition,
    budget: &mut usize,
) -> ExecutionCondition {
    if left == right {
        return left.clone();
    }

    let Some(next) = budget.checked_sub(1) else {
        return ExecutionCondition::Unknown;
    };

    *budget = next;

    if let (ExecutionCondition::Borrowed(left), ExecutionCondition::Borrowed(right)) = (left, right)
    {
        return common_components(left, right, budget).borrowed();
    }

    let fields = |value: &ExecutionCondition| match value {
        ExecutionCondition::Constructed(_, fields) => fields
            .iter()
            .map(|(field, _)| *field)
            .collect::<std::collections::BTreeSet<_>>(),
        _ => Default::default(),
    };

    let mut projections = fields(left);

    projections.extend(fields(right));

    if projections.is_empty() {
        return ExecutionCondition::Unknown;
    }

    let identity = if left.owner_identity() == right.owner_identity() {
        left.owner_identity().clone()
    } else {
        ExecutionCondition::Unknown
    };

    let fields = projections
        .into_iter()
        .map(|projection| {
            let left = left.clone().project(projection);
            let right = right.clone().project(projection);

            (projection, common_components(&left, &right, budget))
        })
        .collect();

    ExecutionCondition::Constructed(std::sync::Arc::new(identity), fields)
}

#[cfg(test)]
mod tests {
    use super::join_components;
    use crate::{ExecutionCondition, ExecutionPlace};
    use bray_bound_tree::{BoundUnitId, StorageProjection};
    use bray_symbols::{ConstantProjectionKind, SymbolOrdinal};
    use std::sync::Arc;

    #[test]
    fn joins_keep_matching_nested_observations_without_inventing_identity() {
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let component = ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0));
        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let identity = ExecutionCondition::Input(place.clone());
        let marker = ExecutionCondition::Boolean(true);

        let nested = |version| {
            ExecutionCondition::Constructed(
                Arc::new(ExecutionCondition::Joined(
                    BoundUnitId::new(0),
                    version,
                    place.clone(),
                )),
                [(field, marker.clone())].into(),
            )
            .borrowed()
        };

        let owner = |value| {
            ExecutionCondition::Constructed(Arc::new(identity.clone()), [(field, value)].into())
        };

        let joined = join_components(&owner(nested(1)), &owner(nested(2)));

        assert_eq!(joined.owner_identity(), &identity);

        assert_eq!(
            joined.clone().component(component).component(component),
            marker
        );

        let different = owner(
            ExecutionCondition::Constructed(
                Arc::new(ExecutionCondition::Unknown),
                [(field, ExecutionCondition::Boolean(false))].into(),
            )
            .borrowed(),
        );

        assert_eq!(
            join_components(&joined, &different)
                .component(component)
                .component(component),
            ExecutionCondition::Unknown
        );
    }
}
