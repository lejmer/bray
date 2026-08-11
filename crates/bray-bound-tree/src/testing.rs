//! Shared fixtures for crates testing bound-tree behavior.

use crate::{
    BoundExpression, BoundExpressionId, BoundTreeBuilder, BoundUnit, CheckedExpressionTypes,
    ExpressionTypeEntry, ExpressionTypeResult, LifecycleObligationId, ScopedCapabilityId,
};

/// Creates a scoped-capability identity for cross-crate semantic tests.
pub fn scoped_capability_id(unit: crate::BoundUnitId, ordinal: u32) -> ScopedCapabilityId {
    ScopedCapabilityId::from_test_slot(unit, ordinal)
}

/// Creates a lifecycle-obligation identity for cross-crate semantic tests.
pub fn lifecycle_obligation_id(unit: crate::BoundUnitId, ordinal: u32) -> LifecycleObligationId {
    LifecycleObligationId::from_test_slot(unit, ordinal)
}

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
