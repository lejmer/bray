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

/// Current storage observations retain their checked ownership availability.
/// Callable entry arguments use the same read-only data lookup without this state.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct CurrentValues {
    slots: BTreeMap<ExecutionPlace, CurrentValue>,
}

impl CurrentValues {
    pub(super) fn slot(&self, place: &ExecutionPlace) -> Option<&CurrentValue> {
        self.slots.get(place)
    }

    pub(super) fn availability(&self, place: &ExecutionPlace) -> ValueAvailability {
        for length in (0..=place.projections.len()).rev() {
            let mut parent = place.clone();

            parent.projections = place.projections[..length].into();

            if let Some(value) = self.slots.get(&parent) {
                return value.availability.clone();
            }
        }

        ValueAvailability::Unknown
    }

    pub(super) fn set_slot(&mut self, place: ExecutionPlace, value: CurrentValue) {
        self.slots.insert(place, value);
    }

    pub(super) fn get(&self, place: &ExecutionPlace) -> Option<&ExecutionCondition> {
        self.slots.get(place).map(|value| &value.value)
    }

    pub(super) fn get_mut(&mut self, place: &ExecutionPlace) -> Option<&mut ExecutionCondition> {
        self.slots.get_mut(place).map(|value| &mut value.value)
    }

    /// Install a value after checked initialization or assignment.
    pub(super) fn insert(&mut self, place: ExecutionPlace, value: ExecutionCondition) {
        self.set_slot(place, CurrentValue { value, availability: ValueAvailability::Present });
    }

    /// Update contents without changing the slot's ownership availability.
    pub(super) fn observe(&mut self, place: ExecutionPlace, value: ExecutionCondition) {
        let availability = self.availability(&place);

        self.set_slot(place, CurrentValue { value, availability });
    }

    pub(super) fn remove(&mut self, place: &ExecutionPlace) {
        self.slots.remove(place);
    }

    pub(super) fn retire_availability(&mut self, mutation: bray_bound_tree::StorageMutation) {
        for slot in self.slots.values_mut() {
            if matches!(&slot.availability, ValueAvailability::Guarded(guard) if guard.depends_on(mutation)) {
                slot.availability = ValueAvailability::Unknown;
            }
        }
    }

    pub(super) fn keys(&self) -> impl Iterator<Item = &ExecutionPlace> {
        self.slots.keys()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&ExecutionPlace, &ExecutionCondition)> {
        self.slots.iter().map(|(place, value)| (place, &value.value))
    }

    pub(super) fn iter_mut(&mut self) -> impl Iterator<Item = (&ExecutionPlace, &mut ExecutionCondition)> {
        self.slots.iter_mut().map(|(place, value)| (place, &mut value.value))
    }

    pub(super) fn range(
        &self,
        range: impl std::ops::RangeBounds<ExecutionPlace>,
    ) -> impl Iterator<Item = (&ExecutionPlace, &ExecutionCondition)> {
        self.slots.range(range).map(|(place, value)| (place, &value.value))
    }

    pub(super) fn values(&self) -> impl Iterator<Item = &ExecutionCondition> {
        self.slots.values().map(|value| &value.value)
    }

    pub(super) fn values_mut(&mut self) -> impl Iterator<Item = &mut ExecutionCondition> {
        self.slots.values_mut().map(|value| &mut value.value)
    }

    pub(super) fn retain(&mut self, mut retain: impl FnMut(&ExecutionPlace, &ExecutionCondition) -> bool) {
        self.slots.retain(|place, value| retain(place, &value.value));
    }

    pub(super) fn extend(&mut self, values: impl IntoIterator<Item = (ExecutionPlace, ExecutionCondition)>) {
        for (place, value) in values {
            self.insert(place, value);
        }
    }

    pub(super) fn observe_all(&mut self, values: impl IntoIterator<Item = (ExecutionPlace, ExecutionCondition)>) {
        for (place, value) in values {
            self.observe(place, value);
        }
    }
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
}

impl<'a> IntoIterator for &'a CurrentValues {
    type Item = (&'a ExecutionPlace, &'a ExecutionCondition);
    type IntoIter = std::iter::Map<
        std::collections::btree_map::Iter<'a, ExecutionPlace, CurrentValue>,
        fn((&'a ExecutionPlace, &'a CurrentValue)) -> Self::Item,
    >;

    fn into_iter(self) -> Self::IntoIter {
        self.slots.iter().map(|(place, value)| (place, &value.value))
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
