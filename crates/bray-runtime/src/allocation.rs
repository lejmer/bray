use std::collections::{HashMap, TryReserveError};
use std::hash::Hash;

/// Secures sequential entries before publishing the obligations that need them.
pub(crate) fn reserve_vec_entries<T>(
    entries: &mut Vec<T>,
    additional: usize,
) -> Result<(), TryReserveError> {
    if additional <= entries.capacity() - entries.len() {
        return Ok(());
    }

    #[cfg(test)]
    if crate::test_support::allocation_should_fail() {
        return Err(Vec::<u8>::new().try_reserve(usize::MAX).unwrap_err());
    }

    entries.try_reserve(additional)
}

/// Secures table entries before publishing the runtime obligations that need them.
pub(crate) fn reserve_map_entries<K: Eq + Hash, V>(
    entries: &mut HashMap<K, V>,
    additional: usize,
) -> Result<(), TryReserveError> {
    if additional <= entries.capacity() - entries.len() {
        return Ok(());
    }

    #[cfg(test)]
    if crate::test_support::allocation_should_fail() {
        return Err(Vec::<u8>::new().try_reserve(usize::MAX).unwrap_err());
    }

    entries.try_reserve(additional)
}

#[cfg(test)]
mod tests {
    use super::reserve_map_entries;
    use crate::test_support::with_allocation_failure;
    use std::collections::HashMap;

    #[test]
    fn failed_growth_preserves_entries_and_reserved_slots_support_reuse() {
        let mut entries = HashMap::new();
        assert!(with_allocation_failure(|| reserve_map_entries(&mut entries, 1)).is_err());
        assert!(entries.is_empty());

        reserve_map_entries(&mut entries, 1).unwrap();
        let capacity = entries.capacity();

        with_allocation_failure(|| {
            for key in 0..capacity {
                reserve_map_entries(&mut entries, 1).unwrap();
                entries.insert(key, key);
            }

            assert!(reserve_map_entries(&mut entries, 1).is_err());
            assert_eq!(entries.len(), capacity);

            for key in 0..capacity {
                assert_eq!(entries.remove(&key), Some(key));
                reserve_map_entries(&mut entries, 1).unwrap();
                entries.insert(key, key + 1);
            }

            assert_eq!(entries.capacity(), capacity);
        });
    }
}
