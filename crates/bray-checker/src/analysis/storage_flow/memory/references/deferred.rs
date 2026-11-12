use std::collections::BTreeMap;
use std::sync::Arc;

use bray_bound_tree::{BoundExecutionSite, StoragePath};

use super::super::model::{Effect, Origin, State, Target, Transfer};
use super::Source;

pub(in crate::analysis::storage_flow::memory) type DeferredTransitions =
    BTreeMap<Target, BTreeMap<BoundExecutionSite, Arc<Transfer>>>;

pub(in crate::analysis::storage_flow::memory) fn merge_deferred(
    target: &mut DeferredTransitions,
    incoming: &DeferredTransitions,
) -> bool {
    let mut changed = false;

    for (owner, entries) in incoming {
        let retained = target.entry(owner.clone()).or_default();

        changed |= merge_producers(retained, entries);
    }

    changed
}

fn merge_producers(
    target: &mut BTreeMap<BoundExecutionSite, Arc<Transfer>>,
    incoming: &BTreeMap<BoundExecutionSite, Arc<Transfer>>,
) -> bool {
    let mut changed = false;

    for (source, entry) in incoming {
        if let Some(previous) = target.get_mut(source) {
            if previous != entry {
                changed |= Arc::make_mut(previous).merge(entry);
            }
        } else {
            target.insert(*source, Arc::clone(entry));
            changed = true;
        }
    }

    changed
}

impl State {
    pub(in crate::analysis::storage_flow) fn capture_deferred_transition(
        &mut self,
        source: BoundExecutionSite,
        owner: StoragePath,
    ) {
        let transfer = self
            .pending
            .remove(&source)
            .expect("formed lazy call retains its prepared reference transition");

        self.deferred
            .entry(Target::storage(owner))
            .or_default()
            .insert(source, Arc::new(transfer));
    }

    pub(in crate::analysis::storage_flow::memory) fn capture_deferred_value(
        &self,
        source: &Target,
        destination: &Target,
    ) -> DeferredTransitions {
        // Output slots retain producer coordinates until completion. Referents and
        // effects already name physical inputs, independently of those output slots.
        self.deferred
            .range(source.clone()..)
            .take_while(|(slot, _)| source.contains(slot))
            .map(|(slot, entries)| {
                let target = destination.project(&slot.projections()[source.projections().len()..]);

                let entries = entries
                    .iter()
                    .map(|(producer, entry)| (*producer, Arc::clone(entry)))
                    .collect();

                (target, entries)
            })
            .collect()
    }

