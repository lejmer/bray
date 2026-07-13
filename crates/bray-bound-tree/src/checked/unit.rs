use bray_symbols::{AnonymousCallableSymbolId, LocalSymbolSnapshot};

use super::{
    CheckedUnitBuildError,
    data::{CheckedUnitData, unit_data_accessors},
    validation::{validate_anonymous_callable, validate_callable_root, validate_expression_root},
};
use crate::{
    BoundCallableBodyId, BoundExpressionId, BoundTree, BoundUnitId, BoundUnitKey, BoundUnitKind,
};

/// A fully checked declared callable or lifecycle body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedCallableBody {
    data: CheckedUnitData,
    root: BoundCallableBodyId,
}

impl CheckedCallableBody {
    /// Validates and assembles one immutable checked callable body.
    pub fn try_new(
        key: &BoundUnitKey,
        tree: BoundTree,
        local_symbols: LocalSymbolSnapshot,
        nested_units: impl IntoIterator<Item = BoundUnitKey>,
        root: BoundCallableBodyId,
    ) -> Result<Self, CheckedUnitBuildError> {
        let data = CheckedUnitData::try_new(
            BoundUnitKind::CallableBody,
            key,
            tree,
            local_symbols,
            nested_units,
        )?;

        validate_callable_root(&data, root)?;

        Ok(Self { data, root })
    }

    /// Returns the exact callable-body root.
    pub const fn root(&self) -> BoundCallableBodyId {
        self.root
    }

    unit_data_accessors!();
}

/// A fully checked anonymous callable signature, contracts, and body.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedAnonymousCallable {
    data: CheckedUnitData,
    callable: AnonymousCallableSymbolId,
    root: BoundCallableBodyId,
}

impl CheckedAnonymousCallable {
    /// Validates and assembles one immutable checked anonymous callable.
    pub fn try_new(
        key: &BoundUnitKey,
        tree: BoundTree,
        local_symbols: LocalSymbolSnapshot,
        nested_units: impl IntoIterator<Item = BoundUnitKey>,
        callable: AnonymousCallableSymbolId,
        root: BoundCallableBodyId,
    ) -> Result<Self, CheckedUnitBuildError> {
        let data = CheckedUnitData::try_new(
            BoundUnitKind::AnonymousCallable,
            key,
            tree,
            local_symbols,
            nested_units,
        )?;

        validate_anonymous_callable(&data, callable)?;
        validate_callable_root(&data, root)?;

        Ok(Self {
            data,
            callable,
            root,
        })
    }

    /// Returns the region-scoped anonymous callable identity checked by this unit.
    pub const fn callable(&self) -> AnonymousCallableSymbolId {
        self.callable
    }

    /// Returns the exact callable-body root.
    pub const fn root(&self) -> BoundCallableBodyId {
        self.root
    }

    unit_data_accessors!();
}

macro_rules! define_checked_expression_unit {
    ($name:ident, $kind:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $name {
            data: CheckedUnitData,
            root: BoundExpressionId,
        }

        impl $name {
            /// Validates and assembles one immutable checked expression unit.
            pub fn try_new(
                key: &BoundUnitKey,
                tree: BoundTree,
                local_symbols: LocalSymbolSnapshot,
                nested_units: impl IntoIterator<Item = BoundUnitKey>,
                root: BoundExpressionId,
            ) -> Result<Self, CheckedUnitBuildError> {
                let data = CheckedUnitData::try_new(
                    BoundUnitKind::$kind,
                    key,
                    tree,
                    local_symbols,
                    nested_units,
                )?;

                validate_expression_root(&data, root)?;

                Ok(Self { data, root })
            }

            /// Returns the exact checked expression root.
            pub const fn root(&self) -> BoundExpressionId {
                self.root
            }

            unit_data_accessors!();
        }
    };
}

define_checked_expression_unit!(
    CheckedRuntimeDefaultUnit,
    RuntimeDefault,
    "A fully checked parameter, field, or payload runtime-default expression."
);
define_checked_expression_unit!(
    CheckedConstantTemplateUnit,
    ConstantTemplate,
    "A fully checked constant definition template."
);
define_checked_expression_unit!(
    CheckedPredicateDefinitionUnit,
    PredicateDefinition,
    "A fully checked predicate definition expression."
);
define_checked_expression_unit!(
    CheckedConstraintUnit,
    Constraint,
    "A fully checked declaration constraint expression."
);
define_checked_expression_unit!(
    CheckedContractClauseUnit,
    ContractClause,
    "A fully checked callable contract-clause expression."
);

