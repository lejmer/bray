use std::collections::{BTreeMap, BTreeSet};

use crate::analysis::storage_flow::model::StorageAbsence;
use bray_bound_tree::{BorrowCapabilityId, BoundExecutionSite, BoundExpressionId};
use bray_symbols::TypeId;

pub(in crate::analysis::storage_flow) use bray_bound_tree::{
    StorageOrigin as Origin, StoragePath as Place, StorageTarget as Target,
};

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::analysis::storage_flow) enum Effect {
    Unknown,
    Ambient,
    Escape,
    Atomic,
    Mutation(Target, bray_symbols::DependencyMutationKind),
    Cleanup(TypeId),
}

impl Effect {
    pub(super) fn retire(&mut self, current: &Target) {
        if let Self::Mutation(target, _) = self
            && current.contains(target)
        {
            target.previous = true;
        }
    }

    pub(in crate::analysis::storage_flow) fn may_retain_address(&self, memory: &State) -> bool {
        match self {
            // Writes confined to an invocation-local slot do not publish an address.
            // Stored reference relations still track later publication of that slot.
            Self::Mutation(target, kind) => {
                kind.is_write()
                    && *kind != bray_symbols::DependencyMutationKind::Clear
                    && target.storage_path().is_none_or(|path| {
                        memory.owned_storage.binary_search(&path.root).is_err()
                            || path
                                .projections
                                .contains(&bray_bound_tree::StorageProjection::OwnedTarget)
                            || memory
                                .contents
                                .get(&Target::storage(Place::root(path.root)))
                                .is_some_and(|contents| contents.escaped)
                    })
            }
            Self::Unknown | Self::Escape | Self::Cleanup(_) => true,
            Self::Ambient | Self::Atomic => false,
        }
    }

