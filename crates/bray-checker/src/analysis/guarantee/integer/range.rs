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
    pub(in crate::analysis::guarantee) fn integer_condition_truth(
        &self,
        state: &ExecutionState,
        condition: &ExecutionCondition,
        ranges: &mut IntegerRanges,
    ) -> Option<bool> {
        let ExecutionCondition::Operation(operator, operands) = condition else {
            return None;
        };

        if !matches!(
            operator,
            BoundOperator::Less
                | BoundOperator::LessEqual
                | BoundOperator::Greater
                | BoundOperator::GreaterEqual
                | BoundOperator::Equal
                | BoundOperator::NotEqual
        ) {
            return None;
        }

        let [left, right] = operands.as_ref() else {
            return None;
        };

        let (left_min, left_max) = self
            .observed_integer_range(state, left, ranges, &mut { ExecutionCondition::WORK_LIMIT })?;

        let (right_min, right_max) = self.observed_integer_range(state, right, ranges, &mut {
            ExecutionCondition::WORK_LIMIT
        })?;

        let (proven, disproven) = match operator {
            BoundOperator::Less => (left_max < right_min, left_min >= right_max),
            BoundOperator::LessEqual => (left_max <= right_min, left_min > right_max),
            BoundOperator::Greater => (left_min > right_max, left_max <= right_min),
            BoundOperator::GreaterEqual => (left_min >= right_max, left_max < right_min),
            BoundOperator::Equal | BoundOperator::NotEqual => {
                let equal = left_min == left_max && right_min == right_max && left_min == right_min;
                let disjoint = left_max < right_min || left_min > right_max;

                if *operator == BoundOperator::Equal {
                    (equal, disjoint)
                } else {
                    (disjoint, equal)
                }
            }
            _ => unreachable!("integer comparison operator was selected above"),
        };

        if proven {
            Some(true)
        } else if disproven {
            Some(false)
        } else {
            None
        }
    }

    pub(in crate::analysis::guarantee) fn observed_integer_range(
        &self,
        state: &ExecutionState,
        value: &ExecutionCondition,
        ranges: &mut IntegerRanges,
        budget: &mut usize,
    ) -> Option<(BigInt, BigInt)> {
        *budget = budget.checked_sub(1)?;

        let range = match value {
            ExecutionCondition::Literal(_) => {
                integer_constant(value, &mut { ExecutionCondition::WORK_LIMIT })
                    .map(|value| (value.clone(), value))
            }
            ExecutionCondition::Expression(expression)
                if self.request.unit().key().kind().uses_machine_arithmetic()
                    && state.expressions.get(expression) == Some(value) =>
            {
                self.semantics
                    .types()
                    .expression(*expression)
                    .and_then(|ty| self.integer_type_range(ty.ty()))
            }
            ExecutionCondition::Expression(expression) => {
                self.integer_range(state, *expression, ranges, budget)
            }
            ExecutionCondition::Operation(operator, operands) => match operands.as_ref() {
                [operand] => {
                    let (lower, upper) =
                        self.observed_integer_range(state, operand, ranges, budget)?;

                    match operator {
                        BoundOperator::Add => Some((lower, upper)),
                        BoundOperator::Subtract => Some((-upper, -lower)),
                        _ => None,
                    }
                }
                [left, right] => refined_arithmetic_range(
                    *operator,
                    left,
                    right,
                    self.observed_integer_range(state, left, ranges, budget)?,
                    self.observed_integer_range(state, right, ranges, budget)?,
                    &state.assumptions,
                    &|value| self.scalar_integer_range(state, value),
                ),
                _ => None,
            },
            _ => self
                .observation_type(value)
                .and_then(|ty| self.integer_type_range(ty)),
        };

        refine_integer_range(value, range, &state.assumptions, &|other| {
            self.scalar_integer_range(state, other)
        })
    }

    pub(in crate::analysis::guarantee) fn integer_condition(
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

    pub(in crate::analysis::guarantee) fn scalar_integer_range(
        &self,
        state: &ExecutionState,
        value: &ExecutionCondition,
    ) -> Option<(BigInt, BigInt)> {
        self.observation_type(value)
            .and_then(|ty| self.integer_type_range(ty))
            .or_else(|| {
                refine_integer_range(value, None, &state.assumptions, &|other| {
                    self.observation_type(other)
                        .and_then(|ty| self.integer_type_range(ty))
                })
            })
    }

    pub(in crate::analysis::guarantee) fn integer_type_range(
        &self,
        ty: TypeId,
    ) -> Option<(BigInt, BigInt)> {
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

        let mut range = if let Some(value) =
            integer_constant(&value, &mut { ExecutionCondition::WORK_LIMIT })
        {
            Some((value.clone(), value))
        } else if self.request.unit().key().kind().uses_machine_arithmetic()
            && state.expressions.contains_key(&expression)
        {
            // The owning execution step already interpreted this operation.
            // Reuse its value snapshot instead of interpreting its source again.
            self.observed_integer_range(state, &value, ranges, budget)
                .or_else(|| {
                    self.semantics
                        .types()
                        .expression(expression)
                        .and_then(|entry| self.integer_type_range(entry.ty()))
                })
        } else {
            self.operation_integer_range(state, expression, ranges, budget)
        };

        range = refine_integer_range(&value, range, &state.assumptions, &|other| {
            self.scalar_integer_range(state, other)
        });

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
            && matches!(
                self.semantics.selections().expression(expression),
                Some(SemanticSelection::Operation(
                    SelectedOperation::Operator { .. }
                ))
            )
            && let Some(physical) = self
                .semantics
                .types()
                .expression(expression)
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

                    refined_arithmetic_range(
                        *operator,
                        &self.value(state, *left),
                        &self.value(state, *right),
                        left_range,
                        right_range,
                        &state.assumptions,
                        &|value| self.scalar_integer_range(state, value),
                    )
                }
                _ => unreachable!("built-in operator retains its arity"),
            };
        }

        self.integer_type_range(ty)
    }
}

