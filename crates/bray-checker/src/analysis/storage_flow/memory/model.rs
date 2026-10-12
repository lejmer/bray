use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::BoundExpressionId;
use bray_symbols::TypeId;

pub(in crate::analysis::storage_flow) use bray_bound_tree::StoragePath as Place;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum Origin {
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
    pub(super) contents: BTreeMap<Target, Contents>,
}

impl State {
    pub(in crate::analysis::storage_flow) fn merge(&mut self, incoming: &Self) -> bool {
        let mut changed = false;

        for (slot, references) in &mut self.slots {
            match incoming.slots.get(slot) {
                Some(incoming) => {
                    changed |= references.merge(incoming);
                }
                None if !references.unknown => {
                    references.unknown = true;
                    changed = true;
                }
                None => {}
            }
        }

        for (slot, references) in &incoming.slots {
            if !self.slots.contains_key(slot) {
                // Joined slots own their target alternatives independently of predecessor state.
                let mut references = references.clone();

                references.unknown = true;
                self.slots.insert(slot.clone(), references);
                changed = true;
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
