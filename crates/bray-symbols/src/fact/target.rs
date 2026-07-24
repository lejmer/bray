use std::sync::Arc;

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

/// The selected-target result for one source module contribution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModuleTargetGate {
    enabled: bool,
    dependencies: Arc<[TargetFactDependency]>,
}

impl ModuleTargetGate {
    /// Creates a module target gate from its result and observed target facts.
    pub fn new(
        enabled: bool,
        dependencies: impl IntoIterator<Item = TargetFactDependency>,
    ) -> Self {
        let mut dependencies = dependencies.into_iter().collect::<Vec<_>>();

        dependencies.sort_unstable_by(|left, right| left.key().cmp(right.key()));
        dependencies.dedup();

        Self {
            enabled,
            dependencies: dependencies.into(),
        }
    }

    /// Returns whether the module contribution participates for the selected target.
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the exact target facts observed while evaluating the gate.
    pub fn dependencies(&self) -> &[TargetFactDependency] {
        &self.dependencies
    }
}
