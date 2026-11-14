use std::sync::OnceLock;
use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::{ExecutionCondition, ExecutionObservationKind as ObservationKind};
use super::retention::Set;
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

        let Some((role, owner)) = value.ownership_identity() else { return false; };

        self.matching(value, false, |selected| {
            !selected.is_unknown() && selected.ownership_identity() == Some((role, owner))
        })
    }

    pub(crate) fn retains_contents(&self, value: &ExecutionCondition) -> bool {
        if self.roots.is_empty() { return false; }

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
    }

    pub(super) fn retains_inline_contents(&self, value: &ExecutionCondition) -> bool {
        self.matching(value, false, |selected| Self::same_contents(selected, value))
    }

    pub(crate) fn retains_contents_between(&self, after: &Self, observed: &ExecutionCondition) -> bool {
        if self.roots.is_empty() || after.roots.is_empty() { return false; }

        self.matching(observed, true, |previous| {
            after.matching(observed, true, |current| Self::same_contents(previous, current))
        })
    }

    fn same_contents(left: &ExecutionCondition, right: &ExecutionCondition) -> bool {
        let left = Self::contents(left);
        let right = Self::contents(right);

        !left.contains_unknown() && !right.contains_unknown() && left == right
    }

    fn contents(mut value: &ExecutionCondition) -> &ExecutionCondition {
        while let crate::ExecutionConditionKind::Observation { contents, kind: ObservationKind::Owned | ObservationKind::Scalar, .. } = value.kind() {
            value = contents;
        }

        value
    }

    fn matching(&self, value: &ExecutionCondition, observe_referents: bool, matches: impl Fn(&ExecutionCondition) -> bool) -> bool {
        if self.roots.is_empty() { return false; }

        let index = self.index.get_or_init(|| SnapshotIndex::new(&self.roots));

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
