use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, OnceLock};

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
#[derive(Clone, Debug)]
pub(super) struct CurrentRoot {
    value: ExecutionCondition,
    availability: ValueAvailability,
    components: BTreeMap<ExecutionPlace, ValueAvailability>,
    observed_components: BTreeSet<ExecutionPlace>,
    // The owning CurrentValues keeps the same immutable observation shape across forks.
    live_parts: OnceLock<Arc<[(ExecutionPlace, ExecutionCondition)]>>,
    scalar_data: OnceLock<Arc<[ExecutionCondition]>>,
}

impl CurrentRoot {
    fn edit(root: &mut Arc<Self>) -> &mut Self {
        let root = Arc::make_mut(root);

        root.live_parts.take();
        root.scalar_data.take();

        root
    }
}

impl PartialEq for CurrentRoot {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other) || (self.value == other.value
            && self.availability == other.availability
            && self.components == other.components
            && self.observed_components == other.observed_components)
    }
}

impl Eq for CurrentRoot {}

/// Current storage owns one value tree per root. All component writes update
/// that tree, so readers never need to reconstruct an aggregate from overlays.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct CurrentValues {
    roots: BTreeMap<ExecutionPlace, Arc<CurrentRoot>>,
    observed_types: Arc<BTreeMap<ExecutionPlace, Option<bray_symbols::TypeId>>>,
    absent_at_entry: Arc<BTreeSet<ExecutionPlace>>,
    entry_borrows: Arc<BTreeSet<ExecutionPlace>>,
}

impl CurrentValues {
    pub(super) fn with_planned_places(
        observed_types: Arc<BTreeMap<ExecutionPlace, Option<bray_symbols::TypeId>>>,
        absent_at_entry: Arc<BTreeSet<ExecutionPlace>>,
    ) -> Self {
        Self { observed_types, absent_at_entry, ..Self::default() }
    }

    fn observed_components<'a>(&'a self, place: &'a ExecutionPlace) -> impl Iterator<Item = &'a ExecutionPlace> {
        self.observed_types.range(place.clone()..)
            .take_while(move |(component, _)| place.contains(component))
            .map(|(component, _)| component)
            .filter(move |component| *component != place)
    }

    /// Seed the complete entry snapshot once. Observing a component does not write it.
    pub(super) fn observe_input(&mut self, place: &ExecutionPlace) {
        assert!(!place.is_pointee(), "physical contents require an actual typed value publication");

        let root = root_place(place);

        if self.absent_at_entry.contains(&root) && !self.roots.contains_key(&root) {
            return;
        }

        let current = self.roots.entry(root.clone()).or_insert_with(|| Arc::new(CurrentRoot {
            value: ExecutionCondition::new(crate::ExecutionConditionKind::Input(root)),
            availability: ValueAvailability::Present,
            components: BTreeMap::new(),
            observed_components: BTreeSet::new(),
            live_parts: OnceLock::new(),
            scalar_data: OnceLock::new(),
        }));

        if !place.projections.is_empty() && !current.observed_components.contains(place) {
            CurrentRoot::edit(current).observed_components.insert(place.clone());
        }
    }

