use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::BoundExpressionId;

use crate::ExecutionCondition;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ExecutionState {
    pub(super) trust_boundaries: BTreeSet<BoundExpressionId>,
    pub(super) assumptions: BTreeSet<(ExecutionCondition, bool)>,
    pub(super) witness_carriers: BTreeMap<ExecutionCondition, BTreeSet<ExecutionCondition>>,
    pub(super) witness_dependencies: BTreeMap<ExecutionCondition, BTreeSet<crate::ExecutionPlace>>,
    pub(super) trusted_assumptions: BTreeSet<(ExecutionCondition, bool)>,
    pub(super) allocation_owners: BTreeMap<ExecutionCondition, ExecutionCondition>,
    pub(super) current: BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    pub(super) expressions: BTreeMap<BoundExpressionId, ExecutionCondition>,
    pub(super) pending_results: BTreeMap<BoundExpressionId, ExecutionCondition>,
    pub(super) result: ExecutionCondition,
    pub(super) entries: BTreeMap<bray_bound_tree::BoundExecutionSite, crate::ExecutionCallEvidence>,
    pub(super) completion_dependencies: BTreeSet<crate::ExecutionCompletionDependency>,
}

impl Default for ExecutionState {
    fn default() -> Self {
        Self {
            trust_boundaries: BTreeSet::new(),
            assumptions: BTreeSet::new(),
            witness_carriers: BTreeMap::new(),
            witness_dependencies: BTreeMap::new(),
            trusted_assumptions: BTreeSet::new(),
            allocation_owners: BTreeMap::new(),
            current: BTreeMap::new(),
            expressions: BTreeMap::new(),
            pending_results: BTreeMap::new(),
            result: ExecutionCondition::Unknown,
            entries: BTreeMap::new(),
            completion_dependencies: BTreeSet::new(),
        }
    }
}

impl ExecutionState {
    pub(super) fn call_trusted_assumptions(
        &self,
        transferred: impl IntoIterator<Item = ExecutionCondition>,
    ) -> BTreeSet<(ExecutionCondition, bool)> {
        let mut transferred = transferred.into_iter().collect::<Vec<_>>();
        let mut owners = BTreeSet::new();

        while let Some(value) = transferred.pop() {
            if value == ExecutionCondition::Unknown {
                continue;
            }

            transferred.extend(value.owned_components().cloned());

            owners.insert(value);
        }

        self.trusted_assumptions
            .iter()
            .filter(|(condition, _)| {
                let mut pending = vec![condition];

                while let Some(condition) = pending.pop() {
                    if let Some(owner) = self.allocation_owners.get(condition)
                        && !owners.contains(owner)
                    {
                        return false;
                    }

                    match condition {
                        ExecutionCondition::Trusted(condition) => pending.push(condition),
                        ExecutionCondition::Operation(_, operands) => {
                            pending.extend(operands.iter())
                        }
                        _ => {}
                    }
                }

                true
            })
            .cloned()
            .collect()
    }

    pub(super) fn carry_guarantee(
        &mut self,
        carrier: ExecutionCondition,
        condition: ExecutionCondition,
    ) {
        self.witness_carriers
            .entry(carrier.owner_identity().clone())
            .or_default()
            .insert(condition);
    }

