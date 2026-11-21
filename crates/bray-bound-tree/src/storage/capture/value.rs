use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use crate::{StorageCapturedTransition, StorageContents, StorageDependencyCompleteness, StorageProjection, StorageTarget as Target, StorageValueBinding};
use super::selection::{selected_storage_values, storage_selector_parent, storage_value_subtree};

/// Inline physical information carried by one evaluated value.
/// Its root is a projection coordinate, never a unique runtime allocation or permission.
#[derive(Clone, Debug, Eq, Ord, PartialOrd)]
pub struct StorageCapturedValue {
    root: Target,
    state: std::sync::Arc<StorageCapturedTransition>,
    state_hash: u64,
}

impl Hash for StorageCapturedValue {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        self.root.hash(hasher);
        self.state_hash.hash(hasher);
    }
}

impl PartialEq for StorageCapturedValue {
    fn eq(&self, other: &Self) -> bool {
        self.root == other.root && self.state_hash == other.state_hash && self.state == other.state
    }
}

impl StorageCapturedValue {
    /// Couples an already captured inline subtree with its projection coordinate.
    pub fn new(root: Target, state: StorageCapturedTransition) -> Self {
        let state_hash = Self::state_hash(&state);

        Self { root, state: std::sync::Arc::new(state), state_hash }
    }

    /// Selects inline data without reading through a pointer or owning allocation.
    pub fn project(&self, projection: StorageProjection) -> Self {
        Self {
            root: self.root.project(&[projection]),
            state: std::sync::Arc::clone(&self.state),
            state_hash: self.state_hash,
        }
    }

