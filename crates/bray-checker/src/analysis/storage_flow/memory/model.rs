use std::collections::{BTreeMap, BTreeSet};

use crate::analysis::storage_flow::model::StorageAbsence;
use bray_bound_tree::{BorrowCapabilityId, BoundExecutionSite, BoundExpressionId};
use bray_symbols::TypeId;

pub(in crate::analysis::storage_flow) use bray_bound_tree::StoragePath as Place;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::analysis::storage_flow) enum Origin {
    Entry(Place),
    Storage(Place),
    Allocation(Place),
    Uninit(Place),
}

/// `previous` summarizes older executions of one creation site. It is never a must-alias.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct Target {
    pub(super) origin: Origin,
    pub(super) previous: bool,
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
            targets: BTreeSet::from([target]),
            unknown: false,
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

    pub(super) fn may_alias_anywhere(&self) -> bool {
        self.unknown
            || self
                .targets
                .iter()
                .any(|target| matches!(target.origin, Origin::Entry(_)))
    }

    pub(super) fn is_exact(&self) -> bool {
        !self.unknown
            && !self.displaced
            && self.targets.len() == 1
            && !self.targets.first().expect("one reference target").previous
    }
}

type Origins = BTreeSet<BoundExpressionId>;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Contents {
    pub(super) storage_type: Option<TypeId>,
    pub(super) initialized: BTreeSet<TypeId>,
    pub(super) pending: BTreeMap<TypeId, Origins>,
    pub(super) allocations: Origins,
    pub(super) invalidations: Origins,
    pub(super) references: References,
}

impl Contents {
    pub(super) fn merge(&mut self, incoming: &Self) -> bool {
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

        let mut changed = self.initialized.len() != count || self.storage_type != previous_type;

        for (&ty, origins) in &incoming.pending {
            changed |= merge_origins(self.pending.entry(ty).or_default(), origins);
        }

        changed |= merge_origins(&mut self.allocations, &incoming.allocations);
        changed |= merge_origins(&mut self.invalidations, &incoming.invalidations);
        changed |= self.references.merge(&incoming.references);

        changed
    }
}

fn merge_origins(target: &mut Origins, incoming: &Origins) -> bool {
    let count = target.len();

    target.extend(incoming.iter().copied());

    target.len() != count
}

/// Reference values are copied between slots. Contents and obligations belong to targets once.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(in crate::analysis::storage_flow) struct State {
    pub(super) slots: BTreeMap<Place, References>,
    pub(super) borrows: BTreeMap<BorrowCapabilityId, References>,
    pub(super) pending: BTreeMap<BoundExecutionSite, BTreeMap<Place, References>>,
    pub(super) contents: BTreeMap<Target, Contents>,
}

impl State {
    pub(in crate::analysis::storage_flow) fn merge(
        &mut self,
        incoming: &Self,
        absent: &BTreeMap<Place, StorageAbsence>,
        incoming_absent: &BTreeMap<Place, StorageAbsence>,
    ) -> bool {
        let mut changed = merge_references(
            &mut self.slots,
            &incoming.slots,
            |slot| {
                absent.iter().any(|(path, reason)| {
                    *reason == StorageAbsence::NoPayload && path.contains(slot)
                })
            },
            |slot| {
                incoming_absent.iter().any(|(path, reason)| {
                    *reason == StorageAbsence::NoPayload && path.contains(slot)
                })
            },
        );

        changed |= merge_references(&mut self.borrows, &incoming.borrows, |_| false, |_| false);

        let absent = BTreeMap::new();

        for (call, references) in &mut self.pending {
            changed |= merge_references(
                references,
                incoming.pending.get(call).unwrap_or(&absent),
                |_| false,
                |_| false,
            );
        }

        for (call, references) in &incoming.pending {
            if !self.pending.contains_key(call) {
                let mut joined = BTreeMap::new();

                changed |= merge_references(&mut joined, references, |_| false, |_| false);
                self.pending.insert(*call, joined);
            }
        }

        for (target, contents) in &mut self.contents {
            match incoming.contents.get(target) {
                Some(incoming) => {
                    changed |= contents.merge(incoming);
                }
                None => {
                    changed |= !contents.initialized.is_empty();
                    contents.initialized.clear();
                }
            }
        }

        for (target, contents) in &incoming.contents {
            if !self.contents.contains_key(target) {
                // Each block owns its merged contents. Origins and obligations may survive one path.
                let mut contents = contents.clone();

                contents.initialized.clear();
                self.contents.insert(target.clone(), contents);
                changed = true;
            }
        }

        changed
    }
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
