use std::sync::Arc;

use bray_base::shared_str;

/// Stable compiler-facing identity of one validated code generation target.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetIdentity(Arc<str>);

impl TargetIdentity {
    /// Creates a target identity unless its canonical representation is empty.
    pub fn try_new(value: impl Into<Arc<str>>) -> Option<Self> {
        let value = shared_str(value);

        if value.is_empty() {
            return None;
        }

        Some(Self(value))
    }

    /// Returns the canonical target identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for TargetIdentity {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