    /// Inline selectors present in this captured snapshot, without inspecting its type.
    pub fn component_projections(&self) -> impl Iterator<Item = StorageProjection> + '_ {
        storage_value_subtree(&self.state.values, &self.root)
            .filter_map(|(target, _)| target.projections().get(self.root.projections().len()).copied())
    }

    /// Attaches prepared observation roles at the producing value's checked boundary.
    /// Existing roles came with the actual value and take precedence over an opaque view.
    pub fn with_observations(mut self, observations: impl IntoIterator<Item = (impl AsRef<[StorageProjection]>, crate::StorageObservationKind)>) -> Self {
        let mut prepared = BTreeMap::<_, std::collections::BTreeSet<_>>::new();

        for (path, kind) in observations {
            let target = self.root.project(path.as_ref());

            if self.state.values.get(&target).is_none_or(|value| !value.is_no_payload() && value.observations.is_empty()) {
                prepared.entry(target).or_default().insert(kind);
            }
        }

        if prepared.is_empty() { return self; }

        let state = std::sync::Arc::make_mut(&mut self.state);

        for (target, kinds) in prepared {
            state.values.entry(target).or_default().observations = kinds;
        }

        self.state_hash = Self::state_hash(state);

        self
    }

    /// Visits actual checked observation roles relative to this captured value.
    pub fn observations(&self) -> impl Iterator<Item = (&[StorageProjection], crate::StorageObservationKind)> + '_ {
        storage_value_subtree(&self.state.values, &self.root)
            .flat_map(move |(target, value)| value.observations.iter().map(move |kind|
                (&target.projections()[self.root.projections().len()..], *kind)))
    }

    /// Installs this captured value's existing information in the produced storage coordinates.
    pub fn materialize(&self, destination: &Target) -> StorageCapturedTransition {
        let destination_of = |target: &Target| {
            if destination == &self.root {
                return target.clone();
            }

            destination.project(&target.projections()[self.root.projections().len()..])
        };

        StorageCapturedTransition {
            values: storage_value_subtree(&self.state.values, &self.root)
                .map(|(target, value)| (destination_of(target), value.clone())).collect(),
            payloads: storage_value_subtree(&self.state.payloads, &self.root)
                .map(|(target, value)| (destination_of(target), value.clone())).collect(),
            ..Default::default()
        }
    }

    /// The value binding at this snapshot's current projection coordinate.
    pub fn binding(&self) -> Option<&StorageValueBinding> {
        self.state.values.get(&self.root)
    }

    /// The storage coordinate of the captured inline value.
    pub fn storage_coordinate(&self) -> Option<&crate::StoragePath> {
        self.root.storage_path()
    }

    /// Attaches the evaluated input's retained support to the same captured value.
    pub fn with_dependency_support(mut self, support: std::sync::Arc<crate::ReportDependencies>) -> Self {
        let state = std::sync::Arc::make_mut(&mut self.state);

        state.values.entry(self.root.clone()).or_default().include_dependency_support(&support);

        self.state_hash = Self::state_hash(state);

        self
    }

    /// Visits support carried by the selected value and its inline components.
    pub fn dependency_support(&self) -> impl Iterator<Item = &std::sync::Arc<crate::ReportDependencies>> + '_ {
        storage_value_subtree(&self.state.values, &self.root)
            .filter_map(|(_, value)| value.dependency_support())
    }

    /// Returns the captured address only when it establishes one current actual target.
    pub fn known_address(&self) -> Option<&Target> {
        self.binding()?.address.as_ref()?.single_location().filter(|target| target.is_exact())
    }

    /// Mutation operands of the evaluated descriptor, including independent conservative bounds.
    pub fn mutation_targets(&self) -> Option<Vec<crate::StorageMutationTarget>> {
        self.binding()?.mutation_targets()
    }

    /// Conservative address regions carried by this descriptor, with no ownership conversion.
    pub fn address_regions(&self) -> Option<Vec<Target>> {
        self.binding()?.reference_regions()
    }

    /// Possible regions carried by this value's inline descriptors.
    pub fn carried_regions(&self) -> impl Iterator<Item = Target> + '_ {
        storage_value_subtree(&self.state.values, &self.root)
            .flat_map(|(_, value)| value.reference_regions().into_iter().flatten()
                .chain(value.owned_backing().and_then(crate::StorageAddressValue::regions).into_iter().flatten()))
    }

    /// All regions directly carried by this snapshot, when no descriptor or backing is unknown.
    pub fn complete_carried_regions(&self) -> Option<std::collections::BTreeSet<Target>> {
        let mut regions = self.carried_sources().cloned().collect::<std::collections::BTreeSet<_>>();

        for (_, value) in storage_value_subtree(&self.state.values, &self.root) {
            if value.is_no_payload() { continue; }

            let mut references = value.carried_references().peekable();

            if value.unknown_dependencies() || references.peek().is_none() && value.observations.iter().any(|kind| {
                matches!(kind, crate::StorageObservationKind::Address | crate::StorageObservationKind::Reference(_))
            }) {
                return None;
            }

            for reference in references {
                regions.extend(reference.regions()?);
            }
        }

        Some(regions)
    }

    /// Whole-value lineage retained even when inline descriptors cannot be enumerated.
    pub fn carried_sources(&self) -> impl Iterator<Item = &Target> + '_ {
        storage_value_subtree(&self.state.values, &self.root)
            .filter_map(|(_, value)| value.whole_sources())
            .flat_map(|(sources, _)| sources)
    }

    /// Retains possible dependencies and duties after the actual descriptor or contents become unknown.
    pub fn bounded(&self) -> Self {
        Self::new(self.root.clone(), self.bounded_transition())
    }

    fn bounded_transition(&self) -> StorageCapturedTransition {
        let mut state = self.materialize(&self.root);

        for value in state.values.values_mut() { *value = std::mem::take(value).into_dependency_bounds(); }
        for contents in state.payloads.values_mut() { *contents = std::mem::take(contents).into_bounds(); }

        state
    }

    /// Adds a feasible alternative for which the incoming capture is unavailable.
    pub fn with_unknown_alternative(&self) -> Self {
        let mut state = self.bounded_transition();

        state.merge(&StorageCapturedTransition::default());

        Self::new(self.root.clone(), state)
    }

    /// Joins physical possibilities while intersecting definite initialized shape.
    pub fn merged(&self, incoming: &Self) -> Self {
        if self == incoming { return self.clone(); }

        let mut state = self.materialize(&self.root);

        state.merge(&incoming.materialize(&self.root));

        Self::new(self.root.clone(), state)
    }

    /// Replaces inline physical information at the same component publication as its data.
    pub fn with_component(&self, path: &[StorageProjection], replacement: Option<&Self>) -> Self {
        if !StorageProjection::remains_in_allocation(path) { return self.clone(); }

        let destination = self.root.project(path);

        // A projection of this snapshot already has the installed physical state.
        if replacement.is_some_and(|value| value.root == destination && std::sync::Arc::ptr_eq(&value.state, &self.state)) {
            return self.clone();
        }

        let mut state = self.state.as_ref().clone();

        state.values.retain(|slot, _| !destination.contains_storage(slot));
        state.payloads.retain(|slot, _| !destination.contains_storage(slot));

        if let Some(replacement) = replacement {
            let replacement = replacement.materialize(&destination);

            state.values.extend(replacement.values);
            state.payloads.extend(replacement.payloads);
        } else {
            state.values.insert(destination, StorageValueBinding::default());
        }

        if let Some(root) = state.values.get_mut(&self.root) { root.completeness.lose_completeness(); }

        Self::new(self.root.clone(), state)
    }

    /// Applies one transition's retirements to the selected capture in order.
    pub fn retired(&self, retirements: &[(Target, crate::StorageValueRetirement)]) -> Option<Self> {
        let mut updated: Option<StorageCapturedTransition> = None;

        for (current, retirement) in retirements {
            let state = updated.as_ref().unwrap_or(self.state.as_ref());

            let affected = storage_value_subtree(&state.values, &self.root)
                .flat_map(|(_, value)| value.references())
                .flat_map(|references| &references.targets)
                .any(|target| retirement.contains(current, target));

            if affected {
                // All retirements of this affected capture share one mutable copy.
                updated.get_or_insert_with(|| self.state.as_ref().clone()).retire_dependencies(current, *retirement);
            }
        }

        updated.map(|state| Self::new(self.root.clone(), state))
    }

    fn state_hash(state: &StorageCapturedTransition) -> u64 {
        let mut hash = std::hash::DefaultHasher::new();

        state.hash(&mut hash);

        hash.finish()
    }
}

