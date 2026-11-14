use hashbrown::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use super::{ExecutionCondition, storage_key::{StorageKey, StorageLookup}};

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(in crate::execution_guarantees) struct ConditionId(usize);

struct Node {
    condition: ExecutionCondition,
    children: Vec<ConditionId>,
}

/// Structural identity is prepared once per retained allocation, using already
/// interned children. Identity carries no proof result or execution authority.
#[derive(Default)]
struct Conditions {
    storage: HashMap<StorageKey, ConditionId>,
    buckets: HashMap<u64, Vec<ConditionId>>,
    nodes: Vec<Node>,
    #[cfg(test)]
    collide: bool,
}

impl Conditions {
    fn intern(&mut self, condition: &ExecutionCondition) -> ConditionId {
        if let Some(value) = self.storage.get(&StorageLookup(condition)) { return *value; }

        let mut hash = std::hash::DefaultHasher::new();
        let mut children = Vec::new();

        condition.hash_parts(&mut hash, &mut |child, _, hash| {
            let child = self.intern(child);

            child.hash(hash);
            children.push(child);
        });

        let hash = hash.finish();

        #[cfg(test)]
        let hash = if self.collide { 0 } else { hash };

        let bucket = self.buckets.entry(hash).or_default();

        let id = bucket.iter().copied().find(|id| {
            let candidate = &self.nodes[id.0];

            candidate.children == children && candidate.condition.eq_parts(condition, &mut |_, _| true)
        }).unwrap_or_else(|| {
            let id = ConditionId(self.nodes.len());

            // The index owns root fields and shares immutable source operands.
            self.nodes.push(Node { condition: condition.clone(), children });
            bucket.push(id);

            id
        });

        self.storage.insert(StorageKey::from(condition), id);

        id
    }
}

/// One analysis shares structural identity independently of evidence scopes.
#[derive(Clone, Default)]
pub(crate) struct ConditionIndex(Arc<Mutex<Conditions>>);

impl ConditionIndex {
    pub(in crate::execution_guarantees) fn intern(&self, condition: &ExecutionCondition) -> ConditionId {
        self.0.lock().expect("condition index lock is not poisoned").intern(condition)
    }

    pub(in crate::execution_guarantees) fn map<T>(&self) -> ConditionMap<T> {
        ConditionMap { conditions: self.clone(), values: HashMap::new() }
    }
}

/// Memo values belong to this exact query scope. Only structural identity is shared.
pub(in crate::execution_guarantees) struct ConditionMap<T> {
    conditions: ConditionIndex,
    values: HashMap<ConditionId, T>,
}

impl<T> Default for ConditionMap<T> {
    fn default() -> Self { ConditionIndex::default().map() }
}

impl<T> ConditionMap<T> {
    pub(super) fn get(&self, condition: &ExecutionCondition) -> Option<&T> {
        self.lookup(condition).1
    }

    /// Retain exact identity across recursive computation without holding this map's lock.
    pub(super) fn lookup(&self, condition: &ExecutionCondition) -> (ConditionId, Option<&T>) {
        let key = self.conditions.intern(condition);

        (key, self.values.get(&key))
    }

    pub(super) fn entry(&mut self, key: ConditionId) -> hashbrown::hash_map::Entry<'_, ConditionId, T, hashbrown::DefaultHashBuilder> {
        self.values.entry(key)
    }

    pub(super) fn insert(&mut self, condition: &ExecutionCondition, value: T) {
        self.values.insert(self.conditions.intern(condition), value);
    }

    pub(super) fn get_or_insert_with(&mut self, condition: &ExecutionCondition, create: impl FnOnce() -> T) -> &mut T {
        self.values.entry(self.conditions.intern(condition)).or_insert_with(create)
    }

    pub(super) fn clear(&mut self) { self.values.clear(); }

}

impl ConditionMap<ExecutionCondition> {
    pub(super) fn is_identity(&self) -> bool {
        self.values.iter().all(|(key, value)| *key == self.conditions.intern(value))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_bound_tree::BoundOperator::{LogicalAnd, LogicalOr};

    use super::{ConditionIndex, Conditions, ExecutionCondition as C};

    #[test]
    fn semantic_keys_merge_equal_graphs_without_conflating_collisions_or_qualifiers() {
        let shared = C::Operation(LogicalOr, vec![C::Boolean(true), C::Unknown].into());
        let copied = C::Operation(LogicalOr, vec![C::Boolean(true), C::Unknown].into());

        let conditions = [shared.clone(), copied,
            C::Operation(LogicalAnd, vec![C::Boolean(true), C::Unknown].into()),
            C::Trusted(Arc::new(shared.clone())), C::Borrowed(Arc::new(shared.clone())),
            C::Entry { condition: Arc::new(shared.clone()), captured: true },
            C::Entry { condition: Arc::new(shared), captured: false }];

        for collide in [false, true] {
            let mut index = Conditions { collide, ..Conditions::default() };

            for left in &conditions {
                for right in &conditions {
                    assert_eq!(Some(left.cmp(right)), left.partial_cmp(right), "shared-storage shortcuts retain structural ordering");
                    assert_eq!(index.intern(left) == index.intern(right), left == right);
                }
            }
        }

        let shared = ConditionIndex::default();
        let mut first = shared.map();
        let mut second = shared.map();

        first.insert(&conditions[0], true);
        second.insert(&conditions[0], false);
        assert_eq!(first.get(&conditions[1]), Some(&true));
        assert_eq!(second.get(&conditions[1]), Some(&false), "structural identity cannot share query authority");
        first.clear();
        assert_eq!(first.get(&conditions[0]), None);
    }

    #[test]
    fn separately_allocated_shared_graphs_intern_by_nodes_without_expanding_paths() {
        let graph = || {
            let mut value = C::Boolean(false);

            for _ in 0..20 {
                value = C::Operation(LogicalOr, vec![value.clone(), value].into());
            }

            value
        };

        let mut index = Conditions::default();
        let first = index.intern(&graph());
        let second = index.intern(&graph());

        assert!(first == second);
        assert_eq!(index.nodes.len(), 21, "equivalent shared graphs need one identity per distinct node");
        assert!(index.storage.len() <= 82, "storage indexing must not expand the exponentially many paths");
    }
}
