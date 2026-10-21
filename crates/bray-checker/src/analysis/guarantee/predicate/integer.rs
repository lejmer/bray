use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundOperator, SelectedOperation, SemanticSelection,
};
use bray_compiler_known::IntegerRepresentation;
use bray_symbols::{ConstantValueKind, TypeId};
use num_bigint::BigInt;

use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use super::arithmetic::arithmetic_range;
use crate::{CheckerRequestContext, ExecutionCondition};

pub(in crate::analysis::guarantee) type IntegerRanges =
    BTreeMap<BoundExpressionId, Option<(BigInt, BigInt)>>;

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn predicate_value_fits(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        target: TypeId,
        ranges: &mut IntegerRanges,
    ) -> bool {
        let Some((minimum, maximum)) = self.integer_type_range(target) else {
            return true;
        };

        self.integer_range(state, expression, ranges, &mut {
            ExecutionCondition::WORK_LIMIT
        })
        .is_some_and(|(lower, upper)| lower >= minimum && upper <= maximum)
    }

    pub(super) fn predicate_input_fits(
        &self,
        state: &ExecutionState,
        input: bray_bound_tree::BoundExecutionSite,
        target: TypeId,
        ranges: &mut IntegerRanges,
    ) -> bool {
        let Some((minimum, maximum)) = self.integer_type_range(target) else {
            return true;
        };
        let value = self.current_input(state, input);
        let range = self.observed_integer_range(state, &value, ranges,
            &mut { ExecutionCondition::WORK_LIMIT }).or_else(|| {
                // An opaque completed provider has its declared result type. Its
                // normal completion is certified independently at the input site.
                matches!(value, ExecutionCondition::Call(_, _) | ExecutionCondition::PostState(_, _))
                    .then(|| self.integer_type_range(target)).flatten()
            });
        range.is_some_and(|(lower, upper)| lower >= minimum && upper <= maximum)
    }

    fn observed_integer_range(
        &self,
        state: &ExecutionState,
        value: &ExecutionCondition,
        ranges: &mut IntegerRanges,
        budget: &mut usize,
    ) -> Option<(BigInt, BigInt)> {
        *budget = budget.checked_sub(1)?;
        let range = match value {
            ExecutionCondition::Literal(_) => integer_literal(value).map(|value| (value.clone(), value)),
            ExecutionCondition::Expression(expression)
                if self.request.unit().key().kind().uses_machine_arithmetic()
                    && state.expressions.get(expression) == Some(value) =>
                self.semantics.types().expression(*expression)
                    .and_then(|ty| self.integer_type_range(ty.ty())),
            ExecutionCondition::Expression(expression) => self.integer_range(state, *expression, ranges, budget),
            ExecutionCondition::Operation(operator, operands) => match operands.as_ref() {
                [operand] => {
                    let (lower, upper) = self.observed_integer_range(state, operand, ranges, budget)?;
                    match operator {
                        BoundOperator::Add => Some((lower, upper)),
                        BoundOperator::Subtract => Some((-upper, -lower)),
                        _ => None,
                    }
                }
                [left, right] => arithmetic_range(*operator,
                    self.observed_integer_range(state, left, ranges, budget)?,
                    self.observed_integer_range(state, right, ranges, budget)?),
                _ => None,
            },
            _ => self.observation_type(value).and_then(|ty| self.integer_type_range(ty)),
        };
        refine_integer_range(value, range, &state.assumptions)
    }

    pub(super) fn integer_condition(
        &self,
        value: BigInt,
        expression: BoundExpressionId,
    ) -> ExecutionCondition {
        let ty = self
            .semantics
            .types()
            .expression(expression)
            .expect("checked integer retains its type")
            .ty();

        let value = self
            .request
            .semantic_values()
            .intern_constant_value(bray_symbols::ConstantValueData::new(
                ty,
                ConstantValueKind::Integer(crate::constant::from_big_integer(value)),
            ))
            .expect("integer condition satisfies its checked scalar representation");

        ExecutionCondition::Literal(self.request.semantic_values().constant_value_data(value))
    }

    pub(in crate::analysis::guarantee) fn integer_type_range(&self, ty: TypeId) -> Option<(BigInt, BigInt)> {
        let representation = crate::representation::type_representation(self.request, ty)?
            .integer_representation()?;

        let (signed, width) = match representation {
            IntegerRepresentation::Signed(width) => (true, width),
            IntegerRepresentation::Unsigned(width) => (false, width),
            IntegerRepresentation::TargetSigned => (
                true,
                self.request
                    .selected_target()
                    .machine()
                    .pointer_width_bits()
                    .get(),
            ),
            IntegerRepresentation::TargetUnsigned => (
                false,
                self.request
                    .selected_target()
                    .machine()
                    .pointer_width_bits()
                    .get(),
            ),
        };

        let limit = BigInt::from(1) << usize::from(width - u16::from(signed));
        let minimum = if signed { -&limit } else { BigInt::from(0) };

        Some((minimum, limit - 1))
    }

    pub(in crate::analysis::guarantee) fn integer_range(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        ranges: &mut IntegerRanges,
        budget: &mut usize,
    ) -> Option<(BigInt, BigInt)> {
        *budget = budget.checked_sub(1)?;

        if let Some(range) = ranges.get(&expression) {
            // The caller owns bounds for this operation while the block cache retains its proof.
            return range.clone();
        }

        let value = self.value(state, expression);

        let mut range = if let Some(value) = integer_literal(&value) {
            Some((value.clone(), value))
        } else if self.request.unit().key().kind().uses_machine_arithmetic()
            && state.expressions.contains_key(&expression)
        {
            // The owning execution step already interpreted this operation.
            // Reuse its value snapshot instead of interpreting its source again.
            self.observed_integer_range(state, &value, ranges, budget)
        } else {
            self.operation_integer_range(state, expression, ranges, budget)
        };

        range = refine_integer_range(&value, range, &state.assumptions);

        // Each block retains the derived bounds so shared arithmetic is not traversed again.
        ranges.insert(expression, range.clone());

        range
    }

    fn operation_integer_range(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        ranges: &mut IntegerRanges,
        budget: &mut usize,
    ) -> Option<(BigInt, BigInt)> {
        let range = self.exact_operation_integer_range(state, expression, ranges, budget)?;
        if self.request.unit().key().kind().uses_machine_arithmetic()
            && matches!(self.semantics.selections().expression(expression),
                Some(SemanticSelection::Operation(SelectedOperation::Operator { .. })))
            && let Some(physical) = self.semantics.types().expression(expression)
                .and_then(|ty| self.integer_type_range(ty.ty()))
            && (range.0 < physical.0 || range.1 > physical.1)
        {
            return Some(physical);
        }
        Some(range)
    }

    fn exact_operation_integer_range(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        ranges: &mut IntegerRanges,
        budget: &mut usize,
    ) -> Option<(BigInt, BigInt)> {
        let ty = self.semantics.types().expression(expression)?.ty();
        let bound = self.request.view().expression(expression)?;

        if let Some(SemanticSelection::Operation(SelectedOperation::Operator {
            target: bray_bound_tree::OperatorTarget::BuiltIn(operator),
            ..
        })) = self.semantics.selections().expression(expression)
        {
            let operands = match bound {
                BoundExpression::Unary(operation) => operation.operands(),
                BoundExpression::Binary(operation) => operation.operands(),
                _ => unreachable!("built-in operation retains its expression"),
            };

            return match operands {
                [operand] => {
                    let (lower, upper) = self.integer_range(state, *operand, ranges, budget)?;

                    match operator {
                        BoundOperator::Add => Some((lower, upper)),
                        BoundOperator::Subtract => Some((-upper, -lower)),
                        BoundOperator::BitwiseNot => {
                            let physical = self.integer_type_range(ty)?;

                            (lower >= physical.0 && upper <= physical.1).then_some(physical)
                        }
                        _ => None,
                    }
                }
                [left, right] => {
                    let left_range = self.integer_range(state, *left, ranges, budget)?;
                    let right_range = self.integer_range(state, *right, ranges, budget)?;

                    if matches!(
                        operator,
                        BoundOperator::BitwiseAnd
                            | BoundOperator::BitwiseOr
                            | BoundOperator::BitwiseXor
                    ) {
                        let physical = self.integer_type_range(ty)?;

                        if left_range.0 >= physical.0
                            && left_range.1 <= physical.1
                            && right_range.0 >= physical.0
                            && right_range.1 <= physical.1
                        {
                            return Some(physical);
                        }
                    }

                    let mut range = arithmetic_range(*operator, left_range, right_range)?;

                    if *operator == BoundOperator::Subtract
                        && ExecutionCondition::operation(
                            BoundOperator::LessEqual,
                            vec![self.value(state, *right), self.value(state, *left)],
                        )
                        .prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                            == Some(true)
                    {
                        range.0 = std::cmp::max(range.0, BigInt::from(0));
                    }

                    Some(range)
                }
                _ => unreachable!("built-in operator retains its arity"),
            };
        }

        self.integer_type_range(ty)
    }
}

