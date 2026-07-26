use std::sync::Arc;

use bray_symbols::{
    AnonymousCallableSymbolId, CallableExecution, LocalSymbolRegionKey, LocalSymbolRegionRole,
    LocalSymbolSnapshot, SymbolFactKind, SymbolKind,
};

use crate::{
    AnyBoundNodeId, BoundBlockId, BoundCallableBodyId, BoundExpressionId, BoundNodeKind, BoundTree,
    BoundUnitId, BoundUnitIdentity, BoundUnitKey, BoundUnitKeyData, BoundUnitKind, BoundUnitView,
    DeclaredBoundUnitKey,
};

/// One immutable bound semantic unit and its exact root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundUnit {
    key: BoundUnitKey,
    tree: BoundTree,
    local_symbols: LocalSymbolSnapshot,
    nested_units: Arc<[BoundUnitKey]>,
    root: BoundUnitRoot,
}

impl BoundUnit {
    /// Creates a bound unit after validating its root, identities, and nested units.
    pub fn try_new(
        key: BoundUnitKey,
        tree: BoundTree,
        local_symbols: LocalSymbolSnapshot,
        nested_units: impl IntoIterator<Item = BoundUnitKey>,
        root: BoundUnitRoot,
    ) -> Result<Self, BoundUnitBuildError> {
        validate_root(key.kind(), &tree, &local_symbols, root)?;

        if local_symbols.region().raw() != tree.unit().raw() {
            return Err(BoundUnitBuildError::LocalSymbolRegionMismatch);
        }

        if !local_region_matches(&key, local_symbols.key()) {
            return Err(BoundUnitBuildError::LocalSymbolRegionMismatch);
        }

        let nested_units = nested_units.into_iter().collect::<Arc<[_]>>();

        validate_nested_units(&key, &nested_units)?;

        Ok(Self {
            key,
            tree,
            local_symbols,
            nested_units,
            root,
        })
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundUnitRoot {
    /// A declared callable or lifecycle body.
    CallableBody(BoundCallableBodyId),
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
            BoundUnitRoot::CallableBody(body) | BoundUnitRoot::AnonymousCallable { body, .. } => {
                Self::CallableBody(body)
            }
            BoundUnitRoot::Expression(expression) => Self::Expression(expression),
            BoundUnitRoot::ExpressionSequence(block) => Self::Block(block),
        }
    }
}

/// A contract violation that prevents creation of a bound unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoundUnitBuildError {
    /// The root category does not match the semantic unit key.
    RootKindMismatch,
    /// The category-specific root does not name a node in the bound tree.
    MissingRoot {
        /// The unit owning the immutable bound tree.
        unit: BoundUnitId,
        /// The exact category of root required by the unit.
        kind: BoundNodeKind,
    },
    /// The local snapshot key does not correspond to the semantic unit key.
    LocalSymbolRegionMismatch,
    /// The anonymous callable belongs to another local-symbol region.
    AnonymousCallableRegionMismatch {
        /// The region owned by the bound unit's local snapshot.
        expected: bray_symbols::LocalSymbolRegionId,
        /// The region carried by the anonymous callable ID.
        actual: bray_symbols::LocalSymbolRegionId,
    },
    /// The anonymous callable does not resolve in the local-symbol snapshot.
    MissingAnonymousCallable {
        /// The exact callable that failed typed snapshot lookup.
        callable: AnonymousCallableSymbolId,
    },
    /// A nested key is not an anonymous callable directly enclosed by this unit.
    InvalidNestedUnit {
        /// The position of the invalid nested key.
        index: usize,
    },
    /// Nested keys are not unique and in canonical source order.
    NonCanonicalNestedUnits {
        /// The first position that is not strictly ordered after its predecessor.
        index: usize,
    },
}

