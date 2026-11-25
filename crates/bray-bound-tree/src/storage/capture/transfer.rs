use std::collections::BTreeMap;
use std::sync::Arc;
use crate::{StoragePath as Place, StorageTarget as Target, StorageValueRetirement as Retirement, StorageAddressValue, StorageValueBinding, StorageContents, StorageCapturedEffects};
use super::merge::{merge_storage_bindings, merge_storage_contents};

/// A value captured before consumption, with inline protected payload state at its destination.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageCapturedTransition {
    // Possible destinations receive a weak merge, never definite replacement or initialization.
    pub weak_outputs: bool,
    pub values: BTreeMap<Target, StorageValueBinding>,
    pub payloads: BTreeMap<Target, StorageContents>,
    /// Exact typed updates made by the selected outcome.
    pub cell_changes: BTreeMap<Target, bray_symbols::DependencyMutationKind>,
    pub failure_effects: BTreeMap<(bray_symbols::DependencyFailureKind, Option<usize>), Arc<StorageCapturedEffects>>,
    pub returned_inputs: BTreeMap<Place, StorageAddressValue>,
}

impl StorageCapturedTransition {
    /// Production consumes output snapshots while entry captures remain available to outcomes.
    pub fn take_outputs(&mut self) -> Self {
        Self {
            weak_outputs: std::mem::take(&mut self.weak_outputs),
            values: std::mem::take(&mut self.values),
            payloads: std::mem::take(&mut self.payloads),
            cell_changes: std::mem::take(&mut self.cell_changes),
            returned_inputs: std::mem::take(&mut self.returned_inputs),
            ..Self::default()
        }
    }

    /// Publishes private producer coordinates after older targets have retired.
    pub fn commit_producers(&mut self) {
        if self.values.keys().any(Target::is_staged) {
            self.values = std::mem::take(&mut self.values).into_iter()
                .map(|(target, value)| (target.commit_producer(), value)).collect();
        }

        if self.payloads.keys().any(Target::is_staged) {
            self.payloads = std::mem::take(&mut self.payloads).into_iter()
                .map(|(target, contents)| (target.commit_producer(), contents)).collect();
        }

        if self.cell_changes.keys().any(Target::is_staged) {
            self.cell_changes = std::mem::take(&mut self.cell_changes).into_iter()
                .map(|(target, change)| (target.commit_producer(), change)).collect();
        }

        for binding in self.values.values_mut() {
            binding.commit_referenced_producers();
        }

        for bounds in self.returned_inputs.values_mut() {
            bounds.commit_producers();
        }
    }

    pub fn merge(&mut self, incoming: &Self) -> bool {
        let changed = merge_storage_bindings(&mut self.values, &incoming.values, |_| false, |_| false);

        let mut changed = merge_storage_contents(&mut self.payloads, &incoming.payloads) || changed;
        let changes = self.cell_changes.len();

        self.cell_changes.retain(|target, ty| incoming.cell_changes.get(target) == Some(ty));
        changed |= self.cell_changes.len() != changes;

        changed |= !self.weak_outputs && incoming.weak_outputs;
        self.weak_outputs |= incoming.weak_outputs;

        for (reference, bounds) in &incoming.returned_inputs {
            changed |= self
                .returned_inputs
                .entry(reference.clone())
                .or_default()
                .merge(bounds);
        }

        for (outcome, incoming) in &incoming.failure_effects {
            if let Some(current) = self.failure_effects.get_mut(outcome) {
                if current != incoming {
                    // Outcome branches share immutable captures until a join adds possible effects.
                    let mut merged = current.as_ref().clone();

                    merged.merge(incoming);
                    changed |= &merged != current.as_ref();
                    *current = std::sync::Arc::new(merged);
                }
            } else {
                self.failure_effects.insert(*outcome, Arc::clone(incoming));
                changed = true;
            }
        }

        changed
    }
}

impl StorageCapturedTransition {
    pub fn references(&self, current: &Target, retirement: Retirement) -> bool {
        self.values
            .values()
            .flat_map(|value| value.references())
            .chain(self.returned_inputs.values())
            .any(|references| {
                references
                    .targets
                    .iter()
                    .any(|target| retirement.contains(current, target))
            })
            || self
                .failure_effects
                .values()
                .any(|effects| effects.references(current, retirement))
            || self.cell_changes.keys().any(|target| retirement.contains(current, target))

    }

    pub fn retire_dependencies(
        &mut self,
        current: &Target,
        retirement: Retirement,
    ) {
        for value in self.values.values_mut() {
            value.retire_references(current, retirement);
        }

        for references in self.returned_inputs.values_mut() {
            references.retire(current, retirement);
        }

        for effects in self.failure_effects.values_mut() {
            if effects.references(current, retirement) {
                Arc::make_mut(effects).retire(current, retirement);
            }
        }

        self.cell_changes.retain(|target, _| !retirement.contains(current, target));
    }
}
