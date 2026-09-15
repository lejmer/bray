use std::sync::Arc;

use bray_symbols::{AnonymousCallableSymbolId, CallableExecution, LocalSymbolSnapshot};

use crate::{
    AnyBoundNodeId, BoundBlockId, BoundCallableBodyId, BoundExpressionId, BoundTree, BoundUnitId,
    BoundUnitIdentity, BoundUnitKey, BoundUnitKind, BoundUnitView,
};

/// One immutable bound semantic unit and its exact root.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct BoundUnit {
    key: BoundUnitKey,
    tree: BoundTree,
    local_symbols: LocalSymbolSnapshot,
    nested_units: Arc<[BoundUnitKey]>,
    root: BoundUnitRoot,
}

impl BoundUnit {
    /// Publishes one completed bound unit, including source-error recovery nodes.
    ///
    /// The producer must supply the matching local snapshot and directly nested keys in
    /// unique source order. The root must belong to this tree and match the unit category.
    pub fn new(
        key: BoundUnitKey,
        tree: BoundTree,
        local_symbols: LocalSymbolSnapshot,
        nested_units: impl IntoIterator<Item = BoundUnitKey>,
        root: BoundUnitRoot,
    ) -> Self {
        assert_eq!(
            local_symbols.region().raw(),
            tree.unit().raw(),
            "bound unit and local snapshot must share identity for {key:?}"
        );

        assert_root(key.kind(), &tree, &local_symbols, root);

        Self {
            key,
            tree,
            local_symbols,
            nested_units: nested_units.into_iter().collect(),
            root,
        }
    }

    /// Returns this unit's stable semantic key.
    pub const fn key(&self) -> &BoundUnitKey {
        &self.key
    }

    /// Returns this unit's compilation-local identity.
    pub const fn unit(&self) -> BoundUnitId {
        self.tree.unit()
    }

    /// Returns the coherent stable and compilation-local identity pair for this unit.
    pub const fn identity(&self) -> BoundUnitIdentity<'_> {
        BoundUnitIdentity::new(&self.key, self.tree.unit())
    }

    /// Returns the immutable source-shaped bound tree.
    pub const fn tree(&self) -> &BoundTree {
        &self.tree
    }

    /// Returns a read-only view over the bound tree and unit key.
    pub fn view(&self) -> BoundUnitView<'_> {
        self.tree.view(&self.key)
    }

    /// Returns the immutable local-symbol and lexical-scope snapshot.
    pub const fn local_symbols(&self) -> &LocalSymbolSnapshot {
        &self.local_symbols
    }

    /// Returns directly nested anonymous callable keys in canonical source order.
    pub fn nested_units(&self) -> &[BoundUnitKey] {
        &self.nested_units
    }

    /// Returns the exact bound node that starts this unit.
    pub const fn root(&self) -> BoundUnitRoot {
        self.root
    }
}

/// Identifies the exact bound node that starts a semantic unit.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BoundUnitRoot {
    /// A declared callable or lifecycle body.
    CallableBody {
        /// The callable's synchronous or asynchronous execution mode.
        execution: CallableExecution,
        /// The callable's bound body.
        body: BoundCallableBodyId,
    },
    /// An anonymous callable and its body.
    AnonymousCallable {
        /// The callable identity in the unit's local-symbol snapshot.
        callable: AnonymousCallableSymbolId,
        /// The callable's synchronous or asynchronous execution mode.
        execution: CallableExecution,
        /// The callable's bound body.
        body: BoundCallableBodyId,
    },
    /// A declaration-owned expression.
    Expression(BoundExpressionId),
    /// An ordered declaration-owned expression sequence.
    ExpressionSequence(BoundBlockId),
}

impl From<BoundUnitRoot> for AnyBoundNodeId {
    fn from(root: BoundUnitRoot) -> Self {
        match root {
            BoundUnitRoot::CallableBody { body, .. }
            | BoundUnitRoot::AnonymousCallable { body, .. } => Self::CallableBody(body),
            BoundUnitRoot::Expression(expression) => Self::Expression(expression),
            BoundUnitRoot::ExpressionSequence(block) => Self::Block(block),
        }
    }
}

