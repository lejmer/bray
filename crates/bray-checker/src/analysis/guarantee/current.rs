use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

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
    observed_components: BTreeSet<ExecutionPlace>,
}

/// Current storage owns one value tree per root. All component writes update
/// that tree, so readers never need to reconstruct an aggregate from overlays.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct CurrentValues {
    roots: BTreeMap<ExecutionPlace, CurrentRoot>,
    observed_types: Arc<BTreeMap<ExecutionPlace, Option<bray_symbols::TypeId>>>,
}

impl CurrentValues {
    pub(super) fn with_observed_types(observed_types: Arc<BTreeMap<ExecutionPlace, Option<bray_symbols::TypeId>>>) -> Self {
        Self { roots: BTreeMap::new(), observed_types }
    }

    fn observed_components<'a>(&'a self, place: &'a ExecutionPlace) -> impl Iterator<Item = &'a ExecutionPlace> {
        self.observed_types.range(place.clone()..)
            .take_while(move |(component, _)| place.contains(component))
            .map(|(component, _)| component)
            .filter(move |component| *component != place)
    }

    /// Seed the complete entry snapshot once. Observing a component does not write it.
    pub(super) fn observe_input(&mut self, place: &ExecutionPlace) {
        let root = root_place(place);

        let current = self.roots.entry(root.clone()).or_insert_with(|| CurrentRoot {
            value: ExecutionCondition::new(crate::ExecutionConditionKind::Input(root)),
            availability: ValueAvailability::Present,
            components: BTreeMap::new(),
            observed_components: BTreeSet::new(),
        });

        if !place.projections.is_empty() {
            current.observed_components.insert(place.clone());
        }
    }

    pub(super) fn slot(&self, place: &ExecutionPlace) -> Option<CurrentValue> {
        Some(CurrentValue {
            value: place.value_in(self)?,
            availability: self.availability(place),
        })
    }

