use std::sync::{Arc, LazyLock, Mutex, Weak};

use super::model::ConditionNode;

const SHARDS: usize = 64;

/// Live condition nodes by structural hash.
///
/// Children are canonical before their parents, so a parent matches an existing
/// node by its own fields and its children's identities. Equal trees then share
/// one allocation, and most equality checks end at pointer identity. Equality
/// and order stay structural, so a node built without interning still compares
/// correctly.
struct Interner {
    shards: [Mutex<hashbrown::HashTable<(u64, Weak<ConditionNode>)>>; SHARDS],
}

static INTERNER: LazyLock<Interner> = LazyLock::new(|| Interner {
    shards: std::array::from_fn(|_| Mutex::new(hashbrown::HashTable::new())),
});

// The table indexes by low bits and tags by high bits, so shards use the middle bits.
fn shard(hash: u64) -> &'static Mutex<hashbrown::HashTable<(u64, Weak<ConditionNode>)>> {
    &INTERNER.shards[(hash >> 32) as usize % SHARDS]
}

/// Returns the live node equal to `node`, or publishes `node` as that node.
pub(super) fn intern(mut node: ConditionNode) -> Arc<ConditionNode> {
    let hash = node.hash();
    let mut found = None;

    // An upgraded candidate can be the last owner. It must drop after the
    // shard lock is released, because dropping a node removes its entry.
    let mut candidates = Vec::new();

    let mut table = shard(hash).lock().expect("condition interner lock is not poisoned");

    table.find(hash, |(entry_hash, entry)| {
        if *entry_hash != hash || found.is_some() {
            return false;
        }

        let Some(existing) = entry.upgrade() else { return false; };

        if existing.same_node(&node) {
            found = Some(existing);

            return true;
        }

        candidates.push(existing);

        false
    });

    let (interned, duplicate) = match found {
        Some(existing) => (existing, Some(node)),
        None => {
            node.mark_interned();

            let interned = Arc::new(node);

            table.insert_unique(hash, (hash, Arc::downgrade(&interned)), |(hash, _)| *hash);

            (interned, None)
        }
    };

    // A duplicate's children can be the last owners of interned nodes.
    drop(table);
    drop(candidates);
    drop(duplicate);

    interned
}

/// Removes a dropped interned node's entry.
pub(super) fn release(node: &ConditionNode) {
    let hash = node.hash();
    let pointer = std::ptr::from_ref(node);
    let mut table = shard(hash).lock().expect("condition interner lock is not poisoned");

    // The removed weak handle owns no node, so it can drop while the lock is held.
    if let Ok(entry) = table.find_entry(hash, |(_, entry)| std::ptr::eq(entry.as_ptr(), pointer)) {
        entry.remove();
    }
}

#[cfg(test)]
mod tests {
    use super::super::{ExecutionCondition as C, ExecutionConditionKind as K};
    use bray_bound_tree::BoundOperator::LogicalAnd;

    // No other test builds this shape, so concurrent tests cannot share its entry.
    fn condition() -> C {
        let operands = [true, false, true, true, false, false, true].map(C::boolean);

        C::new(K::Operation(LogicalAnd, operands.to_vec().into()))
    }

    #[test]
    fn equal_conditions_share_one_node_until_the_last_handle_drops() {
        let first = condition();
        let second = condition();

        assert!(std::ptr::eq(first.kind(), second.kind()));

        let hash = first.cached_hash();

        drop(first);

        let table = super::shard(hash).lock().expect("condition interner lock is not poisoned");

        assert!(table.find(hash, |(entry_hash, _)| *entry_hash == hash).is_some());

        drop(table);
        drop(second);

        let table = super::shard(hash).lock().expect("condition interner lock is not poisoned");

        assert!(table.find(hash, |(entry_hash, _)| *entry_hash == hash).is_none());
    }
}
