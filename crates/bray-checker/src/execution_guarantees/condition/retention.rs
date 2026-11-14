use super::{ExecutionCondition, RetainedSnapshots};
use super::memo::{ConditionMemo, cached_condition};

/// Indexed changed values and their observed component paths.
#[derive(Default)]
pub(crate) struct Set {
    preserved: ConditionMemo,
    contained: ConditionMemo,
    discriminants: std::collections::BTreeSet<ExecutionCondition>,
    copies: RetainedSnapshots,
    values: RetainedSnapshots,
    paths: std::collections::BTreeMap<
        ExecutionCondition,
        std::collections::BTreeSet<Vec<bray_symbols::ConstantProjectionKind>>,
    >,
}

impl Set {
    pub(crate) fn insert_classified(&mut self, value: ExecutionCondition, kind: bray_bound_tree::StorageObservationKind) {
        if kind == bray_bound_tree::StorageObservationKind::Discriminant {
            if self.discriminants.insert(value.discriminant()) { self.clear_preserved(); }
        } else if kind.is_immutable_inline() {
            self.insert(value);
        }
    }

    /// Retained scalar data preserves value facts independently of its former storage.
    /// A retained tag supports only presence and variant observations.
    pub(crate) fn preserves(&self, value: &ExecutionCondition) -> bool {
        if self.values.is_empty() && self.copies.is_empty() && self.discriminants.is_empty() { return false; }

        cached_condition(&self.preserved, value, || self.preserves_uncached(value))
    }

    fn clear_preserved(&mut self) {
        self.preserved.get_mut().expect("retained observation memo lock is not poisoned").clear();
    }

    fn preserves_uncached(&self, value: &ExecutionCondition) -> bool {
        if self.copies.contains(value)
            || self.values.retains_contents(value)
            || matches!(value.kind(), crate::ExecutionConditionKind::Borrowed(referent) if self.values.retains_contents(referent))
            || self.copies.retains_inline_contents(value)
        {
            return true;
        }

        let subject = match value.kind() {
            crate::ExecutionConditionKind::VariantTest(_, subject) => Some(subject.as_ref()),
            _ => value.nullable_absence(true).or_else(|| value.nullable_absence(false)).map(|(subject, _)| subject),
        };

        subject.is_some_and(|subject| self.discriminants.contains(&subject.discriminant()))
    }

    pub(crate) fn insert(&mut self, value: ExecutionCondition) {
        self.preserve_scalar_data(&value);

        if let Some((root, path)) = Self::indexed_path(&value) {
            self.paths.entry(root).or_default().insert(path);
        }

        if self.values.insert(value) {
            self.contained.get_mut().expect("observation membership memo lock is not poisoned").clear();
            self.clear_preserved();
        }
    }

    /// A surviving scalar snapshot retains data without keeping its source slot alive.
    pub(crate) fn preserve_scalar_data(&mut self, value: &ExecutionCondition) {
        if let Some((_, contents)) = value.storage_contents() {
            let copied = contents.clone().argument_value(false);

            if !copied.is_unknown() && self.copies.insert(copied) { self.clear_preserved(); }
        }
    }

    pub(crate) fn contains(&self, value: &ExecutionCondition) -> bool {
        if self.values.is_empty() { return false; }

        cached_condition(&self.contained, value, || {
            if self.values.contains(value) {
                return true;
            }

            let Some((root, path)) = Self::indexed_path(value) else {
                return false;
            };

            let Some(paths) = self.paths.get(&root) else {
                return false;
            };

            (0..=path.len()).any(|length| paths.contains(&path[..length]))
        })
    }

    pub(crate) fn overlaps(&self, value: &ExecutionCondition) -> bool {
        if self.values.is_empty() { return false; }

        if self.contains(value) {
            return true;
        }

        let Some((root, path)) = Self::indexed_path(value) else {
            return false;
        };

        self.paths.get(&root).is_some_and(|paths| {
            paths
                .range(path.clone()..)
                .next()
                .is_some_and(|candidate| candidate.starts_with(&path))
        })
    }

    pub(super) fn indexed_path(
        value: &ExecutionCondition,
    ) -> Option<(
        ExecutionCondition,
        Vec<bray_symbols::ConstantProjectionKind>,
    )> {
        match value.kind() {
            // Retaining a borrow preserves that view and its projections. It
            // cannot retain or retire the referent's ownership on its own.
            crate::ExecutionConditionKind::Borrowed(referent) => {
                let (root, path) = referent.observation_path()?;

                Some((root.borrowed(), path))
            }
            _ if matches!(value.owner_identity().kind(), crate::ExecutionConditionKind::Borrowed(_)) => {
                Self::indexed_path(value.owner_identity())
            }
            _ => value.observation_path(),
        }
    }
}

impl FromIterator<ExecutionCondition> for Set {
    fn from_iter<T: IntoIterator<Item = ExecutionCondition>>(values: T) -> Self {
        let mut set = Self::default();

        for value in values { set.insert(value); }

        set
    }
}
