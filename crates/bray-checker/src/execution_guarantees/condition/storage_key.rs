use std::hash::{Hash, Hasher};
use hashbrown::Equivalent;

use super::ExecutionCondition;

/// A pure query can reuse the same immutable storage without comparing its descendants.
/// Keeping the condition alive prevents allocation reuse from producing false cache hits.
#[derive(Clone)]
pub(super) struct StorageKey(ExecutionCondition);

impl From<&ExecutionCondition> for StorageKey {
    fn from(value: &ExecutionCondition) -> Self {
        // Retain the original child allocations while their addresses are indexed.
        Self(value.clone())
    }
}

/// Lookup borrows the source. Only an inserted key needs to retain its child allocations.
pub(super) struct StorageLookup<'a>(pub(super) &'a ExecutionCondition);

impl Hash for StorageLookup<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.hash_parts(state, &mut |child, _, state| std::ptr::hash(child, state));
    }
}

impl Equivalent<StorageKey> for StorageLookup<'_> {
    fn equivalent(&self, other: &StorageKey) -> bool {
        self.0.eq_parts(&other.0, &mut |left, right| std::ptr::eq(left, right))
    }
}

impl Hash for StorageKey {
    fn hash<H: Hasher>(&self, state: &mut H) { StorageLookup(&self.0).hash(state); }
}

impl Eq for StorageKey {}

impl PartialEq for StorageKey {
    fn eq(&self, other: &Self) -> bool { StorageLookup(&self.0).equivalent(other) }
}

#[cfg(test)]
mod tests {
    use hashbrown::HashMap;
    use std::sync::Arc;

    use bray_bound_tree::BoundOperator::{LogicalAnd, LogicalOr};

    use super::{ExecutionCondition as C, StorageKey, StorageLookup};

    #[test]
    fn shared_ordering_preserves_derived_field_precedence() {
        use crate::execution_guarantees::ExecutionObservationKind::{Owned, Reference};

        let mut shared = C::Boolean(false);

        for _ in 0..7 {
            shared = C::Operation(LogicalOr, vec![shared.clone(), shared].into());
        }

        let mut terms = vec![C::Unknown, C::Result, C::Boolean(false), C::Boolean(true), shared.clone()];

        for value in [C::Boolean(false), C::Boolean(true), shared] {
            let value = Arc::new(value);

            for other in [Arc::new(C::Boolean(false)), Arc::new(C::Boolean(true)), Arc::clone(&value)] {
                terms.extend([
                    C::ValueAlias(Arc::clone(&value), Arc::clone(&other)),
                    C::Observation { owner: Arc::clone(&value), contents: Arc::clone(&other), kind: Owned },
                    C::Observation { owner: Arc::clone(&other), contents: Arc::clone(&value), kind: Reference },
                    C::Entry { condition: Arc::clone(&value), captured: false },
                    C::Entry { condition: Arc::clone(&other), captured: true },
                    C::Constructed(Arc::clone(&value), vec![(bray_bound_tree::StorageProjection::NullableValue, (*other).clone())].into()),
                    C::Operation(LogicalOr, vec![(*value).clone(), (*other).clone()].into()),
                    C::Operation(LogicalAnd, vec![(*other).clone(), (*value).clone(), C::Unknown].into()),
                ]);
            }
        }

        for left in &terms {
            for right in &terms {
                assert_eq!(Some(left.cmp(right)), left.partial_cmp(right));
                assert_eq!(left.eq_parts(right, &mut |left, right| left == right), left == right);
            }
        }
    }

    #[test]
    fn retained_storage_keys_reuse_clones_without_merging_reallocated_or_changed_terms() {
        let operands: super::super::ExecutionOperands = vec![C::Boolean(true), C::Unknown].into();
        let original = C::Operation(LogicalOr, operands.clone());
        let mut cache = HashMap::new();

        cache.insert(StorageKey(original.clone()), 7);
        drop(original);
        assert_eq!(cache.get(&StorageLookup(&C::Operation(LogicalOr, operands.clone()))), Some(&7));
        assert_eq!(cache.get(&StorageLookup(&C::Operation(LogicalAnd, operands.clone()))), None);
        assert_eq!(cache.get(&StorageLookup(&C::Operation(LogicalOr, operands.to_vec().into()))), None);

        let condition = Arc::new(C::Unknown);

        cache.insert(StorageKey(C::Entry { condition: Arc::clone(&condition), captured: true }), 8);
        assert_eq!(cache.get(&StorageLookup(&C::Entry { condition: Arc::clone(&condition), captured: true })), Some(&8));
        assert_eq!(cache.get(&StorageLookup(&C::Entry { condition, captured: false })), None);
    }
}