    pub(super) fn directly_carries_witness(&self, value: &ExecutionCondition) -> bool {
        self.witness_carriers
            .get(value.owner_identity())
            .is_some_and(|conditions| {
                conditions.iter().any(|condition| {
                    (self
                        .trusted_assumptions
                        .contains(&(condition.clone(), true))
                        || condition.prove_trusted(&self.trusted_assumptions, &self.assumptions)
                            == Some(true))
                        && condition
                            .prove(&self.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                            != Some(true)
                })
            })
    }

    pub(super) fn is_witness(&self, value: &ExecutionCondition) -> bool {
        self.witness_carriers.keys().any(|carrier| {
            value
                .owned_values()
                .any(|value| carrier.is_owned_projection_of(value))
                && self.directly_carries_witness(carrier)
        })
    }

    pub(super) fn witness_projection(&self, value: &ExecutionCondition) -> (bool, bool) {
        let mut whole = self.is_witness(value);
        let mut partial = false;
        let mut projected_part = false;
        let mut value = value;

        while let ExecutionCondition::Projection(projection, parent) = value {
            projected_part |= *projection != bray_symbols::ConstantProjectionKind::NullableValue;

            if self.directly_carries_witness(parent) {
                whole |= !projected_part;
                partial |= projected_part;
            }

            value = parent;
        }

        (whole, partial)
    }
    pub(super) fn invalidate_cleanup(&mut self, preserved_header: Option<&crate::ExecutionPlace>) {
        self.assumptions
            .retain(|(condition, _)| !condition.observes_borrowed(None));

        self.witness_carriers.clear();
        self.witness_dependencies.clear();
        self.trusted_assumptions.clear();
        self.allocation_owners.clear();

        for entry in self
            .entries
            .values_mut()
            .filter(|entry| entry.pending_execution)
        {
            entry.assumptions.clear();
            entry.trusted_assumptions.clear();
            entry.trusted_boundary = false;
        }

        // Opaque cleanup may mutate through capabilities. Whole snapshots contain field
        // observations too, so retaining a root would resurrect its old projected contents.
        for (place, value) in &mut self.current {
            if !preserved_header.is_some_and(|header| header.contains(place)) {
                *value = ExecutionCondition::Unknown;
            }
        }
    }

    pub(super) fn join_values(
        &mut self,
        replacements: &BTreeMap<ExecutionCondition, ExecutionCondition>,
        identities: &BTreeMap<ExecutionCondition, ExecutionCondition>,
    ) {
        if replacements.is_empty() && identities.is_empty() {
            return;
        }

        let rename =
            |value: &ExecutionCondition| value.with_joined_values(replacements, identities);

        let rename_identity =
            |value: &ExecutionCondition| value.with_joined_values(identities, identities);

        for facts in [&mut self.assumptions, &mut self.trusted_assumptions] {
            *facts = facts
                .iter()
                .map(|(condition, value)| (rename(condition), *value))
                .filter(|(condition, _)| *condition != ExecutionCondition::Unknown)
                .collect();
        }

        let mut carriers = BTreeMap::<ExecutionCondition, BTreeSet<ExecutionCondition>>::new();

        for (carrier, conditions) in &self.witness_carriers {
            let carrier = rename_identity(carrier);

            if carrier != ExecutionCondition::Unknown {
                carriers.entry(carrier).or_default().extend(
                    conditions
                        .iter()
                        .map(rename)
                        .filter(|condition| *condition != ExecutionCondition::Unknown),
                );
            }
        }

        self.witness_carriers = carriers;

        self.allocation_owners = self
            .allocation_owners
            .iter()
            .map(|(condition, owner)| (rename(condition), rename_identity(owner)))
            .filter(|(condition, _)| *condition != ExecutionCondition::Unknown)
            .collect();

        let mut dependencies =
            BTreeMap::<ExecutionCondition, BTreeSet<crate::ExecutionPlace>>::new();

        for (carrier, owners) in &self.witness_dependencies {
            let carrier = rename_identity(carrier);

            if carrier != ExecutionCondition::Unknown {
                dependencies
                    .entry(carrier)
                    .or_default()
                    .extend(owners.iter().cloned());
            }
        }

        self.witness_dependencies = dependencies;

        for value in self.current.values_mut() {
            *value = rename(value);
        }
    }

    pub(super) fn assign(&mut self, place: crate::ExecutionPlace, value: ExecutionCondition) {
        let replaced = self
            .current
            .range(place.clone()..)
            .take_while(|(observed, _)| place.contains(observed))
            .map(|(observed, _)| observed.clone())
            .collect::<Vec<_>>();

        for observed in replaced {
            self.current.remove(&observed);
        }

        for length in 0..place.projections.len() {
            let mut parent = place.clone();

            parent.projections = place.projections[..length].into();

            if let Some(snapshot) = self.current.get_mut(&parent) {
                *snapshot = snapshot.with_component(&place.projections[length..], value.clone());
            }
        }

        self.current.insert(place, value);
    }

    pub(super) fn invalidate_trusted(&mut self, value: &ExecutionCondition) {
        self.invalidate_trusted_values([value.clone()]);
    }

    pub(super) fn invalidate_trusted_values(
        &mut self,
        values: impl IntoIterator<Item = ExecutionCondition>,
    ) {
        self.retire_trusted_values(values, &BTreeSet::new());
    }

    pub(super) fn retire_trusted_values(
        &mut self,
        values: impl IntoIterator<Item = ExecutionCondition>,
        retained: &BTreeSet<ExecutionCondition>,
    ) {
        self.retire_values(values, [], retained, &BTreeSet::new());
    }

    fn retire_values(
        &mut self,
        values: impl IntoIterator<Item = ExecutionCondition>,
        dependents: impl IntoIterator<Item = ExecutionCondition>,
        retained: &BTreeSet<ExecutionCondition>,
        stable_predicates: &BTreeSet<bray_symbols::PredicateDefinitionSymbolId>,
    ) {
        let mut pending = values.into_iter().collect::<Vec<_>>();
        let mut directly_changed = crate::execution_guarantees::ObservationSet::default();

        if !stable_predicates.is_empty() {
            for value in &pending {
                for component in value.owned_values() {
                    directly_changed.insert(component.clone());
                }
            }
        }

        pending.extend(dependents);

        if pending.is_empty() {
            return;
        }

        let mut dependents = BTreeMap::<ExecutionCondition, BTreeSet<ExecutionCondition>>::new();

        let mut input_dependents =
            BTreeMap::<crate::ExecutionPlace, BTreeSet<ExecutionCondition>>::new();

        // Index current dependencies once, then visit only carriers reached by the worklist.
        for (carrier, dependencies) in &self.witness_dependencies {
            for dependency in dependencies
                .iter()
                .filter_map(|place| place.value_in(&self.current))
            {
                for observed in dependency.observations() {
                    if let ExecutionCondition::Input(place) = observed {
                        input_dependents
                            .entry(place.clone())
                            .or_default()
                            .insert(carrier.clone());
                    } else {
                        let mut owner = observed;

                        loop {
                            dependents
                                .entry(owner.owner_identity().clone())
                                .or_default()
                                .insert(carrier.clone());

                            let ExecutionCondition::Projection(_, parent) = owner else {
                                break;
                            };

                            owner = parent;
                        }
                    }
                }
            }
        }

        let mut retained_observations = crate::execution_guarantees::ObservationSet::default();

        for value in retained {
            retained_observations.insert(value.clone());
        }

        let mut invalidated = BTreeSet::new();
        let mut invalidated_observations = crate::execution_guarantees::ObservationSet::default();

        while let Some(value) = pending.pop() {
            if retained_observations.contains(&value) || !invalidated.insert(value.clone()) {
                continue;
            }

            pending.extend(value.owned_components().cloned());

            if value.owner_identity() != &value {
                pending.push(value.owner_identity().clone());
            }

            invalidated_observations.insert(value.clone());

            if let ExecutionCondition::Input(place) = &value {
                for length in 0..place.projections.len() {
                    let mut prefix = place.clone();

                    prefix.projections = place.projections[..length].into();

                    if let Some(carriers) = input_dependents.get(&prefix) {
                        pending.extend(carriers.iter().cloned());
                    }
                }

                for (_, carriers) in input_dependents
                    .range(place.clone()..)
                    .take_while(|(observed, _)| place.contains(observed))
                {
                    pending.extend(carriers.iter().cloned());
                }
            } else if let Some(carriers) = dependents.get(&value) {
                pending.extend(carriers.iter().cloned());
            }
        }

        let observes = |condition: &ExecutionCondition| {
            condition
                .observations_except(|value| retained_observations.contains(value))
                .any(|observed| invalidated_observations.overlaps(observed))
        };

        let stable = |condition: &ExecutionCondition| {
            let predicate = match condition {
                ExecutionCondition::Trusted(value) => value.as_ref(),
                value => value,
            };

            matches!(predicate, ExecutionCondition::Predicate(symbol, _, _) if stable_predicates.contains(symbol))
                && !condition
                    .observations()
                    .any(|value| directly_changed.overlaps(value))
        };

        let revokes = |condition: &ExecutionCondition| observes(condition) && !stable(condition);

        self.assumptions
            .retain(|(condition, _)| !condition.observes_borrowed_where(&observes));

        let mut retired_conditions = BTreeSet::new();

        self.witness_carriers.retain(|carrier, conditions| {
            let retired = invalidated_observations.contains(carrier)
                && !retained_observations.contains(carrier);

            conditions.retain(|condition| {
                if (retired && (directly_changed.contains(carrier) || !stable(condition)))
                    || revokes(condition)
                {
                    condition.clone().assume(true, &mut retired_conditions);

                    false
                } else {
                    true
                }
            });

            !conditions.is_empty()
        });

        // Another live carrier may independently establish the same condition.
        for conditions in self.witness_carriers.values() {
            for condition in conditions {
                condition
                    .clone()
                    .assume_with(true, &mut |condition, holds| {
                        retired_conditions.remove(&(condition, holds));
                    });
            }
        }

        self.trusted_assumptions
            .retain(|fact| !revokes(&fact.0) && !retired_conditions.contains(fact));

        self.allocation_owners
            .retain(|condition, owner| !observes(condition) && !invalidated.contains(owner));

        self.witness_dependencies.retain(|carrier, _| {
            !invalidated_observations.contains(carrier)
                || retained_observations.contains(carrier)
                || self.witness_carriers.contains_key(carrier)
        });

        for entry in self
            .entries
            .values_mut()
            .filter(|entry| entry.pending_execution)
        {
            entry
                .assumptions
                .retain(|fact| !revokes(&fact.0) && !retired_conditions.contains(fact));

            entry
                .trusted_assumptions
                .retain(|fact| !revokes(&fact.0) && !retired_conditions.contains(fact));
        }
    }

    pub(super) fn invalidate_trusted_places(
        &mut self,
        places: &BTreeSet<crate::ExecutionPlace>,
        values: impl IntoIterator<Item = ExecutionCondition>,
    ) {
        self.invalidate_contents(places, values, &BTreeSet::new());
    }

    pub(super) fn invalidate_contents(
        &mut self,
        places: &BTreeSet<crate::ExecutionPlace>,
        values: impl IntoIterator<Item = ExecutionCondition>,
        stable_predicates: &BTreeSet<bray_symbols::PredicateDefinitionSymbolId>,
    ) {
        self.assumptions.retain(|(condition, _)| {
            !condition.observes_borrowed_where(|value| {
                value.observations().any(|observed| {
                    matches!(observed,
                ExecutionCondition::Input(place) if place.overlaps_any(places))
                })
            })
        });

        let dependent = self
            .witness_dependencies
            .iter()
            .filter(|(_, owners)| owners.iter().any(|owner| owner.overlaps_any(places)))
            .map(|(carrier, _)| carrier.clone())
            .collect::<Vec<_>>();

        self.retire_values(values, dependent, &BTreeSet::new(), stable_predicates);
    }

    pub(super) fn invalidate_trusted_roots(
        &mut self,
        roots: &BTreeSet<crate::execution_guarantees::ExecutionInput>,
    ) {
        self.assumptions.retain(|(condition, _)| !condition.observes_borrowed_where(|value| {
            value.observations().any(|observed| matches!(observed, ExecutionCondition::Input(place) if roots.contains(&place.root)))
        }));

        let dependent = self
            .witness_dependencies
            .iter()
            .filter(|(_, owners)| owners.iter().any(|owner| roots.contains(&owner.root)))
            .map(|(carrier, _)| carrier.clone())
            .collect::<Vec<_>>();

        self.invalidate_trusted_values(dependent);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use bray_symbols::{ConstantProjectionKind, SymbolOrdinal};

    use super::ExecutionState;
    use crate::{ExecutionCondition, ExecutionPlace};

    fn input(ordinal: u32) -> ExecutionCondition {
        ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(ordinal)))
    }