fn integer_literal(value: &ExecutionCondition) -> Option<BigInt> {
    let ExecutionCondition::Literal(value) = value else {
        return None;
    };

    let ConstantValueKind::Integer(value) = value.kind() else {
        return None;
    };

    Some(crate::constant::to_big_integer(value))
}

fn reverse_comparison(operator: BoundOperator) -> BoundOperator {
    match operator {
        BoundOperator::Less => BoundOperator::Greater,
        BoundOperator::LessEqual => BoundOperator::GreaterEqual,
        BoundOperator::Greater => BoundOperator::Less,
        BoundOperator::GreaterEqual => BoundOperator::LessEqual,
        operator => operator,
    }
}


fn refine_integer_range(
    value: &ExecutionCondition,
    mut range: Option<(BigInt, BigInt)>,
    assumptions: &std::collections::BTreeSet<(ExecutionCondition, bool)>,
) -> Option<(BigInt, BigInt)> {
        if let Some((lower, upper)) = &mut range {
            for (condition, holds) in assumptions {
                let ExecutionCondition::Operation(operator, operands) = condition else {
                    continue;
                };

                let [left, right] = operands.as_ref() else {
                    continue;
                };

                let (operator, bound) = if left == value {
                    (*operator, integer_literal(right))
                } else if right == value {
                    (reverse_comparison(*operator), integer_literal(left))
                } else {
                    continue;
                };

                let Some(bound) = bound else {
                    continue;
                };

                match (operator, holds) {
                    (BoundOperator::Equal, true) | (BoundOperator::NotEqual, false) => {
                        *lower = std::cmp::max(std::mem::take(lower), bound.clone());
                        *upper = std::cmp::min(std::mem::take(upper), bound);
                    }
                    (BoundOperator::NotEqual, true) | (BoundOperator::Equal, false)
                        if *lower == bound =>
                    {
                        *lower += 1
                    }
                    (BoundOperator::NotEqual, true) | (BoundOperator::Equal, false)
                        if *upper == bound =>
                    {
                        *upper -= 1
                    }
                    (BoundOperator::Less, true) | (BoundOperator::GreaterEqual, false) => {
                        *upper = std::cmp::min(std::mem::take(upper), bound - 1)
                    }
                    (BoundOperator::LessEqual, true) | (BoundOperator::Greater, false) => {
                        *upper = std::cmp::min(std::mem::take(upper), bound)
                    }
                    (BoundOperator::Greater, true) | (BoundOperator::LessEqual, false) => {
                        *lower = std::cmp::max(std::mem::take(lower), bound + 1)
                    }
                    (BoundOperator::GreaterEqual, true) | (BoundOperator::Less, false) => {
                        *lower = std::cmp::max(std::mem::take(lower), bound)
                    }
                    _ => {}
                }
            }

            if lower > upper {
                range = None;
            }
        }

    range
}
