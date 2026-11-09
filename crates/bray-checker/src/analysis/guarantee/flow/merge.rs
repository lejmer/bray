use crate::execution_guarantees::ExecutionObservationKind as ObservationKind;
use std::collections::BTreeMap;

use crate::execution_guarantees::common_components;

use crate::execution_guarantees::ExecutionCondition;

use super::super::super::id::AnalysisBlockId;
use super::super::state::ExecutionState;
use super::super::current::{CurrentValue, ValueAvailability};

/// Finite identity of the control-flow join that observes initialization.
#[derive(Clone, Copy)]
pub(in crate::analysis) enum ExecutionJoin {
    Boundary,
    Block(AnalysisBlockId),
    Result(bray_bound_tree::BoundExpressionId),
}

impl From<Option<AnalysisBlockId>> for ExecutionJoin {
    fn from(block: Option<AnalysisBlockId>) -> Self {
        block.map_or(Self::Boundary, Self::Block)
    }
}

impl ExecutionJoin {
    fn block(self) -> Option<AnalysisBlockId> {
        match self {
            Self::Block(block) => Some(block),
            Self::Boundary | Self::Result(_) => None,
        }
    }

    fn snapshot(self, place: &crate::ExecutionPlace) -> ExecutionCondition {
        match self {
            Self::Block(block) => joined_observation(block, place),
            Self::Result(expression) => ExecutionCondition::Expression(expression),
            Self::Boundary => ExecutionCondition::Boolean(true),
        }
    }
}

pub(in crate::analysis) fn merge(
    target: &mut Option<ExecutionState>,
    incoming: &Option<ExecutionState>,
    join: ExecutionJoin,
) -> bool {
    let Some(incoming) = incoming else {
        return false;
    };

    let Some(target) = target else {
        // The first reachable predecessor supplies an independently owned flow environment.
        *target = Some(incoming.clone());

        return true;
    };

    let (incoming_joined, mut changed) = join_observations(target, incoming, join);

    let incoming = &incoming_joined;
    let dependencies = target.completion_dependencies.len();

    target
        .completion_dependencies
        .extend(incoming.completion_dependencies.iter().copied());

    changed |= dependencies != target.completion_dependencies.len();

    for (owner, entries) in &incoming.deferred_invocations {
        changed |= target
            .deferred_invocations
            .entry(owner.clone())
            .or_default()
            .merge(entries);
    }

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
        if let Some(previous) = target.witnesses.get_mut(carrier) {
            changed |= previous.merge(witness);
        } else {
            target
                .witnesses
                .insert(carrier.clone(), std::sync::Arc::clone(witness));

            changed = true;
        }
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
    join: ExecutionJoin,
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
    let mut owned_results = BTreeMap::new();
    let mut joined_facts = Vec::new();
    let mut needs_condition_proof = false;
    let (stable_values, stable_identities) = stable_observations(target, incoming);
    let mut availability = BTreeMap::new();
    let mut availability_guards = std::collections::BTreeSet::new();
    let mut availability_facts = Vec::new();

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

        let left_available = resolved_availability(target, place);
        let right_available = resolved_availability(incoming, place);

        let slot = join_current_value(
            (target, &value, &left_available), (incoming, &other, &right_available),
            place, join, &mut joined_facts,
        );

        let joined = slot.value;
        let available = slot.availability;

        if left_available != right_available
            && let ValueAvailability::Guarded(guard) = &available
        {
            availability_facts.push((guard.clone(), left_available, right_available));

            if let Some((_, owner)) = joined.ownership_identity() {
                availability_guards.insert((owner.clone(), guard.clone()));
            }
        }

        joins |= target.current.availability(place) != available
            || incoming.current.availability(place) != available;

        availability.insert(place.clone(), available);

        needs_condition_proof |= join_current_truth(
            (&value, &target.assumptions), (&other, &incoming.assumptions),
            &joined, &mut joined_facts,
        );

        joins |= place.value_in(&target.current).as_ref() != Some(&joined)
            || place.value_in(&incoming.current).as_ref() != Some(&joined);

        places.push((place.clone(), joined.clone()));
    }

    // Assemble the same complete snapshots that field assignment will expose before
    // renaming predicates. A joined field must not change its parent's meaning later.
    let mut joined_values = places.iter().cloned().collect::<BTreeMap<_, _>>();

    for (place, value) in &places {
        super::super::state::update_parent_observations(&mut joined_values, place, value, None);
    }

    // Availability is evidence about the predecessor's actual initialized storage,
    // independent of the contents or any authority carried by that storage.
    for (guard, left, right) in availability_facts {
        assume_availability(target, &guard, left);
        assume_availability(incoming_joined.to_mut(), &guard, right);
    }

    for (place, joined) in &joined_values {
        if *joined == ExecutionCondition::Unknown { continue; }

        let left = place
            .value_in(&target.current)
            .unwrap_or(ExecutionCondition::Unknown);

        let right = place
            .value_in(&incoming.current)
            .unwrap_or(ExecutionCondition::Unknown);

        if left != right
            || target.owned_results.get(&left) != incoming.owned_results.get(&right)
        {
            owned_results.insert(
                joined.clone(),
                super::witnesses::owned_alternatives(
                    (target, &left),
                    (incoming, &right),
                    joined,
                ),
            );
        }
    }

    for (state, values, identities) in [
        (&*target, &mut left, &mut left_identities),
        (incoming, &mut right, &mut right_identities),
    ] {
        needs_condition_proof |= current_replacements(
            state,
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
        .into_iter().filter(|_| join.block().is_some())
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
            (&target.assumptions, &incoming.assumptions),
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

        if value != other || target.owned_results.get(value) != incoming.owned_results.get(other) {
            owned_results.insert(
                joined.clone(),
                super::witnesses::owned_alternatives((target, value), (incoming, other), &joined),
            );
        }

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
            &availability_guards,
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
            let slot = CurrentValue {
                value,
                availability: availability.remove(&place).expect("joined place has availability"),
            };

            changed |= target.current.slot(&place).as_ref() != Some(&slot);
            target.current.set_slot(place.clone(), slot.clone());
            incoming.current.set_slot(place, slot);
        }

        for (expression, value) in results {
            changed |= target.pending_results.get(&expression) != Some(&value);
            target.pending_results.insert(expression, value.clone());
            incoming.pending_results.insert(expression, value);
        }
    }

    changed |= publish_owned_results(target, &mut incoming_joined, owned_results);

    (incoming_joined, changed)
}

