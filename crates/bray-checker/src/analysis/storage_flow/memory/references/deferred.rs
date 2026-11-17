use std::collections::BTreeMap;
use std::sync::Arc;

use bray_bound_tree::{BoundExecutionSite, StoragePath};

use super::super::model::{Origin, Retirement, State, Target, Transfer};
use super::super::Effect;
use super::Source;
use super::slots::storage_subtree;

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
        let mut result = DeferredTransitions::new();

        for (slot, entries) in super::slots::selected_subtree(&self.deferred, source) {
            let target = destination.project(&slot.projections()[source.projections().len()..]);

            merge_producers(result.entry(target).or_default(), entries);
        }

        result
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

            for (slot, entries) in storage_subtree(&self.deferred, &source)
            {
                let target = destination.project(&slot.projections()[source.projections().len()..]);

                // A deferred frame's reference entry describes its captures. It is
                // immutable with that frame, independently of mutable sibling fields.
                if let Some(captures) = self.values.get(slot) {
                    pending.values.entry(target.clone()).and_modify(|value| { value.merge(captures); }).or_insert_with(|| captures.clone());
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
            .filter(move |path| root.contains_storage(path))
            .filter(move |path| self.unexposed_storage(std::slice::from_ref(path), []).next().is_some())
    }

    pub(in crate::analysis::storage_flow) fn deferred_effects(
        &self,
        source: &Source,
        producers: &std::collections::BTreeSet<BoundExecutionSite>,
    ) -> Option<super::super::EffectOutcomes> {
        let Source::Value(source) = source
        else {
            return None;
        };

        let entries = self.deferred.get(source)?;
        let mut effects = super::super::EffectOutcomes::new([]);

        for (_, entry) in entries
            .iter()
            .filter(|(source, _)| producers.contains(source))
        {
            effects.merge(entry.deferred_effects.as_ref()?);
        }

        Some(effects)
    }

    pub(in crate::analysis::storage_flow) fn capture_deferred_cleanup(
        &mut self,
        invocation: BoundExecutionSite,
        captures: Vec<Effect>,
        result: Vec<Effect>,
    ) {
        let transfer = self.pending.get_mut(&invocation)
            .expect("deferred cleanup is captured with its prepared value transition");

        let result = result.into_iter().flat_map(|effect| {
            let Effect::Mutation { footprint: target, kind, access_type, .. } = &effect
            else {
                return vec![effect];
            };

            let Origin::Returned { reference, .. } = &target.origin
            else {
                return vec![effect];
            };

            let Some(bounds) = transfer.returned_inputs.get(reference).filter(|_| !target.previous) else {
                return vec![effect];
            };

            // A pending result has no current referent. Its possible cleanup
            // effects retain physical inputs and independent ambient storage.
            let (origins, unbounded) = Self::effect_origins_from(target, Some(bounds));

            let mut effects = origins.cloned().map(|origin| Effect::bounded(origin, *kind, *access_type))
                .collect::<Vec<_>>();

            if unbounded {
                effects.push(Effect::unbounded(*kind, *access_type, true));
            }

            effects
        }).collect();

        transfer.captured_cleanup = Some(captures.into_iter().collect());
        transfer.result_cleanup = Some(result);
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
        let Source::Value(source) = source
        else {
            return false;
        };

        let Some(entries) = self.deferred.get(source)
        else {
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

        let Some(completed) = completed
        else {
            return false;
        };

        self.pending.insert(invocation, completed);

        true
    }
}

impl Transfer {
    fn relocate(&mut self, source: &Target, destination: &Target) {
        let produced = self.payloads.keys().filter(|target| matches!(Retirement::for_payload(target), Retirement::Producer))
            .cloned().collect::<Vec<_>>();

        let relocate_path = |path: StoragePath| {
            let Some(prefix) = source.storage_path().filter(|prefix| prefix.contains(&path)) else {
                return path;
            };

            let destination = destination.storage_path().expect("deferred output keeps storage coordinates");

            StoragePath {
                root: destination.root,
                projections: destination.projections.iter().copied()
                    .chain(path.projections[prefix.projections.len()..].iter().copied()).collect(),
            }
        };

        let relocate_referent = |mut target: Target| {
            if produced.iter().any(|root| root.contains(&target)) {
                match &mut target.origin {
                    Origin::Returned { reference, .. } => *reference = relocate_path(reference.clone()),
                    Origin::Allocation { allocation, .. } => *allocation = relocate_path(allocation.clone()),
                    _ => unreachable!("produced referents retain their returned or allocation origin"),
                }
            }

            target
        };

        let relocate_slot = |target: Target| {
            if source.contains_storage(&target) {
                Target { previous: target.previous,
                    ..destination.project(&target.projections()[source.projections().len()..]) }
            } else {
                relocate_referent(target)
            }
        };

        self.values = std::mem::take(&mut self.values).into_iter().map(|(slot, mut value)| {
            for references in value.references_mut() {
                references.targets = std::mem::take(&mut references.targets).into_iter().map(&relocate_referent).collect();
            }

            (relocate_slot(slot), value)
        }).collect();

        // Captured inputs and conservative storage bounds name existing physical
        // regions. Only positively identified pending referents change namespace.
        for references in self.returned_inputs.values_mut() {
            references.targets = std::mem::take(&mut references.targets).into_iter().map(&relocate_referent).collect();
        }

        self.payloads = std::mem::take(&mut self.payloads).into_iter()
            .map(|(slot, contents)| (relocate_slot(slot), contents)).collect();

        self.returned_inputs = std::mem::take(&mut self.returned_inputs).into_iter()
            .map(|(slot, references)| (relocate_path(slot), references)).collect();

        self.deferred = std::mem::take(&mut self.deferred).into_iter()
            .map(|(slot, entries)| (relocate_slot(slot), entries)).collect();
    }

}

impl Transfer {
    pub(in crate::analysis::storage_flow::memory) fn references(&self, current: &Target, retirement: Retirement) -> bool {
        self.values
            .values()
            .chain(self.inputs.values())
            .flat_map(|value| value.references())
            .chain(self.returned_inputs.values())
            .any(|references| {
                references
                    .targets
                    .iter()
                    .any(|target| retirement.contains(current, target))
            })
            || self.deferred_effects.iter().flat_map(|effects| effects.possible())
                .chain([&self.captured_cleanup, &self.result_cleanup].into_iter().flatten().flatten())
                .any(|effect| effect.references(current, retirement))
            || self
                .failure_effects
                .as_ref()
                .is_some_and(|effects| effects.references(current, retirement))
            || self
                .deferred
                .values()
                .flat_map(|entries| entries.values())
                .any(|transfer| transfer.references(current, retirement))
    }

    pub(in crate::analysis::storage_flow::memory) fn retire_dependencies(
        &mut self,
        current: &Target,
        retirement: Retirement,
    ) {
        for references in self
            .values
            .values_mut()
            .chain(self.inputs.values_mut())
            .flat_map(|value| value.references_mut())
            .chain(self.returned_inputs.values_mut())
        {
            references.retire(current, retirement);
        }

        if let Some(effects) = &mut self.deferred_effects {
            *effects = effects.try_map(|effect| {
                // A retiring allocation changes descriptors in both outcomes, with shared paths.
                let mut effect = effect.clone();

                effect.retire(current, retirement);

                Ok::<_, std::convert::Infallible>(effect)
            }).expect("retirement preserves resolved effect outcomes");
        }

        for effects in [&mut self.captured_cleanup, &mut self.result_cleanup].into_iter().flatten() {
            *effects = std::mem::take(effects)
                .into_iter()
                .map(|mut effect| {
                    effect.retire(current, retirement);

                    effect
                })
                .collect();
        }

        if let Some(effects) = &mut self.failure_effects
            && effects.references(current, retirement)
        {
            Arc::make_mut(effects).retire(current, retirement);
        }

        for entries in self.deferred.values_mut() {
            for transfer in entries.values_mut() {
                if transfer.references(current, retirement) {
                    Arc::make_mut(transfer).retire_dependencies(current, retirement);
                }
            }
        }
    }
}
