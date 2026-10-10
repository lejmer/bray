use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::BoundExpressionId;

use crate::ExecutionCondition;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ExecutionState {
    pub(super) trust_boundaries: BTreeSet<BoundExpressionId>,
    pub(super) assumptions: BTreeSet<(ExecutionCondition, bool)>,
    pub(super) witness_carriers: BTreeSet<ExecutionCondition>,
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
            witness_carriers: BTreeSet::new(),
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

            if let ExecutionCondition::Constructed(_, fields) = &value {
                transferred.extend(fields.iter().map(|(_, value)| value.clone()));
            }

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

    pub(super) fn is_witness(&self, value: &ExecutionCondition) -> bool {
        let mut pending = vec![value];
        let mut carriers = BTreeSet::new();
        let mut places = BTreeSet::new();

        while let Some(value) = pending.pop() {
            if self.witness_carriers.contains(value) {
                carriers.insert(value);

                if let ExecutionCondition::Input(place) = value {
                    places.insert(place.clone());
                }
            }

            if let ExecutionCondition::Constructed(_, fields) = value {
                pending.extend(fields.iter().map(|(_, value)| value));
            }
        }

        if carriers.is_empty() {
            return false;
        }

        self.trusted_assumptions.iter().any(|(condition, holds)| {
            condition.observations().any(|observed| {
                carriers.contains(observed)
                    || matches!(observed, ExecutionCondition::Input(place) if place.overlaps_any(&places))
            }) && condition.prove(&self.assumptions, &mut { ExecutionCondition::WORK_LIMIT }) != Some(*holds)
        })
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

        // Cleanup can mutate observations through owned capabilities, just like an opaque call.
        for (place, value) in &mut self.current {
            if !place.projections.is_empty()
                && !preserved_header.is_some_and(|header| header.contains(place))
            {
                *value = ExecutionCondition::Unknown;
            }
        }
    }

    pub(super) fn join_values(
        &mut self,
        replacements: &BTreeMap<ExecutionCondition, ExecutionCondition>,
    ) {
        if replacements.is_empty() {
            return;
        }

        let rename = |value: &ExecutionCondition| value.with_joined_values(replacements);

        for facts in [&mut self.assumptions, &mut self.trusted_assumptions] {
            *facts = facts
                .iter()
                .map(|(condition, value)| (rename(condition), *value))
                .filter(|(condition, _)| *condition != ExecutionCondition::Unknown)
                .collect();
        }

        self.witness_carriers = self
            .witness_carriers
            .iter()
            .map(rename)
            .filter(|value| *value != ExecutionCondition::Unknown)
            .collect();

        self.allocation_owners = self
            .allocation_owners
            .iter()
            .map(|(condition, owner)| (rename(condition), rename(owner)))
            .filter(|(condition, _)| *condition != ExecutionCondition::Unknown)
            .collect();

        let mut dependencies =
            BTreeMap::<ExecutionCondition, BTreeSet<crate::ExecutionPlace>>::new();

        for (carrier, owners) in &self.witness_dependencies {
            let carrier = rename(carrier);

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
        self.current.retain(|observed, _| !place.contains(observed));
        self.current.insert(place, value);
    }

    pub(super) fn invalidate_trusted(&mut self, value: &ExecutionCondition) {
        self.invalidate_trusted_values([value.clone()]);
    }

    pub(super) fn invalidate_trusted_values(
        &mut self,
        values: impl IntoIterator<Item = ExecutionCondition>,
    ) {
        let mut pending = values.into_iter().collect::<Vec<_>>();

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
                        dependents
                            .entry(observed.clone())
                            .or_default()
                            .insert(carrier.clone());
                    }
                }
            }
        }

        let mut invalidated = BTreeSet::new();
        let mut invalidated_inputs = BTreeSet::new();

        while let Some(value) = pending.pop() {
            if !invalidated.insert(value.clone()) {
                continue;
            }

            if let ExecutionCondition::Constructed(_, fields) = &value {
                pending.extend(fields.iter().map(|(_, value)| value.clone()));
            }

            if let ExecutionCondition::Input(place) = &value {
                invalidated_inputs.insert(place.clone());

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
            condition.observations().any(|observed| {
                invalidated.contains(observed)
                    || matches!(observed, ExecutionCondition::Input(place) if place.overlaps_any(&invalidated_inputs))
            })
        };

        self.assumptions
            .retain(|(condition, _)| !condition.observes_borrowed_where(&observes));

        self.witness_carriers.retain(|carrier| !observes(carrier));

        self.trusted_assumptions
            .retain(|(condition, _)| !observes(condition));

        self.allocation_owners
            .retain(|condition, owner| !observes(condition) && !invalidated.contains(owner));

        self.witness_dependencies
            .retain(|carrier, _| !observes(carrier));

        for entry in self
            .entries
            .values_mut()
            .filter(|entry| entry.pending_execution)
        {
            entry
                .assumptions
                .retain(|(condition, _)| !observes(condition));

            entry
                .trusted_assumptions
                .retain(|(condition, _)| !observes(condition));
        }
    }

    pub(super) fn invalidate_trusted_places(
        &mut self,
        places: &BTreeSet<crate::ExecutionPlace>,
        values: impl IntoIterator<Item = ExecutionCondition>,
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

        self.invalidate_trusted_values(dependent.into_iter().chain(values));
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
    fn invalidation_follows_long_chains_and_cycles_without_retiring_independent_witnesses() {
        let mut state = ExecutionState::default();

        for ordinal in 1..=512 {
            let dependency = ExecutionPlace::argument(SymbolOrdinal::new(ordinal));
            let carrier = input(ordinal + 1);

            state.current.insert(dependency.clone(), input(ordinal));

            state
                .witness_dependencies
                .insert(carrier.clone(), BTreeSet::from([dependency]));

            state.witness_carriers.insert(carrier.clone());
            state.trusted_assumptions.insert((carrier, true));
        }

        let cycle = ExecutionPlace::argument(SymbolOrdinal::new(514));
        let independent = input(1000);

        state.current.insert(cycle.clone(), input(513));

        state
            .witness_dependencies
            .insert(input(1), BTreeSet::from([cycle]));

        state.witness_carriers.insert(independent.clone());

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
                state.witness_carriers,
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

            state.witness_carriers.insert(carrier.clone());
            state.trusted_assumptions.insert((carrier, true));
        }

        state.assumptions.insert((changed.clone(), true));
        state.assumptions.insert((changed.clone().borrowed(), true));
        state.trusted_assumptions.insert((captured.clone(), true));
        state.trusted_assumptions.insert((live, true));
        state.invalidate_trusted(&changed);

        assert_eq!(state.witness_carriers, BTreeSet::from([input(22)]));

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

        state.witness_carriers.extend((1..=512).map(input));
        state.witness_carriers.insert(child.clone());
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

        state.trusted_assumptions = BTreeSet::from([(
            ExecutionCondition::Entry {
                condition: Arc::new(parent),
                captured: false,
            },
            true,
        )]);

        assert!(state.is_witness(&child));
    }
}
