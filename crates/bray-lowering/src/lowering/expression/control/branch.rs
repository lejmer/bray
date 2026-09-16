use bray_bound_tree::{
    BoundExpressionId, BoundOperator, BoundStructuredExpression, BoundStructuredExpressionKind,
};
use bray_ir::{
    MirBlockId, MirBlockKind, MirEdge, MirImmediateValue, MirOperand, MirTerminatorKind,
};

use super::super::super::LoweringError;
use super::super::super::block::LoweredExpression;
use super::super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(in crate::lowering::expression) fn lower_structured(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        match expression.kind() {
            BoundStructuredExpressionKind::Unit => {
                let ty = self.expression_type(id)?;
                let value = self.unit_operand(ty);

                Ok(LoweredExpression::continuing(
                    current,
                    Some(value),
                    self.source(expression.origin()),
                ))
            }
            BoundStructuredExpressionKind::Absence => {
                let ty = self.expression_type(id)?;
                let value = Self::immediate_operand(ty, MirImmediateValue::NullableAbsent);

                Ok(LoweredExpression::continuing(
                    current,
                    Some(value),
                    self.source(expression.origin()),
                ))
            }
            BoundStructuredExpressionKind::Conditional => {
                self.lower_conditional(id, expression, current)
            }
            BoundStructuredExpressionKind::PatternTest => {
                self.lower_pattern_test(id, expression, current)
            }
            BoundStructuredExpressionKind::Condition
            | BoundStructuredExpressionKind::PatternBinding => {
                Err(LoweringError::UnsupportedExpression(id))
            }
            BoundStructuredExpressionKind::While => self.lower_while(id, expression, current),
            BoundStructuredExpressionKind::Loop => self.lower_loop(id, expression, current),
            BoundStructuredExpressionKind::BooleanAllFold
            | BoundStructuredExpressionKind::BooleanAnyFold => {
                self.lower_boolean_fold(id, expression, current)
            }
            BoundStructuredExpressionKind::Tuple
            | BoundStructuredExpressionKind::Array
            | BoundStructuredExpressionKind::RepeatedArray
            | BoundStructuredExpressionKind::Range => self.lower_aggregate(id, expression, current),
            BoundStructuredExpressionKind::ArrayGenerator => self.lower_generator_region(
                id,
                expression,
                current,
                bray_ir::MirGeneratorKind::Array,
            ),
            BoundStructuredExpressionKind::GeneralGenerator => self.lower_generator_region(
                id,
                expression,
                current,
                bray_ir::MirGeneratorKind::General,
            ),
            BoundStructuredExpressionKind::ElementIndex
            | BoundStructuredExpressionKind::SliceIndex => self.lower_index(id, current),
            BoundStructuredExpressionKind::Borrow => self.lower_borrow(id, expression, current),
            BoundStructuredExpressionKind::NullablePropagation => {
                self.lower_nullable_propagation(id, expression, current)
            }
            BoundStructuredExpressionKind::ResultPropagation => {
                self.lower_result_or_run_result_propagation(id, expression, current)
            }
            BoundStructuredExpressionKind::TrustBoundary => {
                self.lower_trust_boundary(id, expression, current)
            }
            BoundStructuredExpressionKind::Assertion => {
                self.lower_assertion(id, expression, current)
            }
            BoundStructuredExpressionKind::Catch => self.lower_catch(id, expression, current),
            BoundStructuredExpressionKind::Panic => self.lower_panic(id, expression, current),
            BoundStructuredExpressionKind::With => self.lower_with(id, expression, current),
        }
    }

    fn lower_conditional(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let conditions = expression.operands();
        let blocks = expression.blocks();

        if conditions.is_empty()
            || (blocks.len() != conditions.len() && blocks.len() != conditions.len() + 1)
        {
            return Err(LoweringError::UnsupportedExpression(id));
        }

        let mut current = current;

        let source = self.source(expression.origin());

        let (join, result, ty) = self.push_result_join(id, expression.origin())?;

        for (index, then_block) in blocks.iter().take(conditions.len()).enumerate() {
            let depth = self.active_scopes.len();
            let scope = self.begin_condition_scope(conditions[index])?;

            let then_entry = self
                .builder
                .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

            let else_entry = self
                .builder
                .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

            if !self.lower_condition(conditions[index], current, then_entry, else_entry)? {
                self.finish_unreachable_blocks(&[then_entry, else_entry], &source)?;

                if scope.is_some() {
                    self.active_scopes.pop();
                }

                return if index == 0 {
                    self.finish_unreachable_blocks(&[join], &source)?;

                    Ok(LoweredExpression::terminated(source))
                } else {
                    Ok(LoweredExpression::continuing(
                        join,
                        Some(MirOperand::Value(result)),
                        source,
                    ))
                };
            }

            let mut then_completion = if self.builder.is_reachable(current, then_entry) {
                self.lower_yielding_block(*then_block, then_entry, join, ty, depth)?
            } else {
                self.finish_unreachable_blocks(&[then_entry], &source)?;

                LoweredExpression::terminated(Self::retained_source(&source))
            };

            if let (Some(scope), Some(block)) = (scope, then_completion.block) {
                then_completion.block =
                    Some(self.finish_scope(scope, block, &source, (*then_block).into())?);
            }

            self.finish_result_edge(then_completion, join, ty)?;

            current = if let Some(scope) = scope {
                let completion =
                    self.finish_scope(scope, else_entry, &source, conditions[index].into())?;

                self.active_scopes.pop();

                completion
            } else {
                else_entry
            };
        }

        let else_completion = match blocks.get(conditions.len()).copied() {
            Some(block) => {
                self.lower_yielding_block(block, current, join, ty, self.active_scopes.len())?
            }
            None => LoweredExpression::continuing(
                current,
                Some(self.unit_operand(ty)),
                Self::retained_source(&source),
            ),
        };

        self.finish_result_edge(else_completion, join, ty)?;

        Ok(LoweredExpression::continuing(
            join,
            Some(MirOperand::Value(result)),
            source,
        ))
    }

    pub(in crate::lowering::expression) fn lower_short_circuit(
        &mut self,
        id: BoundExpressionId,
        operator: BoundOperator,
        operands: &[BoundExpressionId],
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let [left_id, right_id] = operands else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let left = self.lower_expression(*left_id, current)?;

        let Some(current) = left.block else {
            return Ok(left);
        };

        let Some(left) = left.value else {
            return Err(LoweringError::MissingOperationResult(*left_id));
        };

        let source = self.expression_source(id)?;

        let right_entry = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let join = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let ty = self.expression_type(id)?;

        let result = self
            .builder
            .push_block_parameter(join, Self::retained_source(&source), ty)?;

        let short_edge = MirEdge::new(join, [Self::retained_operand(&left)]);
        let right_edge = MirEdge::new(right_entry, []);

        let (then_edge, else_edge) = match operator {
            BoundOperator::LogicalOr => (short_edge, right_edge),
            BoundOperator::LogicalAnd => (right_edge, short_edge),
            _ => {
                return Err(LoweringError::UnsupportedOperator {
                    expression: id,
                    operator,
                });
            }
        };

        self.set_terminator(
            current,
            Self::retained_source(&source),
            MirTerminatorKind::Branch {
                condition: left,
                then_edge,
                else_edge,
            },
        )?;

        let right = self.lower_expression(*right_id, right_entry)?;

        if let Some(right_block) = right.block {
            let Some(right) = right.value else {
                return Err(LoweringError::MissingOperationResult(*right_id));
            };

            self.set_terminator(
                right_block,
                Self::retained_source(&source),
                MirTerminatorKind::Goto(MirEdge::new(join, [right])),
            )?;
        }

        Ok(LoweredExpression::continuing(
            join,
            Some(MirOperand::Value(result)),
            source,
        ))
    }
}