fn validate_root(
    kind: BoundUnitKind,
    tree: &BoundTree,
    local_symbols: &LocalSymbolSnapshot,
    root: BoundUnitRoot,
) -> Result<(), BoundUnitBuildError> {
    match (kind, root) {
        (BoundUnitKind::CallableBody, BoundUnitRoot::CallableBody(body)) => {
            validate_callable_root(tree, body)
        }
        (
            BoundUnitKind::AnonymousCallable,
            BoundUnitRoot::AnonymousCallable { callable, body, .. },
        ) => {
            validate_callable_root(tree, body)?;

            if callable.region() != local_symbols.region() {
                return Err(BoundUnitBuildError::AnonymousCallableRegionMismatch {
                    expected: local_symbols.region(),
                    actual: callable.region(),
                });
            }

            if local_symbols.anonymous_callable(callable).is_none() {
                return Err(BoundUnitBuildError::MissingAnonymousCallable { callable });
            }

            Ok(())
        }
        (
            BoundUnitKind::RuntimeDefault
            | BoundUnitKind::ConstantTemplate
            | BoundUnitKind::EmbeddedConstant
            | BoundUnitKind::PredicateDefinition
            | BoundUnitKind::TargetGate,
            BoundUnitRoot::Expression(expression),
        ) => validate_expression_root(tree, expression),
        (
            BoundUnitKind::Constraint | BoundUnitKind::ContractClause,
            BoundUnitRoot::ExpressionSequence(block),
        ) => validate_block_root(tree, block),
        _ => Err(BoundUnitBuildError::RootKindMismatch),
    }
}

fn validate_callable_root(
    tree: &BoundTree,
    root: BoundCallableBodyId,
) -> Result<(), BoundUnitBuildError> {
    if tree.callable_body(root).is_none() {
        return Err(BoundUnitBuildError::MissingRoot {
            unit: tree.unit(),
            kind: BoundNodeKind::CallableBody,
        });
    }

    Ok(())
}

fn validate_expression_root(
    tree: &BoundTree,
    root: BoundExpressionId,
) -> Result<(), BoundUnitBuildError> {
    if tree.expression(root).is_none() {
        return Err(BoundUnitBuildError::MissingRoot {
            unit: tree.unit(),
            kind: BoundNodeKind::Expression,
        });
    }

    Ok(())
}

fn validate_block_root(tree: &BoundTree, root: BoundBlockId) -> Result<(), BoundUnitBuildError> {
    if tree.block(root).is_none() {
        return Err(BoundUnitBuildError::MissingRoot {
            unit: tree.unit(),
            kind: BoundNodeKind::Block,
        });
    }

    Ok(())
}

fn local_region_matches(key: &BoundUnitKey, actual: &LocalSymbolRegionKey) -> bool {
    match key.data() {
        BoundUnitKeyData::CallableBody(declared) => {
            declared_region_matches(declared, LocalSymbolRegionRole::CallableBody, actual)
        }
        BoundUnitKeyData::AnonymousCallable(_) => anonymous_region_matches(key, actual),
        BoundUnitKeyData::RuntimeDefault(declared) => {
            let Some(fact) = runtime_default_fact(declared.owner().kind()) else {
                return false;
            };

            declared_region_matches(
                declared,
                LocalSymbolRegionRole::DeclarationFact(fact),
                actual,
            )
        }
        BoundUnitKeyData::ConstantTemplate(declared) => declared_region_matches(
            declared,
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::ConstantDefinition),
            actual,
        ),
        BoundUnitKeyData::EmbeddedConstant(declared) => {
            declared_region_matches(declared, LocalSymbolRegionRole::EmbeddedConstant, actual)
        }
        BoundUnitKeyData::PredicateDefinition(declared) => declared_region_matches(
            declared,
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::PredicateDefinition),
            actual,
        ),
        BoundUnitKeyData::Constraint(declared) => declared_region_matches(
            declared,
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::GenericConstraints),
            actual,
        ),
        BoundUnitKeyData::ContractClause(declared) => declared_region_matches(
            declared,
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::CallableContracts),
            actual,
        ),
        BoundUnitKeyData::TargetGate(declared) => {
            declared_region_matches(declared, LocalSymbolRegionRole::TargetGate, actual)
        }
    }
}

fn declared_region_matches(
    key: &DeclaredBoundUnitKey,
    role: LocalSymbolRegionRole,
    actual: &LocalSymbolRegionKey,
) -> bool {
    actual.owner() == key.owner()
        && actual.role() == role
        && actual.anchors() == [key.source().syntax()]
        && actual.ordinal().is_none()
}

fn anonymous_region_matches(key: &BoundUnitKey, actual: &LocalSymbolRegionKey) -> bool {
    let mut anchors = Vec::new();
    let mut current = key;

    let owner = loop {
        match current.data() {
            BoundUnitKeyData::AnonymousCallable(anonymous) => {
                anchors.push(anonymous.source().syntax());
                current = anonymous.enclosing();
            }
            BoundUnitKeyData::CallableBody(declared)
            | BoundUnitKeyData::RuntimeDefault(declared)
            | BoundUnitKeyData::ConstantTemplate(declared)
            | BoundUnitKeyData::EmbeddedConstant(declared)
            | BoundUnitKeyData::PredicateDefinition(declared)
            | BoundUnitKeyData::Constraint(declared)
            | BoundUnitKeyData::ContractClause(declared)
            | BoundUnitKeyData::TargetGate(declared) => break declared.owner(),
        }
    };

    anchors.reverse();

    actual.owner() == owner
        && actual.role() == LocalSymbolRegionRole::AnonymousCallable
        && actual.anchors() == anchors
        && actual.ordinal().is_none()
}

