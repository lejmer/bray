use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use super::{CompilationFactKey, FactCell, FactQueryError};

#[derive(Debug)]
pub(crate) struct FactCellMap<K, V> {
    cells: Mutex<BTreeMap<K, Arc<FactCell<V>>>>,
}

impl<K, V> FactCellMap<K, V>
where
    K: Ord,
{
    pub(crate) const fn new() -> Self {
        Self {
            cells: Mutex::new(BTreeMap::new()),
        }
    }

    pub(crate) fn cell(&self, key: K) -> Result<Arc<FactCell<V>>, FactQueryError> {
        let mut cells = self
            .cells
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        Ok(Arc::clone(
            cells
                .entry(key)
                .or_insert_with(|| Arc::new(FactCell::new())),
        ))
    }

    pub(crate) fn updated(
        &self,
        reusable: &std::collections::BTreeSet<CompilationFactKey>,
        fact_key: impl Fn(&K) -> CompilationFactKey,
    ) -> Self
    where
        K: Clone,
    {
        let cells = self
            .cells
            .lock()
            .unwrap_or_else(|_| panic!("fact cache map must remain available"));

        // The new map owns its keys while ready cells share their immutable publication storage.
        let cells = cells
            .iter()
            .filter(|(key, cell)| {
                let fact_key = fact_key(key);

                reusable.contains(&fact_key) && cell.is_ready_for(&fact_key)
            })
            .map(|(key, cell)| (key.clone(), Arc::clone(cell)))
            .collect();

        Self {
            cells: Mutex::new(cells),
        }
    }

    pub(crate) fn keys(&self) -> Vec<K>
    where
        K: Clone,
    {
        let cells = self
            .cells
            .lock()
            .unwrap_or_else(|_| panic!("fact cache map must remain available"));

        // Snapshot invalidation owns stable cache keys after releasing the map lock.
        cells.keys().cloned().collect()
    }

    #[cfg(test)]
    pub(crate) fn is_published(&self, key: &K) -> Result<bool, FactQueryError> {
        let cells = self
            .cells
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        Ok(cells.get(key).is_some_and(|cell| cell.get().is_some()))
    }

    #[cfg(test)]
    pub(crate) fn shares_cell_with(&self, other: &Self, key: &K) -> bool {
        let left = self
            .cells
            .lock()
            .unwrap_or_else(|_| panic!("fact cache map must remain available"))
            .get(key)
            .cloned();
        let right = other
            .cells
            .lock()
            .unwrap_or_else(|_| panic!("fact cache map must remain available"))
            .get(key)
            .cloned();

        match (left, right) {
            (Some(left), Some(right)) => Arc::ptr_eq(&left, &right),
            _ => false,
        }
    }
}
