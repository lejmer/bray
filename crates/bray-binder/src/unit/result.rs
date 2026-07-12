use bray_bound_tree::{BoundTree, BoundUnitKey, BoundUnitView};
use bray_symbols::{LocalScopeId, LocalSymbolSnapshot};

/// Frozen binder-owned construction state awaiting category-specific checked-unit assembly.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct BoundUnitConstructionResult {
    key: BoundUnitKey,
    tree: BoundTree,
    local_symbols: LocalSymbolSnapshot,
    root_scope: LocalScopeId,
}

impl BoundUnitConstructionResult {
    pub(super) const fn new(
        key: BoundUnitKey,
        tree: BoundTree,
        local_symbols: LocalSymbolSnapshot,
        root_scope: LocalScopeId,
    ) -> Self {
        Self {
            key,
            tree,
            local_symbols,
            root_scope,
        }
    }

    pub(crate) const fn key(&self) -> &BoundUnitKey {
        &self.key
    }

    pub(crate) const fn tree(&self) -> &BoundTree {
        &self.tree
    }

    pub(crate) const fn local_symbols(&self) -> &LocalSymbolSnapshot {
        &self.local_symbols
    }

    pub(crate) const fn root_scope(&self) -> LocalScopeId {
        self.root_scope
    }

    pub(crate) fn view(&self) -> BoundUnitView<'_> {
        self.tree.view(&self.key)
    }

    pub(crate) fn into_parts(self) -> (BoundUnitKey, BoundTree, LocalSymbolSnapshot, LocalScopeId) {
        (self.key, self.tree, self.local_symbols, self.root_scope)
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundUnitId;
    use bray_symbols::{LocalScopeBoundary, LocalSymbolRegionId, SymbolOrdinal};

    use crate::unit::test_support::{builder, finish, fixture, push_scope, representative_binding};

    #[test]
    fn construction_is_deterministic_and_freezes_both_stores() {
        let fixture = fixture();

        let first = representative_unit(&fixture, LocalSymbolRegionId::new(4));
        let second = representative_unit(&fixture, LocalSymbolRegionId::new(4));

        assert_eq!(first, second);
        assert_eq!(first.tree().unit(), BoundUnitId::new(7));
        assert_eq!(first.view().unit(), BoundUnitId::new(7));
        assert_eq!(first.key(), &fixture.key);

        let [binding] = first.local_symbols().bindings() else {
            panic!("representative unit must contain one binding");
        };

        assert_eq!(binding.key().anchors(), &[fixture.first, fixture.second]);
        assert_eq!(binding.key().region(), first.local_symbols().key());

        assert_eq!(
            first
                .local_symbols()
                .scope(first.root_scope())
                .map(|scope| scope.boundary()),
            Some(LocalScopeBoundary::Root)
        );
    }

    fn representative_unit(
        fixture: &crate::unit::test_support::Fixture,
        region: LocalSymbolRegionId,
    ) -> super::BoundUnitConstructionResult {
        let mut builder = builder(fixture, region);

        let root = builder.root_scope();

        let block = push_scope(
            &mut builder,
            root,
            LocalScopeBoundary::Block,
            fixture.first,
            2,
        );

        let binding = representative_binding(
            &mut builder,
            block,
            [fixture.first, fixture.second],
            SymbolOrdinal::new(0),
            false,
        );

        assert_eq!(builder.activate_local(block, binding), Ok(()));

        finish(builder)
    }
}
