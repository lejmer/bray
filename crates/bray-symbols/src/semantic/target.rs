use std::sync::Arc;

use crate::{ConstantSymbolId, ConstantValueId, SymbolKey};

/// One exact compiler-known target property and the value required from it.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetPropertyDependency {
    key: SymbolKey,
    property: ConstantSymbolId,
    value: ConstantValueId,
}

impl TargetPropertyDependency {
    /// Creates one target-property dependency.
    ///
    /// `key` must be the stable semantic key for `property`.
    pub const fn new(key: SymbolKey, property: ConstantSymbolId, value: ConstantValueId) -> Self {
        Self {
            key,
            property,
            value,
        }
    }

    /// Returns the target property's stable semantic key.
    pub const fn key(&self) -> &SymbolKey {
        &self.key
    }

    /// Returns the compiler-known target property declaration.
    pub const fn property(&self) -> ConstantSymbolId {
        self.property
    }

    /// Returns the required canonical value.
    pub const fn value(&self) -> ConstantValueId {
        self.value
    }
}

/// The selected product and target result for one source module contribution.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ModuleContributionGate {
    enabled: bool,
    dependencies: Arc<[TargetPropertyDependency]>,
}

impl ModuleContributionGate {
    /// Creates a contribution gate from its result and observed target properties.
    pub fn new(
        enabled: bool,
        dependencies: impl IntoIterator<Item = TargetPropertyDependency>,
    ) -> Self {
        let mut dependencies = dependencies.into_iter().collect::<Vec<_>>();

        dependencies.sort_unstable_by(|left, right| left.key().cmp(right.key()));
        dependencies.dedup();

        Self {
            enabled,
            dependencies: dependencies.into(),
        }
    }

    /// Returns whether the module contribution participates in the selected product.
    pub const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the exact target properties observed while evaluating the gate.
    pub fn dependencies(&self) -> &[TargetPropertyDependency] {
        &self.dependencies
    }
}
