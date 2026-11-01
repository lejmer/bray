use std::collections::BTreeMap;

use bray_bound_tree::BoundOperator;
use bray_symbols::ConstantValueKind;

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

    let before = target.deferred_invocations.len();

    target
        .deferred_invocations
        .retain(|value, source| incoming.deferred_invocations.get(value) == Some(source));

    changed |= before != target.deferred_invocations.len();

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
    for (carrier, witness) in &incoming.witnesses {
        changed |= target
            .witnesses
            .entry(carrier.clone())
            .or_default()
            .merge(witness);
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
    let mut results = Vec::new();
    let mut joined_facts = Vec::new();
    let mut needs_condition_proof = false;
    let mut stable_values = std::collections::BTreeSet::new();
    let mut stable_identities = std::collections::BTreeSet::new();

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

        if value.owner_identity() == other.owner_identity() {
            stable_identities.insert(value.owner_identity().clone());
        }

        let joined = if value == other {
            stable_values.insert(value.clone());
            stable_values.insert(value.clone().borrowed());
            stable_identities.insert(value.clone());

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
            needs_condition_proof |= matches!(
                value,
                ExecutionCondition::Literal(_) | ExecutionCondition::Boolean(_)
            ) || matches!(
                other,
                ExecutionCondition::Literal(_) | ExecutionCondition::Boolean(_)
            );

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

    for (current, values, identities) in [
        (&target.current, &mut left, &mut left_identities),
        (&incoming.current, &mut right, &mut right_identities),
    ] {
        needs_condition_proof |= current_replacements(
            current,
            &joined_values,
            &stable_values,
            &stable_identities,
            values,
            identities,
        );
    }

    for expression in target
        .pending_results
        .keys()
        .chain(incoming.pending_results.keys())
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
    {
        let value = target
            .pending_results
            .get(&expression)
            .unwrap_or(&ExecutionCondition::Unknown);

        let other = incoming
            .pending_results
            .get(&expression)
            .unwrap_or(&ExecutionCondition::Unknown);

        let joined = common_components(
            value,
            other,
            Some(ExecutionCondition::Expression(expression)),
            &mut joined_facts,
            &mut { ExecutionCondition::WORK_LIMIT },
        );

        joins |= value != &joined || other != &joined;

        result_replacements(
            value,
            &joined,
            &stable_values,
            &stable_identities,
            &mut left,
            &mut left_identities,
        );

        result_replacements(
            other,
            &joined,
            &stable_values,
            &stable_identities,
            &mut right,
            &mut right_identities,
        );

        results.push((expression, joined));
    }

    if joins {
        if needs_condition_proof {
            joined_facts.extend(common_join_conditions(target, incoming, &joined_values));
        }

        let conditional = super::witnesses::conditional_guarantees(
            (target, &left, &left_identities),
            (incoming_joined.as_ref(), &right, &right_identities),
            &joined_values,
            &results,
        );

        target.join_values(&left, &left_identities);

        let incoming = incoming_joined.to_mut();

        incoming.join_values(&right, &right_identities);

        super::witnesses::retain_conditional_guarantees(target, incoming, &conditional);

        for (condition, holds) in joined_facts {
            condition.clone().assume(holds, &mut target.assumptions);
            condition.assume(holds, &mut incoming.assumptions);
        }

        for (place, value) in joined_values {
            changed |= target.current.get(&place) != Some(&value);
            target.current.insert(place.clone(), value.clone());
            incoming.current.insert(place, value);
        }

        for (expression, value) in results {
            changed |= target.pending_results.get(&expression) != Some(&value);
            target.pending_results.insert(expression, value.clone());
            incoming.pending_results.insert(expression, value);
        }
    }

    (incoming_joined, changed)
}

fn current_replacements(
    current: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    joined_values: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    stable_values: &std::collections::BTreeSet<ExecutionCondition>,
    stable_identities: &std::collections::BTreeSet<ExecutionCondition>,
    map: &mut BTreeMap<ExecutionCondition, ExecutionCondition>,
    identities: &mut BTreeMap<ExecutionCondition, ExecutionCondition>,
) -> bool {
    let mut needs_condition_proof = false;

    for (place, joined) in joined_values {
        let value = place
            .value_in(current)
            .unwrap_or(ExecutionCondition::Unknown);

        let value = &value;

        // Rename the complete current observation only for an established owner.
        // Both predecessors must still establish the predicate after renaming.
        // Snapshots share immutable components, so borrowed keys need no tree copy.
        if value != joined
            && value.owner_identity().observation_path().is_some()
            && joined.owner_identity().observation_path().is_some()
        {
            let borrowed = value.clone().borrowed();

            if !stable_values.contains(&borrowed) {
                insert_replacement(map, &borrowed, &joined.clone().borrowed());
            }
        }

        needs_condition_proof |= value != joined && stable_values.contains(value);

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

        if matches!(
            value,
            ExecutionCondition::Constructed(_, _) | ExecutionCondition::Present(_)
        ) {
            result_replacements(
                value,
                joined,
                stable_values,
                stable_identities,
                map,
                identities,
            );

            continue;
        }

        // Contents and containing identity have different substitution domains.
        // A branch-local alias can disappear while another place still holds
        // the same value on both paths. Rename the changed place, not that value.
        if !stable_values.contains(value) {
            insert_replacement(map, value, joined);
        }

        for identity in [value, value.owner_identity()] {
            if identity != joined.owner_identity() && !stable_identities.contains(identity) {
                insert_replacement(identities, identity, joined.owner_identity());
            }
        }
    }

    needs_condition_proof
}

fn common_join_conditions(
    left: &ExecutionState,
    right: &ExecutionState,
    joined: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
) -> Vec<(ExecutionCondition, bool)> {
    let mut left_inputs = BTreeMap::new();
    let mut right_inputs = BTreeMap::new();
    let mut left_candidates = BTreeMap::new();
    let mut right_candidates = BTreeMap::new();

    for (place, value) in joined {
        if value == &ExecutionCondition::Unknown {
            continue;
        }

        for (state, inputs, candidates) in [
            (left, &mut left_inputs, &mut left_candidates),
            (right, &mut right_inputs, &mut right_candidates),
        ] {
            let input = place
                .value_in(&state.current)
                .unwrap_or(ExecutionCondition::Unknown);

            if input == *value {
                continue;
            }

            insert_replacement(inputs, value, &input);

            if !matches!(
                input,
                ExecutionCondition::Literal(_)
                    | ExecutionCondition::Boolean(_)
                    | ExecutionCondition::Unknown
            ) {
                insert_replacement(candidates, &input, value);
            }
        }
    }

    // These are proposals, not renamings of established facts. A copied input may
    // still exist elsewhere, and a literal may occur in unrelated slots. Prove each
    // proposal against both predecessors before exposing it on the joined value.
    let candidates = [(left, &left_candidates), (right, &right_candidates)]
        .into_iter()
        .flat_map(|(state, replacements)| {
            state
                .assumptions
                .iter()
                .map(move |(condition, holds)| (condition.with_equalities(replacements), *holds))
        })
        .take(ExecutionCondition::WORK_LIMIT)
        .collect::<std::collections::BTreeSet<_>>();

    candidates
        .into_iter()
        .filter(|(condition, holds)| {
            [(left, &left_inputs), (right, &right_inputs)]
                .into_iter()
                .all(|(state, inputs)| {
                    let instantiated = condition.with_equalities(inputs);

                    let proven = instantiated
                        .prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT });

                    proven == Some(*holds)
                })
        })
        .collect()
}