#[cfg(test)]
mod tests {
    use bray_source::TextSize;
    use bray_symbols::{
        AnonymousCallableSymbolId, LocalScopeBoundary, LocalSymbolRegionId, LocalSymbolRegionKey,
        LocalSymbolRegionRole, LocalSymbolSnapshotBuilder, SymbolFactKind, SymbolKind,
    };

    use super::{
        CheckedAnonymousCallable, CheckedCallableBody, CheckedConstantTemplateUnit,
        CheckedConstraintUnit, CheckedContractClauseUnit, CheckedPredicateDefinitionUnit,
        CheckedRuntimeDefaultUnit,
    };
    use crate::test_support::{
        error_expression, local_snapshot, runtime_default_key, source_anchor,
        source_anchor_with_version, symbol_key,
    };
    use crate::{
        BoundCallableBody, BoundCallableBodyId, BoundExpressionId, BoundNodeKind, BoundNodeOrigin,
        BoundTree, BoundTreeBuilder, BoundUnitId, BoundUnitKey, BoundUnitKind,
        CheckedUnitBuildError,
    };

    #[test]
    fn callable_units_own_recovery_trees_snapshots_and_ordered_nested_keys() {
        let source = source_anchor();

        let owner = symbol_key(SymbolKind::Function, 0);
        let key = valid_key(BoundUnitKey::callable_body(owner.clone(), source));

        let local_symbols = local_snapshot(
            20,
            owner,
            LocalSymbolRegionRole::CallableBody,
            [source.syntax()],
        );

        let (tree, root) = callable_tree(BoundUnitId::new(20));

        let first = BoundUnitKey::anonymous_callable(
            key.clone(),
            source_anchor_with_version(source.source_version().raw() + 1),
        );

        let second = BoundUnitKey::anonymous_callable(
            key.clone(),
            source_anchor_with_version(source.source_version().raw() + 2),
        );

        let Ok(unit) = CheckedCallableBody::try_new(
            &key,
            tree,
            local_symbols,
            [first.clone(), second.clone()],
            root,
        ) else {
            panic!("a complete recovery body must be publishable");
        };

        assert_eq!(unit.unit(), BoundUnitId::new(20));
        assert_eq!(unit.root(), root);

        assert!(unit.tree().callable_body(root).is_some());

        assert_eq!(unit.local_symbols().region(), LocalSymbolRegionId::new(20));
        assert_eq!(unit.nested_units(), &[first, second]);
    }

    #[test]
    fn expression_units_preserve_distinct_categories_and_exact_roots() {
        let source = source_anchor();

        let runtime_owner = runtime_default_key(0);
        let runtime_key = valid_key(BoundUnitKey::runtime_default(runtime_owner.clone(), source));

        let runtime_symbols = local_snapshot(
            1,
            runtime_owner,
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::CallableParameterDefault),
            [source.syntax()],
        );

        let (runtime_tree, runtime_root) = expression_tree(BoundUnitId::new(1));

        let runtime = publish_expression_unit(CheckedRuntimeDefaultUnit::try_new(
            &runtime_key,
            runtime_tree,
            runtime_symbols,
            [],
            runtime_root,
        ));

        let constant_owner = symbol_key(SymbolKind::Constant, 1);
        let constant_key = valid_key(BoundUnitKey::constant_template(
            constant_owner.clone(),
            source,
        ));