    /// A retained terminal snapshot is readable, but cannot preserve live data.
    pub(super) fn live_value(&self, place: &ExecutionPlace) -> Option<ExecutionCondition> {
        let root = self.roots.get(&root_place(place))?;

        if root.availability == ValueAvailability::Absent
            || !matches!(self.availability(place), ValueAvailability::Present | ValueAvailability::Guarded(_))
            || root.components.range(place.clone()..)
                .take_while(|(component, _)| place.contains(component))
                .any(|(_, availability)| *availability == ValueAvailability::Absent) {
            return None;
        }

        place.value_in(self)
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
            value: ExecutionCondition::unknown(),
            availability: ValueAvailability::Unknown,
            components: BTreeMap::new(),
            observed_components: BTreeSet::new(),
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

            if absent.is_empty() && let Some(value) = self.live_value(place) {
                return vec![(place.clone(), value)];
            }

            let mut candidates = root.components.keys().cloned().collect::<std::collections::BTreeSet<_>>();

            candidates.extend(root.observed_components.iter().cloned());
            candidates.extend(self.observed_components(place).cloned());

            collect_owned_places(&root.value, place, &mut candidates, &mut { ExecutionCondition::WORK_LIMIT });

            let mut live = Vec::<ExecutionPlace>::new();

            for candidate in candidates {
                if absent.iter().any(|absent| absent.contains(&candidate) || candidate.contains(absent))
                    || live.iter().any(|parent| parent.contains(&candidate)) {
                    continue;
                }

                if self.live_value(&candidate).is_none() {
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

    match value.kind() {
        crate::ExecutionConditionKind::Observation { contents, kind: crate::execution_guarantees::ExecutionObservationKind::Owned, .. } => {
            collect_owned_places(contents, place, places, budget);
        }
        crate::ExecutionConditionKind::Tagged(_, contents) => {
            collect_owned_places(contents, place, places, budget);
        }
        crate::ExecutionConditionKind::Present(value) => {
            let child = place.clone().component(bray_symbols::ConstantProjectionKind::NullableValue);

            places.insert(child.clone());
            collect_owned_places(value, &child, places, budget);
        }
        crate::ExecutionConditionKind::Constructed(_, fields) => {
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

    #[test]
    fn observed_input_components_remain_live_after_a_sibling_moves() {
        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let first = root.clone().component(bray_symbols::ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));
        let second = root.clone().component(bray_symbols::ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)));
        let mut values = super::CurrentValues::default();

        values.observe_input(&first);
        values.observe_input(&second);
        values.retire(&first);
        values.observe_input(&first);

        assert_eq!(values.availability(&first), ValueAvailability::Absent);
        assert_eq!(values.availability(&second), ValueAvailability::Present);
        assert!(first.value_in(&values).is_some(), "terminal data remains readable for postconditions");
        assert!(values.live_value(&first).is_none());
        assert!(values.live_value(&root).is_none(), "a partial owner cannot preserve the retired child");
        assert_eq!(values.live_value(&second), Some(ExecutionCondition::new(crate::ExecutionConditionKind::Input(second.clone()))));

        assert_eq!(values.live_entries().collect::<Vec<_>>(),
            vec![(second.clone(), ExecutionCondition::new(crate::ExecutionConditionKind::Input(second.clone())))]);

        values.retire(&root);

        assert!(values.live_values().next().is_none());
        assert!(values.live_value(&second).is_none());
    }

    #[test]
    fn producing_and_replacing_an_opaque_owner_initializes_its_observed_components() {
        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let first = root.clone().component(bray_symbols::ConstantProjectionKind::TupleElement(SymbolOrdinal::new(0)));
        let second = root.clone().component(bray_symbols::ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1)));
        let source = ExecutionPlace::argument(SymbolOrdinal::new(1));

        let mut values = super::CurrentValues::with_observed_types(Arc::new(std::collections::BTreeMap::from([
            (first.clone(), None), (second.clone(), None),
        ])));

        assert!(values.live_values().next().is_none());

        values.set_slot(first.clone(), super::CurrentValue {
            value: ExecutionCondition::new(crate::ExecutionConditionKind::Input(source.clone())), availability: ValueAvailability::Present,
        });

        assert!(values.live_value(&root).is_none(), "initializing one field does not initialize the whole owner");

        assert_eq!(values.live_entries().collect::<Vec<_>>(),
            vec![(first.clone(), ExecutionCondition::new(crate::ExecutionConditionKind::Input(source.clone())))]);

        values.insert(root.clone(), ExecutionCondition::new(crate::ExecutionConditionKind::Input(source.clone())));

        assert_eq!(values.keys().cloned().collect::<Vec<_>>(), vec![root.clone()]);

        values.retire(&first);

        assert_eq!(values.live_entries().collect::<Vec<_>>(), vec![(second,
            ExecutionCondition::new(crate::ExecutionConditionKind::Input(source.component(bray_symbols::ConstantProjectionKind::TupleElement(SymbolOrdinal::new(1))))))]);

        values.insert(root.clone(), ExecutionCondition::new(crate::ExecutionConditionKind::Input(root)));

        assert_eq!(values.availability(&first), ValueAvailability::Present);
    }

