use std::collections::BTreeMap;

use crate::execution_guarantees::ExecutionValues;
use crate::{ExecutionCondition, ExecutionPlace};

/// Whether the observation has a value, separately from knowledge of its data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum ValueAvailability {
    Present,
    Absent,
    Guarded(ExecutionCondition),
    Unknown,
}

/// One current slot can have unknown contents without having been moved from.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CurrentValue {
    pub(super) value: ExecutionCondition,
    pub(super) availability: ValueAvailability,
}

/// One authoritative object description, with initialization observations for
/// its components. Components do not store independent copies of object data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CurrentRoot {
    value: ExecutionCondition,
    availability: ValueAvailability,
    components: BTreeMap<ExecutionPlace, ValueAvailability>,
}

/// Current storage owns one value tree per root. All component writes update
/// that tree, so readers never need to reconstruct an aggregate from overlays.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct CurrentValues {
    roots: BTreeMap<ExecutionPlace, CurrentRoot>,
}

impl CurrentValues {
    pub(super) fn slot(&self, place: &ExecutionPlace) -> Option<CurrentValue> {
        Some(CurrentValue {
            value: place.value_in(self)?,
            availability: self.availability(place),
        })
    }

    pub(super) fn availability(&self, place: &ExecutionPlace) -> ValueAvailability {
        let Some(root) = self.roots.get(&root_place(place)) else {
            return ValueAvailability::Unknown;
        };

        for length in (1..=place.projections.len()).rev() {
            let mut parent = place.clone();

            parent.projections = place.projections[..length].into();

            if let Some(availability) = root.components.get(&parent) {
                return availability.clone();
            }
        }

        root.availability.clone()
    }

    /// Publish joined observations without changing component initialization.
    pub(super) fn set_slot(&mut self, place: ExecutionPlace, value: CurrentValue) {
        self.write(place, value, None);
    }

    pub(super) fn write(&mut self, place: ExecutionPlace, value: CurrentValue,
        storage: Option<&[bray_bound_tree::StorageProjection]>) {
        let root_key = root_place(&place);
        let root = self.roots.entry(root_key).or_insert_with(|| CurrentRoot {
            value: ExecutionCondition::Unknown,
            availability: ValueAvailability::Unknown,
            components: BTreeMap::new(),
        });

        if place.projections.is_empty() {
            root.value = value.value;
            root.availability = value.availability;
        } else {
            root.value = root.value.with_component(&place.projections, value.value, storage);
            root.components.insert(place, value.availability);
        }
    }

    /// Checked replacement also replaces initialization of its whole subtree.
    pub(super) fn replace(&mut self, place: ExecutionPlace, value: CurrentValue,
        storage: Option<&[bray_bound_tree::StorageProjection]>) {
        if let Some(root) = self.roots.get_mut(&root_place(&place)) {
            root.components.retain(|component, _| !place.contains(component));
        }

        self.write(place, value, storage);
    }

    /// Direct lookup is for complete retained roots. Component reads use the
    /// ordinary value projection in ExecutionPlace::value_in.
    pub(super) fn get(&self, place: &ExecutionPlace) -> Option<&ExecutionCondition> {
        self.roots.get(place).map(|root| &root.value)
    }

    pub(super) fn get_mut(&mut self, place: &ExecutionPlace) -> Option<&mut ExecutionCondition> {
        self.roots.get_mut(place).map(|root| &mut root.value)
    }

    pub(super) fn insert(&mut self, place: ExecutionPlace, value: ExecutionCondition) {
        self.replace(place, CurrentValue { value, availability: ValueAvailability::Present }, None);
    }

    /// Terminal formal observations outlive their local storage for contract
    /// checking. Retiring the root preserves that data without retaining it as
    /// a live owner or making the storage initialized again.
    pub(super) fn retire(&mut self, place: &ExecutionPlace) {
        let Some(root) = self.roots.get_mut(&root_place(place)) else { return; };

        root.components.retain(|component, _| !place.contains(component));

        if place.projections.is_empty() {
            root.availability = ValueAvailability::Absent;
        } else {
            root.components.insert(place.clone(), ValueAvailability::Absent);
        }
    }

    pub(super) fn retire_availability(&mut self, mutation: bray_bound_tree::StorageMutation) {
        for root in self.roots.values_mut() {
            for availability in std::iter::once(&mut root.availability).chain(root.components.values_mut()) {
                if matches!(availability, ValueAvailability::Guarded(guard) if guard.depends_on(mutation)) {
                    *availability = ValueAvailability::Unknown;
                }
            }
        }
    }

    pub(super) fn keys(&self) -> impl Iterator<Item = &ExecutionPlace> {
        self.roots.iter().flat_map(|(place, root)| std::iter::once(place).chain(root.components.keys()))
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&ExecutionPlace, &ExecutionCondition)> {
        self.roots.iter().map(|(place, root)| (place, &root.value))
    }

