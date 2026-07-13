use bray_symbols::{AnonymousCallableSymbolId, LocalSymbolSnapshot};

use super::{
    CheckedUnitBuildError,
    data::{CheckedUnitData, unit_data_accessors},
    validation::{validate_anonymous_callable, validate_callable_root, validate_expression_root},
};
use crate::{
    BoundCallableBodyId, BoundExpressionId, BoundTree, BoundUnitId, BoundUnitKey, BoundUnitKind,
    CheckedControlFlowFacts,
};

/// A declared callable body after binding and control-flow checking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlFlowCheckedCallableBody {
    data: CheckedUnitData,
    control_flow_facts: CheckedControlFlowFacts,
    root: BoundCallableBodyId,
}

impl ControlFlowCheckedCallableBody {
    /// Validates and assembles one control-flow-checked callable body.
    pub fn try_new(
        key: &BoundUnitKey,
        tree: BoundTree,
        local_symbols: LocalSymbolSnapshot,
        nested_units: impl IntoIterator<Item = BoundUnitKey>,
        control_flow_facts: CheckedControlFlowFacts,
        root: BoundCallableBodyId,
    ) -> Result<Self, CheckedUnitBuildError> {
        let data = CheckedUnitData::try_new(
            BoundUnitKind::CallableBody,
            key,
            tree,
            local_symbols,
            nested_units,
        )?;

        validate_control_flow(&data, control_flow_facts, BoundUnitKind::CallableBody)?;
        validate_callable_root(&data, root)?;

        Ok(Self {
            data,
            control_flow_facts,
            root,
        })
    }

    /// Returns the exact callable-body root.
    pub const fn root(&self) -> BoundCallableBodyId {
        self.root
    }

    unit_data_accessors!();

    /// Returns the durable facts established by control-flow checking.
    pub const fn control_flow_facts(&self) -> CheckedControlFlowFacts {
        self.control_flow_facts
    }
}

/// An anonymous callable after binding and control-flow checking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ControlFlowCheckedAnonymousCallable {
    data: CheckedUnitData,
    control_flow_facts: CheckedControlFlowFacts,
    callable: AnonymousCallableSymbolId,
    root: BoundCallableBodyId,
}