const fn runtime_default_fact(owner: SymbolKind) -> Option<SymbolFactKind> {
    match owner {
        SymbolKind::CallableParameterDefaultProvider => {
            Some(SymbolFactKind::CallableParameterDefault)
        }
        SymbolKind::StructFieldDefaultProvider => Some(SymbolFactKind::StructFieldDefault),
        SymbolKind::UnionPayloadDefaultProvider => Some(SymbolFactKind::UnionPayloadFieldDefault),
        _ => None,
    }
}

fn validate_nested_units(
    enclosing: &BoundUnitKey,
    nested_units: &[BoundUnitKey],
) -> Result<(), BoundUnitBuildError> {
    for (index, nested) in nested_units.iter().enumerate() {
        let BoundUnitKeyData::AnonymousCallable(anonymous) = nested.data() else {
            return Err(BoundUnitBuildError::InvalidNestedUnit { index });
        };

        if anonymous.enclosing() != enclosing {
            return Err(BoundUnitBuildError::InvalidNestedUnit { index });
        }

        if index > 0 && nested_units[index - 1].source() >= nested.source() {
            return Err(BoundUnitBuildError::NonCanonicalNestedUnits { index });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use bray_source::TextSize;
    use bray_symbols::{
        LocalScopeBoundary, LocalSymbolRegionId, LocalSymbolRegionKey, LocalSymbolRegionRole,
        LocalSymbolSnapshot, LocalSymbolSnapshotBuilder, SymbolKind,
    };

    use super::{BoundUnit, BoundUnitBuildError, BoundUnitRoot};
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

        let unit = match BoundUnit::try_new(
            key.clone(),
            tree,
            locals,
            [nested.clone()],
            BoundUnitRoot::CallableBody(root),
        ) {
            Ok(unit) => unit,
            Err(error) => panic!("valid bound unit must publish: {error:?}"),
        };

        assert_eq!(unit.key(), &key);
        assert_eq!(unit.unit(), BoundUnitId::new(20));
        assert_eq!(unit.identity().key(), &key);
        assert_eq!(unit.identity().unit(), BoundUnitId::new(20));
        assert_eq!(unit.root(), BoundUnitRoot::CallableBody(root));
        assert!(unit.tree().callable_body(root).is_some());
        assert_eq!(unit.local_symbols().region(), LocalSymbolRegionId::new(20));
        assert_eq!(unit.nested_units(), &[nested]);
    }

    #[test]
    fn bound_units_reject_mismatched_root_categories() {
        let (key, tree, locals, _) = callable_parts(21);

        let result = BoundUnit::try_new(
            key,
            tree,
            locals,
            [],
            BoundUnitRoot::Expression(crate::BoundExpressionId::from_slot(BoundUnitId::new(21), 0)),
        );

        assert_eq!(result, Err(BoundUnitBuildError::RootKindMismatch));
    }

    #[test]
    fn bound_units_reject_roots_and_local_snapshots_from_other_units() {
        let (key, tree, _, _) = callable_parts(22);

        let (_, _, locals, foreign_root) = callable_parts(23);

        let foreign_root_result = BoundUnit::try_new(
            key.clone(),
            tree.clone(),
            local_snapshot(22, key.declared_owner().clone()),
            [],
            BoundUnitRoot::CallableBody(foreign_root),
        );

        assert_eq!(
            foreign_root_result,
            Err(BoundUnitBuildError::MissingRoot {
                unit: BoundUnitId::new(22),
                kind: crate::BoundNodeKind::CallableBody,
            })
        );

        let local_result = BoundUnit::try_new(
            key,
            tree,
            locals,
            [],
            BoundUnitRoot::CallableBody(BoundCallableBodyId::from_slot(BoundUnitId::new(22), 0)),
        );

        assert_eq!(
            local_result,
            Err(BoundUnitBuildError::LocalSymbolRegionMismatch)
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

        match builder.finish() {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test local snapshot must be valid: {error:?}"),
        }
    }
}
