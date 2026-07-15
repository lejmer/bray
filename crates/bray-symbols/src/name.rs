use std::borrow::Borrow;
use std::sync::Arc;

use bray_base::NonEmptySharedStr;

/// A validated ordinary semantic symbol name.
///
/// Missing or recovered names remain absent instead of being represented by an empty name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SymbolName(NonEmptySharedStr);

impl SymbolName {
    /// Creates a name unless its canonical representation is empty.
    pub fn try_new(name: impl Into<Arc<str>>) -> Option<Self> {
        NonEmptySharedStr::try_new(name).map(Self)
    }

    /// Returns the canonical name text.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl AsRef<str> for SymbolName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Borrow<str> for SymbolName {
    fn borrow(&self) -> &str {
        self.as_str()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::SymbolName;

    #[test]
    fn names_reject_absent_text_and_reuse_shared_storage() {
        assert_eq!(SymbolName::try_new(""), None);

        let text: Arc<str> = Arc::from("item");

        let Some(name) = SymbolName::try_new(Arc::clone(&text)) else {
            panic!("non-empty symbol name must be valid");
        };

        assert_eq!(name.as_str(), "item");
    }
}