    pub(in crate::analysis::storage_flow) fn stage_deferred_values(
        &mut self,
        invocation: BoundExecutionSite,
        values: impl IntoIterator<Item = (Target, Target)>,
    ) {
        if self.deferred.is_empty() {
            return;
        }

        for (source, destination) in values {
            let pending = self.pending.entry(invocation).or_default();

            for (slot, entries) in self.deferred.range(source.clone()..)
                .take_while(|(slot, _)| source.contains(slot))
            {
                let target = destination.project(&slot.projections()[source.projections().len()..]);

                // A deferred frame's reference entry describes its captures. It is
                // immutable with that frame, independently of mutable sibling fields.
                if let Some(captures) = self.values.get(slot) {
                    pending.values.entry(target.clone()).or_default().merge(captures);
                }

                let retained = pending.deferred.entry(target).or_default();

                merge_producers(retained, entries);
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn publish_deferred_inputs(
        &mut self,
        invocation: BoundExecutionSite,
        destination: StoragePath,
    ) {
        let target = Target::storage(destination);

        // Task observation captures are live when its outer future is formed.
        // Its own returned values remain staged until that future completes.
        if let Some(captures) = self.pending.get_mut(&invocation)
            .and_then(|transfer| transfer.deferred.remove(&target))
        {
            self.deferred.entry(target).or_default().extend(captures);
        }
    }

    pub(in crate::analysis::storage_flow) fn unexposed_deferred_paths<'a>(
        &'a self,
        root: &'a StoragePath,
    ) -> impl Iterator<Item = StoragePath> + 'a {
        self.deferred
            .keys()
            .filter_map(Target::observation_path)
            .filter(move |path| root.contains(path))
            .filter(move |path| self.unexposed_storage(std::slice::from_ref(path), []).next().is_some())
    }

    pub(in crate::analysis::storage_flow) fn deferred_effects(
        &self,
        source: &Source,
        producers: &std::collections::BTreeSet<BoundExecutionSite>,
    ) -> Option<Vec<Effect>> {
        let Source::Value(source) = source else {
            return None;
        };

        let entries = self.deferred.get(source)?;
        let mut effects = std::collections::BTreeSet::new();

        for (_, entry) in entries
            .iter()
            .filter(|(source, _)| producers.contains(source))
        {
            effects.extend(entry.deferred_effects.as_ref()?.iter().cloned());
        }

        Some(effects.into_iter().collect())
    }

    pub(in crate::analysis::storage_flow) fn capture_deferred_cleanup(
        &mut self,
        invocation: BoundExecutionSite,
        captures: Vec<Effect>,
        result: Vec<Effect>,
    ) {
        let transfer = self.pending.get_mut(&invocation)
            .expect("deferred cleanup is captured with its prepared value transition");
        transfer.captured_cleanup = Some(captures.into_iter().collect());
        transfer.result_cleanup = Some(result.into_iter().collect());
    }

    pub(in crate::analysis::storage_flow) fn deferred_cleanup_effects(
        &self,
        source: &Source,
        producers: &std::collections::BTreeSet<BoundExecutionSite>,
        discard_result: bool,
    ) -> Option<Vec<Effect>> {
        let Source::Value(source) = source else { return None; };
        let entries = self.deferred.get(source)?;
        let mut effects = std::collections::BTreeSet::new();

        for (_, entry) in entries.iter().filter(|(source, _)| producers.contains(source)) {
            effects.extend(entry.captured_cleanup.as_ref()?.iter().cloned());
            if discard_result {
                effects.extend(entry.result_cleanup.as_ref()?.iter().cloned());
            }
        }

        Some(effects.into_iter().collect())
    }

    pub(in crate::analysis::storage_flow) fn prepare_deferred_completion(
        &mut self,
        invocation: BoundExecutionSite,
        source: &Source,
        result: StoragePath,
        producers: impl IntoIterator<Item = (BoundExecutionSite, StoragePath)>,
    ) -> bool {
        let Source::Value(source) = source else {
            return false;
        };

        let Some(entries) = self.deferred.get(source) else {
            return false;
        };

        let result = Target::storage(result);
        let mut completed: Option<Transfer> = None;

        for (producer, output) in producers {
            let entry = entries
                .get(&producer)
                .unwrap_or_else(|| panic!(
                    "deferred completion {invocation:?} requires producer {producer:?} in {source:?}; retained producers: {:?}",
                    entries.keys(),
                ));

            // Alternative producers keep independent staged outputs until normal completion joins them.
            let mut transfer = entry.as_ref().clone();

            transfer.relocate(&Target::storage(output), &result);

            if let Some(previous) = &mut completed {
                previous.merge(&transfer);
            } else {
                completed = Some(transfer);
            }
        }

        let Some(completed) = completed else {
            return false;
        };

        self.pending.insert(invocation, completed);

        true
    }
}

impl Transfer {
    fn relocate(&mut self, source: &Target, destination: &Target) {
        let relocate = |target: Target| {
            if source.contains(&target) {
                destination.project(&target.projections()[source.projections().len()..])
            } else if let Origin::Returned {
                reference,
                projections,
            } = &target.origin
                && let Some(path) = source.storage_path()
                && path.contains(reference)
                && let Some(destination) = destination.storage_path()
            {
                Target {
                    origin: Origin::Returned {
                        reference: StoragePath {
                            root: destination.root,
                            projections: destination
                                .projections
                                .iter()
                                .copied()
                                .chain(
                                    reference.projections[path.projections.len()..]
                                        .iter()
                                        .copied(),
                                )
                                .collect(),
                        },
                        projections: Arc::clone(projections),
                    },
                    previous: target.previous,
                }
            } else {
                target
            }
        };

        self.values = std::mem::take(&mut self.values)
            .into_iter()
            .map(|(slot, mut references)| {
                references.targets = references.targets.into_iter().map(&relocate).collect();

                (relocate(slot), references)
            })
            .collect();

        for references in self.inputs.values_mut() {
            references.targets = std::mem::take(&mut references.targets).into_iter().map(&relocate).collect();
        }

        self.payloads = std::mem::take(&mut self.payloads)
            .into_iter()
            .map(|(slot, contents)| (relocate(slot), contents))
            .collect();

        self.returned_inputs = std::mem::take(&mut self.returned_inputs)
            .into_iter()
            .map(|(slot, references)| {
                let slot = relocate(Target::storage(slot))
                    .storage_path()
                    .expect("relocated returned bounds keep their storage slot")
                    .clone();

                (slot, references)
            })
            .collect();

        self.deferred = std::mem::take(&mut self.deferred)
            .into_iter()
            .map(|(slot, entries)| {
                let target = relocate(slot.clone());

                (target, entries)
            })
            .collect();
    }
}

impl Transfer {
    pub(in crate::analysis::storage_flow::memory) fn references(&self, current: &Target) -> bool {
        self.values
            .values()
            .chain(self.inputs.values())
            .chain(self.returned_inputs.values())
            .any(|references| {
                references
                    .targets
                    .iter()
                    .any(|target| current.contains(target))
            })
            || [&self.deferred_effects, &self.captured_cleanup, &self.result_cleanup].into_iter().flatten().flatten().any(|effect| {
                effect
                    .mutation()
                    .is_some_and(|(target, _)| current.contains(target))
            })
            || self
                .failure_effects
                .as_ref()
                .is_some_and(|effects| effects.references(current))
            || self
                .deferred
                .values()
                .flat_map(|entries| entries.values())
                .any(|transfer| transfer.references(current))
    }

    pub(in crate::analysis::storage_flow::memory) fn retire_dependencies(
        &mut self,
        current: &Target,
    ) {
        for references in self
            .values
            .values_mut()
            .chain(self.inputs.values_mut())
            .chain(self.returned_inputs.values_mut())
        {
            references.retire(current);
        }

        for effects in [&mut self.deferred_effects, &mut self.captured_cleanup, &mut self.result_cleanup].into_iter().flatten() {
            *effects = std::mem::take(effects)
                .into_iter()
                .map(|mut effect| {
                    effect.retire(current);

                    effect
                })
                .collect();
        }

        if let Some(effects) = &mut self.failure_effects
            && effects.references(current)
        {
            Arc::make_mut(effects).retire(current);
        }

        for entries in self.deferred.values_mut() {
            for transfer in entries.values_mut() {
                if transfer.references(current) {
                    Arc::make_mut(transfer).retire_dependencies(current);
                }
            }
        }
    }
}