    /// Publish the installed entry descriptor and remember its caller-owned origin.
    pub(super) fn observe_entry_borrow(&mut self, place: ExecutionPlace) {
        self.observe_input(&place);

        let value = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone())).borrowed();

        self.insert(place.clone(), value);
        Arc::make_mut(&mut self.entry_borrows).insert(place);
    }

    /// Select actual live descriptors that reach independently owned inputs.
    pub(super) fn entry_borrows_under<'a>(&'a self, owner: &'a ExecutionPlace)
        -> impl Iterator<Item = ExecutionCondition> + 'a
    {
        self.entry_borrows.range(owner.clone()..).take_while(move |place| owner.contains(place))
            .filter_map(|place| self.live_value(place))
            .filter(|value| value.is_reference() && matches!(value.referent_owner_identity().kind(),
                crate::ExecutionConditionKind::Input(origin) if self.entry_borrows.contains(origin)))
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
        let key = root_place(place);

        let Some(root) = self.roots.get(&key) else {
            return missing_availability(&self.absent_at_entry, &key);
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

    pub(super) fn is_subtree_absent(&self, place: &ExecutionPlace) -> bool {
        self.availability(place) == ValueAvailability::Absent
            && self.roots.get(&root_place(place)).is_none_or(|root| {
                root.components.range(place.clone()..)
                    .take_while(|(component, _)| place.contains(component))
                    .all(|(_, availability)| *availability == ValueAvailability::Absent)
            })
    }

    pub(super) fn set_slot(&mut self, place: ExecutionPlace, value: CurrentValue) {
        self.write(place, value, None);
    }

    /// Publishes the assembled root once. Component values are already in it.
    pub(super) fn publish_joined_slot(&mut self, place: ExecutionPlace, slot: CurrentValue) -> bool {
        if place.projections.is_empty() {
            let changed = self.slot(&place).as_ref() != Some(&slot);

            if changed {
                self.set_slot(place, slot);
            }

            return changed;
        }

        let changed = self.availability(&place) != slot.availability;

        let Some(root) = self.roots.get_mut(&root_place(&place)) else {
            if changed {
                self.set_slot(place, slot);
            }

            return changed;
        };

        if root.components.get(&place) != Some(&slot.availability) {
            CurrentRoot::edit(root).components.insert(place, slot.availability);
        }

        changed
    }

    pub(super) fn write(&mut self, place: ExecutionPlace, value: CurrentValue,
        storage: Option<&[bray_bound_tree::StorageProjection]>) {
        let root_key = root_place(&place);

        let root = self.roots.entry(root_key).or_insert_with_key(|root| Arc::new(CurrentRoot {
            value: ExecutionCondition::unknown(),
            availability: missing_availability(&self.absent_at_entry, root),
            components: BTreeMap::new(),
            observed_components: BTreeSet::new(),
            live_parts: OnceLock::new(),
            scalar_data: OnceLock::new(),
        }));

        let root = CurrentRoot::edit(root);

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
        if value.availability == ValueAvailability::Absent && value.value.is_unknown()
            && self.roots.get(&place).is_none_or(|root| root.observed_components.is_empty())
            && self.clear_planned_root(&place)
        {
            return;
        }

        if let Some(root) = self.roots.get_mut(&root_place(&place)) {
            CurrentRoot::edit(root).components.retain(|component, _| !place.contains(component));
        }

        self.write(place, value, storage);
    }

    /// Direct lookup is for complete retained roots. Component reads use the
    /// ordinary value projection in ExecutionPlace::value_in.
    pub(super) fn get(&self, place: &ExecutionPlace) -> Option<&ExecutionCondition> {
        self.roots.get(place).map(|root| &root.value)
    }

    pub(super) fn get_mut(&mut self, place: &ExecutionPlace) -> Option<&mut ExecutionCondition> {
        self.roots.get_mut(place).map(|root| &mut CurrentRoot::edit(root).value)
    }

    pub(super) fn insert(&mut self, place: ExecutionPlace, value: ExecutionCondition) {
        self.replace(place, CurrentValue { value, availability: ValueAvailability::Present }, None);
    }

    /// Terminal formal observations outlive their local storage for contract
    /// checking. Retiring the root preserves that data without retaining it as
    /// a live owner or making the storage initialized again.
    pub(super) fn retire(&mut self, place: &ExecutionPlace) {
        let Some(root) = self.roots.get_mut(&root_place(place)) else { return; };

        let root = CurrentRoot::edit(root);

        root.components.retain(|component, _| !place.contains(component));

        if place.projections.is_empty() {
            root.availability = ValueAvailability::Absent;
        } else {
            root.components.insert(place.clone(), ValueAvailability::Absent);
        }
    }

    /// Clears a whole root when the checked plan supplies its absence.
    pub(super) fn clear_planned_root(&mut self, place: &ExecutionPlace) -> bool {
        if !place.projections.is_empty() || !self.absent_at_entry.contains(place) {
            return false;
        }

        self.roots.remove(place);

        true
    }

    pub(super) fn retire_availability(&mut self, mutation: bray_bound_tree::StorageMutation) {
        self.roots.retain(|place, _| !place.depends_on(mutation));

        let stale = |availability: &ValueAvailability| {
            matches!(availability, ValueAvailability::Guarded(guard) if guard.depends_on(mutation))
        };

        for root in self.roots.values_mut() {
            if std::iter::once(&root.availability).chain(root.components.values()).any(&stale) {
                let root = CurrentRoot::edit(root);

                for availability in std::iter::once(&mut root.availability).chain(root.components.values_mut()).filter(|availability| stale(availability)) {
                    *availability = ValueAvailability::Unknown;
                }
            }
        }
    }

    pub(super) fn keys(&self) -> impl Iterator<Item = &ExecutionPlace> {
        self.roots.iter().flat_map(|(place, root)| std::iter::once(place).chain(root.components.keys()))
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&ExecutionPlace, &ExecutionCondition)> {
        self.root_slots().map(|(place, value, _)| (place, value))
    }

    pub(super) fn root_slots(&self) -> impl Iterator<Item = (&ExecutionPlace, &ExecutionCondition, &ValueAvailability)> {
        self.roots.iter().map(|(place, root)| (place, &root.value, &root.availability))
    }

    /// Maximal live subtrees of the retained actual objects. A terminal field
    /// remains readable as data, but cannot keep its former owner alive.
    pub(super) fn live_values(&self) -> impl Iterator<Item = ExecutionCondition> + '_ {
        self.roots.iter().flat_map(|(place, root)| self.live_values_at(place, root).cloned())
    }

    pub(super) fn live_entries(&self) -> impl Iterator<Item = (ExecutionPlace, ExecutionCondition)> + '_ {
        self.roots.iter().flat_map(|(place, root)| self.live_entries_at(place, root)
            .map(|(place, value)| (place.clone(), value.clone())))
    }

    /// Reads a complete live observation already exposed by the retained root.
    pub(super) fn live_entry(&self, place: &ExecutionPlace) -> Option<&ExecutionCondition> {
        let (root_place, root) = self.roots.get_key_value(&root_place(place))?;

        self.live_entries_at(root_place, root)
            .find_map(|(candidate, value)| (candidate == place).then_some(value))
    }

    fn live_entries_at<'a>(&'a self, place: &'a ExecutionPlace, root: &'a CurrentRoot)
        -> impl Iterator<Item = (&'a ExecutionPlace, &'a ExecutionCondition)> {
        let (whole, parts) = if place.is_pointee() || root.availability == ValueAvailability::Absent {
            (None, &[][..])
        } else if matches!(root.availability, ValueAvailability::Present | ValueAvailability::Guarded(_))
            && root.components.values().all(|part| *part != ValueAvailability::Absent)
        {
            (Some((place, &root.value)), &[][..])
        } else {
            (None, root.live_parts.get_or_init(|| self.live_components(place, root).into()).as_ref())
        };

        whole.into_iter().chain(parts.iter().map(|(part, value)| (part, value)))
    }

    fn live_values_at<'a>(&'a self, place: &'a ExecutionPlace, root: &'a CurrentRoot)
        -> impl Iterator<Item = &'a ExecutionCondition> {
        let backing = (place.is_pointee() && root.availability == ValueAvailability::Present)
            .then(|| root.value.owning_storage_components());

        self.live_entries_at(place, root).map(|(_, value)| value).chain(backing.into_iter().flatten())
    }

    /// Scalar copies are a view of each retained root and share its invalidation boundary.
    pub(super) fn live_scalar_data(&self) -> impl Iterator<Item = &ExecutionCondition> + '_ {
        self.roots.iter().flat_map(|(place, root)| root.scalar_data.get_or_init(|| {
            let mut pending = self.live_values_at(place, root).collect::<Vec<_>>();
            let mut copies = Vec::new();

            while let Some(value) = pending.pop() {
                copies.extend(value.retained_scalar_data());
                pending.extend(value.owned_components());
            }

            copies.into()
        }).iter())
    }

    fn live_components(&self, place: &ExecutionPlace, root: &CurrentRoot)
        -> Vec<(ExecutionPlace, ExecutionCondition)>
    {
        let absent = root.components.iter().filter_map(|(place, availability)| {
            (*availability == ValueAvailability::Absent).then_some(place)
        }).collect::<Vec<_>>();

        let mut candidates = root.components.keys().cloned().collect::<BTreeSet<_>>();

        candidates.extend(root.observed_components.iter().cloned());
        candidates.extend(self.observed_components(place).cloned());

        collect_owned_places(&root.value, place, &mut candidates, &mut { ExecutionCondition::WORK_LIMIT });

        let mut live = Vec::<(ExecutionPlace, ExecutionCondition)>::new();

        for candidate in candidates {
            if absent.iter().any(|absent| absent.contains(&candidate) || candidate.contains(absent))
                || live.iter().any(|(parent, _)| parent.contains(&candidate)) {
                continue;
            }

            if let Some(value) = self.live_value(&candidate) {
                live.push((candidate, value));
            }
        }

        live
    }

    pub(super) fn update_values(&mut self, mut update: impl FnMut(&ExecutionCondition) -> ExecutionCondition) {
        for root in self.roots.values_mut() {
            let value = update(&root.value);

            if !std::ptr::eq(value.kind(), root.value.kind()) {
                CurrentRoot::edit(root).value = value;
            }
        }
    }

    pub(super) fn install_rewritten_values(
        &mut self,
        values: impl IntoIterator<Item = (ExecutionPlace, ExecutionCondition)>,
    ) {
        for (place, value) in values {
            let root = self.roots.get_mut(&place).expect("rewriting a current value retains its root");

            CurrentRoot::edit(root).value = value;
        }
    }

    pub(super) fn extend(&mut self, values: impl IntoIterator<Item = (ExecutionPlace, ExecutionCondition)>) {
        for (place, value) in values {
            self.insert(place, value);
        }
    }

}