/// Join possible contents over the initialized predecessors. Missing storage
/// contributes no value, whereas unknown data remains a conservative value.
fn join_current_value(
    left: (&ExecutionState, &ExecutionCondition, &ValueAvailability),
    right: (&ExecutionState, &ExecutionCondition, &ValueAvailability),
    place: &crate::ExecutionPlace,
    join: ExecutionJoin,
    facts: &mut Vec<(ExecutionCondition, bool)>,
) -> CurrentValue {
    let result = || join.block().map(|block| joined_observation(block, place));

    let value = if *left.2 == ValueAvailability::Absent {
        right.1.clone()
    } else if *right.2 == ValueAvailability::Absent || left.1 == right.1 {
        left.1.clone()
    } else if *left.1 == ExecutionCondition::Unknown || *right.1 == ExecutionCondition::Unknown {
        ExecutionCondition::Unknown
    } else {
        common_components(left.1, right.1, result(), (&left.0.assumptions, &right.0.assumptions), facts,
            &mut { ExecutionCondition::WORK_LIMIT })
    };

    let availability = if left.2 == right.2 {
        left.2.clone()
    } else if matches!(left.2, ValueAvailability::Unknown) || matches!(right.2, ValueAvailability::Unknown) {
        ValueAvailability::Unknown
    } else {
        ValueAvailability::Guarded(ExecutionCondition::InitializedSnapshot(
            place.clone(), std::sync::Arc::new(join.snapshot(place)),
            std::sync::Arc::new(value.owner_identity().clone()),
        ))
    };

    CurrentValue { value, availability }
}

