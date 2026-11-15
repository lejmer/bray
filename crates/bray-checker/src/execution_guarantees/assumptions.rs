use std::collections::BTreeSet;
use std::sync::{Arc, LazyLock};

use super::ExecutionCondition;

/// Shared evidence snapshots detach only when their established conditions change.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ExecutionAssumptions(Arc<BTreeSet<(ExecutionCondition, bool)>>);

impl Default for ExecutionAssumptions {
    fn default() -> Self {
        static EMPTY: LazyLock<Arc<BTreeSet<(ExecutionCondition, bool)>>> =
            LazyLock::new(|| Arc::new(BTreeSet::new()));

        Self(Arc::clone(&EMPTY))
    }
}

impl std::ops::Deref for ExecutionAssumptions {
    type Target = BTreeSet<(ExecutionCondition, bool)>;

    fn deref(&self) -> &Self::Target { &self.0 }
}

impl ExecutionAssumptions {
    pub(in crate::execution_guarantees) fn storage_key(&self) -> usize { Arc::as_ptr(&self.0).addr() }

    #[cfg(test)]
    pub(crate) fn shares_storage(&self, other: &Self) -> bool { Arc::ptr_eq(&self.0, &other.0) }

    #[cfg(test)]
    pub(crate) fn clear(&mut self) { *self = Self::default(); }

    pub(crate) fn insert(&mut self, condition: (ExecutionCondition, bool)) -> bool {
        if let Some(conditions) = Arc::get_mut(&mut self.0) { return conditions.insert(condition); }
        if self.0.contains(&condition) { return false; }

        Arc::make_mut(&mut self.0).insert(condition)
    }

    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&(ExecutionCondition, bool)) -> bool) {
        if let Some(conditions) = Arc::get_mut(&mut self.0) { conditions.retain(keep); return; }

        // Evaluate each predicate once and retain snapshot identity when nothing changes.
        let removed = self.0.iter().filter(|condition| !keep(condition)).cloned().collect::<Vec<_>>();

        if removed.is_empty() { return; }

        if removed.len() == self.len() { *self = Self::default(); return; }

        let conditions = Arc::make_mut(&mut self.0);

        for condition in removed { conditions.remove(&condition); }
    }
}

impl Extend<(ExecutionCondition, bool)> for ExecutionAssumptions {
    fn extend<T: IntoIterator<Item = (ExecutionCondition, bool)>>(&mut self, conditions: T) {
        for condition in conditions { self.insert(condition); }
    }
}

impl From<BTreeSet<(ExecutionCondition, bool)>> for ExecutionAssumptions {
    fn from(conditions: BTreeSet<(ExecutionCondition, bool)>) -> Self {
        if conditions.is_empty() { Self::default() } else { Self(Arc::new(conditions)) }
    }
}

impl<const N: usize> From<[(ExecutionCondition, bool); N]> for ExecutionAssumptions {
    fn from(conditions: [(ExecutionCondition, bool); N]) -> Self { BTreeSet::from(conditions).into() }
}

impl FromIterator<(ExecutionCondition, bool)> for ExecutionAssumptions {
    fn from_iter<T: IntoIterator<Item = (ExecutionCondition, bool)>>(conditions: T) -> Self {
        conditions.into_iter().collect::<BTreeSet<_>>().into()
    }
}

impl<'a> IntoIterator for &'a ExecutionAssumptions {
    type Item = &'a (ExecutionCondition, bool);
    type IntoIter = std::collections::btree_set::Iter<'a, (ExecutionCondition, bool)>;

    fn into_iter(self) -> Self::IntoIter { self.0.iter() }
}

impl IntoIterator for ExecutionAssumptions {
    type Item = (ExecutionCondition, bool);
    type IntoIter = std::collections::btree_set::IntoIter<(ExecutionCondition, bool)>;

    fn into_iter(self) -> Self::IntoIter {
        // Consuming a shared snapshot owns its keys without mutating another reader.
        Arc::unwrap_or_clone(self.0).into_iter()
    }
}
