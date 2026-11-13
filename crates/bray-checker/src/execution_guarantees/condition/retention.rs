use super::{ExecutionCondition, ExecutionObservationKind as ObservationKind};
use super::memo::{ConditionMemo, cached_condition};

/// Indexed changed values and their observed component paths.
#[derive(Default)]
pub(crate) struct Set {
    preserved: ConditionMemo,
    discriminants: std::collections::BTreeSet<ExecutionCondition>,
    copies: std::collections::BTreeSet<ExecutionCondition>,
    values: std::collections::BTreeSet<ExecutionCondition>,
    paths: std::collections::BTreeMap<
        ExecutionCondition,
        std::collections::BTreeSet<Vec<bray_symbols::ConstantProjectionKind>>,
    >,
}

impl Set {
    /// Tests ownership against actual retained snapshots, without a prefix grant.
    pub(crate) fn retains_owner(
        snapshots: &std::collections::BTreeSet<ExecutionCondition>,
        value: &ExecutionCondition,
    ) -> bool {
        if snapshots.is_empty() { return false; }

        let Some((role, owner)) = value.ownership_identity() else { return false; };

        Self::retained_snapshot(snapshots, value, false, |selected| {
            !selected.is_unknown() && selected.ownership_identity() == Some((role, owner))
        })
    }

    /// Tests actual selected data independently of the containing owner's identity.
    pub(crate) fn retains_contents(
        snapshots: &std::collections::BTreeSet<ExecutionCondition>,
        value: &ExecutionCondition,
    ) -> bool {
        if snapshots.is_empty() { return false; }

        if let ExecutionCondition::Observation { owner, contents, kind: ObservationKind::Reference } = value
            && contents.ownership_identity().is_some_and(|(kind, _)| kind == ObservationKind::Owned)
            && **owner == contents.owner_identity().clone().borrowed()
            && Self::retained_snapshot(snapshots, contents, false, |selected| {
                selected.has_same_owner(contents) && Self::same_snapshot_contents(selected, contents)
            })
        {
            return true;
        }

        Self::retained_snapshot(snapshots, value, true, |selected| {
            Self::same_snapshot_contents(selected, value)
        })
    }

    /// Compares actual predecessor and terminal data at the same observed path.
    pub(crate) fn retains_contents_between(
        before: &std::collections::BTreeSet<ExecutionCondition>,
        after: &std::collections::BTreeSet<ExecutionCondition>,
        observed: &ExecutionCondition,
    ) -> bool {
        if before.is_empty() || after.is_empty() { return false; }

        Self::retained_snapshot(before, observed, true, |previous| {
            Self::retained_snapshot(after, observed, true, |current| Self::same_snapshot_contents(previous, current))
        })
    }

    fn same_snapshot_contents(left: &ExecutionCondition, right: &ExecutionCondition) -> bool {
        let left = Self::snapshot_contents(left);
        let right = Self::snapshot_contents(right);

        !left.contains_unknown() && !right.contains_unknown() && left == right
    }

    fn snapshot_contents(mut value: &ExecutionCondition) -> &ExecutionCondition {
        while let ExecutionCondition::Observation { contents, kind: ObservationKind::Owned | ObservationKind::Scalar, .. } = value {
            value = contents;
        }

        value
    }

    fn retained_snapshot(
        snapshots: &std::collections::BTreeSet<ExecutionCondition>,
        value: &ExecutionCondition,
        observe_referents: bool,
        matches: impl Fn(&ExecutionCondition) -> bool,
    ) -> bool {
        if snapshots.is_empty() { return false; }

        let path = Self::indexed_path(value);
        let mut pending = snapshots.iter().collect::<Vec<_>>();

        // Actual snapshots form a finite shared graph. Visit each value once so
        // unrelated roots cannot spend another value's structural work budget.
        let mut visited = std::collections::BTreeSet::new();

        while let Some(snapshot) = pending.pop() {
            if !visited.insert(snapshot) {
                continue;
            }

            if snapshot.has_same_owner(value) && matches(snapshot) {
                return true;
            }

            if let (Some((root, path)), Some((base, prefix))) = (&path, Self::indexed_path(snapshot))
                && *root == base && path.starts_with(&prefix)
                && path.len() - prefix.len() <= ExecutionCondition::WORK_LIMIT
            {
                let mut selected = snapshot.clone();

                for projection in &path[prefix.len()..] {
                    selected = selected.component(*projection);
                }

                if matches!(snapshot.owner_identity(), ExecutionCondition::Borrowed(_))
                    && !path[prefix.len()..].is_empty()
                {
                    selected = selected.borrowed();
                }

                if matches(&selected) {
                    return true;
                }
            }

            // The base of a constructed value names its lineage, not another
            // independently retained copy of its former contents.
            pending.extend(snapshot.owned_components());

            if observe_referents {
                // A live loan retains its actual reached data without owning it.
                // Ownership queries never traverse this reference boundary.
                match snapshot {
                    ExecutionCondition::Borrowed(referent)
                    | ExecutionCondition::Observation { contents: referent, kind: ObservationKind::Reference, .. } => pending.push(referent),
                    _ => {}
                }
            }
        }

        false
    }

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
            || Self::retains_contents(&self.values, value)
            || matches!(value, ExecutionCondition::Borrowed(referent) if Self::retains_contents(&self.values, referent))
            || Self::retained_snapshot(&self.copies, value, false,
                |selected| Self::same_snapshot_contents(selected, value))
        {
            return true;
        }

        let subject = match value {
            ExecutionCondition::VariantTest(_, subject) => Some(subject.as_ref()),
            value => value.nullable_absence(true).or_else(|| value.nullable_absence(false)).map(|(subject, _)| subject),
        };

        subject.is_some_and(|subject| self.discriminants.contains(&subject.discriminant()))
    }

    pub(crate) fn insert(&mut self, value: ExecutionCondition) {
        self.preserve_scalar_data(&value);

        if let Some((root, path)) = Self::indexed_path(&value) {
            self.paths.entry(root).or_default().insert(path);
        }

        if self.values.insert(value) { self.clear_preserved(); }
    }

    /// A surviving scalar snapshot retains data without keeping its source slot alive.
    pub(crate) fn preserve_scalar_data(&mut self, value: &ExecutionCondition) {
        if let Some((_, contents)) = value.storage_contents() {
            let copied = contents.clone().argument_value(false);

            if !copied.is_unknown() && self.copies.insert(copied) { self.clear_preserved(); }
        }
    }

    pub(crate) fn contains(&self, value: &ExecutionCondition) -> bool {
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
    }

    pub(crate) fn overlaps(&self, value: &ExecutionCondition) -> bool {
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
        match value {
            // Retaining a borrow preserves that view and its projections. It
            // cannot retain or retire the referent's ownership on its own.
            ExecutionCondition::Borrowed(referent) => {
                let (root, path) = referent.observation_path()?;

                Some((root.borrowed(), path))
            }
            value if matches!(value.owner_identity(), ExecutionCondition::Borrowed(_)) => {
                Self::indexed_path(value.owner_identity())
            }
            value => value.observation_path(),
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