        let constant_symbols = local_snapshot(
            2,
            constant_owner,
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::ConstantDefinition),
            [source.syntax()],
        );

        let (constant_tree, constant_root) = expression_tree(BoundUnitId::new(2));

        let constant = publish_expression_unit(CheckedConstantTemplateUnit::try_new(
            &constant_key,
            constant_tree,
            constant_symbols,
            [],
            constant_root,
        ));

        let predicate_owner = symbol_key(SymbolKind::Predicate, 2);
        let predicate_key = valid_key(BoundUnitKey::predicate_definition(
            predicate_owner.clone(),
            source,
        ));

        let predicate_symbols = local_snapshot(
            3,
            predicate_owner,
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::PredicateDefinition),
            [source.syntax()],
        );

        let (predicate_tree, predicate_root) = expression_tree(BoundUnitId::new(3));

        let predicate = publish_expression_unit(CheckedPredicateDefinitionUnit::try_new(
            &predicate_key,
            predicate_tree,
            predicate_symbols,
            [],
            predicate_root,
        ));

        let constraint_owner = symbol_key(SymbolKind::Function, 3);
        let constraint_key = valid_key(BoundUnitKey::constraint(constraint_owner.clone(), source));

        let constraint_symbols = local_snapshot(
            4,
            constraint_owner,
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::GenericConstraints),
            [source.syntax()],
        );

        let (constraint_tree, constraint_root) = expression_tree(BoundUnitId::new(4));

        let constraint = publish_expression_unit(CheckedConstraintUnit::try_new(
            &constraint_key,
            constraint_tree,
            constraint_symbols,
            [],
            constraint_root,
        ));

        let contract_owner = symbol_key(SymbolKind::CallableContract, 4);
        let contract_key = valid_key(BoundUnitKey::contract_clause(
            contract_owner.clone(),
            source,
        ));

        let contract_symbols = local_snapshot(
            5,
            contract_owner,
            LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::CallableContracts),
            [source.syntax()],
        );

        let (contract_tree, contract_root) = expression_tree(BoundUnitId::new(5));

        let contract = publish_expression_unit(CheckedContractClauseUnit::try_new(
            &contract_key,
            contract_tree,
            contract_symbols,
            [],
            contract_root,
        ));

        assert_eq!(runtime.root(), runtime_root);
        assert_eq!(constant.root(), constant_root);
        assert_eq!(predicate.root(), predicate_root);
        assert_eq!(constraint.root(), constraint_root);
        assert_eq!(contract.root(), contract_root);
    }

    #[test]
    fn anonymous_callable_units_keep_their_exact_callable_identity() {
        let source = source_anchor();

        let owner = symbol_key(SymbolKind::Function, 0);
        let enclosing = valid_key(BoundUnitKey::callable_body(owner.clone(), source));

        let key = BoundUnitKey::anonymous_callable(
            enclosing,
            source_anchor_with_version(source.source_version().raw() + 1),
        );

        let (local_symbols, callables) = anonymous_callable_snapshot(31, owner, source.syntax(), 1);

        let [callable] = callables.as_slice() else {
            panic!("test snapshot must contain exactly one anonymous callable");
        };

        let (tree, root) = callable_tree(BoundUnitId::new(31));

        let Ok(unit) =
            CheckedAnonymousCallable::try_new(&key, tree, local_symbols, [], *callable, root)
        else {
            panic!("a matching anonymous callable unit must be publishable");
        };

        assert_eq!(unit.callable(), *callable);
        assert_eq!(unit.root(), root);
    }

    #[test]
    fn anonymous_callable_units_reject_foreign_and_missing_callable_ids() {
        let source = source_anchor();

        let owner = symbol_key(SymbolKind::Function, 0);
        let enclosing = valid_key(BoundUnitKey::callable_body(owner.clone(), source));

        let key = BoundUnitKey::anonymous_callable(
            enclosing,
            source_anchor_with_version(source.source_version().raw() + 1),
        );

        let (foreign_snapshot, foreign_callables) =
            anonymous_callable_snapshot(30, owner.clone(), source.syntax(), 1);

        let [foreign_callable] = foreign_callables.as_slice() else {
            panic!("test snapshot must contain exactly one foreign callable");
        };

        let (local_symbols, _) = anonymous_callable_snapshot(31, owner.clone(), source.syntax(), 1);
        let (tree, root) = callable_tree(BoundUnitId::new(31));

        let wrong_region = CheckedAnonymousCallable::try_new(
            &key,
            tree,
            local_symbols,
            [],
            *foreign_callable,
            root,
        );

        assert_eq!(
            wrong_region,
            Err(CheckedUnitBuildError::AnonymousCallableRegionMismatch {
                expected: LocalSymbolRegionId::new(31),
                actual: foreign_snapshot.region(),
            })
        );

        let (local_symbols, _) = anonymous_callable_snapshot(31, owner.clone(), source.syntax(), 1);
        let (_, two_callables) = anonymous_callable_snapshot(31, owner, source.syntax(), 2);

        let [_, missing_callable] = two_callables.as_slice() else {
            panic!("test snapshot must construct a second callable slot");
        };

        let (tree, root) = callable_tree(BoundUnitId::new(31));

        let missing = CheckedAnonymousCallable::try_new(
            &key,
            tree,
            local_symbols,
            [],
            *missing_callable,
            root,
        );

        assert_eq!(
            missing,
            Err(CheckedUnitBuildError::MissingAnonymousCallable {
                callable: *missing_callable,
            })
        );
    }

    #[test]
    fn construction_rejects_wrong_categories_roots_and_local_regions() {
        let source = source_anchor();

        let owner = symbol_key(SymbolKind::Function, 0);
        let key = valid_key(BoundUnitKey::callable_body(owner.clone(), source));
        let predicate_key = valid_key(BoundUnitKey::predicate_definition(
            symbol_key(SymbolKind::Predicate, 1),
            source,
        ));

        let (wrong_kind_tree, wrong_kind_root) = callable_tree(BoundUnitId::new(1));

        let wrong_kind = CheckedCallableBody::try_new(
            &predicate_key,
            wrong_kind_tree,
            local_snapshot(
                1,
                symbol_key(SymbolKind::Predicate, 1),
                LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::PredicateDefinition),
                [source.syntax()],
            ),
            [],
            wrong_kind_root,
        );

        assert_eq!(
            wrong_kind,
            Err(CheckedUnitBuildError::UnitKindMismatch {
                expected: BoundUnitKind::CallableBody,
                actual: BoundUnitKind::PredicateDefinition,
            })
        );

        let (tree, _) = callable_tree(BoundUnitId::new(2));

        let foreign_root = callable_tree(BoundUnitId::new(3)).1;
        let missing_root = CheckedCallableBody::try_new(
            &key,
            tree,
            local_snapshot(
                2,
                owner.clone(),
                LocalSymbolRegionRole::CallableBody,
                [source.syntax()],
            ),
            [],
            foreign_root,
        );

        assert_eq!(
            missing_root,
            Err(CheckedUnitBuildError::MissingRoot {
                unit: BoundUnitId::new(2),
                kind: BoundNodeKind::CallableBody,
            })
        );

        let (tree, root) = callable_tree(BoundUnitId::new(4));

        let wrong_region = CheckedCallableBody::try_new(
            &key,
            tree,
            local_snapshot(
                4,
                owner,
                LocalSymbolRegionRole::AnonymousCallable,
                [source.syntax()],
            ),
            [],
            root,
        );

        assert_eq!(
            wrong_region,
            Err(CheckedUnitBuildError::LocalSymbolRegionMismatch)
        );
    }

    #[test]
    fn nested_keys_must_be_direct_unique_and_canonically_ordered() {
        let source = source_anchor();

        let owner = symbol_key(SymbolKind::Function, 0);
        let key = valid_key(BoundUnitKey::callable_body(owner.clone(), source));

        let first = BoundUnitKey::anonymous_callable(
            key.clone(),
            source_anchor_with_version(source.source_version().raw() + 1),
        );

        let second = BoundUnitKey::anonymous_callable(
            key.clone(),
            source_anchor_with_version(source.source_version().raw() + 2),
        );

        let other_enclosing = valid_key(BoundUnitKey::callable_body(
            symbol_key(SymbolKind::Function, 1),
            source,
        ));

        let indirect = BoundUnitKey::anonymous_callable(other_enclosing, second.source());

        let invalid = checked_callable(&key, owner.clone(), [indirect]);

        assert_eq!(
            invalid,
            Err(CheckedUnitBuildError::InvalidNestedUnit { index: 0 })
        );

        let duplicate = checked_callable(&key, owner.clone(), [first.clone(), first.clone()]);

        assert_eq!(
            duplicate,
            Err(CheckedUnitBuildError::NonCanonicalNestedUnits { index: 1 })
        );

        let reversed = checked_callable(&key, owner, [second, first]);

        assert_eq!(
            reversed,
            Err(CheckedUnitBuildError::NonCanonicalNestedUnits { index: 1 })
        );
    }

    #[test]
    fn checked_units_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CheckedCallableBody>();
        assert_send_sync::<CheckedAnonymousCallable>();
        assert_send_sync::<CheckedRuntimeDefaultUnit>();
        assert_send_sync::<CheckedConstantTemplateUnit>();
        assert_send_sync::<CheckedPredicateDefinitionUnit>();
        assert_send_sync::<CheckedConstraintUnit>();
        assert_send_sync::<CheckedContractClauseUnit>();
    }

    fn callable_tree(unit: BoundUnitId) -> (BoundTree, BoundCallableBodyId) {
        let mut builder = BoundTreeBuilder::new(unit);
        let body = BoundCallableBody::error(BoundNodeOrigin::source(source_anchor()), None);

        let Ok(root) = builder.push_callable_body(body) else {
            panic!("one test callable body must fit in an empty tree");
        };

        (builder.finish(), root)
    }

    fn expression_tree(unit: BoundUnitId) -> (BoundTree, BoundExpressionId) {
        let mut builder = BoundTreeBuilder::new(unit);

        let Ok(root) = builder.push_expression(error_expression()) else {
            panic!("one test expression must fit in an empty tree");
        };

        (builder.finish(), root)
    }

    fn checked_callable(
        key: &BoundUnitKey,
        owner: bray_symbols::SymbolKey,
        nested: impl IntoIterator<Item = BoundUnitKey>,
    ) -> Result<CheckedCallableBody, CheckedUnitBuildError> {
        let (tree, root) = callable_tree(BoundUnitId::new(40));

        CheckedCallableBody::try_new(
            key,
            tree,
            local_snapshot(
                40,
                owner,
                LocalSymbolRegionRole::CallableBody,
                [key.source().syntax()],
            ),
            nested,
            root,
        )
    }

    fn anonymous_callable_snapshot(
        region: u32,
        owner: bray_symbols::SymbolKey,
        syntax: bray_declarations::SyntaxAnchor,
        callable_count: usize,
    ) -> (
        bray_symbols::LocalSymbolSnapshot,
        Vec<AnonymousCallableSymbolId>,
    ) {
        let Some(key) = LocalSymbolRegionKey::try_new(
            owner,
            LocalSymbolRegionRole::AnonymousCallable,
            [syntax],
            None,
        ) else {
            panic!("test anonymous region must have a source anchor");
        };

        let mut builder = LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(region), key);

        let root = match builder.push_scope(None, LocalScopeBoundary::Root, syntax, TextSize::ZERO)
        {
            Ok(scope) => scope,
            Err(error) => panic!("test root scope must be valid: {error:?}"),
        };

        let mut callables = Vec::new();

        for index in 0..callable_count {
            let callable_scope = match builder.push_scope(
                Some(root),
                LocalScopeBoundary::Callable,
                syntax,
                TextSize::ZERO,
            ) {
                Ok(scope) => scope,
                Err(error) => panic!("test callable scope must be valid: {error:?}"),
            };

            let Ok(index) = u32::try_from(index) else {
                panic!("test callable ordinal must fit in u32");
            };

            let ordinal = Some(bray_symbols::SymbolOrdinal::new(index));

            let callable = match builder.push_anonymous_callable(
                root,
                callable_scope,
                [syntax],
                ordinal,
                false,
            ) {
                Ok(callable) => callable,
                Err(error) => panic!("test anonymous callable must be valid: {error:?}"),
            };

            callables.push(callable);
        }

        let snapshot = match builder.finish() {
            Ok(snapshot) => snapshot,
            Err(error) => panic!("test anonymous snapshot must be valid: {error:?}"),
        };

        (snapshot, callables)
    }

    fn publish_expression_unit<T>(result: Result<T, CheckedUnitBuildError>) -> T {
        match result {
            Ok(unit) => unit,
            Err(error) => panic!("matching expression unit must be publishable: {error:?}"),
        }
    }

    fn valid_key(key: Option<BoundUnitKey>) -> BoundUnitKey {
        match key {
            Some(key) => key,
            None => panic!("test owner must support the requested bound unit category"),
        }
    }
}
