use std::collections::BTreeMap;
use std::sync::Mutex;

struct Computation<T> {
    completed: Option<(usize, T)>,
    exhausted: BTreeMap<usize, T>,
}

impl<T> Default for Computation<T> {
    fn default() -> Self { Self { completed: None, exhausted: BTreeMap::new() } }
}

impl<T: Clone> Computation<T> {
    fn reuse(&self, budget: &mut usize) -> Option<T> {
        if let Some((cost, value)) = &self.completed && *budget >= *cost {
            *budget -= *cost;

            return Some(value.clone());
        }

        let value = self.exhausted.get(budget)?.clone();
        *budget = 0;

        Some(value)
    }
}

/// Pure execution queries retain their original semantic work cost on a cache hit.
pub(super) struct ExecutionMemo<K, T> {
    values: Mutex<BTreeMap<K, Computation<T>>>,
}

impl<K, T> Default for ExecutionMemo<K, T> {
    fn default() -> Self { Self { values: Mutex::new(BTreeMap::new()) } }
}

pub(super) fn memoized<K: Clone + Ord, T: Clone>(
    memo: Option<&ExecutionMemo<K, T>>,
    key: &K,
    budget: &mut usize,
    compute: impl FnOnce(&mut usize) -> T,
) -> T {
    let Some(memo) = memo else { return compute(budget); };
    let cached = memo.values.lock().expect("execution query memo lock is not poisoned")
        .get(key).and_then(|cached| cached.reuse(budget));

    if let Some(value) = cached { return value; }

    let initial = *budget;
    let value = compute(budget);
    assert!(*budget <= initial, "execution queries cannot increase their work budget");
    let mut values = memo.values.lock().expect("execution query memo lock is not poisoned");

    // Entries retain immutable shallow condition keys and reusable results, without
    // holding the memo lock during recursive inference or copying operand trees.
    let entry = values.entry(key.clone()).or_default();

    if *budget > 0 {
        entry.completed = Some((initial - *budget, value.clone()));
    } else {
        // Each exhausted limit has its own partial result. Neither another limit
        // nor a completed query can erase the result at this exact work budget.
        entry.exhausted.insert(initial, value.clone());
    }

    value
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::{ExecutionMemo, memoized};

    #[test]
    fn exhausted_limits_and_completed_queries_do_not_evict_each_other() {
        let memo = ExecutionMemo::default();
        let attempts = Cell::new(0);

        for initial in [1, 2, 1, 4, 2, 4, 1, 3] {
            let mut budget = initial;
            let result = memoized(Some(&memo), &0, &mut budget, |budget| {
                attempts.set(attempts.get() + 1);

                if *budget < 3 {
                    *budget = 0;

                    return false;
                }

                *budget -= 3;

                true
            });

            assert_eq!(result, initial >= 3);
            assert_eq!(budget, initial.saturating_sub(3));
        }

        assert_eq!(attempts.get(), 3, "each exhausted limit and the completed query need computation once");
    }
}