/// Only independently live objects can prevent rebasing a moved result. Retired
/// storage keeps terminal data, but that snapshot is no longer another owner.
pub(super) fn stable_observations(
    left: &ExecutionState,
    right: &ExecutionState,
) -> (
    std::collections::BTreeSet<ExecutionCondition>,
    std::collections::BTreeSet<ExecutionCondition>,
) {
    let right_values = right.current.live_entries().collect::<BTreeMap<_, _>>();
    let mut values = std::collections::BTreeSet::new();
    let mut identities = std::collections::BTreeSet::new();

    for (place, value) in left.current.live_entries() {
        let Some(other) = right_values.get(&place) else { continue; };

        if resolved_availability(left, &place) == ValueAvailability::Absent
            || resolved_availability(right, &place) == ValueAvailability::Absent
        {
            continue;
        }

        if let Some((role, owner)) = value.ownership_identity()
            && other.ownership_identity() == Some((role, owner))
        {
            identities.insert(owner.clone());
        }

        if &value == other {
            values.insert(value.clone().borrowed());
            values.insert(value);
        }
    }

    (values, identities)
}

fn resolved_availability(state: &ExecutionState, place: &crate::ExecutionPlace) -> ValueAvailability {
    match state.current.availability(place) {
        ValueAvailability::Guarded(guard) => match guard.prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT }) {
            Some(true) => ValueAvailability::Present,
            Some(false) => ValueAvailability::Absent,
            None => ValueAvailability::Guarded(guard),
        },
        available => available,
    }
}

fn assume_availability(state: &mut ExecutionState, guard: &ExecutionCondition, available: ValueAvailability) {
    let condition = match available {
        ValueAvailability::Present => (guard.clone(), true),
        ValueAvailability::Absent => (guard.clone(), false),
        ValueAvailability::Guarded(previous) if &previous == guard => return,
        ValueAvailability::Guarded(previous) => (
            ExecutionCondition::operation(bray_bound_tree::BoundOperator::Equal, vec![guard.clone(), previous]), true,
        ),
        ValueAvailability::Unknown => return,
    };

    condition.0.assume(condition.1, &mut state.assumptions);
}

fn joined_observation(block: AnalysisBlockId, place: &crate::ExecutionPlace) -> ExecutionCondition {
    ExecutionCondition::Joined(
        block.unit(),
        u32::try_from(block.to_index().expect("flow block has an index"))
            .expect("flow block index fits"),
        place.clone(),
    )
}

fn join_current_truth(
    left: (&ExecutionCondition, &std::collections::BTreeSet<(ExecutionCondition, bool)>),
    right: (&ExecutionCondition, &std::collections::BTreeSet<(ExecutionCondition, bool)>),
    joined: &ExecutionCondition,
    facts: &mut Vec<(ExecutionCondition, bool)>,
) -> bool {
    if left.0 == right.0 || *joined == ExecutionCondition::Unknown {
        return false;
    }

    let needs_proof = [left.0, right.0].into_iter().any(|value| {
        matches!(value, ExecutionCondition::Literal(_) | ExecutionCondition::Boolean(_))
    });

    let [Some(left), Some(right)] = [left, right].map(|(value, assumptions)| {
        value.prove(assumptions, &mut { ExecutionCondition::WORK_LIMIT })
    }) else {
        return needs_proof;
    };

    if left == right {
        facts.push((joined.clone(), left));
    }

    needs_proof
}

fn publish_owned_results(
    target: &mut ExecutionState,
    incoming: &mut std::borrow::Cow<'_, ExecutionState>,
    owned_results: BTreeMap<
        ExecutionCondition,
        bray_symbols::DependencyOwnedResult<bray_bound_tree::StorageProjection, ExecutionCondition>,
    >,
) -> bool {
    let mut changed = false;

    for (value, ownership) in owned_results {
        if ownership.is_empty() {
            changed |= target.owned_results.remove(&value).is_some();
            incoming.to_mut().owned_results.remove(&value);
            continue;
        }

        changed |= target.owned_results.get(&value) != Some(&ownership);

        target
            .owned_results
            .insert(value.clone(), ownership.clone());

        incoming.to_mut().owned_results.insert(value, ownership);
    }

    changed
}

