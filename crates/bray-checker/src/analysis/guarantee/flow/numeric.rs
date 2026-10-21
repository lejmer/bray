use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundOperator, OperatorTarget, SelectedOperation,
    SemanticSelection,
};
use bray_symbols::{ConstantValueData, ConstantValueKind};
use num_bigint::BigInt;

use super::model::ExecutionFlowDomain;
use super::super::predicate::{IntegerRanges, arithmetic_range};
use super::super::state::ExecutionState;
use crate::{CheckerRequestContext, ExecutionCondition};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    /// Interpret machine operations at their actual execution state, before invalidation.
    /// Symbolic exact arithmetic is retained only when its result cannot wrap.
    pub(super) fn machine_operation_value(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<ExecutionCondition> {
        if !self.request.unit().key().kind().uses_machine_arithmetic() {
            return None;
        }
        let (operator, operands, ty) = match (
            self.request.view().expression(expression)?,
            self.semantics.selections().expression(expression)?,
        ) {
            (BoundExpression::Unary(operation),
                SemanticSelection::Operation(SelectedOperation::Operator { target: OperatorTarget::BuiltIn(operator), .. })) =>
                (*operator, operation.operands(), self.semantics.types().expression(expression)?.ty()),
            (BoundExpression::Binary(operation),
                SemanticSelection::Operation(SelectedOperation::Operator { target: OperatorTarget::BuiltIn(operator), .. })) =>
                (*operator, operation.operands(), self.semantics.types().expression(expression)?.ty()),
            (BoundExpression::Assignment(operation),
                SemanticSelection::Operation(SelectedOperation::CompoundAssignment(selected))) => {
                let OperatorTarget::BuiltIn(operator) = selected.target() else { return None; };
                (operator, operation.operands(), selected.value_type())
            }
            _ => return None,
        };
        if matches!(operator, BoundOperator::LogicalAnd | BoundOperator::LogicalOr
            | BoundOperator::LogicalNot | BoundOperator::Equal | BoundOperator::NotEqual
            | BoundOperator::Less | BoundOperator::LessEqual | BoundOperator::Greater | BoundOperator::GreaterEqual)
        {
            return None;
        }
        let role = crate::representation::type_representation(self.request, ty)?;
        let representation = role.integer_representation()?;
        let mut ranges = IntegerRanges::new();
        let bounds = operands.iter().map(|operand| self.integer_range(
            state, *operand, &mut ranges, &mut { ExecutionCondition::WORK_LIMIT }))
            .collect::<Option<Vec<_>>>();

        if let Some(bounds) = &bounds
            && bounds.iter().all(|(lower, upper)| lower == upper)
        {
            let kinds = bounds.iter().map(|(value, _)| crate::constant::from_big_integer(value.clone()))
                .collect::<Vec<_>>();
            let width = self.request.selected_target().machine().pointer_width_bits();
            let kind = match kinds.as_slice() {
                [value] if matches!(operator, BoundOperator::Subtract | BoundOperator::BitwiseNot) =>
                    Some(crate::constant::fold_machine_integer_unary(operator, value, role, width)),
                [value] if operator == BoundOperator::Add => Some(ConstantValueKind::Integer(value.clone())),
                [left, right] => crate::constant::fold_machine_integer_binary(
                    operator, left, right, representation, width),
                _ => None,
            };
            if let Some(kind) = kind {
                let values = self.request.semantic_values();
                let value = values.intern_constant_value(ConstantValueData::new(ty, kind))
                    .expect("machine observation retains a value in its semantic store");
                return Some(ExecutionCondition::Literal(values.constant_value_data(value)));
            }
        }

        let (minimum, maximum) = self.integer_type_range(ty)?;
        let exact = bounds.and_then(|bounds| match bounds.as_slice() {
            [(lower, upper)] if operator == BoundOperator::Add => Some((lower.clone(), upper.clone())),
            [(lower, upper)] if operator == BoundOperator::Subtract => Some((-upper, -lower)),
            [(lower, upper)] if operator == BoundOperator::BitwiseNot && minimum < BigInt::from(0) =>
                Some((-upper - 1, -lower - 1)),
            [left, right] => arithmetic_range(operator, left.clone(), right.clone()),
            _ => None,
        });
        if exact.is_some_and(|(lower, upper)| lower >= minimum && upper <= maximum) {
            let values = operands.iter().map(|operand| state.expressions.get(operand)
                .cloned().unwrap_or_else(|| self.value(state, *operand))).collect();
            Some(ExecutionCondition::operation(operator, values))
        } else {
            Some(if matches!(self.request.view().expression(expression), Some(BoundExpression::Assignment(_))) {
                ExecutionCondition::Unknown
            } else {
                ExecutionCondition::Expression(expression)
            })
        }
    }
}