pub(in crate::analysis::guarantee) fn refined_arithmetic_range(
    operator: BoundOperator,
    left: &ExecutionCondition,
    right: &ExecutionCondition,
    left_range: (BigInt, BigInt),
    right_range: (BigInt, BigInt),
    assumptions: &std::collections::BTreeSet<(ExecutionCondition, bool)>,
    typed_range: &impl Fn(&ExecutionCondition) -> Option<(BigInt, BigInt)>,
) -> Option<(BigInt, BigInt)> {
    let nonnegative = left_range.0 >= BigInt::from(0) && right_range.0 >= BigInt::from(0);
    let mut range = arithmetic_range(operator, left_range, right_range)?;

    if operator == BoundOperator::Subtract
        && ExecutionCondition::operation(
            BoundOperator::LessEqual,
            vec![right.clone(), left.clone()],
        )
        .prove(assumptions, &mut { ExecutionCondition::WORK_LIMIT })
            == Some(true)
    {
        range.0 = range.0.max(BigInt::from(0));
    }

    if operator == BoundOperator::Add && nonnegative {
        let equalities = ExecutionCondition::equalities(assumptions, None);
        let left = left.with_equalities(&equalities);
        let right = right.with_equalities(&equalities);

        let known: std::collections::BTreeSet<_> = assumptions
            .iter()
            .map(|(condition, holds)| (condition.with_equalities(&equalities), *holds))
            .collect();

        let sum = ExecutionCondition::operation(operator, vec![left.clone(), right.clone()]);

        for (condition, holds) in &known {
            let ExecutionCondition::Operation(comparison, operands) = condition else {
                continue;
            };

            if !matches!(
                (comparison, holds),
                (BoundOperator::LessEqual, true) | (BoundOperator::Greater, false)
            ) {
                continue;
            }

            let [
                _,
                ExecutionCondition::Operation(BoundOperator::Subtract, difference),
            ] = operands.as_ref()
            else {
                continue;
            };

            let [limit, _] = difference.as_ref() else {
                continue;
            };

            let Some((minimum, maximum)) =
                integer_constant(limit, &mut { ExecutionCondition::WORK_LIMIT })
                    .map(|value| (value.clone(), value))
                    .or_else(|| typed_range(limit))
            else {
                continue;
            };

            let bound = ExecutionCondition::operation(
                BoundOperator::LessEqual,
                vec![sum.clone(), limit.clone()],
            );

            if minimum >= BigInt::from(0)
                && unsigned_arithmetic_is_bounded(&bound, &known, &|value| {
                    value == &left || value == &right || value == limit
                })
            {
                range.1 = range.1.min(maximum);
            }
        }
    }

    Some(range)
}

