use std::hash::{Hash, Hasher};

use super::{ExecutionCondition, ExecutionConditionKind};

impl Hash for ExecutionCondition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.cached_hash().hash(state);
    }
}

/// Computes one node's hash from its root fields and already hashed children.
pub(super) fn structural_hash(kind: &ExecutionConditionKind) -> u64 {
    let mut hash = std::hash::DefaultHasher::new();

    kind.hash(&mut hash);

    hash.finish()
}

#[cfg(test)]
mod tests {
    use std::hash::{Hash, Hasher};

    use super::{ExecutionCondition as C, ExecutionConditionKind as K};
    use bray_bound_tree::BoundOperator::LogicalOr;

    fn tree(depth: usize) -> C {
        if depth == 0 { return C::boolean(false); }

        C::new(K::Operation(LogicalOr, vec![tree(depth - 1), tree(depth - 1)].into()))
    }

    #[test]
    fn shared_and_independent_equal_graphs_hash_identically() {
        let mut shared = C::boolean(false);

        for _ in 0..10 {
            shared = C::new(K::Operation(LogicalOr, vec![shared.clone(), shared].into()));
        }

        let independent = tree(10);

        let digest = |value: &C| {
            let mut hash = std::hash::DefaultHasher::new();

            value.hash(&mut hash);

            hash.finish()
        };

        let child = std::sync::Arc::new(C::boolean(false));
        let shared_slots = C::new(K::ValueAlias(std::sync::Arc::clone(&child), std::sync::Arc::clone(&child)));
        let copied_slots = C::new(K::ValueAlias(child, std::sync::Arc::new(C::boolean(false))));

        assert_eq!(shared_slots, copied_slots);
        assert_eq!(shared_slots.observations().count(), 2, "one shared child slot is visited once");
        assert_eq!(copied_slots.observations().count(), 3, "distinct handle slots remain distinct observations");
        assert_eq!(shared, independent);
        assert_eq!(shared.cmp(&independent), std::cmp::Ordering::Equal);
        assert_eq!(digest(&shared), digest(&independent));
    }

    #[test]
    fn hash_collisions_do_not_merge_distinct_conditions() {
        let left = C::with_test_hash(K::Boolean(false), 7);
        let equal = C::with_test_hash(K::Boolean(false), 7);
        let right = C::with_test_hash(K::Boolean(true), 7);

        assert_eq!(left, equal);
        assert_ne!(left, right);
        assert!(left < right);
        assert_eq!(left.partial_cmp(&equal), Some(std::cmp::Ordering::Equal));
    }
}