fn result_replacements(
    value: &ExecutionCondition,
    joined: &ExecutionCondition,
    stable_values: &std::collections::BTreeSet<ExecutionCondition>,
    stable_identities: &std::collections::BTreeSet<ExecutionCondition>,
    values: &mut BTreeMap<ExecutionCondition, ExecutionCondition>,
    identities: &mut BTreeMap<ExecutionCondition, ExecutionCondition>,
) {
    if value == joined
        || joined == &ExecutionCondition::Unknown
        || matches!(
            value,
            ExecutionCondition::Borrowed(_)
                | ExecutionCondition::Literal(_)
                | ExecutionCondition::Boolean(_)
                | ExecutionCondition::Unknown
        )
    {
        return;
    }

    if !stable_values.contains(value) {
        insert_replacement(values, value, joined);
    }

    // A complete owned snapshot also names its tag through the base observation.
    // Move that observation with its payload, preserving any independently live alias.
    let owner = value.owner_identity();

    if owner != value
        && owner != joined.owner_identity()
        && !stable_values.contains(owner)
        && !stable_identities.contains(owner)
    {
        insert_replacement(values, owner, joined.owner_identity());
    }

    for identity in [value, value.owner_identity()] {
        if identity != joined.owner_identity() && !stable_identities.contains(identity) {
            insert_replacement(identities, identity, joined.owner_identity());
        }
    }

    match value {
        ExecutionCondition::Constructed(_, fields) => {
            for (field, component) in fields.iter() {
                result_replacements(
                    component,
                    &joined.clone().project(*field),
                    stable_values,
                    stable_identities,
                    values,
                    identities,
                );
            }
        }
        ExecutionCondition::Present(component) => {
            result_replacements(
                component,
                &joined
                    .clone()
                    .project(bray_bound_tree::StorageProjection::NullableValue),
                stable_values,
                stable_identities,
                values,
                identities,
            );
        }
        _ => {}
    }
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
    common_components(left, right, None, &mut Vec::new(), &mut {
        ExecutionCondition::WORK_LIMIT
    })
}

