use std::sync::OnceLock;
use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::{ExecutionCondition, ExecutionObservationKind as ObservationKind};
use super::retention::Set;
use super::memo::{ConditionMemo, cached_condition};
use super::traversal::NodeIdentity;

/// Actual retained roots own their query index. Adding a root invalidates that index.
#[derive(Default)]
pub(crate) struct RetainedSnapshots {
    roots: BTreeSet<ExecutionCondition>,
    index: OnceLock<SnapshotIndex>,
}

impl std::ops::Deref for RetainedSnapshots {
    type Target = BTreeSet<ExecutionCondition>;

    fn deref(&self) -> &Self::Target { &self.roots }
}

impl From<BTreeSet<ExecutionCondition>> for RetainedSnapshots {
    fn from(roots: BTreeSet<ExecutionCondition>) -> Self { Self { roots, index: OnceLock::new() } }
}

impl From<&BTreeSet<ExecutionCondition>> for RetainedSnapshots {
    fn from(roots: &BTreeSet<ExecutionCondition>) -> Self { roots.clone().into() }
}

impl FromIterator<ExecutionCondition> for RetainedSnapshots {
    fn from_iter<T: IntoIterator<Item = ExecutionCondition>>(values: T) -> Self {
        values.into_iter().collect::<BTreeSet<_>>().into()
    }
}

impl RetainedSnapshots {
    pub(crate) fn insert(&mut self, value: ExecutionCondition) -> bool {
        if !self.roots.insert(value) { return false; }

        self.index.take();

        true
    }

    pub(crate) fn retains_owner(&self, value: &ExecutionCondition) -> bool {
        if self.roots.is_empty() { return false; }

        let Some(identity) = value.ownership_identity() else { return false; };

        cached_condition(&self.index().owned, value, || {
            self.matching(value, false, |selected| {
                !selected.is_unknown() && selected.ownership_identity() == Some(identity)
            })
        })
    }

    /// Actual frames held in the retained owning tree.
    pub(crate) fn owned_deferred_frames(&self) -> impl Iterator<Item = &std::sync::Arc<super::super::ExecutionDeferredValue>> {
        self.roots.iter().any(ExecutionCondition::contains_deferred).then(|| self.index()).into_iter()
            .flat_map(|index| index.nodes.iter())
            .filter_map(|(value, reached)| match value.kind() {
                crate::ExecutionConditionKind::DeferredValue { frame, .. } if !reached => Some(frame),
                _ => None,
            })
    }

    /// A live reference can keep a carrier observable when checked effects preserve its data.
    pub(crate) fn retains_witness_carrier(&self, carrier: &ExecutionCondition, observes: impl Fn(&ExecutionCondition) -> bool) -> bool {
        self.matching(carrier, true, |selected| {
            !selected.is_unknown() && selected.has_same_owner(carrier) && observes(&selected.clone().into_referent())
        })
    }

    /// Checks the actual retained tree for an unchanged snapshot without following loans.
    pub(crate) fn retains_owned_snapshot(&self, value: &ExecutionCondition) -> bool {
        let (borrowed, owner) = value.owner_match_key();

        let index = self.index();

        index.owners.get(&(borrowed, owner.clone())).into_iter().flatten().any(|id| {
            let (snapshot, reached) = &index.nodes[*id];

            !reached && snapshot == value
        })
    }

    pub(crate) fn retains_contents(&self, value: &ExecutionCondition) -> bool {
        if self.roots.is_empty() { return false; }

        cached_condition(&self.index().preserved, value, || {
            // A retained field also retains its unchanged borrowed view. This
            // compares current contents and does not transfer referent ownership.
            if let crate::ExecutionConditionKind::Borrowed(referent) = value.kind()
                && self.matching(referent, true, |selected| Self::same_contents(selected, referent))
            {
                return true;
            }

            if let crate::ExecutionConditionKind::Observation { owner, contents, kind: ObservationKind::Reference } = value.kind()
                && contents.ownership_identity().is_some_and(|(kind, _)| kind == ObservationKind::Owned)
                && **owner == contents.owner_identity().clone().borrowed()
                && self.matching(contents, false, |selected| {
                    selected.has_same_owner(contents) && Self::same_contents(selected, contents)
                })
            {
                return true;
            }

            self.matching(value, true, |selected| Self::same_contents(selected, value))
        })
    }

