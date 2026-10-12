use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundUnit, BoundUnitRoot, CheckedExpressionSemantics,
};
use bray_symbols::SemanticValueStore;

use crate::ExecutionCondition;

use super::{condition_literals, expression_condition};

/// Normalizes checked contract predicates while preserving unknown meaning.
pub fn execution_conditions(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
) -> Result<Vec<ExecutionCondition>, bray_symbols::SemanticValueStoreError> {
    Ok(predicate_conditions(unit, semantics, values)?
        .into_iter()
        .map(|(_, condition, _)| condition)
        .collect())
}

/// Normalizes predicate meaning and preserves explicit trusted contract requirements.
pub fn predicate_conditions(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
) -> Result<Vec<(BoundExpressionId, ExecutionCondition, bool)>, bray_symbols::SemanticValueStoreError>
{
    predicate_conditions_with_inputs(unit, semantics, values, &BTreeMap::new())
}

/// Normalizes contract type clauses using exact declaration-local input placeholders.
pub fn predicate_conditions_with_inputs(
    unit: &BoundUnit,
    semantics: &CheckedExpressionSemantics,
    values: &SemanticValueStore,
    inputs: &BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
) -> Result<Vec<(BoundExpressionId, ExecutionCondition, bool)>, bray_symbols::SemanticValueStoreError>
{
    let roots = match unit.root() {
        BoundUnitRoot::Expression(expression) => vec![expression],
        BoundUnitRoot::ExpressionSequence(block) => unit
            .view()
            .block(block)
            .map(|block| {
                block
                    .items()
                    .iter()
                    .filter_map(|item| item.expression())
                    .collect()
            })
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    let literals = condition_literals(semantics, values);

    let mut pending = roots
        .into_iter()
        .rev()
        .map(|root| (root, false))
        .collect::<Vec<_>>();

    let mut conditions = Vec::new();

    while let Some((root, trusted)) = pending.pop() {
        match unit.view().expression(root) {
            Some(BoundExpression::Structured(structured))
                if matches!(
                    structured.kind(),
                    bray_bound_tree::BoundStructuredExpressionKind::TrustBoundary
                        | bray_bound_tree::BoundStructuredExpressionKind::Condition
                ) =>
            {
                if let [operand] = structured.operands() {
                    pending.push((
                        *operand,
                        trusted
                            || structured.kind()
                                == bray_bound_tree::BoundStructuredExpressionKind::TrustBoundary,
                    ));

                    continue;
                }
            }
            _ => {}
        }

        let mut condition = expression_condition(
            unit,
            semantics,
            values,
            &literals,
            inputs,
            &BTreeMap::new(),
            root,
            &mut { crate::ExecutionCondition::WORK_LIMIT },
        );

        let is_trusted = trusted || contains_trusted_condition(unit, root);

        if trusted {
            condition = condition.requiring_trusted_predicates();
        }

        conditions.push((root, condition, is_trusted));
    }

    Ok(conditions)
}

fn contains_trusted_condition(unit: &BoundUnit, root: BoundExpressionId) -> bool {
    let mut pending = vec![root];
    let mut budget = ExecutionCondition::WORK_LIMIT;

    while let Some(expression) = pending.pop() {
        let Some(remaining) = budget.checked_sub(1) else {
            return true;
        };

        budget = remaining;

        let Some(bound) = unit.view().expression(expression) else {
            continue;
        };

        if matches!(bound, BoundExpression::Structured(expression)
            if expression.kind() == bray_bound_tree::BoundStructuredExpressionKind::TrustBoundary)
        {
            return true;
        }

        pending.extend(bound.child_expressions());
    }

    false
}