impl StorageCapturedTransition {
    /// Captures the same inline value, initialization for every consumer.
    pub fn capture(bindings: &BTreeMap<Target, StorageValueBinding>, contents: &BTreeMap<Target, StorageContents>, evaluated: &BTreeMap<Target, StorageContents>, source: &Target, destination: &Target) -> Self {
        let inexact = storage_selector_parent(source).is_some();
        let complete = bindings.get(source).is_some_and(|value| value.whole_complete());
        let mut values = BTreeMap::<Target, StorageValueBinding>::new();

        for (source_slot, value) in selected_storage_values(bindings, source) {
            let overlap = source_slot.projections()[..source.projections().len()] != *source.projections();
            let slot = destination.project(&source_slot.projections()[source.projections().len()..]);
            let mut value = value.clone();

            if inexact || overlap {
                for references in value.references_mut() {
                    references.bound();
                }

                if inexact && value.completeness.tracked() && !complete {
                    value.completeness.lose_completeness();
                }
            }

            values.entry(slot).and_modify(|current| { current.merge(&value); }).or_insert(value);
        }

        if inexact && !complete {
            let value = values.entry(destination.clone()).or_default();

            if !value.is_no_payload() { value.completeness = StorageDependencyCompleteness::Partial; }
        }

        let mut transfer = StorageCapturedTransition {
            values,
            payloads: Default::default(),
            ..Default::default()
        };

        for (target, contents) in selected_storage_values(&contents, source) {
            let suffix = &target.projections()[source.projections().len()..];

            if suffix.contains(&StorageProjection::UninitPayload) {
                // Evaluated inline storage owns its contents after the source is consumed.
                let slot = destination.project(suffix);
                let mut contents = evaluated.get(target).unwrap_or(contents).clone();

                if inexact || target.projections()[..source.projections().len()] != *source.projections() {
                    contents.clear_initialization();
                }

                transfer.payloads.entry(slot.clone()).and_modify(|previous| { previous.merge(&contents, &slot); }).or_insert(contents);
            }
        }

        transfer
    }
}
