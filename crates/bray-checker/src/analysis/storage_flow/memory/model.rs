use std::collections::{BTreeMap, BTreeSet};

use crate::analysis::storage_flow::model::StorageAbsence;
use bray_bound_tree::{BorrowCapabilityId, BoundExecutionSite, BoundExpressionId};
use bray_symbols::TypeId;

pub(in crate::analysis::storage_flow) use bray_bound_tree::StoragePath as Place;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::analysis::storage_flow) enum Origin {
    Entry {
        reference: Place,
        // Successive borrow layers at the same component path denote distinct entry snapshots.
        layer: usize,
        projections: std::sync::Arc<[bray_bound_tree::StorageProjection]>,
    },
    // One returned referent has stable identity without equating it with a lifetime anchor.
    Returned { reference: Place, projections: std::sync::Arc<[bray_bound_tree::StorageProjection]> },
    Storage(Place),
    Allocation(Place),
}

/// `previous` summarizes older executions of one creation site. It is never a must-alias.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::analysis::storage_flow) struct Target {
    // Epoch precedes the path in ordering so subtree ranges cannot cross an older cell.
    pub(super) previous: bool,
    pub(in crate::analysis::storage_flow) origin: Origin,
}

impl Origin {
    pub(in crate::analysis::storage_flow) fn entry(reference: Place) -> Self {
        Self::Entry {
            reference,
            layer: 1,
            projections: std::sync::Arc::from([]),
        }
    }
}

impl Target {
    pub(in crate::analysis::storage_flow) fn storage(place: Place) -> Self {
        Self {
            origin: Origin::Storage(place),
            previous: false,
        }
    }

    pub(in crate::analysis::storage_flow) fn storage_path(&self) -> Option<&Place> {
        match &self.origin {
            Origin::Storage(place) if !self.previous => Some(place),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::analysis::storage_flow) enum Effect {
    Atomic,
    Mutation(Target, bray_symbols::DependencyMutationKind),
    Cleanup(TypeId),
}

impl Effect {
    pub(in crate::analysis::storage_flow) fn mutation(&self) -> Option<(&Target, bray_symbols::DependencyMutationKind)> {
        match self {
            Self::Mutation(target, kind) => Some((target, *kind)),
            Self::Atomic | Self::Cleanup(_) => None,
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct References {
    pub(super) targets: BTreeSet<Target>,
    pub(super) unknown: bool,
    pub(super) displaced: bool,
}

impl References {
    pub(super) fn one(target: Target) -> Self {
        Self {
            // A dynamic selector is bounded by this region even without one exact cell.
            unknown: false,
            targets: BTreeSet::from([target]),
            displaced: false,
        }
    }

    pub(super) fn merge(&mut self, incoming: &Self) -> bool {
        let count = self.targets.len();
        let unknown = self.unknown;
        let displaced = self.displaced;

        // Targets contain shared immutable projection paths.
        self.targets.extend(incoming.targets.iter().cloned());
        self.unknown |= incoming.unknown;
        self.displaced |= incoming.displaced;

        self.targets.len() != count || self.unknown != unknown || self.displaced != displaced
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
    pub(super) storage_type: Option<TypeId>,
    pub(super) initialized: BTreeSet<TypeId>,
    pub(super) pending: BTreeMap<TypeId, Origins>,
    pub(super) allocations: Origins,
    pub(super) invalidations: Origins,
}

impl Contents {
    pub(super) fn merge(&mut self, incoming: &Self) -> bool {
        let previous_exposure = self.exposed;

        self.exposed |= incoming.exposed;

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

        let mut changed = self.initialized.len() != count || self.storage_type != previous_type || self.exposed != previous_exposure;

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

/// A value captured before consumption, with inline protected payload state at its destination.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Transfer {
    pub(super) values: BTreeMap<Target, References>,
    pub(super) payloads: BTreeMap<Target, Contents>,
}

impl Transfer {
    pub(super) fn merge(&mut self, incoming: &Self) -> bool {
        let changed = merge_references(&mut self.values, &incoming.values, |_| false, |_| false);

        merge_contents(&mut self.payloads, &incoming.payloads) || changed
    }
}

/// Reference values are copied between slots. Contents and obligations belong to targets once.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::analysis::storage_flow) struct State {
    pub(super) owned_storage: std::sync::Arc<[bray_bound_tree::StorageIdentityId]>,
    pub(super) returned_roots: BTreeMap<Place, BTreeSet<bray_bound_tree::StorageIdentityId>>,
    pub(super) values: BTreeMap<Target, References>,
    pub(super) borrows: BTreeMap<BorrowCapabilityId, References>,
    pub(super) pending: BTreeMap<BoundExecutionSite, Transfer>,
    pub(super) deferred_effects: BTreeMap<
        BoundExecutionSite,
        Option<BTreeSet<Effect>>,
    >,
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

        let mut changed = merge_references(
            &mut self.values,
            &incoming.values,
            |slot| {
                slot.storage_path().is_some_and(|slot| {
                    absent.iter().any(|(path, reason)| {
                        *reason == StorageAbsence::NoPayload && path.contains(slot)
                    })
                })
            },
            |slot| {
                slot.storage_path().is_some_and(|slot| {
                    incoming_absent.iter().any(|(path, reason)| {
                        *reason == StorageAbsence::NoPayload && path.contains(slot)
                    })
                })
            },
        );

        for (reference, roots) in &incoming.returned_roots {
            let previous = self.returned_roots.entry(reference.clone()).or_default();
            let count = previous.len();

            previous.extend(roots);
            changed |= previous.len() != count;
        }

        changed |= merge_references(&mut self.borrows, &incoming.borrows, |_| false, |_| false);

        let absent = Transfer::default();

        for (call, transfer) in &mut self.pending {
            changed |= transfer.merge(incoming.pending.get(call).unwrap_or(&absent));
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
            Some(incoming) => {
                changed |= references.merge(incoming);
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
    ) -> impl Iterator<Item = (&Place, &Origin, bool)> {
        self.slots.iter().flat_map(|(slot, references)| {
            references
                .targets
                .iter()
                .map(move |target| (slot, &target.origin, references.is_exact()))
        })
    }
}
