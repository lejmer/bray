use std::sync::Arc;

/// Creates immutable shared string storage.
pub fn shared_str(text: impl Into<Arc<str>>) -> Arc<str> {
    text.into()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::shared_str;

    #[test]
    fn shared_strings_can_be_built_from_borrowed_and_owned_text() {
        let borrowed = shared_str("module main\n");
        let owned = shared_str(String::from("module main\n"));

        assert_eq!(borrowed.as_ref(), "module main\n");
        assert_eq!(owned.as_ref(), "module main\n");
    }

    #[test]
    fn shared_strings_can_reuse_existing_shared_storage() {
        let original = shared_str("module main\n");
        let shared = shared_str(Arc::clone(&original));

        assert!(Arc::ptr_eq(&original, &shared));
    }

    #[test]
    fn shared_strings_are_send_and_sync() {
        assert_send_sync::<Arc<str>>();
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