fn common_components(
    left: &ExecutionCondition,
    right: &ExecutionCondition,
    result: Option<ExecutionCondition>,
    facts: &mut Vec<(ExecutionCondition, bool)>,
    budget: &mut usize,
) -> ExecutionCondition {
    if left == right {
        return left.clone();
    }

    let Some(next) = budget.checked_sub(1) else {
        return ExecutionCondition::Unknown;
    };

    *budget = next;

    if left == &ExecutionCondition::Unknown || right == &ExecutionCondition::Unknown {
        return ExecutionCondition::Unknown;
    }

    if let (ExecutionCondition::Borrowed(left), ExecutionCondition::Borrowed(right)) = (left, right)
    {
        // Joining an owned result does not identify different borrowed referents.
        return common_components(left, right, None, facts, budget).borrowed();
    }

    if matches!(left, ExecutionCondition::Borrowed(_))
        || matches!(right, ExecutionCondition::Borrowed(_))
    {
        return ExecutionCondition::Unknown;
    }

    let fields = |value: &ExecutionCondition| match value {
        ExecutionCondition::Constructed(_, fields) => fields
            .iter()
            .map(|(field, _)| *field)
            .collect::<std::collections::BTreeSet<_>>(),
        ExecutionCondition::Present(_) => {
            [bray_bound_tree::StorageProjection::NullableValue].into()
        }
        _ => Default::default(),
    };

    let mut projections = fields(left);

    projections.extend(fields(right));

    if projections.is_empty() {
        let joined = result.unwrap_or(ExecutionCondition::Unknown);

        if joined != ExecutionCondition::Unknown
            && let (ExecutionCondition::Literal(left), ExecutionCondition::Literal(right)) =
                (left, right)
            && matches!(
                (left.kind(), right.kind()),
                (ConstantValueKind::Integer(_), ConstantValueKind::Integer(_))
            )
            && let Ok(ConstantValueKind::Boolean(ordered)) = crate::constant::fold_binary(
                BoundOperator::LessEqual,
                left.kind(),
                right.kind(),
                crate::ConstantEvaluationLimits::default().integer_bits(),
            )
        {
            // A yielded result keeps these bounds for this evaluation. Literals
            // remain independent values and are never renamed to the result.
            let (lower, upper) = if ordered {
                (left, right)
            } else {
                (right, left)
            };

            for (operator, bound) in [
                (BoundOperator::GreaterEqual, lower),
                (BoundOperator::LessEqual, upper),
            ] {
                facts.push((
                    ExecutionCondition::operation(
                        operator,
                        vec![
                            joined.clone(),
                            ExecutionCondition::Literal(std::sync::Arc::clone(bound)),
                        ],
                    ),
                    true,
                ));
            }
        }

        return joined;
    }

    // Ordinary observations retain only a shared identity. Yielded owners instead
    // move their initialized components under the result region's identity.
    let identity = result.clone().unwrap_or_else(|| {
        if left.owner_identity() == right.owner_identity() {
            left.owner_identity().clone()
        } else {
            ExecutionCondition::Unknown
        }
    });

    let absent = |value: &ExecutionCondition| {
        matches!(value, ExecutionCondition::Literal(value)
        if matches!(value.kind(), ConstantValueKind::NullableAbsent))
    };

    let fields = projections
        .into_iter()
        .map(|projection| {
            if projection == bray_bound_tree::StorageProjection::NullableValue
                && (absent(left) || absent(right))
            {
                // The joined nullable owns its payload only on the present path.
                // Keep that conditional owner just as for inactive union payloads.
                return (projection, identity.clone().project(projection));
            }

            let left = left.clone().project(projection);
            let right = right.clone().project(projection);

            let result = result
                .as_ref()
                .map(|result| result.clone().project(projection));

            (
                projection,
                common_components(&left, &right, result, facts, budget),
            )
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

    #[test]
    fn disappearing_alias_does_not_rename_a_surviving_observation() {
        use crate::analysis::guarantee::state::ExecutionState;
        use crate::analysis::id::AnalysisBlockId;

        let original = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let alias = ExecutionPlace::argument(SymbolOrdinal::new(1));
        let value = ExecutionCondition::Input(original.clone());
        let mut left = ExecutionState::default();

        left.current.insert(original.clone(), value.clone());
        left.current.insert(alias.clone(), value.clone());
        left.assumptions.insert((value.clone(), true));

        let mut right = ExecutionState::default();

        right.current.insert(original.clone(), value.clone());
        right.assumptions.insert((value.clone(), true));

        let mut merged = Some(left);

        super::merge(
            &mut merged,
            &Some(right),
            Some(AnalysisBlockId::from_slot(BoundUnitId::new(0), 0)),
        );

        let merged = merged.expect("both predecessors are reachable");

        assert_eq!(merged.current[&original], value);
        assert_eq!(merged.current[&alias], ExecutionCondition::Unknown);
        assert!(merged.assumptions.contains(&(value, true)));
    }
}
