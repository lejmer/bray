use std::sync::Arc;

/// Creates an immutable shared slice from an ordered item sequence.
pub fn shared_slice<T>(items: impl IntoIterator<Item = T>) -> Arc<[T]> {
    Arc::<[T]>::from(items.into_iter().collect::<Vec<_>>())
}

/// Creates a deterministically sorted, duplicate-free immutable shared slice.
pub fn sorted_unique_shared_slice<T>(items: impl IntoIterator<Item = T>) -> Arc<[T]>
where
    T: Ord,
{
    let mut items: Vec<_> = items.into_iter().collect();

    items.sort_unstable();
    items.dedup();

    shared_slice(items)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{shared_slice, sorted_unique_shared_slice};

    #[test]
    fn shared_slices_can_be_built_from_ordered_items() {
        let shared = shared_slice([1, 2, 3]);

        assert_eq!(shared.as_ref(), &[1, 2, 3]);
    }

    #[test]
    fn shared_slices_are_send_and_sync_when_items_are_send_and_sync() {
        assert_send_sync::<Arc<[u32]>>();
    }

    #[test]
    fn sorted_unique_shared_slices_have_deterministic_set_order() {
        let shared = sorted_unique_shared_slice([3, 1, 2, 1, 3]);

        assert_eq!(shared.as_ref(), &[1, 2, 3]);
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