    #[test]
    fn content_changes_keep_spatial_evidence_but_never_extend_its_carrier_or_storage() {
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>(
            "ValidWrite",
        );

        let substitution = crate::test_support::empty_substitution(symbol.into());
        let owner = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let carrier = input(1);

        let spatial = ExecutionCondition::Trusted(Arc::new(ExecutionCondition::Predicate(
            symbol.into(),
            substitution,
            [carrier.clone(), ExecutionCondition::Boolean(true)].into(),
        )));

        let observed = input(0).borrowed();
        let mut state = ExecutionState::default();

        state.current.insert(owner.clone(), input(0));

        state
            .witness_dependencies
            .insert(carrier.clone(), BTreeSet::from([owner.clone()]));

        state.carry_guarantee(carrier.clone(), spatial.clone());
        state.carry_guarantee(carrier.clone(), observed.clone());

        state
            .trusted_assumptions
            .extend([(spatial.clone(), true), (observed, true)]);

        let stable = BTreeSet::from([symbol.into()]);

        state.invalidate_contents(&BTreeSet::from([owner.clone()]), [input(0)], &stable);

        assert_eq!(
            state.trusted_assumptions,
            BTreeSet::from([(spatial.clone(), true)])
        );

        assert!(state.witness_dependencies.contains_key(&carrier));

        let mut ended = state.clone();

        ended.invalidate_trusted_roots(&BTreeSet::from([owner.root]));
        assert!(ended.trusted_assumptions.is_empty());
        assert!(ended.witness_dependencies.is_empty());

        // A carrier can establish a condition about a separate pointer. Overwriting
        // that carrier must revoke the condition even though the pointer is unchanged.
        let external = input(2);

        state.witness_carriers.clear();
        state.witness_dependencies.clear();
        state.carry_guarantee(external.clone(), spatial.clone());
        state.invalidate_contents(&BTreeSet::new(), [external], &stable);
        assert!(state.trusted_assumptions.is_empty());
        assert!(state.witness_carriers.is_empty());

        let mut overwritten = ExecutionState::default();

        overwritten.carry_guarantee(carrier.clone(), spatial.clone());
        overwritten.trusted_assumptions.insert((spatial, true));
        overwritten.invalidate_contents(&BTreeSet::new(), [carrier], &stable);
        assert!(overwritten.trusted_assumptions.is_empty());
    }

