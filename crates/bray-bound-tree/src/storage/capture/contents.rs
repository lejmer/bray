use std::collections::{BTreeMap, BTreeSet};
use crate::{BoundExpressionId, StorageTarget as Target};
use bray_symbols::TypeId;

type Origins = BTreeSet<BoundExpressionId>;

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageContents {
    pub exposed: bool,
    pub escaped: bool,
    pub storage_type: Option<TypeId>,
    pub initialized: BTreeSet<TypeId>,
    pub pending: BTreeMap<TypeId, Origins>,
    pub allocations: Origins,
    pub invalidations: Origins,
}

impl StorageContents {
    /// Retains possible exposure, provenance and outstanding cleanup after the actual value is lost.
    pub fn into_bounds(mut self) -> Self {
        self.storage_type = None;
        self.initialized.clear();

        self
    }

    pub fn merge(&mut self, incoming: &Self, target: &Target) -> bool {
        let previous_exposure = self.exposed;
        let previous_escape = self.escaped;

        self.exposed |= incoming.exposed;
        self.escaped |= incoming.escaped;

        let previous_type = self.storage_type;

        match (self.storage_type, incoming.storage_type) {
            (Some(left), Some(right)) => assert_eq!(
                left, right,
                "one physical storage path has one declared type: {target:?}"
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