pub(in crate::analysis::guarantee) fn unsigned_arithmetic_is_bounded(
    condition: &ExecutionCondition,
    known: &std::collections::BTreeSet<(ExecutionCondition, bool)>,
    unsigned: &impl Fn(&ExecutionCondition) -> bool,
) -> bool {
    use bray_bound_tree::BoundOperator;

    let ExecutionCondition::Operation(BoundOperator::LessEqual, operands) = condition else {
        return false;
    };

    let [value, limit] = operands.as_ref() else {
        return false;
    };

    if let ExecutionCondition::Operation(BoundOperator::Subtract, difference) = value
        && let [end, start] = difference.as_ref()
        && [end, start, limit].into_iter().all(unsigned)
    {
        // These established bounds rule out underflow before using the difference.
        return [(start, end), (end, limit)]
            .into_iter()
            .all(|(left, right)| {
                ExecutionCondition::operation(
                    BoundOperator::LessEqual,
                    vec![left.clone(), right.clone()],
                )
                .prove(known, &mut { ExecutionCondition::WORK_LIMIT })
                    == Some(true)
            });
    }

    if let ExecutionCondition::Operation(BoundOperator::Multiply, factors) = value
        && let [left, right] = factors.as_ref()
        && [left, right, limit].into_iter().all(unsigned)
        && let ExecutionCondition::Literal(bound) = limit
    {
        let zero =
            ExecutionCondition::Literal(std::sync::Arc::new(bray_symbols::ConstantValueData::new(
                bound.ty(),
                ConstantValueKind::Integer(bray_symbols::IntegerConstant::from_u64(0)),
            )));

        // For nonzero unsigned a, a*b <= limit iff b <= floor(limit/a).
        // The rejected branch is short-circuited, so zero never requires division.
        return [(left, right), (right, left)]
            .into_iter()
            .any(|(factor, count)| {
                let nonzero = ExecutionCondition::operation(
                    BoundOperator::NotEqual,
                    vec![factor.clone(), zero.clone()],
                );

                let available = ExecutionCondition::operation(
                    BoundOperator::Divide,
                    vec![limit.clone(), factor.clone()],
                );

                let excess = ExecutionCondition::operation(
                    BoundOperator::Greater,
                    vec![count.clone(), available],
                );

                let rejected =
                    ExecutionCondition::operation(BoundOperator::LogicalAnd, vec![nonzero, excess]);

                rejected.prove(known, &mut { ExecutionCondition::WORK_LIMIT }) == Some(false)
            });
    }

    let ExecutionCondition::Operation(BoundOperator::Add, sum) = value else {
        return false;
    };

    let [left, right] = sum.as_ref() else {
        return false;
    };

    if ![left, right, limit].into_iter().all(unsigned) {
        return false;
    }

    // A bounded unsigned subtraction cannot wrap. The resulting spare-capacity
    // bound proves the sum without applying integer algebra to floating-point values.
    // Each comparison uses the existing indexed proof set, not another scan.
    [(left, right), (right, left)]
        .into_iter()
        .any(|(used, count)| {
            let available = ExecutionCondition::operation(
                BoundOperator::Subtract,
                vec![limit.clone(), used.clone()],
            );

            [
                ExecutionCondition::operation(
                    BoundOperator::LessEqual,
                    vec![used.clone(), limit.clone()],
                ),
                ExecutionCondition::operation(
                    BoundOperator::LessEqual,
                    vec![count.clone(), available],
                ),
            ]
            .iter()
            .all(|condition| {
                condition.prove(known, &mut { ExecutionCondition::WORK_LIMIT }) == Some(true)
            })
        })
}

fn integer_constant(value: &ExecutionCondition, budget: &mut usize) -> Option<BigInt> {
    *budget = budget.checked_sub(1)?;

    match value {
        ExecutionCondition::Literal(value) => match value.kind() {
            ConstantValueKind::Integer(value) => Some(crate::constant::to_big_integer(value)),
            _ => None,
        },
        ExecutionCondition::Operation(operator, operands) => match operands.as_ref() {
            [operand] => {
                let value = integer_constant(operand, budget)?;

                match operator {
                    BoundOperator::Add => Some(value),
                    BoundOperator::Subtract => Some(-value),
                    _ => None,
                }
            }
            [left, right] => {
                let left = integer_constant(left, budget)?;
                let right = integer_constant(right, budget)?;

                // Range evaluation owns both endpoints, so each exact bound is supplied twice.
                let (minimum, maximum) =
                    arithmetic_range(*operator, (left.clone(), left), (right.clone(), right))?;

                (minimum == maximum).then_some(minimum)
            }
            _ => None,
        },
        _ => None,
    }
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
    range: Option<(BigInt, BigInt)>,
    assumptions: &std::collections::BTreeSet<(ExecutionCondition, bool)>,
    typed_range: &impl Fn(&ExecutionCondition) -> Option<(BigInt, BigInt)>,
) -> Option<(BigInt, BigInt)> {
    let (mut lower, mut upper) =
        range.map_or((None, None), |(lower, upper)| (Some(lower), Some(upper)));

    for (condition, holds) in assumptions {
        let ExecutionCondition::Operation(operator, operands) = condition else {
            continue;
        };

        let [left, right] = operands.as_ref() else {
            continue;
        };

        let (operator, other) = if left == value {
            (*operator, right)
        } else if right == value {
            (reverse_comparison(*operator), left)
        } else {
            continue;
        };

        let bounds = integer_constant(other, &mut { ExecutionCondition::WORK_LIMIT })
            .map(|value| (value.clone(), value))
            .or_else(|| typed_range(other));

        let Some((minimum, maximum)) = bounds else {
            continue;
        };

        match (operator, holds) {
            (BoundOperator::Equal, true) | (BoundOperator::NotEqual, false) => {
                lower = Some(match lower {
                    Some(lower) => lower.max(minimum),
                    None => minimum,
                });

                upper = Some(match upper {
                    Some(upper) => upper.min(maximum),
                    None => maximum,
                });
            }
            (BoundOperator::NotEqual, true) | (BoundOperator::Equal, false)
                if minimum == maximum =>
            {
                if let Some(lower) = &mut lower
                    && *lower == minimum
                {
                    *lower += 1;
                }

                if let Some(upper) = &mut upper
                    && *upper == maximum
                {
                    *upper -= 1;
                }
            }
            (BoundOperator::Less, true) | (BoundOperator::GreaterEqual, false) => {
                upper = Some(match upper {
                    Some(upper) => upper.min(&maximum - 1),
                    None => maximum - 1,
                })
            }
            (BoundOperator::LessEqual, true) | (BoundOperator::Greater, false) => {
                upper = Some(match upper {
                    Some(upper) => upper.min(maximum),
                    None => maximum,
                })
            }
            (BoundOperator::Greater, true) | (BoundOperator::LessEqual, false) => {
                lower = Some(match lower {
                    Some(lower) => lower.max(&minimum + 1),
                    None => minimum + 1,
                })
            }
            (BoundOperator::GreaterEqual, true) | (BoundOperator::Less, false) => {
                lower = Some(match lower {
                    Some(lower) => lower.max(minimum),
                    None => minimum,
                })
            }
            _ => {}
        }
    }

    let (lower, upper) = (lower?, upper?);

    (lower <= upper).then_some((lower, upper))
}