    #[test]
    fn retiring_a_container_preserves_only_its_transferred_child_value() {
        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));

        let left = root
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

        let right = root
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)));

        let child = ExecutionCondition::Input(left);
        let mut state = ExecutionState::default();

        for value in [
            ExecutionCondition::Input(root.clone()),
            child.clone(),
            ExecutionCondition::Input(right),
        ] {
            state.carry_guarantee(value.clone(), value.clone());
            state.trusted_assumptions.insert((value, true));
        }

        let retained = BTreeSet::from([child.clone()]);
        let mut moved = state.clone();

        moved.retire_trusted_values([ExecutionCondition::Input(root.clone())], &retained);

        assert_eq!(
            moved
                .witness_carriers
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>(),
            retained
        );

        assert_eq!(
            moved.trusted_assumptions,
            BTreeSet::from([(child.clone(), true)])
        );

        // Retaining a value cannot extend the storage required by its witness.
        state
            .witness_dependencies
            .insert(child, BTreeSet::from([root.clone()]));

        state.invalidate_trusted_roots(&BTreeSet::from([root.root]));
        state.retire_trusted_values([ExecutionCondition::Input(root)], &retained);

        assert!(state.witness_carriers.is_empty());
        assert!(state.trusted_assumptions.is_empty());
    }

    #[test]
    fn invalidation_follows_long_chains_and_cycles_without_retiring_independent_witnesses() {
        let mut state = ExecutionState::default();

        for ordinal in 1..=512 {
            let dependency = ExecutionPlace::argument(SymbolOrdinal::new(ordinal));
            let carrier = input(ordinal + 1);

            state.current.insert(dependency.clone(), input(ordinal));

            state
                .witness_dependencies
                .insert(carrier.clone(), BTreeSet::from([dependency]));

            state.carry_guarantee(carrier.clone(), carrier.clone());
            state.trusted_assumptions.insert((carrier, true));
        }

        let cycle = ExecutionPlace::argument(SymbolOrdinal::new(514));
        let independent = input(1000);

        state.current.insert(cycle.clone(), input(513));

        state
            .witness_dependencies
            .insert(input(1), BTreeSet::from([cycle]));

        state.carry_guarantee(independent.clone(), independent.clone());

        state
            .trusted_assumptions
            .insert((independent.clone(), true));

        for batch in 0..3 {
            let mut state = state.clone();

            if batch == 1 {
                state.invalidate_trusted_roots(&BTreeSet::from([
                    ExecutionPlace::argument(SymbolOrdinal::new(1)).root,
                    ExecutionPlace::argument(SymbolOrdinal::new(256)).root,
                ]));
            } else if batch == 2 {
                state.invalidate_trusted_places(
                    &BTreeSet::from([
                        ExecutionPlace::argument(SymbolOrdinal::new(1)),
                        ExecutionPlace::argument(SymbolOrdinal::new(256)),
                    ]),
                    [input(1), input(256)],
                );
            } else {
                state.invalidate_trusted(&input(1));
            }

            assert_eq!(
                state
                    .witness_carriers
                    .keys()
                    .cloned()
                    .collect::<BTreeSet<_>>(),
                BTreeSet::from([independent.clone()])
            );

            assert_eq!(
                state.trusted_assumptions,
                BTreeSet::from([(independent.clone(), true)])
            );

            assert!(state.witness_dependencies.is_empty());
        }
    }

    #[test]
    fn invalidation_overlaps_parent_and_child_paths_but_preserves_siblings_and_snapshots() {
        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));

        let left = root
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

        let child = left
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));

        let right = root
            .clone()
            .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)));

        let changed = ExecutionCondition::Input(left);

        let captured = ExecutionCondition::Entry {
            condition: Arc::new(changed.clone()),
            captured: true,
        };

        let live = ExecutionCondition::Entry {
            condition: Arc::new(changed.clone()),
            captured: false,
        };

        let mut state = ExecutionState::default();

        for (place, ordinal) in [root, child, right].into_iter().zip(0u32..) {
            let dependency = ExecutionPlace::argument(SymbolOrdinal::new(ordinal + 10));
            let carrier = input(ordinal + 20);

            state
                .current
                .insert(dependency.clone(), ExecutionCondition::Input(place));

            state
                .witness_dependencies
                .insert(carrier.clone(), BTreeSet::from([dependency]));

            state.carry_guarantee(carrier.clone(), carrier.clone());
            state.trusted_assumptions.insert((carrier, true));
        }

        state.assumptions.insert((changed.clone(), true));
        state.assumptions.insert((changed.clone().borrowed(), true));
        state.trusted_assumptions.insert((captured.clone(), true));
        state.trusted_assumptions.insert((live, true));
        state.invalidate_trusted(&changed);

        assert_eq!(
            state
                .witness_carriers
                .keys()
                .cloned()
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([input(22)])
        );

        assert_eq!(
            state.trusted_assumptions,
            BTreeSet::from([(captured, true), (input(22), true)])
        );

        assert_eq!(state.assumptions, BTreeSet::from([(changed, true)]));
        assert_eq!(state.witness_dependencies.len(), 1);
    }

    #[test]
    fn witness_lookup_preserves_component_overlap_and_entry_snapshots() {
        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));

        let child = ExecutionCondition::Input(
            root.clone()
                .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0))),
        );

        let sibling = ExecutionCondition::Input(
            root.clone()
                .component(ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1))),
        );

        let parent = ExecutionCondition::Input(root);
        let mut state = ExecutionState::default();

        for value in (1..=512).map(input) {
            state.carry_guarantee(value.clone(), value);
        }

        state.carry_guarantee(child.clone(), parent.clone());
        state.trusted_assumptions.insert((parent.clone(), true));

        assert!(state.is_witness(&child));
        assert!(!state.is_witness(&sibling));

        state.assumptions.insert((parent.clone(), true));

        assert!(!state.is_witness(&child));

        state.assumptions.clear();

        state.trusted_assumptions = BTreeSet::from([(
            ExecutionCondition::Entry {
                condition: Arc::new(parent.clone()),
                captured: true,
            },
            true,
        )]);

        assert!(!state.is_witness(&child));

        let live = ExecutionCondition::Entry {
            condition: Arc::new(parent),
            captured: false,
        };

        state.witness_carriers.clear();
        state.carry_guarantee(child.clone(), live.clone());
        state.trusted_assumptions = BTreeSet::from([(live, true)]);

        assert!(state.is_witness(&child));
    }
    #[test]
    fn retiring_a_carrier_revokes_its_unobserved_guarantee_but_keeps_moved_children() {
        let child = input(1);

        let owner = ExecutionCondition::Constructed(
            Arc::new(input(0)),
            [(
                bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(0)),
                child.clone(),
            )]
            .into(),
        );

        let owner_condition = input(20);
        let child_condition = input(21);
        let mut state = ExecutionState::default();

        state.carry_guarantee(owner.clone(), owner_condition.clone());
        state.carry_guarantee(child.clone(), child_condition.clone());

        state.trusted_assumptions.extend([
            (owner_condition.clone(), true),
            (child_condition.clone(), true),
        ]);

        state.retire_trusted_values([owner.clone()], &BTreeSet::from([child.clone()]));
        assert!(!state.witness_carriers.contains_key(&owner));
        assert!(state.is_witness(&child));
        assert!(!state.trusted_assumptions.contains(&(owner_condition, true)));
        assert!(state.trusted_assumptions.contains(&(child_condition, true)));
    }
}
