//! Shared fixtures for crates testing bound-tree behavior.

use crate::{BoundExpression, BoundExpressionId, BoundTreeBuilder};

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
