use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundOperator, OperatorTarget, SelectedOperation,
    SemanticSelection,
};
use bray_symbols::{ConstantValueData, ConstantValueKind};
use num_bigint::BigInt;

use super::super::integer::{IntegerRanges, refined_arithmetic_range};
use super::super::state::ExecutionState;
use super::model::ExecutionFlowDomain;
use crate::{CheckerRequestContext, ExecutionCondition};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis::guarantee) fn preserve_integer_entry_conditions(
        &self,
        state: &ExecutionState,
        conditions: &[(ExecutionCondition, bray_symbols::PredicateDomain)],
        entry: &mut crate::ExecutionCallEvidence,
    ) {
        let mut ranges = IntegerRanges::new();

        for (condition, _) in conditions {
            let condition = condition.substitute(
                &|place| {
                    place
                        .value_in(&entry.arguments)
                        .unwrap_or(ExecutionCondition::Unknown)
                },
                &ExecutionCondition::Unknown,
                &mut { ExecutionCondition::WORK_LIMIT },
            );

            let mut pending = vec![condition];
            let mut budget = ExecutionCondition::WORK_LIMIT;

            while let Some(condition) = pending.pop() {
                let Some(remaining) = budget.checked_sub(1) else {
                    break;
                };

                budget = remaining;

                if let ExecutionCondition::Operation(
                    BoundOperator::LogicalAnd
                    | BoundOperator::LogicalOr
                    | BoundOperator::LogicalNot,
                    operands,
                ) = &condition
                {
                    pending.extend(operands.iter().cloned());
                } else if let Some(value) =
                    self.integer_condition_truth(state, &condition, &mut ranges)
                {
                    // Publish an established entry proof before the invocation can mutate storage.
                    // Later consumers use the same captured conditions as other call evidence.
                    condition.assume(value, &mut entry.assumptions);
                }
            }
        }
    }

    pub(super) fn preserve_integer_value(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
    ) {
        let (ty, component) = match self.semantics.selections().expression(expression) {
            Some(SemanticSelection::Call(call)) => match call.resolution().result() {
                bray_bound_tree::BoundCallResult::Immediate(ty) => (Some(ty), false),
                _ => (None, false),
            },
            _ if matches!(
                self.request.view().expression(expression),
                Some(
                    BoundExpression::Name(_)
                        | BoundExpression::PatternReference(_)
                        | BoundExpression::MemberAccess(_)
                )
            ) =>
            {
                (
                    self.semantics
                        .types()
                        .expression(expression)
                        .map(|entry| entry.ty()),
                    true,
                )
            }
            _ => (None, false),
        };

        if let Some((minimum, maximum)) = ty.and_then(|ty| self.integer_type_range(ty)) {
            let value = self.value(state, expression);

            if component && !matches!(value, ExecutionCondition::Projection(..)) {
                return;
            }

            // Normally returned scalars and reads of their components retain their checked
            // representation. Do not assume bounds for an unevaluated arithmetic result.

            for (operator, bound) in [
                (bray_bound_tree::BoundOperator::GreaterEqual, minimum),
                (bray_bound_tree::BoundOperator::LessEqual, maximum),
            ] {
                ExecutionCondition::operation(
                    operator,
                    vec![value.clone(), self.integer_condition(bound, expression)],
                )
                .assume(true, &mut state.assumptions);
            }
        }
    }

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
            (
                BoundExpression::Unary(operation),
                SemanticSelection::Operation(SelectedOperation::Operator {
                    target: OperatorTarget::BuiltIn(operator),
                    ..
                }),
            ) => (
                *operator,
                operation.operands(),
                self.semantics.types().expression(expression)?.ty(),
            ),
            (
                BoundExpression::Binary(operation),
                SemanticSelection::Operation(SelectedOperation::Operator {
                    target: OperatorTarget::BuiltIn(operator),
                    ..
                }),
            ) => (
                *operator,
                operation.operands(),
                self.semantics.types().expression(expression)?.ty(),
            ),
            (
                BoundExpression::Assignment(operation),
                SemanticSelection::Operation(SelectedOperation::CompoundAssignment(selected)),
            ) => {
                let OperatorTarget::BuiltIn(operator) = selected.target() else {
                    return None;
                };

                (operator, operation.operands(), selected.value_type())
            }
            _ => return None,
        };

        if matches!(
            operator,
            BoundOperator::LogicalAnd
                | BoundOperator::LogicalOr
                | BoundOperator::LogicalNot
                | BoundOperator::Equal
                | BoundOperator::NotEqual
                | BoundOperator::Less
                | BoundOperator::LessEqual
                | BoundOperator::Greater
                | BoundOperator::GreaterEqual
        ) {
            return None;
        }

        let role = crate::representation::type_representation(self.request, ty)?;
        let representation = role.integer_representation()?;
        let mut ranges = IntegerRanges::new();

        let bounds = operands
            .iter()
            .map(|operand| {
                self.integer_range(state, *operand, &mut ranges, &mut {
                    ExecutionCondition::WORK_LIMIT
                })
            })
            .collect::<Option<Vec<_>>>();

        if let Some(bounds) = &bounds
            && bounds.iter().all(|(lower, upper)| lower == upper)
        {
            let kinds = bounds
                .iter()
                .map(|(value, _)| crate::constant::from_big_integer(value.clone()))
                .collect::<Vec<_>>();

            let width = self
                .request
                .selected_target()
                .machine()
                .pointer_width_bits();

            // A signed minimum literal is one checked source value. Its positive spelling
            // is not an intermediate machine operand of the selected signed type.
            let signed_literal = match kinds.as_slice() {
                [value]
                    if operator == BoundOperator::Subtract
                        && matches!(
                            self.request.view().expression(operands[0]),
                            Some(BoundExpression::Literal(_))
                        )
                        && !crate::constant::fits_integer_representation(
                            value,
                            representation,
                            || width,
                        )
                        && crate::constant::fits_integer_representation(
                            &value.negated(),
                            representation,
                            || width,
                        ) =>
                {
                    Some(ConstantValueKind::Integer(value.negated()))
                }
                _ => None,
            };

            let kind = signed_literal.or_else(|| match kinds.as_slice() {
                [value]
                    if matches!(
                        operator,
                        BoundOperator::Subtract | BoundOperator::BitwiseNot
                    ) =>
                {
                    Some(crate::constant::fold_machine_integer_unary(
                        operator, value, role, width,
                    ))
                }
                [value] if operator == BoundOperator::Add => {
                    Some(ConstantValueKind::Integer(value.clone()))
                }
                [left, right] => crate::constant::fold_machine_integer_binary(
                    operator,
                    left,
                    right,
                    representation,
                    width,
                ),
                _ => None,
            });

            if let Some(kind) = kind {
                let values = self.request.semantic_values();

                let value = values
                    .intern_constant_value(ConstantValueData::new(ty, kind))
                    .expect("machine observation retains a value in its semantic store");

                return Some(ExecutionCondition::Literal(
                    values.constant_value_data(value),
                ));
            }
        }

        let (minimum, maximum) = self.integer_type_range(ty)?;

        if operator == BoundOperator::Multiply && minimum == BigInt::from(0) {
            let product = ExecutionCondition::operation(
                operator,
                operands
                    .iter()
                    .map(|operand| self.value(state, *operand))
                    .collect(),
            );

            let bound = ExecutionCondition::operation(
                BoundOperator::LessEqual,
                vec![
                    product.clone(),
                    self.integer_condition(maximum.clone(), expression),
                ],
            );

            if super::super::integer::unsigned_arithmetic_is_bounded(
                &bound,
                &state.assumptions,
                &|value| {
                    self.unsigned_value(value)
                        || operands.iter().any(|operand| {
                            // The captured value keeps the checked operand's unsigned type,
                            // including deterministic call identities without an embedded type.
                            self.unsigned_value(&ExecutionCondition::Expression(*operand))
                                && self.value(state, *operand) == *value
                        })
                },
            ) {
                return Some(product);
            }
        }

        let exact = bounds.and_then(|bounds| match bounds.as_slice() {
            [(lower, upper)] if operator == BoundOperator::Add => {
                Some((lower.clone(), upper.clone()))
            }
            [(lower, upper)] if operator == BoundOperator::Subtract => Some((-upper, -lower)),
            [(lower, upper)]
                if operator == BoundOperator::BitwiseNot && minimum < BigInt::from(0) =>
            {
                Some((-upper - 1, -lower - 1))
            }
            [left, right] => refined_arithmetic_range(
                operator,
                &self.value(state, operands[0]),
                &self.value(state, operands[1]),
                left.clone(),
                right.clone(),
                &state.assumptions,
                &|value| self.scalar_integer_range(state, value),
            ),
            _ => None,
        });

        if exact.is_some_and(|(lower, upper)| lower >= minimum && upper <= maximum) {
            let values = operands
                .iter()
                .map(|operand| {
                    state
                        .expressions
                        .get(operand)
                        .cloned()
                        .unwrap_or_else(|| self.value(state, *operand))
                })
                .collect();

            Some(ExecutionCondition::operation(operator, values))
        } else {
            Some(
                if matches!(
                    self.request.view().expression(expression),
                    Some(BoundExpression::Assignment(_))
                ) {
                    ExecutionCondition::Unknown
                } else {
                    ExecutionCondition::Expression(expression)
                },
            )
        }
    }
}
