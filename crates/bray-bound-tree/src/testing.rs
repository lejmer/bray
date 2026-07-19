//! Shared fixtures for crates testing bound-tree behavior.

use crate::{
    BoundExpression, BoundExpressionId, BoundTreeBuilder, BoundUnit, CheckedExpressionTypes,
    ExpressionTypeEntry, ExpressionTypeResult,
};

/// Appends a valid expression to a test bound tree.
pub fn push_expression(
    tree: &mut BoundTreeBuilder,
    expression: BoundExpression,
) -> BoundExpressionId {
    match tree.push_expression(expression) {
        Ok(expression) => expression,
        Err(error) => panic!("test expression must be valid: {error:?}"),
    }
}

/// Creates checked types that assign one result to each supplied expression.
pub fn checked_expression_types(
    unit: &BoundUnit,
    expressions: impl IntoIterator<Item = BoundExpressionId>,
    result: ExpressionTypeResult,
) -> CheckedExpressionTypes {
    CheckedExpressionTypes::new(
        unit.unit(),
        unit.key().kind(),
        expressions
            .into_iter()
            .map(|expression| ExpressionTypeEntry::new(expression, result)),
    )
}