fn current_replacements(
    state: &ExecutionState,
    joined_values: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    stable_values: &std::collections::BTreeSet<ExecutionCondition>,
    stable_identities: &std::collections::BTreeSet<ExecutionCondition>,
    map: &mut BTreeMap<ExecutionCondition, ExecutionCondition>,
    identities: &mut BTreeMap<ExecutionCondition, ExecutionCondition>,
) -> bool {
    let mut needs_condition_proof = false;

    for (place, joined) in joined_values {
        // Terminal snapshots remain readable for postconditions. They cannot
        // rename data retained by live storage through another alias.
        if resolved_availability(state, place) == ValueAvailability::Absent {
            continue;
        }

        let value = place
            .value_in(&state.current)
            .unwrap_or(ExecutionCondition::Unknown);

        let value = &value;

        // Rename the complete current observation only for an established owner.
        // Both predecessors must still establish the predicate after renaming.
        // Snapshots share immutable components, so borrowed keys need no tree copy.
        if value != joined
            && value.ownership_identity().is_some_and(|(_, owner)| owner.observation_path().is_some())
            && joined.ownership_identity().is_some_and(|(_, owner)| owner.observation_path().is_some())
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
            || value == joined.owner_identity() && !matches!(joined, ExecutionCondition::Observation { kind: ObservationKind::Owned | ObservationKind::Reference, .. })
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
            ExecutionCondition::Constructed(_, _) | ExecutionCondition::Observation { kind: ObservationKind::Owned | ObservationKind::Reference, .. } | ExecutionCondition::Borrowed(_) | ExecutionCondition::Present(_)
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

        if let (Some((role, owner)), Some((joined_role, joined_owner))) = (value.ownership_identity(), joined.ownership_identity())
            && role == joined_role && owner != joined_owner && !stable_identities.contains(owner)
        {
            insert_replacement(identities, owner, joined_owner);
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

pub(super) fn result_replacements(
    value: &ExecutionCondition,
    joined: &ExecutionCondition,
    stable_values: &std::collections::BTreeSet<ExecutionCondition>,
    stable_identities: &std::collections::BTreeSet<ExecutionCondition>,
    values: &mut BTreeMap<ExecutionCondition, ExecutionCondition>,
    identities: &mut BTreeMap<ExecutionCondition, ExecutionCondition>,
) {
    // A retained component is a complete unchanged observation. Shield it from
    // broader substitutions of the container's changed contents.
    if value == joined {
        if let Some((_, owner)) = value.ownership_identity() {
            values.insert(value.clone(), value.clone());
            identities.insert(owner.clone(), owner.clone());
        }
        return;
    }

    if value == joined.owner_identity() && !matches!(joined, ExecutionCondition::Observation { kind: ObservationKind::Owned | ObservationKind::Reference, .. })
        || joined == &ExecutionCondition::Unknown
        || matches!(
            value,
            ExecutionCondition::Literal(_)
                | ExecutionCondition::Boolean(_)
                | ExecutionCondition::Unknown
        )
    {
        return;
    }

    if let Some((ObservationKind::Reference, owner)) = value.ownership_identity() {
        if let Some((ObservationKind::Reference, joined_owner)) = joined.ownership_identity() {
            if !stable_values.contains(value) {
                insert_replacement(values, value, joined);
            }

            if owner != joined_owner && !stable_identities.contains(owner) {
                insert_replacement(identities, owner, joined_owner);
            }

            reference_data_replacements(&value.clone().into_referent(), &joined.clone().into_referent(),
                stable_values, values, &mut { ExecutionCondition::WORK_LIMIT });
        }

        return;
    }

    if !stable_values.contains(value) {
        insert_replacement(values, value, joined);
    }

    // A complete owned snapshot also names its tag through the base observation.
    // Move that observation with its payload, preserving any independently live alias.
    if let (Some((role, owner)), Some((joined_role, joined_owner))) = (value.ownership_identity(), joined.ownership_identity())
        && role == joined_role
    {
        if owner != value && owner != joined_owner
            && !stable_values.contains(owner) && !stable_identities.contains(owner)
        {
            insert_replacement(values, owner, joined_owner);
        }

        if owner != joined_owner && !stable_identities.contains(owner) {
            insert_replacement(identities, owner, joined_owner);
        }
    }

    match value {
        ExecutionCondition::Observation { contents, kind: ObservationKind::Owned | ObservationKind::Reference, .. } => {
            let source = value.current_referent().unwrap_or(contents);

            let joined = joined.current_referent().unwrap_or_else(|| match joined {
                ExecutionCondition::Observation { contents, kind: ObservationKind::Owned | ObservationKind::Reference, .. } => contents.as_ref(),
                value => value,
            });

            result_replacements(source, joined, stable_values, stable_identities, values, identities);
        }
        _ => {
            let mut projections = value.computed_value().component_projections();
            projections.extend(joined.computed_value().component_projections());

            for field in projections {
                result_replacements(
                    &value.clone().project(field), &joined.clone().project(field),
                    stable_values, stable_identities, values, identities,
                );
            }
        }
    }
}

fn reference_data_replacements(
    source: &ExecutionCondition,
    joined: &ExecutionCondition,
    stable_values: &std::collections::BTreeSet<ExecutionCondition>,
    values: &mut BTreeMap<ExecutionCondition, ExecutionCondition>,
    budget: &mut usize,
) {
    let Some(remaining) = budget.checked_sub(1) else { return; };

    *budget = remaining;

    if let ExecutionCondition::Observation { contents, kind: ObservationKind::Owned, .. } = source {
        reference_data_replacements(contents, joined, stable_values, values, budget);
        return;
    }
    if let ExecutionCondition::Observation { contents, kind: ObservationKind::Owned, .. } = joined {
        reference_data_replacements(source, contents, stable_values, values, budget);
        return;
    }

    if source == joined {
        // An unchanged datum shields itself from replacements of its changed
        // container, including implicit fields exposed by the joined snapshot.
        values.insert(source.clone(), source.clone());
        return;
    }

    if !stable_values.contains(source)
        && !matches!(source, ExecutionCondition::Unknown | ExecutionCondition::Literal(_) | ExecutionCondition::Boolean(_))
    {
        // Rebase actual reached data, without replacing any carrier identity.
        insert_replacement(values, source, joined);
    }

    let mut projections = source.component_projections();
    projections.extend(joined.component_projections());

    for field in projections {
        reference_data_replacements(&source.clone().project(field), &joined.clone().project(field),
            stable_values, values, budget);
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

#[cfg(test)]
mod tests {
    use super::{BTreeMap, ExecutionCondition, ExecutionState, current_replacements};

    #[test]
    fn retired_snapshots_cannot_rename_live_loan_data() {
        let place = crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0));
        let owner = ExecutionCondition::Input(crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(1)));
        let source = owner.clone().borrowed();
        let field = bray_bound_tree::StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0));
        let joined = source.with_referent(ExecutionCondition::constructed(owner,
            [(field, ExecutionCondition::Boolean(false))].into()));

        for retired in [false, true] {
            let mut state = ExecutionState::default();
            state.current.insert(place.clone(), source.clone());
            if retired { state.current.retire(&place); }
            let mut values = BTreeMap::new();
            let mut identities = BTreeMap::new();
            current_replacements(&state, &[(place.clone(), joined.clone())].into(),
                &Default::default(), &Default::default(), &mut values, &mut identities);

            assert_eq!(values.is_empty(), retired, "only a live observation contributes a value replacement");
            assert!(identities.is_empty(), "loan contents cannot replace their physical identity");
            assert_eq!(state.current.get(&place), Some(&source), "terminal data remains available for contract checking");
        }
    }
    #[test]
    fn reference_data_join_preserves_unchanged_nested_data() {
        let (_, expressions) = crate::test_support::expression_unit(bray_bound_tree::BoundUnitId::new(0), |tree, origin| {
            vec![crate::test_support::push_expression(tree, crate::test_support::integer_literal_expression(origin, None))]
        });
        let source = ExecutionCondition::Expression(expressions[0]);
        let field = bray_bound_tree::StorageProjection::TupleElement(bray_symbols::SymbolOrdinal::new(0));
        let marker = source.clone().project(field).project(field).project(field);
        let data = ExecutionCondition::Input(crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(9)))
            .with_component(&[bray_symbols::ConstantProjectionKind::TupleElement(bray_symbols::SymbolOrdinal::new(0)); 3], marker.clone(), None);
        let joined = ExecutionCondition::owned_value(source.clone(), data);
        let mut values = BTreeMap::new();
        super::reference_data_replacements(&source, &joined, &Default::default(), &mut values,
            &mut { ExecutionCondition::WORK_LIMIT });

        assert_eq!(marker.with_joined_values(&values, &BTreeMap::new()), marker,
            "joining changed containers must not erase unchanged nested data");
    }

}