    pub(in crate::analysis::storage_flow) fn mutation(
        &self,
    ) -> Option<(&Target, bray_symbols::DependencyMutationKind)> {
        match self {
            Self::Mutation(_, bray_symbols::DependencyMutationKind::Escape) => None,
            Self::Mutation(target, kind) => Some((target, *kind)),
            Self::Unknown | Self::Ambient | Self::Escape | Self::Atomic | Self::Cleanup(_) => None,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct References {
    pub(super) targets: BTreeSet<Target>,
    pub(super) unknown: bool,
    pub(super) displaced: bool,
    pub(super) inexact: bool,
}

impl References {
    pub(super) fn one(target: Target) -> Self {
        Self {
            // A dynamic selector is bounded by this region even without one exact cell.
            unknown: false,
            targets: BTreeSet::from([target]),
            displaced: false,
            inexact: false,
        }
    }

    pub(super) fn merge(&mut self, incoming: &Self) -> bool {
        let count = self.targets.len();
        let unknown = self.unknown;
        let displaced = self.displaced;
        let inexact = self.inexact;

        // Targets contain shared immutable projection paths.
        self.targets.extend(incoming.targets.iter().cloned());
        self.unknown |= incoming.unknown;
        self.displaced |= incoming.displaced;
        self.inexact |= incoming.inexact;

        self.targets.len() != count
            || self.unknown != unknown
            || self.displaced != displaced
            || self.inexact != inexact
    }

    pub(super) fn retire(&mut self, current: &Target) {
        let targets = self
            .targets
            .range(current.clone()..)
            .take_while(|target| current.contains(target))
            .cloned()
            .collect::<Vec<_>>();

        for target in targets {
            self.targets.remove(&target);

            self.targets.insert(Target {
                previous: true,
                ..target
            });
        }
    }

    pub(super) fn is_exact(&self) -> bool {
        !self.unknown
            && !self.displaced
            && !self.inexact
            && self.targets.len() == 1
            && self
                .targets
                .first()
                .expect("one reference target")
                .is_exact()
    }
}

type Origins = BTreeSet<BoundExpressionId>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Contents {
    pub(super) exposed: bool,
    pub(super) escaped: bool,
    pub(super) storage_type: Option<TypeId>,
    pub(super) initialized: BTreeSet<TypeId>,
    pub(super) pending: BTreeMap<TypeId, Origins>,
    pub(super) allocations: Origins,
    pub(super) invalidations: Origins,
}

impl Contents {
    pub(super) fn merge(&mut self, incoming: &Self) -> bool {
        let previous_exposure = self.exposed;
        let previous_escape = self.escaped;

        self.exposed |= incoming.exposed;
        self.escaped |= incoming.escaped;

        let previous_type = self.storage_type;

        match (self.storage_type, incoming.storage_type) {
            (Some(left), Some(right)) => assert_eq!(
                left, right,
                "one physical storage path has one declared type"
            ),
            (None, Some(ty)) => self.storage_type = Some(ty),
            _ => {}
        }

        let count = self.initialized.len();

        self.initialized
            .retain(|ty| incoming.initialized.contains(ty));

        let mut changed = self.initialized.len() != count
            || self.storage_type != previous_type
            || self.exposed != previous_exposure
            || self.escaped != previous_escape;

        for (&ty, origins) in &incoming.pending {
            changed |= merge_origins(self.pending.entry(ty).or_default(), origins);
        }

        changed |= merge_origins(&mut self.allocations, &incoming.allocations);
        changed |= merge_origins(&mut self.invalidations, &incoming.invalidations);

        changed
    }
}

fn merge_origins(target: &mut Origins, incoming: &Origins) -> bool {
    let count = target.len();

    target.extend(incoming.iter().copied());

    target.len() != count
}

/// Effects resolved before input ownership transfers, retained only until the call outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(in crate::analysis::storage_flow) struct CapturedEffects {
    pub(in crate::analysis::storage_flow) mutations: Vec<Effect>,
    pub(in crate::analysis::storage_flow) escaped: Vec<Target>,
    pub(in crate::analysis::storage_flow) invalidation: bray_bound_tree::StorageInvalidation,
}

impl CapturedEffects {
    pub(super) fn references(&self, current: &Target) -> bool {
        self.escaped.iter().any(|target| current.contains(target))
            || self.mutations.iter().any(
                |effect| matches!(effect, Effect::Mutation(target, _) if current.contains(target)),
            )
    }

    pub(super) fn retire(&mut self, current: &Target) {
        for effect in &mut self.mutations {
            effect.retire(current);
        }

        for target in &mut self.escaped {
            if current.contains(target) {
                target.previous = true;
            }
        }

        self.normalize();
    }

    fn normalize(&mut self) {
        // Retiring or joining incarnations can merge already summarized targets.
        self.mutations.sort_unstable();
        self.mutations.dedup();
        self.escaped.sort_unstable();
        self.escaped.dedup();
    }

    fn merge(&mut self, incoming: &Self) {
        // Outcome sets copy descriptors and share immutable projection paths.
        self.mutations.extend(incoming.mutations.iter().cloned());
        self.escaped.extend(incoming.escaped.iter().cloned());
        self.normalize();
        self.invalidation.merge(&incoming.invalidation);
    }
}

/// A value captured before consumption, with inline protected payload state at its destination.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Transfer {
    pub(super) values: BTreeMap<Target, References>,
    pub(super) payloads: BTreeMap<Target, Contents>,
    pub(super) failure_effects: Option<std::sync::Arc<CapturedEffects>>,
}

impl Transfer {
    pub(super) fn merge(&mut self, incoming: &Self) -> bool {
        let changed = merge_references(&mut self.values, &incoming.values, |_| false, |_| false);

        let mut changed = merge_contents(&mut self.payloads, &incoming.payloads) || changed;

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

/// Reference values are copied between slots. Contents and obligations belong to targets once.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::analysis::storage_flow) struct State {
    pub(super) owned_storage: std::sync::Arc<[bray_bound_tree::StorageIdentityId]>,
    pub(super) unaddressed_storage: std::sync::Arc<[bray_bound_tree::StorageIdentityId]>,
    pub(super) returned_inputs: BTreeMap<Place, BTreeSet<Target>>,
    pub(super) values: BTreeMap<Target, References>,
    pub(super) borrows: BTreeMap<BorrowCapabilityId, References>,
    pub(super) pending: BTreeMap<BoundExecutionSite, Transfer>,
    pub(super) deferred_effects: BTreeMap<BoundExecutionSite, Option<BTreeSet<Effect>>>,
    pub(super) contents: BTreeMap<Target, Contents>,
    // Consumed inline values remain available to the current operation's value transfers.
    // Proof queries read physical contents only. This snapshot never crosses an operation edge.
    pub(super) evaluated: BTreeMap<Target, Contents>,
}

impl State {
    pub(in crate::analysis::storage_flow) fn merge(
        &mut self,
        incoming: &Self,
        absent: &BTreeMap<Place, StorageAbsence>,
        incoming_absent: &BTreeMap<Place, StorageAbsence>,
    ) -> bool {
        assert!(
            self.evaluated.is_empty() && incoming.evaluated.is_empty(),
            "evaluated values must be transferred before joining control flow"
        );

        let no_payload = |absence: &BTreeMap<Place, StorageAbsence>, slot: &Target| {
            slot.storage_path().is_some_and(|slot| {
                absence.iter().any(|(path, reason)| {
                    *reason == StorageAbsence::NoPayload && path.contains(slot)
                })
            })
        };

        let left_absent = |slot: &Target| no_payload(absent, slot);
        let right_absent = |slot: &Target| no_payload(incoming_absent, slot);

        let mut changed = merge_references(
            &mut self.values,
            &incoming.values,
            left_absent,
            right_absent,
        );

        for (reference, roots) in &incoming.returned_inputs {
            let previous = self.returned_inputs.entry(reference.clone()).or_default();
            let count = previous.len();

            previous.extend(roots.iter().cloned());
            changed |= previous.len() != count;
        }

        changed |= merge_references(&mut self.borrows, &incoming.borrows, |_| false, |_| false);

        let empty = Transfer::default();

        for (call, transfer) in &mut self.pending {
            changed |= transfer.merge(incoming.pending.get(call).unwrap_or(&empty));
        }

        for (call, transfer) in &incoming.pending {
            if !self.pending.contains_key(call) {
                let mut joined = Transfer::default();

                changed |= joined.merge(transfer);
                self.pending.insert(*call, joined);
            }
        }

        for (call, effects) in &mut self.deferred_effects {
            match (
                effects.as_mut(),
                incoming.deferred_effects.get(call).and_then(Option::as_ref),
            ) {
                (Some(effects), Some(incoming)) => {
                    let before = effects.len();

                    effects.extend(incoming.iter().cloned());
                    changed |= effects.len() != before;
                }
                (Some(_), None) => {
                    *effects = None;
                    changed = true;
                }
                _ => {}
            }
        }

        for call in incoming.deferred_effects.keys() {
            if !self.deferred_effects.contains_key(call) {
                self.deferred_effects.insert(*call, None);
                changed = true;
            }
        }

        changed |= merge_contents(&mut self.contents, &incoming.contents);

        changed
    }
}

fn merge_contents(
    target: &mut BTreeMap<Target, Contents>,
    incoming: &BTreeMap<Target, Contents>,
) -> bool {
    let mut changed = false;

    for (key, contents) in target.iter_mut() {
        match incoming.get(key) {
            Some(incoming) => changed |= contents.merge(incoming),
            None => {
                changed |= !contents.initialized.is_empty();
                contents.initialized.clear();
            }
        }
    }

    for (key, contents) in incoming {
        if !target.contains_key(key) {
            // Joined contents retain possible obligations, but no one-sided initialization proof.
            let mut contents = contents.clone();

            contents.initialized.clear();
            target.insert(key.clone(), contents);
            changed = true;
        }
    }

    changed
}

fn merge_references<K: Clone + Ord>(
    target: &mut BTreeMap<K, References>,
    incoming: &BTreeMap<K, References>,
    target_absent: impl Fn(&K) -> bool,
    incoming_absent: impl Fn(&K) -> bool,
) -> bool {
    let mut changed = false;

    for (slot, references) in target.iter_mut() {
        match incoming.get(slot) {
            Some(incoming) if target_absent(slot) => {
                if references != incoming {
                    // The inactive predecessor contributes no address alternative.
                    // Each value owns its alternatives and shares immutable target paths.
                    *references = incoming.clone();
                    changed = true;
                }
            }
            Some(_) if incoming_absent(slot) => {}
            Some(incoming) => {
                let nullable = references.targets.is_empty() != incoming.targets.is_empty();
                let inexact = references.inexact;

                changed |= references.merge(incoming);
                references.inexact |= nullable;
                changed |= references.inexact != inexact;
            }
            None if !references.unknown && !incoming_absent(slot) => {
                references.unknown = true;
                changed = true;
            }
            None => {}
        }
    }

    for (slot, references) in incoming {
        if !target.contains_key(slot) {
            // Joined slots own their target alternatives independently of predecessor state.
            let mut references = references.clone();

            references.unknown |= !target_absent(slot);
            target.insert(slot.clone(), references);
            changed = true;
        }
    }

    changed
}

/// Reference values at a unit boundary, with the live-payload conditions used by the same join.
#[derive(Clone, Debug, Default)]
pub(in crate::analysis::storage_flow) struct ReferenceExit {
    pub(super) slots: BTreeMap<Place, References>,
    pub(super) absent: BTreeSet<Place>,
}

impl ReferenceExit {
    pub(in crate::analysis::storage_flow) fn merge(&mut self, incoming: &Self) {
        merge_references(
            &mut self.slots,
            &incoming.slots,
            |slot| self.absent.iter().any(|path| path.contains(slot)),
            |slot| incoming.absent.iter().any(|path| path.contains(slot)),
        );

        // Retain only absence established by both predecessors, including enclosing payloads.
        self.absent = self
            .absent
            .iter()
            .chain(&incoming.absent)
            .filter(|slot| {
                self.absent.iter().any(|path| path.contains(slot))
                    && incoming.absent.iter().any(|path| path.contains(slot))
            })
            .cloned()
            .collect();
    }

    pub(in crate::analysis::storage_flow) fn targets(
        &self,
    ) -> impl Iterator<Item = (&Place, &Origin, bool, bool)> {
        self.slots.iter().flat_map(|(slot, references)| {
            references.targets.iter().map(move |target| {
                (
                    slot,
                    &target.origin,
                    references.is_exact(),
                    !references.unknown && !references.displaced,
                )
            })
        })
    }
}