fn assert_root(
    kind: BoundUnitKind,
    tree: &BoundTree,
    local_symbols: &LocalSymbolSnapshot,
    root: BoundUnitRoot,
) {
    let exists = match (kind, root) {
        (BoundUnitKind::CallableBody, BoundUnitRoot::CallableBody { body, .. }) => {
            tree.callable_body(body).is_some()
        }
        (
            BoundUnitKind::AnonymousCallable,
            BoundUnitRoot::AnonymousCallable { callable, body, .. },
        ) => {
            assert!(
                local_symbols.anonymous_callable(callable).is_some(),
                "anonymous root {callable:?} must belong to local snapshot {:?}",
                local_symbols.region()
            );

            tree.callable_body(body).is_some()
        }
        (
            BoundUnitKind::RuntimeDefault
            | BoundUnitKind::ConstantTemplate
            | BoundUnitKind::EmbeddedConstant
            | BoundUnitKind::PredicateDefinition
            | BoundUnitKind::TargetGate,
            BoundUnitRoot::Expression(expression),
        ) => tree.expression(expression).is_some(),
        (
            BoundUnitKind::Constraint | BoundUnitKind::ContractClause,
            BoundUnitRoot::ExpressionSequence(block),
        ) => tree.block(block).is_some(),
        _ => panic!("bound root {root:?} must match unit kind {kind:?}"),
    };

    assert!(
        exists,
        "bound root {root:?} must be committed in unit {:?}",
        tree.unit()
    );
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;
    use bray_symbols::{
        LocalScopeBoundary, LocalSymbolRegionId, LocalSymbolRegionKey, LocalSymbolRegionRole,
        LocalSymbolSnapshot, LocalSymbolSnapshotBuilder, SymbolKind,
    };

    use super::{BoundUnit, BoundUnitRoot};
    use crate::test_support::{source_anchor, source_anchor_with_version, symbol_key};
    use crate::{
        BoundCallableBody, BoundCallableBodyId, BoundNodeOrigin, BoundTree, BoundTreeBuilder,
        BoundUnitId, BoundUnitKey,
    };

    #[test]
    fn bound_units_publish_one_tree_root_and_local_snapshot() {
        let (key, tree, locals, root) = callable_parts(20);

        let nested = BoundUnitKey::anonymous_callable(
            key.clone(),
            source_anchor_with_version(key.source().source_version().raw() + 1),
        );

        let unit = BoundUnit::new(
            key.clone(),
            tree,
            locals,
            [nested.clone()],
            BoundUnitRoot::CallableBody {
                execution: bray_symbols::CallableExecution::Synchronous,
                body: root,
            },
        );

        assert_eq!(unit.key(), &key);
        assert_eq!(unit.unit(), BoundUnitId::new(20));
        assert_eq!(unit.identity().key(), &key);
        assert_eq!(unit.identity().unit(), BoundUnitId::new(20));

        assert_eq!(
            unit.root(),
            BoundUnitRoot::CallableBody {
                execution: bray_symbols::CallableExecution::Synchronous,
                body: root,
            }
        );

        assert!(unit.tree().callable_body(root).is_some());
        assert_eq!(unit.local_symbols().region(), LocalSymbolRegionId::new(20));
        assert_eq!(unit.nested_units(), &[nested]);
    }

    #[test]
    #[should_panic(expected = "must match unit kind")]
    fn bound_units_reject_mismatched_root_categories() {
        let (key, tree, locals, _) = callable_parts(21);

        BoundUnit::new(
            key,
            tree,
            locals,
            [],
            BoundUnitRoot::Expression(crate::BoundExpressionId::from_slot(BoundUnitId::new(21), 0)),
        );
    }

    #[test]
    fn bound_units_reject_roots_and_local_snapshots_from_other_units() {
        let (key, tree, _, _) = callable_parts(22);

        let (_, _, locals, foreign_root) = callable_parts(23);

        assert!(
            std::panic::catch_unwind(|| BoundUnit::new(
                key.clone(),
                tree.clone(),
                local_snapshot(22, key.declared_owner().clone()),
                [],
                BoundUnitRoot::CallableBody {
                    execution: bray_symbols::CallableExecution::Synchronous,
                    body: foreign_root
                },
            ))
            .is_err()
        );

        assert!(
            std::panic::catch_unwind(|| BoundUnit::new(
                key,
                tree,
                locals,
                [],
                BoundUnitRoot::CallableBody {
                    execution: bray_symbols::CallableExecution::Synchronous,
                    body: BoundCallableBodyId::from_slot(BoundUnitId::new(22), 0)
                },
            ))
            .is_err()
        );
    }

    fn callable_parts(
        unit: u32,
    ) -> (
        BoundUnitKey,
        BoundTree,
        LocalSymbolSnapshot,
        BoundCallableBodyId,
    ) {
        let owner = symbol_key(SymbolKind::Function, 0);

        let Some(key) = BoundUnitKey::callable_body(owner.clone(), source_anchor()) else {
            panic!("function must support a callable body");
        };

        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(unit));
        let body = BoundCallableBody::error(BoundNodeOrigin::source(key.source()), None);

        let root = match tree.push_callable_body(body) {
            Ok(root) => root,
            Err(error) => panic!("test callable body must fit: {error:?}"),
        };

        (key, tree.finish(), local_snapshot(unit, owner), root)
    }

    fn local_snapshot(unit: u32, owner: bray_symbols::SymbolKey) -> LocalSymbolSnapshot {
        let anchor = source_anchor().syntax();

        let Some(key) = LocalSymbolRegionKey::try_new(
            owner,
            LocalSymbolRegionRole::CallableBody,
            [anchor],
            None,
        ) else {
            panic!("test local region key must be valid");
        };

        let mut builder = LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(unit), key);

        if let Err(error) =
            builder.push_scope(None, LocalScopeBoundary::Root, anchor, TextSize::ZERO)
        {
            panic!("test root scope must be valid: {error:?}");
        }

        builder.finish()
    }
}
