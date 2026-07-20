use crate::{ConstantSymbolId, ConstantValueId, SymbolKey};

/// One exact compiler-known target fact and the value required from it.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetFactDependency {
    key: SymbolKey,
    fact: ConstantSymbolId,
    value: ConstantValueId,
}

impl TargetFactDependency {
    /// Creates one target-fact dependency.
    ///
    /// `key` must be the stable semantic key for `fact`.
    pub const fn new(key: SymbolKey, fact: ConstantSymbolId, value: ConstantValueId) -> Self {
        Self { key, fact, value }
    }

    /// Returns the target fact's stable semantic key.
    pub const fn key(&self) -> &SymbolKey {
        &self.key
    }

    /// Returns the compiler-known target fact declaration.
    pub const fn fact(&self) -> ConstantSymbolId {
        self.fact
    }

    /// Returns the required canonical value.
    pub const fn value(&self) -> ConstantValueId {
        self.value
    }
}
