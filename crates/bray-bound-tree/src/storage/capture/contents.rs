use std::collections::{BTreeMap, BTreeSet};
use crate::{BoundExecutionSite, BoundExpressionId, StorageTarget as Target};
use bray_symbols::TypeId;

type Origins = BTreeSet<BoundExpressionId>;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
enum Initialization {
    Observed,
    Written,
    Consumed,
}

#[derive(Clone, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct StorageContents {
    pub exposed: bool,
    pub escaped: bool,
    pub storage_type: Option<TypeId>,
    initialized: BTreeMap<TypeId, Initialization>,
    pub pending: BTreeMap<TypeId, Origins>,
    pub allocations: Origins,
    pub invalidations: BTreeSet<BoundExecutionSite>,
}

impl StorageContents {
    pub fn is_initialized(&self, ty: TypeId) -> bool {
        self.initialized.get(&ty).is_some_and(|state| *state != Initialization::Consumed)
    }

    pub fn has_initialization(&self) -> bool {
        self.initialized.values().any(|state| *state != Initialization::Consumed)
    }

    /// Exact typed cells changed by this execution and retained at its exit.
    pub fn cell_changes(&self) -> impl Iterator<Item = bray_symbols::DependencyMutationKind> + '_ {
        self.initialized.iter().filter_map(|(&ty, state)| match state {
            Initialization::Written => Some(bray_symbols::DependencyMutationKind::Replace(ty)),
            Initialization::Consumed => Some(bray_symbols::DependencyMutationKind::Discharge(ty)),
            Initialization::Observed => None,
        })
    }

    pub fn observe_initialized(&mut self, ty: TypeId) {
        self.initialized.entry(ty).and_modify(|state| {
            if *state == Initialization::Consumed { *state = Initialization::Observed; }
        }).or_insert(Initialization::Observed);
    }

    pub fn record_write(&mut self, ty: TypeId) {
        self.initialized.insert(ty, Initialization::Written);
    }

    /// Records a definite whole-value transfer out of this typed cell.
    pub fn record_consumption(&mut self, ty: TypeId) {
        self.initialized.insert(ty, Initialization::Consumed);
        self.pending.remove(&ty);
    }

    /// Types whose duties have already been transferred from this exact cell.
    pub fn consumed_types(&self) -> impl Iterator<Item = TypeId> + '_ {
        self.initialized.iter().filter_map(|(&ty, state)| (*state == Initialization::Consumed).then_some(ty))
    }

    pub fn clear_initialization(&mut self) -> bool {
        let changed = !self.initialized.is_empty();

        self.initialized.clear();

        changed
    }

    pub fn retain_initialization(&mut self, preserve_consumption: bool, mut retain: impl FnMut(TypeId) -> bool) {
        self.initialized.retain(|ty, state| {
            if *state == Initialization::Consumed { preserve_consumption } else { retain(*ty) }
        });
    }

    /// Ends the lifetime at the operation that retired this storage.
    pub fn retire(&mut self, origin: BoundExecutionSite) {
        self.retain_initialization(true, |_| false);
        self.invalidations.insert(origin);
    }

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

        let mut changed = self.storage_type != previous_type
            || self.exposed != previous_exposure
            || self.escaped != previous_escape;

        self.initialized.retain(|ty, kind| {
            let Some(other) = incoming.initialized.get(ty) else {
                changed = true;

                return false;
            };

            if (*kind == Initialization::Consumed) != (*other == Initialization::Consumed) {
                changed = true;

                return false;
            }

            let joined = (*kind).min(*other);

            changed |= *kind != joined;
            *kind = joined;

            true
        });

        for (&ty, origins) in &incoming.pending {
            changed |= merge_origins(self.pending.entry(ty).or_default(), origins);
        }

        changed |= merge_origins(&mut self.allocations, &incoming.allocations);
        changed |= merge_origins(&mut self.invalidations, &incoming.invalidations);

        changed
    }
}

fn merge_origins<T: Copy + Ord>(target: &mut BTreeSet<T>, incoming: &BTreeSet<T>) -> bool {
    let count = target.len();

    target.extend(incoming.iter().copied());

    target.len() != count
}
