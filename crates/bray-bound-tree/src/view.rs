use crate::{
    AnyBoundNodeId, BoundBlock, BoundBlockId, BoundCallableBody, BoundCallableBodyId,
    BoundCallableBodyKind, BoundExpression, BoundExpressionId, BoundPattern, BoundPatternId,
    BoundTree, BoundTreeBuilder, BoundUnitId, BoundUnitKey, BoundUnitKind,
};

/// A read-only view of available bound structure.
///
/// Checker services can inspect available nodes without receiving mutable access to the tree.
#[derive(Clone, Copy, Debug)]
pub struct BoundUnitView<'unit> {
    key: &'unit BoundUnitKey,
    storage: BoundUnitStorageView<'unit>,
}

impl<'unit> BoundUnitView<'unit> {
    pub(crate) const fn building(
        builder: &'unit BoundTreeBuilder,
        key: &'unit BoundUnitKey,
    ) -> Self {
        Self {
            key,
            storage: BoundUnitStorageView::Building(builder),
        }
    }

    pub(crate) const fn published(tree: &'unit BoundTree, key: &'unit BoundUnitKey) -> Self {
        Self {
            key,
            storage: BoundUnitStorageView::Published(tree),
        }
    }

    /// Returns the compilation-local identity of the bound unit.
    pub const fn unit(self) -> BoundUnitId {
        match self.storage {
            BoundUnitStorageView::Building(builder) => builder.unit(),
            BoundUnitStorageView::Published(tree) => tree.unit(),
        }
    }

    /// Returns the stable semantic key of the bound unit.
    pub const fn key(self) -> &'unit BoundUnitKey {
        self.key
    }

    /// Returns the exact semantic category of the bound unit.
    pub fn kind(self) -> BoundUnitKind {
        self.key.kind()
    }

    /// Returns one committed expression through checked typed access.
    pub fn expression(self, id: BoundExpressionId) -> Option<&'unit BoundExpression> {
        match self.storage {
            BoundUnitStorageView::Building(builder) => builder.expression(id),
            BoundUnitStorageView::Published(tree) => tree.expression(id),
        }
    }

    /// Returns the direct expression parent of one committed expression, when present.
    pub fn expression_parent(self, id: BoundExpressionId) -> Option<BoundExpressionId> {
        match self.storage {
            BoundUnitStorageView::Building(builder) => builder.expression_parent(id),
            BoundUnitStorageView::Published(tree) => tree.expression_parent(id),
        }
    }

    /// Returns a member expression's runtime receiver, excluding namespace and type qualifiers.
    pub fn value_receiver(self, member: BoundExpressionId) -> Option<BoundExpressionId> {
        let receiver = match self.expression(member)? {
            BoundExpression::MemberAccess(member) => member.receiver(),
            BoundExpression::TraitQualifiedMember(member) => member.receiver(),
            _ => return None,
        };

        let qualifier = matches!(self.expression(receiver),
            Some(BoundExpression::Name(name)) if name.target().is_compile_time_qualifier());

        (!qualifier).then_some(receiver)
    }

    /// Returns one committed pattern through checked typed access.
    pub fn pattern(self, id: BoundPatternId) -> Option<&'unit BoundPattern> {
        match self.storage {
            BoundUnitStorageView::Building(builder) => builder.pattern(id),
            BoundUnitStorageView::Published(tree) => tree.pattern(id),
        }
    }

    /// Returns one committed block through checked typed access.
    pub fn block(self, id: BoundBlockId) -> Option<&'unit BoundBlock> {
        match self.storage {
            BoundUnitStorageView::Building(builder) => builder.block(id),
            BoundUnitStorageView::Published(tree) => tree.block(id),
        }
    }

    /// Returns one committed callable body through checked typed access.
    pub fn callable_body(self, id: BoundCallableBodyId) -> Option<&'unit BoundCallableBody> {
        match self.storage {
            BoundUnitStorageView::Building(builder) => builder.callable_body(id),
            BoundUnitStorageView::Published(tree) => tree.callable_body(id),
        }
    }

    /// Returns whether a substantial node retains semantic recovery state.
    ///
    /// Returns `None` for a foreign or missing node ID.
    pub fn node_is_recovered(self, id: AnyBoundNodeId) -> Option<bool> {
        match id {
            AnyBoundNodeId::Expression(id) => {
                self.expression(id).map(BoundExpression::is_recovered)
            }
            AnyBoundNodeId::Pattern(id) => self.pattern(id).map(BoundPattern::is_recovered),
            AnyBoundNodeId::Block(id) => self.block(id).map(BoundBlock::is_recovered),
            AnyBoundNodeId::CallableBody(id) => self
                .callable_body(id)
                .map(|body| matches!(body.kind(), BoundCallableBodyKind::Error(_))),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum BoundUnitStorageView<'unit> {
    Building(&'unit BoundTreeBuilder),
    Published(&'unit BoundTree),
}

#[cfg(test)]
mod tests {
    use bray_declarations::DeclarationId;
    use bray_symbols::{ModulePathKey, PackageIdentity, SymbolKey, SymbolKind, SymbolRootKey};

    use crate::test_support::{error_expression, source_anchor};
    use crate::{BoundTreeBuilder, BoundUnitId, BoundUnitKey, BoundUnitKind};

    #[test]
    fn views_read_committed_builder_nodes_without_mutation_access() {
        let key = callable_body_key();

        let mut builder = BoundTreeBuilder::new(BoundUnitId::new(7));

        let expression = error_expression();

        let Ok(expression) = builder.push_expression(expression) else {
            panic!("one test expression must fit in the empty arena");
        };

        let view = builder.view(&key);

        assert_eq!(view.unit(), BoundUnitId::new(7));
        assert!(std::ptr::eq(view.key(), &key));
        assert_eq!(view.kind(), BoundUnitKind::CallableBody);
        assert!(view.expression(expression).is_some());
    }

    #[test]
    fn views_use_the_same_checked_access_after_publication() {
        let key = callable_body_key();

        let mut builder = BoundTreeBuilder::new(BoundUnitId::new(7));

        let Ok(expression) = builder.push_expression(error_expression()) else {
            panic!("one test expression must fit in the empty arena");
        };

        let tree = builder.finish();
        let view = tree.view(&key);

        assert!(view.expression(expression).is_some());

        assert!(
            view.node_is_recovered(expression.into())
                .is_some_and(|value| value)
        );
    }

    #[test]
    fn views_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<crate::BoundUnitView<'static>>();
    }

    fn callable_body_key() -> BoundUnitKey {
        let Some(package) = PackageIdentity::try_new("example.package") else {
            panic!("test package identity is non-empty");
        };

        let Some(path) = ModulePathKey::try_new(["example"]) else {
            panic!("test module path is non-empty");
        };

        let module = SymbolKey::module(SymbolRootKey::Package(package), path);

        let Some(owner) =
            SymbolKey::source_declaration(module, SymbolKind::Function, DeclarationId::new(0))
        else {
            panic!("functions can own source declarations");
        };

        let Some(key) = BoundUnitKey::callable_body(owner, source_anchor()) else {
            panic!("functions can own callable bodies");
        };

        key
    }
}
