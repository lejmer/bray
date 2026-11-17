use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use crate::{StoragePath as Place, StoragePath, StorageOrigin as Origin, StorageTarget as Target, StorageValueRetirement as Retirement, StorageAddressValue, StorageValueBinding, StorageContents, StorageEffect, StorageCapturedEffects};
use super::merge::{merge_storage_bindings, merge_storage_contents};

type EffectOutcomes = bray_symbols::DependencyEffectOutcomes<StorageEffect>;

/// A frame's private storage and the physical effects bound when execution starts.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageDeferredTransition {
    /// Local pending result coordinate. Portable outputs are published by their value bindings.
    pub output: Option<StoragePath>,
    /// Captured inputs, effects, cleanup and pending outputs of this frame.
    pub transfer: StorageCapturedTransition,
}

impl StorageDeferredTransition {
    /// Joins alternative captures of one producer. Missing knowledge stays unknown.
    pub fn merge_alternative(current: &mut Option<Arc<Self>>, incoming: &Option<Arc<Self>>) -> bool {
        let Some(previous) = current else { return false; };

        let Some(incoming) = incoming else {
            *current = None;

            return true;
        };

        assert_eq!(previous.output, incoming.output, "one producer has one private result coordinate");

        previous != incoming && Arc::make_mut(previous).transfer.merge(&incoming.transfer)
    }
}

/// A value captured before consumption, with inline protected payload state at its destination.
#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageCapturedTransition {
    // Possible destinations receive a weak merge, never definite replacement or initialization.
    pub weak_outputs: bool,
    /// Evaluated reference descriptors retained by this deferred frame, separate from published outputs.
    pub inputs: BTreeMap<Target, StorageValueBinding>,
    pub values: BTreeMap<Target, StorageValueBinding>,
    pub payloads: BTreeMap<Target, StorageContents>,
    pub failure_effects: Option<std::sync::Arc<StorageCapturedEffects>>,
    pub deferred_effects: Option<EffectOutcomes>,
    pub captured_cleanup: Option<BTreeSet<StorageEffect>>,
    pub result_cleanup: Option<BTreeSet<StorageEffect>>,
    pub returned_inputs: BTreeMap<Place, StorageAddressValue>,
}

impl StorageCapturedTransition {
    /// Production consumes output snapshots while entry captures remain available to outcomes.
    pub fn take_outputs(&mut self) -> Self {
        Self {
            weak_outputs: std::mem::take(&mut self.weak_outputs),
            values: std::mem::take(&mut self.values),
            payloads: std::mem::take(&mut self.payloads),
            returned_inputs: std::mem::take(&mut self.returned_inputs),
            ..Self::default()
        }
    }

    pub fn merge(&mut self, incoming: &Self) -> bool {
        let changed = merge_storage_bindings(&mut self.inputs, &incoming.inputs, |_| false, |_| false);
        let changed = merge_storage_bindings(&mut self.values, &incoming.values, |_| false, |_| false) || changed;

        let mut changed = merge_storage_contents(&mut self.payloads, &incoming.payloads) || changed;

        changed |= !self.weak_outputs && incoming.weak_outputs;
        self.weak_outputs |= incoming.weak_outputs;

        for (reference, bounds) in &incoming.returned_inputs {
            changed |= self
                .returned_inputs
                .entry(reference.clone())
                .or_default()
                .merge(bounds);
        }

        for (effects, incoming) in [
            (&mut self.captured_cleanup, &incoming.captured_cleanup),
            (&mut self.result_cleanup, &incoming.result_cleanup),
        ] {
            if let Some(current) = effects {
                if let Some(incoming) = incoming {
                    let before = current.len();

                    current.extend(incoming.iter().cloned());
                    changed |= before != current.len();
                } else {
                    *effects = None;
                    changed = true;
                }
            }
        }

        if let Some(current) = &mut self.deferred_effects {
            if let Some(incoming) = &incoming.deferred_effects {
                changed |= current.merge(incoming);
            } else {
                self.deferred_effects = None;
                changed = true;
            }
        }

        if let Some(incoming) = &incoming.failure_effects {
            if let Some(current) = &mut self.failure_effects {
                if current != incoming {
                    // Outcome branches share immutable captures until a join adds possible effects.
                    let mut merged = current.as_ref().clone();

                    merged.merge(incoming);
                    changed |= &merged != current.as_ref();
                    *current = std::sync::Arc::new(merged);
                }
            } else {
                self.failure_effects = Some(std::sync::Arc::clone(incoming));
                changed = true;
            }
        }

        changed
    }
}

impl StorageCapturedTransition {
    pub fn relocate(&mut self, source: &Target, destination: &Target) {
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

    }

}

impl StorageCapturedTransition {
    pub fn references(&self, current: &Target, retirement: Retirement) -> bool {
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

    }

    pub fn retire_dependencies(
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

    }
}
