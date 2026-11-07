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

    pub(super) fn vacate(&mut self, place: ExecutionPlace) {
        self.set_slot(place, CurrentValue {
            value: ExecutionCondition::Unknown,
            availability: ValueAvailability::Absent,
        });
    }

    pub(super) fn remove(&mut self, place: &ExecutionPlace) {
        self.slots.remove(place);
    }

    pub(super) fn clear(&mut self) {
        self.slots.clear();
    }

    pub(super) fn keys(&self) -> impl Iterator<Item = &ExecutionPlace> {
        self.slots.keys()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&ExecutionPlace, &ExecutionCondition)> {
        self.slots.iter().map(|(place, value)| (place, &value.value))
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
