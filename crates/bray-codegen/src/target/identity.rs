use std::sync::Arc;

use bray_base::NonEmptySharedStr;

/// Stable compiler-facing identity of one validated code generation target.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetIdentity(NonEmptySharedStr);

impl TargetIdentity {
    /// Creates a target identity unless its canonical representation is empty.
    pub fn try_new(value: impl Into<Arc<str>>) -> Option<Self> {
        NonEmptySharedStr::try_new(value).map(Self)
    }

    /// Returns the canonical target identity.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl AsRef<str> for TargetIdentity {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