    /// Maximal live subtrees of the retained actual objects. A terminal field
    /// remains readable as data, but cannot keep its former owner alive.
    pub(super) fn live_values(&self) -> impl Iterator<Item = ExecutionCondition> + '_ {
        self.live_entries().map(|(_, value)| value)
    }

    pub(super) fn live_entries(&self) -> impl Iterator<Item = (ExecutionPlace, ExecutionCondition)> + '_ {
        self.roots.iter().flat_map(|(place, root)| {
            if root.availability == ValueAvailability::Absent {
                return Vec::new();
            }

            let absent = root.components.iter().filter_map(|(place, availability)| {
                (*availability == ValueAvailability::Absent).then_some(place)
            }).collect::<Vec<_>>();

            if absent.is_empty() {
                return vec![(place.clone(), root.value.clone())];
            }

            let mut candidates = root.components.keys().cloned().collect::<std::collections::BTreeSet<_>>();

            collect_owned_places(&root.value, place, &mut candidates, &mut { ExecutionCondition::WORK_LIMIT });
            let mut live = Vec::<ExecutionPlace>::new();

            for candidate in candidates {
                if absent.iter().any(|absent| absent.contains(&candidate) || candidate.contains(absent))
                    || live.iter().any(|parent| parent.contains(&candidate)) {
                    continue;
                }

                live.push(candidate);
            }

            live.into_iter().filter_map(|place| place.value_in(self).map(|value| (place, value))).collect()
        })
    }

    pub(super) fn values_mut(&mut self) -> impl Iterator<Item = &mut ExecutionCondition> {
        self.roots.values_mut().map(|root| &mut root.value)
    }

    pub(super) fn retain(&mut self, mut retain: impl FnMut(&ExecutionPlace, &ExecutionCondition) -> bool) {
        self.roots.retain(|place, root| retain(place, &root.value));
    }

    pub(super) fn extend(&mut self, values: impl IntoIterator<Item = (ExecutionPlace, ExecutionCondition)>) {
        for (place, value) in values {
            self.insert(place, value);
        }
    }


}

fn collect_owned_places(
    value: &ExecutionCondition,
    place: &ExecutionPlace,
    places: &mut std::collections::BTreeSet<ExecutionPlace>,
    budget: &mut usize,
) {
    let Some(remaining) = budget.checked_sub(1) else { return; };

    *budget = remaining;

    match value {
        ExecutionCondition::Observation { contents, kind: crate::execution_guarantees::ExecutionObservationKind::Owned, .. } => {
            collect_owned_places(contents, place, places, budget);
        }
        ExecutionCondition::Tagged(_, contents) => {
            collect_owned_places(contents, place, places, budget);
        }
        ExecutionCondition::Present(value) => {
            let child = place.clone().component(bray_symbols::ConstantProjectionKind::NullableValue);

            places.insert(child.clone());
            collect_owned_places(value, &child, places, budget);
        }
        ExecutionCondition::Constructed(_, fields) => {
            for (projection, value) in fields.iter() {
                let Some(projection) = ExecutionPlace::projection_kind(*projection) else { continue; };
                let child = place.clone().component(projection);

                places.insert(child.clone());
                collect_owned_places(value, &child, places, budget);
            }
        }
        _ => {}
    }
}

fn root_place(place: &ExecutionPlace) -> ExecutionPlace {
    let mut root = place.clone();

    root.projections = [].into();
    root
}

impl FromIterator<(ExecutionPlace, ExecutionCondition)> for CurrentValues {
    fn from_iter<T: IntoIterator<Item = (ExecutionPlace, ExecutionCondition)>>(values: T) -> Self {
        let mut current = Self::default();

        current.extend(values);
        current
    }
}

impl<const N: usize> From<[(ExecutionPlace, ExecutionCondition); N]> for CurrentValues {
    fn from(values: [(ExecutionPlace, ExecutionCondition); N]) -> Self {
        values.into_iter().collect()
    }
}

impl ExecutionValues for CurrentValues {
    fn get_value(&self, place: &ExecutionPlace) -> Option<&ExecutionCondition> {
        self.get(place)
    }

    fn get_value_mut(&mut self, place: &ExecutionPlace) -> Option<&mut ExecutionCondition> {
        self.get_mut(place)
    }

    fn formal_input(&self, _place: &ExecutionPlace) -> Option<ExecutionCondition> {
        None
    }

}

impl<'a> IntoIterator for &'a CurrentValues {
    type Item = (&'a ExecutionPlace, &'a ExecutionCondition);
    type IntoIter = std::iter::Map<
        std::collections::btree_map::Iter<'a, ExecutionPlace, CurrentRoot>,
        fn((&'a ExecutionPlace, &'a CurrentRoot)) -> Self::Item,
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.roots.iter().map(|(place, root)| (place, &root.value))
    }
}

#[cfg(test)]
mod tests {
    use super::ValueAvailability;
    use crate::analysis::guarantee::flow::merge_execution;
    use crate::analysis::guarantee::state::ExecutionState;
    use crate::{ExecutionCondition, ExecutionPlace};
    use bray_symbols::SymbolOrdinal;
    use std::sync::Arc;

