use std::sync::Arc;

/// Immutable shared text containing at least one byte.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NonEmptySharedStr(Arc<str>);

impl NonEmptySharedStr {
    /// Creates shared text unless its canonical representation is empty.
    pub fn try_new(text: impl Into<Arc<str>>) -> Option<Self> {
        let text = shared_str(text);

        if text.is_empty() {
            return None;
        }

        Some(Self(text))
    }

    /// Returns the validated non-empty text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for NonEmptySharedStr {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Creates an immutable shared string.
pub fn shared_str(text: impl Into<Arc<str>>) -> Arc<str> {
    text.into()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{NonEmptySharedStr, shared_str};

    #[test]
    fn non_empty_shared_strings_validate_and_reuse_storage() {
        assert_eq!(NonEmptySharedStr::try_new(""), None);

        let text: Arc<str> = Arc::from("item");

        let Some(non_empty) = NonEmptySharedStr::try_new(Arc::clone(&text)) else {
            panic!("non-empty shared text must be valid");
        };

        assert_eq!(non_empty.as_str(), "item");
        assert!(Arc::ptr_eq(&non_empty.0, &text));
    }

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
        assert_send_sync::<NonEmptySharedStr>();
    }

    fn assert_send_sync<T: Send + Sync>() {}
}
