use std::collections::BTreeMap;
use crate::{StorageCapturedTransition, StorageContents, StorageDependencyCompleteness, StorageProjection, StorageTarget as Target, StorageValueBinding};
use super::selection::{selected_storage_values, storage_selector_parent};

/// Inline physical information carried by one evaluated value.
/// Its root is a projection coordinate, never a unique runtime allocation or permission.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageCapturedValue {
    root: Target,
    state: std::sync::Arc<StorageCapturedTransition>,
}

impl StorageCapturedValue {
    /// Couples an already captured inline subtree with its projection coordinate.
    pub fn new(root: Target, state: StorageCapturedTransition) -> Self {
        Self { root, state: std::sync::Arc::new(state) }
    }

    /// Selects inline data without reading through a pointer or owning allocation.
    pub fn project(&self, projection: StorageProjection) -> Self {
        Self { root: self.root.project(&[projection]), state: std::sync::Arc::clone(&self.state) }
    }

    /// Inline selectors present in this captured snapshot, without inspecting its type.
    pub fn component_projections(&self) -> impl Iterator<Item = StorageProjection> + '_ {
        self.state.values.keys().filter(|target| self.root.contains_storage(target))
            .filter_map(|target| target.projections().get(self.root.projections().len()).copied())
    }

    /// Attaches prepared observation roles at the producing value's checked boundary.
    /// Existing roles came with the actual value and take precedence over an opaque view.
    pub fn with_observations(mut self, observations: impl IntoIterator<Item = (Vec<StorageProjection>, crate::StorageObservationKind)>) -> Self {
        let mut prepared = BTreeMap::<_, std::collections::BTreeSet<_>>::new();

        for (path, kind) in observations {
            prepared.entry(self.root.project(&path)).or_default().insert(kind);
        }

        let state = std::sync::Arc::make_mut(&mut self.state);

        for (target, kinds) in prepared {
            let value = state.values.entry(target).or_default();

            if !value.is_no_payload() && value.observations.is_empty() { value.observations = kinds; }
        }

        self
    }

    /// Visits actual checked observation roles relative to this captured value.
    pub fn observations(&self) -> impl Iterator<Item = (&[StorageProjection], crate::StorageObservationKind)> + '_ {
        self.state.values.iter().filter(|(target, _)| self.root.contains_storage(target))
            .flat_map(move |(target, value)| value.observations.iter().map(move |kind|
                (&target.projections()[self.root.projections().len()..], *kind)))
    }

    /// Installs this captured value's existing information in the produced storage coordinates.
    pub fn materialize(&self, destination: &Target) -> StorageCapturedTransition {
        StorageCapturedTransition::capture(&self.state.values, &self.state.payloads,
            &BTreeMap::new(), &self.root, destination)
    }

    /// Returns the captured address only when it establishes one current actual target.
    pub fn known_address(&self) -> Option<&Target> {
        let address = self.state.values.get(&self.root)?.reference()?;

        address.is_exact().then(|| address.targets.first()).flatten().filter(|target| target.is_exact() && !target.previous)
    }

    /// Mutation operands of the evaluated descriptor, including independent conservative bounds.
    pub fn mutation_targets(&self) -> Option<Vec<crate::StorageMutationTarget>> {
        self.state.values.get(&self.root)?.mutation_targets()
    }

    /// Conservative address regions carried by this descriptor, with no ownership conversion.
    pub fn address_regions(&self) -> Option<Vec<Target>> {
        self.state.values.get(&self.root)?.reference_regions()
    }

    /// Retains possible dependencies and duties after the actual descriptor or contents become unknown.
    pub fn bounded(&self) -> Self {
        let mut state = self.materialize(&self.root);

        for value in state.values.values_mut() { *value = std::mem::take(value).into_dependency_bounds(); }
        for contents in state.payloads.values_mut() { *contents = std::mem::take(contents).into_bounds(); }

        Self::new(self.root.clone(), state)
    }

    /// Adds a feasible alternative for which the incoming capture is unavailable.
    pub fn with_unknown_alternative(&self) -> Self {
        let bounded = self.bounded();
        let mut state = bounded.materialize(&self.root);

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

        // Copy the snapshot before replacing this component.
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

    /// Retires captured descriptors affected by the same producer or lifetime transition.
    pub fn retired(&self, current: &Target, retirement: crate::StorageValueRetirement) -> Self {
        if !self.state.references(current, retirement) { return self.clone(); }

        // Only affected captures copy their maps. Unchanged values share their immutable snapshot.
        let mut state = self.state.as_ref().clone();

        state.retire_dependencies(current, retirement);

        Self::new(self.root.clone(), state)
    }

    /// Whether this captured graph contains descriptors affected by the retirement.
    pub fn references(&self, current: &Target, retirement: crate::StorageValueRetirement) -> bool {
        self.state.references(current, retirement)
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
                    contents.initialized.clear();
                }

                transfer.payloads.entry(slot.clone()).and_modify(|previous| { previous.merge(&contents, &slot); }).or_insert(contents);
            }
        }

        transfer
    }
}