    fn retained_owner() -> (ExecutionState, ExecutionPlace, ExecutionCondition) {
        let place = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let owner = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone()));
        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidRead");

        let claim = ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(ExecutionCondition::predicate(
            symbol.into(), crate::test_support::empty_substitution(symbol.into()), vec![owner.clone().borrowed()],
        ))));

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
        let old = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(1))));
        let sibling = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(2))));
        let replacement = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(3))));
        let mut state = ExecutionState::default();

        state.assign(root.clone(), ExecutionCondition::constructed(ExecutionCondition::new(crate::ExecutionConditionKind::Input(root.clone())),
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

        let owner = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place));
        let mut stable = crate::execution_guarantees::ObservationSet::default();

        stable.insert(owner.clone());
        stable.insert(owner.clone().borrowed());
        state.trusted_assumptions.clear();
        state.invalidate_cleanup(&stable, &[owner.clone()].into_iter().collect(), None);
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

        unknown.assign(place.clone(), ExecutionCondition::unknown(), None);

        let mut joined = Some(present);

        merge_execution(&mut joined, &Some(unknown), None.into());

        let state = joined.expect("reachable join");

        assert_eq!(state.current.get(&place), Some(&ExecutionCondition::unknown()));
        assert_eq!(claim.prove_trusted(&state.trusted_assumptions, &state.assumptions), None);
    }
    #[test]
    fn alias_joins_preserve_a_common_marker_across_different_contents_views() {
        let root = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let holder = ExecutionPlace::argument(SymbolOrdinal::new(1));

        let (_, expressions) = crate::test_support::expression_unit(bray_bound_tree::BoundUnitId::new(0), |tree, origin| {
            (0..2).map(|_| crate::test_support::push_expression(tree, crate::test_support::integer_literal_expression(origin, None))).collect()
        });

        for (owner, holder_owner) in [
            (ExecutionCondition::new(crate::ExecutionConditionKind::Input(root.clone())), ExecutionCondition::new(crate::ExecutionConditionKind::Input(holder.clone()))),
            (ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expressions[0])), ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expressions[1]))),
        ] {
        let marker = bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(0));
        let progress = bray_bound_tree::StorageProjection::TupleElement(SymbolOrdinal::new(1));
        let loan = owner.clone().borrowed();

        let left_value = loan.with_referent(ExecutionCondition::constructed(owner.clone(),
            [(progress, ExecutionCondition::boolean(true))].into()));

        let right_value = loan.with_referent(ExecutionCondition::constructed(
            ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(9)))),
            [(marker, owner.clone().project(marker)), (progress, ExecutionCondition::boolean(false))].into()));

        let symbol = crate::test_support::compiler_known_symbol::<bray_symbols::PredicateSymbolId>("ValidRead");

        let held = |value: ExecutionCondition| ExecutionCondition::new(crate::ExecutionConditionKind::Trusted(Arc::new(ExecutionCondition::predicate(
            symbol.into(), crate::test_support::empty_substitution(symbol.into()), vec![value.borrowed()],
        ))));

        let claim = held(loan.clone().project(marker));
        let block = crate::analysis::id::AnalysisBlockId::from_slot(bray_bound_tree::BoundUnitId::new(0), 42);

        assert_eq!(left_value.clone().project(marker).into_referent(), right_value.clone().project(marker).into_referent());

        for both_proven in [true, false] {
            let mut left = ExecutionState::default();

            left.current.insert(root.clone(), left_value.clone());

            left.current.insert(holder.clone(), ExecutionCondition::constructed(holder_owner.clone(),
                [(marker, left_value.clone()), (progress, ExecutionCondition::boolean(true))].into()));

            left.carry_guarantee(loan.clone(), claim.clone());
            claim.clone().assume(true, &mut left.trusted_assumptions);

            let mut right = left.clone();

            right.current.insert(root.clone(), right_value.clone());

            right.current.insert(holder.clone(), ExecutionCondition::constructed(holder_owner.clone(),
                [(marker, right_value.clone()), (progress, ExecutionCondition::boolean(false))].into()));

            if !both_proven {
                right.witnesses.clear();
                right.trusted_assumptions.clear();
            }

            let mut joined = Some(left);

            merge_execution(&mut joined, &Some(right.clone()), Some(block).into());

            let state = joined.as_ref().expect("reachable alias join");
            let current = root.value_in(&state.current).expect("loan survives the join");
            let required = held(current.clone().project(marker));

            assert_eq!(required.prove_trusted(&state.trusted_assumptions, &state.assumptions) == Some(true), both_proven);
            assert!(!crate::execution_guarantees::RetainedSnapshots::from(&[current].into()).retains_owner(&owner));

            for _ in 0..4 {
                merge_execution(&mut joined, &Some(right.clone()), Some(block).into());
            }

            assert!(!merge_execution(&mut joined, &Some(right), Some(block).into()), "alias joins must stabilize");
        }
        }
    }

}