fn missing_availability(
    absent_at_entry: &BTreeSet<ExecutionPlace>,
    root: &ExecutionPlace,
) -> ValueAvailability {
    if absent_at_entry.contains(root) { ValueAvailability::Absent } else { ValueAvailability::Unknown }
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

pub(super) fn root_place(place: &ExecutionPlace) -> ExecutionPlace {
    ExecutionPlace { root: place.root.clone(), projections: Arc::default() }
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

    fn value_at(&self, place: &ExecutionPlace) -> Option<ExecutionCondition> {
        if place.projections.is_empty() {
            return self.get(place).cloned()
                .or_else(|| self.absent_at_entry.contains(place).then(ExecutionCondition::unknown));
        }

        let root = root_place(place);

        let Some(value) = self.get(&root) else {
            return self.absent_at_entry.contains(&root).then(ExecutionCondition::unknown);
        };

        Some(place.projections.iter().copied().fold(value.clone(), ExecutionCondition::component))
    }

    fn formal_input(&self, _place: &ExecutionPlace) -> Option<ExecutionCondition> {
        None
    }

}

impl<'a> IntoIterator for &'a CurrentValues {
    type Item = (&'a ExecutionPlace, &'a ExecutionCondition);
    type IntoIter = std::iter::Map<
        std::collections::btree_map::Iter<'a, ExecutionPlace, Arc<CurrentRoot>>,
        fn((&'a ExecutionPlace, &'a Arc<CurrentRoot>)) -> Self::Item,
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
    use bray_bound_tree::{BoundUnitId, BoundUnitKind, StorageIdentity, StorageOrigin, StoragePath, StoragePlanBuilder, StorageTarget};
    use std::sync::Arc;

    #[test]
    fn expired_private_storage_joins_its_bypass_without_materializing_an_unknown_root() {
        use bray_bound_tree::{
            BoundExpression, BoundStructuredExpression, BoundStructuredExpressionKind,
            CheckedDependencyContracts, CheckedExpressionSemantics,
            CheckedLiteralValues, CheckedMemoryOperations, CheckedPatterns, CheckedSemanticSelections,
            Liveness, StorageAccess, StorageAccessRoot, StorageScope,
        };

        use crate::test_support::{
            TestCheckerContext, callable_entry, completed_expression_check, expression_unit,
            push_expression, semantic_values,
        };

        let unit = BoundUnitId::new(0);

        let (bound, expressions) = expression_unit(unit, |tree, origin| {
            let operand = push_expression(tree, BoundExpression::Structured(BoundStructuredExpression::new(
                origin, BoundStructuredExpressionKind::Unit, [], [], [], None, false,
            )));

            vec![push_expression(tree, BoundExpression::Structured(BoundStructuredExpression::new(
                origin, BoundStructuredExpressionKind::Tuple, [operand], [], [], None, false,
            )))]
        });

        let expression = expressions[0];

        let (types, diagnostics) = completed_expression_check(&bound, &crate::ExpressionTypeInput::new()).into_parts();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");

        let ty = types.expression(expression).expect("the one-element tuple has a checked type").ty();
        let kind = bound.key().kind();
        let scope = StorageScope::Block(bound.tree().blocks().next().expect("the callable owns its body block").0);
        let mut storage = StoragePlanBuilder::new(unit, kind);
        let identity = storage.push_identity(StorageIdentity::Temporary(expression)).expect("planned temporary");

        storage.set_identity_type(identity, ty).expect("the temporary has a checked type");
        storage.set_value_slot(expression, identity).expect("the expression produces the temporary");

        storage.push_access(StorageAccess::new(
            StorageAccessRoot::Storage(identity), [], ty, bound.key().source(), false,
        )).expect("the temporary has a complete root access");

        let storage = storage.finish();
        let entry = callable_entry(bound.key());
        let context = TestCheckerContext::new(false);
        let request = crate::CheckerUnitView::new(&bound, &entry, &context);

        let selections = CheckedSemanticSelections::try_new(&bound, &types, []).expect("the tuple constructs inline");

        let literals = CheckedLiteralValues::try_new(&bound, &types, semantic_values(),
            std::num::NonZeroU16::new(64).expect("fixture target width"), []).expect("the fixture uses no scalar literal");

        let semantics = CheckedExpressionSemantics::try_new(types, selections, literals).expect("the fixture uses one unit");
        let patterns = CheckedPatterns::new(unit, kind, [], [], [], []);

        let crate::CheckerOutcome::Complete(prepared) = crate::prepare_body_analysis(request, &semantics, &patterns, &storage)
            else { panic!("the body must prepare"); };

        let (prepared, diagnostics) = prepared.into_parts();

        assert!(!diagnostics.has_errors(), "{diagnostics:?}");

        let memory = CheckedMemoryOperations::try_new(unit, kind, [], false).expect("no memory operations");

        let dependencies = CheckedDependencyContracts::try_new(&bound, &storage, [], [], [], [], false)
            .expect("the temporary contains no dependency contract");

        let liveness = Liveness::try_new(unit, kind, [], [], [], [], [], false).expect("no retained loans");
        let contracts = Default::default();

        let flow = crate::analysis::guarantee::prepare_execution_flow(
            &prepared, request, &semantics, std::borrow::Cow::Borrowed(&[]), &storage,
            &memory, &contracts, &dependencies, &liveness,
        );

        let place = ExecutionPlace::storage_identity(&storage, identity).expect("temporary root");
        let formal = ExecutionPlace::argument(SymbolOrdinal::new(0));
        let formal_value = ExecutionCondition::new(crate::ExecutionConditionKind::Input(formal.clone()));
        let value = ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression));
        let mut bypass = flow.entry_state(&[]).expect("the callable can enter");

        assert!(bypass.current.is_subtree_absent(&place));
        assert!(bypass.current.get(&place).is_none(), "planned absence needs no entry root");
        bypass.vacate(place.clone());
        assert!(bypass.current.get(&place).is_none(), "first call setup needs no empty root");

        bypass.current.insert(formal.clone(), formal_value.clone());
        bypass.current.retire(&formal);

        let mut expired = bypass.clone();

        expired.current.insert(place.clone(), value.clone());
        flow.expire_trusted_scope(&mut expired, scope);
        assert!(expired.current.get(&place).is_none(), "lexical retirement releases the private root");

        for (first, second) in [(&bypass, &expired), (&expired, &bypass)] {
            let mut joined = Some(first.clone());

            assert!(!merge_execution(&mut joined, &Some(second.clone()), super::super::flow::ExecutionJoin::Boundary));

            let mut joined = joined.expect("both predecessors reach the join");

            assert!(joined.current.is_subtree_absent(&place));
            assert!(joined.current.get(&place).is_none(), "an absent slot contributes no materialized root");
            assert_eq!(joined.current.get(&formal), Some(&formal_value), "formal completion still reads its terminal value");
            assert_eq!(joined.current.availability(&ExecutionPlace::argument(SymbolOrdinal::new(1))), ValueAvailability::Unknown);

            joined.current.insert(place.clone(), value.clone());
            assert_eq!(joined.current.live_value(&place), Some(value.clone()));

            let mut conditional = Some(second.clone());

            assert!(merge_execution(&mut conditional, &Some(joined), super::super::flow::ExecutionJoin::Boundary));

            let conditional = conditional.expect("conditional initialization reaches the join");

            assert!(matches!(conditional.current.availability(&place), ValueAvailability::Guarded(_)));

            let mut vacant = conditional;

            vacant.vacate(place.clone());
            assert!(vacant.current.is_subtree_absent(&place));
            assert!(vacant.current.get(&place).is_none(), "repeated call setup releases the previous result");
        }
    }

    #[test]
    fn physical_snapshots_do_not_retain_owners_or_reuse_previous_producers() {
        let unit = BoundUnitId::new(0);

        let (_, expressions) = crate::test_support::integer_expression_unit(unit, 1);

        let producer = expressions[0];
        let mutation = bray_bound_tree::StorageMutation::Operation(producer.into());
        let mut storage = StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);
        let reference = storage.push_identity(StorageIdentity::Temporary(producer)).expect("test producer is in its storage unit");
        let storage = storage.finish();

        let target = StorageTarget {
            previous: false,
            origin: StorageOrigin::Returned {
                reference: StoragePath::root(reference),
                projections: Arc::new([]),
            },
        };

        let place = ExecutionPlace::pointee(&storage, &target, crate::test_support::error_type());
        let value = ExecutionCondition::new(crate::ExecutionConditionKind::Input(ExecutionPlace::argument(SymbolOrdinal::new(0))));
        let mut state = ExecutionState::default();

        state.current.insert(place.clone(), value.clone());

        assert_eq!(state.current.live_value(&place), Some(value.clone()));
        assert!(state.current.live_values().next().is_none(), "observing a pointee supplies no independent owner");

        state.current.retire(&place);

        assert!(state.current.live_value(&place).is_none());
        assert_eq!(place.value_in(&state.current), Some(value));

        let observation = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone()));

        assert!(observation.depends_on(mutation));

        state.retire_evaluation(mutation);

        assert!(state.current.get(&place).is_none(), "a genuine new producer must not inherit consumed contents");
        assert!(observation.without_evaluation(mutation).is_unknown());
        assert!(crate::execution_guarantees::ExecutionValues::formal_input(&std::collections::BTreeMap::new(), &place).is_none());
    }

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

        // A fork can retire the cached live part without changing the original owner.
        let mut fork = values.clone();

        fork.retire(&second);
        assert!(fork.live_values().next().is_none());
        assert!(values.live_value(&second).is_some());
        assert_eq!(values.live_entries().next().map(|(place, _)| place), Some(second.clone()));

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

        let mut values = super::CurrentValues::with_planned_places(Arc::new(std::collections::BTreeMap::from([
            (first.clone(), None), (second.clone(), None),
        ])), Default::default());

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
        state.invalidate_cleanup(&stable, &[owner.clone()].into_iter().collect(), None, |_| None);
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

        let (_, expressions) = crate::test_support::integer_expression_unit(bray_bound_tree::BoundUnitId::new(0), 2);

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