impl ControlFlowCheckedAnonymousCallable {
    /// Validates and assembles one control-flow-checked anonymous callable.
    pub fn try_new(
        key: &BoundUnitKey,
        tree: BoundTree,
        local_symbols: LocalSymbolSnapshot,
        nested_units: impl IntoIterator<Item = BoundUnitKey>,
        control_flow_facts: CheckedControlFlowFacts,
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

        validate_control_flow(&data, control_flow_facts, BoundUnitKind::AnonymousCallable)?;
        validate_anonymous_callable(&data, callable)?;
        validate_callable_root(&data, root)?;

        Ok(Self {
            data,
            control_flow_facts,
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

    /// Returns the durable facts established by control-flow checking.
    pub const fn control_flow_facts(&self) -> CheckedControlFlowFacts {
        self.control_flow_facts
    }
}

macro_rules! define_control_flow_expression_unit {
    ($name:ident, $kind:ident, $description:literal) => {
        #[doc = $description]
        #[derive(Clone, Debug, Eq, PartialEq)]
        pub struct $name {
            data: CheckedUnitData,
            control_flow_facts: CheckedControlFlowFacts,
            root: BoundExpressionId,
        }

        impl $name {
            /// Validates and assembles one control-flow-checked expression unit.
            pub fn try_new(
                key: &BoundUnitKey,
                tree: BoundTree,
                local_symbols: LocalSymbolSnapshot,
                nested_units: impl IntoIterator<Item = BoundUnitKey>,
                control_flow_facts: CheckedControlFlowFacts,
                root: BoundExpressionId,
            ) -> Result<Self, CheckedUnitBuildError> {
                let data = CheckedUnitData::try_new(
                    BoundUnitKind::$kind,
                    key,
                    tree,
                    local_symbols,
                    nested_units,
                )?;

                validate_control_flow(&data, control_flow_facts, BoundUnitKind::$kind)?;
                validate_expression_root(&data, root)?;

                Ok(Self {
                    data,
                    control_flow_facts,
                    root,
                })
            }

            /// Returns the exact bound expression root.
            pub const fn root(&self) -> BoundExpressionId {
                self.root
            }

            unit_data_accessors!();

            /// Returns the durable facts established by control-flow checking.
            pub const fn control_flow_facts(&self) -> CheckedControlFlowFacts {
                self.control_flow_facts
            }
        }
    };
}

define_control_flow_expression_unit!(
    ControlFlowCheckedRuntimeDefaultUnit,
    RuntimeDefault,
    "A runtime-default expression after binding and control-flow checking."
);
define_control_flow_expression_unit!(
    ControlFlowCheckedConstantTemplateUnit,
    ConstantTemplate,
    "A constant definition template after binding and control-flow checking."
);
define_control_flow_expression_unit!(
    ControlFlowCheckedPredicateDefinitionUnit,
    PredicateDefinition,
    "A predicate definition after binding and control-flow checking."
);
define_control_flow_expression_unit!(
    ControlFlowCheckedConstraintUnit,
    Constraint,
    "A declaration constraint after binding and control-flow checking."
);
define_control_flow_expression_unit!(
    ControlFlowCheckedContractClauseUnit,
    ContractClause,
    "A callable contract clause after binding and control-flow checking."
);

fn validate_control_flow(
    data: &CheckedUnitData,
    facts: CheckedControlFlowFacts,
    expected_kind: BoundUnitKind,
) -> Result<(), CheckedUnitBuildError> {
    if facts.unit() != data.unit() {
        return Err(CheckedUnitBuildError::ControlFlowUnitMismatch {
            expected: data.unit(),
            actual: facts.unit(),
        });
    }

    if facts.kind() != expected_kind {
        return Err(CheckedUnitBuildError::ControlFlowKindMismatch {
            expected: expected_kind,
            actual: facts.kind(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use bray_symbols::{LocalSymbolRegionRole, SymbolKind};

    use super::{
        ControlFlowCheckedAnonymousCallable, ControlFlowCheckedCallableBody,
        ControlFlowCheckedConstantTemplateUnit, ControlFlowCheckedConstraintUnit,
        ControlFlowCheckedContractClauseUnit, ControlFlowCheckedPredicateDefinitionUnit,
        ControlFlowCheckedRuntimeDefaultUnit,
    };
    use crate::test_support::{local_snapshot, recovered_control_flow, source_anchor, symbol_key};
    use crate::{
        BoundCallableBody, BoundNodeOrigin, BoundTreeBuilder, BoundUnitId, BoundUnitKey,
        BoundUnitKind, CheckedUnitBuildError,
    };

    #[test]
    fn control_flow_stage_validates_exact_unit_and_category() {
        let source = source_anchor();
        let owner = symbol_key(SymbolKind::Function, 0);

        let Some(key) = BoundUnitKey::callable_body(owner.clone(), source) else {
            panic!("function key must support a callable body");
        };

        let (tree, root) = callable_tree(BoundUnitId::new(10));
        let matching = ControlFlowCheckedCallableBody::try_new(
            &key,
            tree,
            local_snapshot(
                10,
                owner.clone(),
                LocalSymbolRegionRole::CallableBody,
                [source.syntax()],
            ),
            [],
            recovered_control_flow(BoundUnitId::new(10), BoundUnitKind::CallableBody),
            root,
        );

        assert!(matching.is_ok());

        let (tree, root) = callable_tree(BoundUnitId::new(11));

        let foreign_unit = ControlFlowCheckedCallableBody::try_new(
            &key,
            tree,
            local_snapshot(
                11,
                owner,
                LocalSymbolRegionRole::CallableBody,
                [source.syntax()],
            ),
            [],
            recovered_control_flow(BoundUnitId::new(12), BoundUnitKind::CallableBody),
            root,
        );

        assert_eq!(
            foreign_unit,
            Err(CheckedUnitBuildError::ControlFlowUnitMismatch {
                expected: BoundUnitId::new(11),
                actual: BoundUnitId::new(12),
            })
        );
    }

    #[test]
    fn staged_units_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ControlFlowCheckedCallableBody>();
        assert_send_sync::<ControlFlowCheckedAnonymousCallable>();
        assert_send_sync::<ControlFlowCheckedRuntimeDefaultUnit>();
        assert_send_sync::<ControlFlowCheckedConstantTemplateUnit>();
        assert_send_sync::<ControlFlowCheckedPredicateDefinitionUnit>();
        assert_send_sync::<ControlFlowCheckedConstraintUnit>();
        assert_send_sync::<ControlFlowCheckedContractClauseUnit>();
    }

    fn callable_tree(unit: BoundUnitId) -> (crate::BoundTree, crate::BoundCallableBodyId) {
        let mut builder = BoundTreeBuilder::new(unit);
        let body = BoundCallableBody::error(BoundNodeOrigin::source(source_anchor()), None);

        let Ok(root) = builder.push_callable_body(body) else {
            panic!("one test callable body must fit in an empty tree");
        };

        (builder.finish(), root)
    }
}