    pub(crate) fn retains_inline_contents(&self, value: &ExecutionCondition) -> bool {
        self.matching(value, false, |selected| Self::same_contents(selected, value))
    }

    pub(crate) fn retains_contents_between(&self, after: &Self, observed: &ExecutionCondition) -> bool {
        if self.roots.is_empty() || after.roots.is_empty() { return false; }

        self.matching(observed, true, |previous| {
            after.matching(observed, true, |current| Self::same_contents(previous, current))
        })
    }

    fn same_contents(left: &ExecutionCondition, right: &ExecutionCondition) -> bool {
        let left = Self::contents(left).without_captured_storage();
        let right = Self::contents(right).without_captured_storage();

        !left.contains_unknown() && !right.contains_unknown() && left == right
    }

    fn contents(mut value: &ExecutionCondition) -> &ExecutionCondition {
        loop {
            let inline = value.inline_contents();

            match inline.kind() {
                crate::ExecutionConditionKind::CapturedValue { value: contents, .. } | crate::ExecutionConditionKind::DeferredValue { value: contents, .. } => value = contents,
                _ => return inline,
            }
        }
    }

    fn index(&self) -> &SnapshotIndex {
        self.index.get_or_init(|| SnapshotIndex::new(&self.roots))
    }

    fn matching(&self, value: &ExecutionCondition, observe_referents: bool, matches: impl Fn(&ExecutionCondition) -> bool) -> bool {
        if self.roots.is_empty() { return false; }

        let index = self.index();

        let (borrowed, owner) = value.owner_match_key();

        if index.owners.get(&(borrowed, owner.clone())).into_iter().flatten().any(|id| {
            let (snapshot, reached) = &index.nodes[*id];

            (observe_referents || !reached) && matches(snapshot)
        }) {
            return true;
        }

        let Some((root, path)) = Set::indexed_path(value) else { return false; };

        index.paths.get(&root).into_iter().flatten().any(|(prefix, id)| {
            let (snapshot, reached) = &index.nodes[*id];

            if !observe_referents && *reached || !path.starts_with(prefix)
                || path.len() - prefix.len() > ExecutionCondition::WORK_LIMIT { return false; }

            let mut selected = snapshot.clone();

            for projection in &path[prefix.len()..] {
                selected = selected.component(*projection);
            }

            if matches!((snapshot.owner_identity()).kind(), crate::ExecutionConditionKind::Borrowed(_)) && prefix.len() != path.len() {
                selected = selected.borrowed();
            }

            matches(&selected)
        })
    }
}

/// Candidate indexing never grants retention. Queries still check the selected actual snapshot.
#[derive(Default)]
struct SnapshotIndex {
    owned: ConditionMemo,
    preserved: ConditionMemo,
    nodes: Vec<(ExecutionCondition, bool)>,
    owners: BTreeMap<(bool, ExecutionCondition), Vec<usize>>,
    paths: BTreeMap<ExecutionCondition, Vec<(Vec<bray_symbols::ConstantProjectionKind>, usize)>>,
}

impl SnapshotIndex {
    fn new(roots: &BTreeSet<ExecutionCondition>) -> Self {
        let mut index = Self::default();
        let mut pending = roots.iter().map(|value| (value, false)).collect::<Vec<_>>();
        let mut visited = HashSet::new();

        while let Some((snapshot, reached)) = pending.pop() {
            if !visited.insert((NodeIdentity(snapshot), reached)) { continue; }

            let id = index.nodes.len();

            let (borrowed, owner) = snapshot.owner_match_key();

            index.owners.entry((borrowed, owner.clone())).or_default().push(id);

            if let Some((root, path)) = Set::indexed_path(snapshot) {
                index.paths.entry(root).or_default().push((path, id));
            }

            index.nodes.push((snapshot.clone(), reached));
            pending.extend(snapshot.owned_components().map(|value| (value, reached)));

            // A loan retains its reached data, but never the referent's ownership.
            match snapshot.kind() {
                crate::ExecutionConditionKind::Borrowed(referent)
                | crate::ExecutionConditionKind::Observation { contents: referent, kind: ObservationKind::Reference, .. } => pending.push((referent, true)),
                _ => {}
            }
        }

        index
    }
}