    fn retained_owner() -> (ExecutionState, ExecutionPlace, ExecutionCondition) {
        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let owner = ExecutionCondition::Input(place.clone());
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidRead");

        let claim = ExecutionCondition::Trusted(Arc::new(ExecutionCondition::predicate(
            symbol.into(), crate::test_support::empty_substitution(symbol.into()), vec![owner.clone().borrowed()],
        )));

        let mut state = ExecutionState::default();

        state.assign(place.clone(), owner.clone(), None);
        state.carry_guarantee(owner, claim.clone());
        state.trusted_assumptions.insert((claim.clone(), true));

        (state, place, claim)
    }

    #[test]
    fn component_writes_keep_one_object_and_retirement_keeps_only_terminal_data() {
        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let left = bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let right = bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(1));
        let old = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));
        let sibling = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(2)));
        let replacement = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(3)));
        let mut state = ExecutionState::default();

        state.assign(root.clone(), ExecutionCondition::constructed(ExecutionCondition::Input(root.clone()),
            [(left, old), (right, sibling.clone())].into()), None);
        let field = root.clone().project(&[left]).expect("tuple field");

        state.assign(field.clone(), replacement.clone(), Some(&[left]));
        assert_eq!(state.current.iter().count(), 1);
        assert_eq!(field.value_in(&state.current), Some(replacement.clone()));
        state.current.retire(&field);
        assert_eq!(field.value_in(&state.current), Some(replacement.clone()), "terminal data remains observable");
        let live = state.current.live_values().flat_map(|value| state.owned_values(&value)).collect::<Vec<_>>();

        assert!(!live.contains(&replacement), "retired field must not retain its former owner");
        assert!(live.contains(&sibling), "independent live field retains its actual owner");
    }

    #[test]
    fn available_carrier_does_not_reprove_an_unestablished_clause_after_mutation() {
        let (mut state, place, claim) = retained_owner();
        let owner = ExecutionCondition::Input(place);
        let mut stable = crate::execution_guarantees::ObservationSet::default();

        stable.insert(owner.clone());
        stable.insert(owner.clone().borrowed());
        state.trusted_assumptions.clear();
        state.invalidate_cleanup(&stable, &[owner.clone()].into_iter().collect());
        assert!(state.witnesses.contains_key(&owner), "the carrier is still a possible owner");
        assert_eq!(claim.prove_trusted(&state.trusted_assumptions, &state.assumptions), None,
            "possible ownership must not establish a predicate clause");
    }

    #[test]
    fn conditional_move_keeps_authority_only_for_initialized_cleanup() {
        let (present, place, claim) = retained_owner();

        let mut absent = ExecutionState::default();

        absent.vacate(place.clone());

        for (left, right) in [(present.clone(), absent.clone()), (absent.clone(), present)] {
            let mut joined = Some(left);

            merge_execution(&mut joined, &Some(right.clone()), None.into());

            let state = joined.as_ref().expect("reachable join");

            let ValueAvailability::Guarded(guard) = state.current.availability(&place) else {
                panic!("conditionally initialized owner requires a guard");
            };

            assert_eq!(claim.prove_trusted(&state.trusted_assumptions, &state.assumptions), None);

            let mut cleanup = state.assumptions.clone();

            guard.assume(true, &mut cleanup);
            assert_eq!(claim.prove_trusted(&state.trusted_assumptions, &cleanup), Some(true));
            assert!(!merge_execution(&mut joined, &Some(right), None.into()), "availability joins must converge");
        }
    }

    #[test]
    fn separate_joins_do_not_share_initialization_truth() {
        let (present, place, _) = retained_owner();

        let mut absent = ExecutionState::default();

        absent.vacate(place.clone());

        let guards = [0, 1].map(|index| {
            let mut joined = Some(present.clone());
            let block = crate::analysis::id::AnalysisBlockId::from_slot(bray_bound_tree::BoundUnitId::new(0), index);

            merge_execution(&mut joined, &Some(absent.clone()), Some(block).into());

            let ValueAvailability::Guarded(guard) = joined.expect("reachable join").current.availability(&place) else {
                panic!("conditionally initialized owner requires a guard");
            };

            guard
        });

        let mut assumptions = Default::default();

        guards[0].clone().assume(true, &mut assumptions);
        assert_ne!(guards[0], guards[1]);
        assert_eq!(guards[1].prove(&assumptions, &mut { ExecutionCondition::WORK_LIMIT }), None);
    }

    #[test]
    fn unknown_storage_does_not_become_an_absent_predecessor() {
        let (present, place, claim) = retained_owner();

        let mut unknown = ExecutionState::default();

        unknown.assign(place.clone(), ExecutionCondition::Unknown, None);

        let mut joined = Some(present);

        merge_execution(&mut joined, &Some(unknown), None.into());

        let state = joined.expect("reachable join");

        assert_eq!(state.current.get(&place), Some(&ExecutionCondition::Unknown));
        assert_eq!(claim.prove_trusted(&state.trusted_assumptions, &state.assumptions), None);
    }
}
