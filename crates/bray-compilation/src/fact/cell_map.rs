use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use super::{FactCell, FactQueryError};

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

    #[cfg(test)]
    pub(crate) fn is_published(&self, key: &K) -> Result<bool, FactQueryError> {
        let cells = self
            .cells
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        Ok(cells.get(key).is_some_and(|cell| cell.get().is_some()))
    }
}