#[cfg(test)]
mod tests {
    use super::refine_integer_range;
    use crate::{ExecutionCondition, ExecutionPlace};
    use bray_bound_tree::BoundOperator;
    use bray_symbols::SymbolOrdinal;
    use num_bigint::BigInt;
    use std::collections::BTreeSet;

    #[test]
    fn completed_integer_bounds_require_both_endpoints() {
        let value = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let other = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));

        let lower = ExecutionCondition::operation(
            BoundOperator::GreaterEqual,
            vec![value.clone(), other.clone()],
        );

        let upper = ExecutionCondition::operation(
            BoundOperator::LessEqual,
            vec![value.clone(), other.clone()],
        );

        let typed = |subject: &ExecutionCondition| {
            (subject == &other).then(|| (BigInt::from(1), BigInt::from(3)))
        };

        assert_eq!(
            refine_integer_range(
                &value,
                None,
                &BTreeSet::from([(lower.clone(), true)]),
                &typed
            ),
            None
        );

        assert_eq!(
            refine_integer_range(
                &value,
                None,
                &BTreeSet::from([(upper.clone(), true)]),
                &typed
            ),
            None
        );

        assert_eq!(
            refine_integer_range(
                &value,
                None,
                &BTreeSet::from([(lower, true), (upper, true)]),
                &typed
            ),
            Some((1.into(), 3.into()))
        );
    }

    #[test]
    fn typed_relational_bounds_cover_every_admitted_integer_pair() {
        let value = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(0)));
        let other = ExecutionCondition::Input(ExecutionPlace::argument(SymbolOrdinal::new(1)));

        for operator in [
            BoundOperator::Equal,
            BoundOperator::NotEqual,
            BoundOperator::Less,
            BoundOperator::LessEqual,
            BoundOperator::Greater,
            BoundOperator::GreaterEqual,
        ] {
            for holds in [false, true] {
                for (minimum, maximum) in [(-2, 2), (0, 0), (3, 3), (7, 9)] {
                    let condition =
                        ExecutionCondition::operation(operator, vec![value.clone(), other.clone()]);

                    let range = refine_integer_range(
                        &value,
                        Some(((-4).into(), 4.into())),
                        &BTreeSet::from([(condition, holds)]),
                        &|subject| (subject == &other).then(|| (minimum.into(), maximum.into())),
                    );

                    for left in -4..=4 {
                        for right in minimum..=maximum {
                            let actual = match operator {
                                BoundOperator::Equal => left == right,
                                BoundOperator::NotEqual => left != right,
                                BoundOperator::Less => left < right,
                                BoundOperator::LessEqual => left <= right,
                                BoundOperator::Greater => left > right,
                                BoundOperator::GreaterEqual => left >= right,
                                _ => unreachable!(),
                            };

                            if actual == holds {
                                assert!(
                                    range
                                        .as_ref()
                                        .is_some_and(|(low, high)| *low <= BigInt::from(left)
                                            && BigInt::from(left) <= *high),
                                    "{operator:?} {holds}, right [{minimum}, {maximum}], admitted {left}, {right}: {range:?}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
