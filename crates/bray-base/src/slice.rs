use std::sync::Arc;

/// Creates immutable shared slice storage from an ordered item sequence.
pub fn shared_slice<T>(items: impl IntoIterator<Item = T>) -> Arc<[T]> {
    Arc::<[T]>::from(items.into_iter().collect::<Vec<_>>())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::shared_slice;

    #[test]
    fn shared_slices_can_be_built_from_ordered_items() {
        let shared = shared_slice([1, 2, 3]);

        assert_eq!(shared.as_ref(), &[1, 2, 3]);
    }

    #[test]
    fn shared_slices_are_send_and_sync_when_items_are_send_and_sync() {
        assert_send_sync::<Arc<[u32]>>();
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
