use bray_bound_tree::BoundUnitKey;
use bray_symbols::{AnonymousCallableSymbolId, LocalScopeId};

/// The exact local identity and nested-unit boundary for one anonymous callable.
///
/// Construction and access remain inside the binder so only validated callable/scope/unit
/// relationships can reach parameter binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AnonymousCallableBoundary {
    callable: AnonymousCallableSymbolId,
    scope: LocalScopeId,
    unit: BoundUnitKey,
}

impl AnonymousCallableBoundary {
    pub(super) const fn new(
        callable: AnonymousCallableSymbolId,
        scope: LocalScopeId,
        unit: BoundUnitKey,
    ) -> Self {
        Self {
            callable,
            scope,
            unit,
        }
    }

    pub(crate) const fn callable(&self) -> AnonymousCallableSymbolId {
        self.callable
    }

    pub(crate) const fn scope(&self) -> LocalScopeId {
        self.scope
    }

    pub(crate) const fn unit(&self) -> &BoundUnitKey {
        &self.unit
    }
}
