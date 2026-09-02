use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use super::{
    CapacityResource, CompilationFactKey, FactCell, FactQueryError, FactRuntimeFailure,
    SynchronizationComponent,
};

const MAX_RETAINED_FACTS_PER_KIND: usize = 4_096;

#[derive(Debug)]
pub(crate) struct FactCellMap<K, V> {
    retention_limit: usize,
    state: Mutex<FactCellMapState<K, V>>,
}

#[derive(Debug)]
struct FactCellMapState<K, V> {
    cells: BTreeMap<K, FactCellMapEntry<V>>,
    next_access: u64,
}

#[derive(Debug)]
struct FactCellMapEntry<V> {
    cell: Arc<FactCell<V>>,
    last_access: u64,
}

impl<K, V> FactCellMap<K, V>
where
    K: Ord,
{
    pub(crate) fn new() -> Self {
        Self::with_retention_limit(MAX_RETAINED_FACTS_PER_KIND)
    }

    fn with_retention_limit(retention_limit: usize) -> Self {
        assert!(retention_limit > 0, "fact retention limit must be positive");

        Self {
            retention_limit,
            state: Mutex::new(FactCellMapState {
                cells: BTreeMap::new(),
                next_access: 0,
            }),
        }
    }

    pub(crate) fn cell(&self, key: K) -> Result<Arc<FactCell<V>>, FactQueryError> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| FactRuntimeFailure::SynchronizationPoisoned {
                component: SynchronizationComponent::CellMap,
                fact: None,
                task: None,
            })?;

        let access = state.next_access;

        state.next_access = state
            .next_access
            .checked_add(1)
            .ok_or(FactRuntimeFailure::CapacityExhausted {
                resource: CapacityResource::CellMapAccessIdentity,
                fact: None,
                task: None,
            })?;

        if let Some(entry) = state.cells.get_mut(&key) {
            entry.last_access = access;

            let cell = Arc::clone(&entry.cell);

            reclaim_entries(&mut state.cells, self.retention_limit, Some(access));

            return Ok(cell);
        }

        reclaim_entries(
            &mut state.cells,
            self.retention_limit.saturating_sub(1),
            None,
        );

        let cell = Arc::new(FactCell::new());

        state.cells.insert(
            key,
            FactCellMapEntry {
                cell: Arc::clone(&cell),
                last_access: access,
            },
        );

        Ok(cell)
    }

    pub(crate) fn updated(
        &self,
        reusable: &std::collections::BTreeSet<CompilationFactKey>,
        semantic_key: impl Fn(&K) -> CompilationFactKey,
    ) -> Self
    where
        K: Clone,
    {
        // Snapshot reuse has no fallible boundary. Poison here is a violated compiler invariant,
        // while ordinary query access reports the exact synchronization component.
        let state = self
            .state
            .lock()
            .unwrap_or_else(|_| panic!("fact cache map must remain available"));

        // The new map owns its keys while ready cells share their immutable publication storage.
        let mut cells = state
            .cells
            .iter()
            .filter(|(key, entry)| {
                let semantic_key = semantic_key(key);

                reusable.contains(&semantic_key) && entry.cell.is_ready_for(&semantic_key)
            })
            .map(|(key, entry)| (key.clone(), entry.last_access, Arc::clone(&entry.cell)))
            .collect::<Vec<_>>();

        cells.sort_by_key(|(_, access, _)| std::cmp::Reverse(*access));
        cells.truncate(self.retention_limit);

        Self {
            retention_limit: self.retention_limit,
            state: Mutex::new(FactCellMapState {
                next_access: cells
                    .iter()
                    .map(|(_, access, _)| *access)
                    .max()
                    .unwrap_or(0)
                    .saturating_add(1),
                cells: cells
                    .into_iter()
                    .map(|(key, last_access, cell)| (key, FactCellMapEntry { cell, last_access }))
                    .collect(),
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn keys(&self) -> Vec<K>
    where
        K: Clone,
    {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|_| panic!("fact cache map must remain available"));

        state.cells.keys().cloned().collect()
    }

    #[cfg(test)]
    pub(crate) fn is_published(&self, key: &K) -> Result<bool, FactQueryError> {
        let state = self
            .state
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        Ok(state
            .cells
            .get(key)
            .is_some_and(|entry| entry.cell.get().is_some()))
    }

    #[cfg(test)]
    pub(crate) fn shares_cell_with(&self, other: &Self, key: &K) -> bool {
        let left = self
            .state
            .lock()
            .unwrap_or_else(|_| panic!("fact cache map must remain available"))
            .cells
            .get(key)
            .map(|entry| Arc::clone(&entry.cell));

        let right = other
            .state
            .lock()
            .unwrap_or_else(|_| panic!("fact cache map must remain available"))
            .cells
            .get(key)
            .map(|entry| Arc::clone(&entry.cell));

        match (left, right) {
            (Some(left), Some(right)) => Arc::ptr_eq(&left, &right),
            _ => false,
        }
    }
}

fn reclaim_entries<K, V>(
    cells: &mut BTreeMap<K, FactCellMapEntry<V>>,
    retained: usize,
    protected_access: Option<u64>,
) where
    K: Ord,
{
    while cells.len() > retained {
        let oldest_access = cells
            .values()
            .filter(|entry| {
                let reclaimable = entry.cell.get().is_some()
                    || (Arc::strong_count(&entry.cell) == 1 && entry.cell.is_vacant());

                reclaimable && Some(entry.last_access) != protected_access
            })
            .min_by_key(|entry| entry.last_access)
            .map(|entry| entry.last_access);

        let Some(oldest_access) = oldest_access else {
            return;
        };

        cells.retain(|_, entry| entry.last_access != oldest_access);
    }
}

#[cfg(test)]
mod tests {
    use super::FactCellMap;
    use crate::fact::{
        CancellationToken, CapacityResource, CompilationFactKey, FactQueryError, FactRuntime,
        FactRuntimeFailure,
    };

    #[test]
    fn completed_entries_are_reclaimed_by_recent_use() {
        let cache = FactCellMap::with_retention_limit(2);
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();

        publish(&cache, &runtime, &cancellation, 1);
        publish(&cache, &runtime, &cancellation, 2);

        let _ = cache
            .cell(1)
            .unwrap_or_else(|error| panic!("cached fact must remain readable: {error:?}"));

        publish(&cache, &runtime, &cancellation, 3);

        assert_eq!(cache.keys(), [1, 3]);
    }

    #[test]
    fn completed_in_flight_bursts_converge_to_the_retention_limit() {
        let cache = FactCellMap::with_retention_limit(2);
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();

        let cells = [1, 2, 3].map(|key| {
            cache
                .cell(key)
                .unwrap_or_else(|error| panic!("fact cell must be available: {error:?}"))
        });

        for (key, cell) in [1, 2, 3].into_iter().zip(cells) {
            cell.get_or_compute(
                &runtime,
                CompilationFactKey::SyntaxTree,
                &cancellation,
                || Ok(key),
            )
            .unwrap_or_else(|error| panic!("fact must publish: {error:?}"));
        }

        let _ = cache
            .cell(3)
            .unwrap_or_else(|error| panic!("cached fact must remain readable: {error:?}"));

        assert_eq!(cache.keys().len(), 2);
        assert!(cache.keys().contains(&3));
    }

    #[test]
    fn abandoned_entries_are_reclaimed_to_the_retention_limit() {
        let cache = FactCellMap::<u32, u32>::with_retention_limit(2);
        let runtime = FactRuntime::default();
        let cancellation = CancellationToken::new();

        for key in 0..32 {
            let cell = cache
                .cell(key)
                .unwrap_or_else(|error| panic!("fact cell must be available: {error:?}"));

            let result = cell.get_or_compute(
                &runtime,
                CompilationFactKey::SyntaxTree,
                &cancellation,
                || Err(FactQueryError::InfrastructureFailure),
            );

            assert_eq!(result, Err(FactQueryError::InfrastructureFailure));
        }

        assert_eq!(cache.keys().len(), 2);
    }

    #[test]
    fn exhausted_access_identity_reports_the_bounded_resource() {
        let cache = FactCellMap::<u32, u32>::with_retention_limit(2);

        cache
            .state
            .lock()
            .unwrap_or_else(|_| panic!("test cell map should begin available"))
            .next_access = u64::MAX;

        let error = match cache.cell(1) {
            Ok(_) => panic!("an exhausted access identity must reject another cell access"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            FactQueryError::Runtime(error)
                if matches!(
                    error.cause(),
                    FactRuntimeFailure::CapacityExhausted {
                        resource: CapacityResource::CellMapAccessIdentity,
                        fact: None,
                        task: None,
                    }
                )
        ));
    }

    fn publish(
        cache: &FactCellMap<u32, u32>,
        runtime: &FactRuntime,
        cancellation: &CancellationToken,
        key: u32,
    ) {
        let cell = cache
            .cell(key)
            .unwrap_or_else(|error| panic!("fact cell must be available: {error:?}"));

        cell.get_or_compute(
            runtime,
            CompilationFactKey::SyntaxTree,
            cancellation,
            || Ok(key),
        )
        .unwrap_or_else(|error| panic!("fact must publish: {error:?}"));
    }
}
