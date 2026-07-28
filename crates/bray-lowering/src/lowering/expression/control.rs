use bray_bound_tree::{
    BoundControlTransferExpression, BoundControlTransferKind, BoundExpressionId, BoundOperator,
    BoundStructuredExpression, BoundStructuredExpressionKind,
};
use bray_ir::{
    MirBlockId, MirBlockKind, MirEdge, MirOperand, MirSourceAnchor, MirTerminatorKind,
};
use bray_symbols::{ConstantValueKind, TypeId};

use super::super::block::LoweredExpression;
use super::super::lowerer::{Lowerer, YieldTarget};
use super::super::LoweringError;

impl Lowerer<'_> {
    pub(super) fn lower_structured(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        match expression.kind() {
            BoundStructuredExpressionKind::Unit => {
                let ty = self.expression_type(id)?;
                let value = self.unit_operand(ty)?;

                Ok(LoweredExpression::continuing(
                    current,
                    Some(value),
                    self.source(expression.origin()),
                ))
            }
            BoundStructuredExpressionKind::Absence => {
                let ty = self.expression_type(id)?;
                let value = self.constant_operand(ty, ConstantValueKind::NullableAbsent)?;

                Ok(LoweredExpression::continuing(
                    current,
                    Some(value),
                    self.source(expression.origin()),
                ))
            }
            BoundStructuredExpressionKind::Conditional => {
                self.lower_conditional(id, expression, current)
            }
            _ => Err(LoweringError::UnsupportedExpression(id)),
        }
    }

    fn lower_conditional(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let [condition] = expression.operands() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let condition = self.lower_expression(*condition, current)?;

        let Some(current) = condition.block else {
            return Ok(condition);
        };

        let Some(condition) = condition.value else {
            return Err(LoweringError::MissingOperationResult(
                expression.operands()[0],
            ));
        };

        let ([then_block] | [then_block, _]) = expression.blocks() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let source = self.source(expression.origin());

        let then_entry = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let else_entry = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let join = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let ty = self.expression_type(id)?;

        let result = self
            .builder
            .push_block_parameter(join, Self::retained_source(&source), ty)?;

        self.builder.set_terminator(
            current,
            Self::retained_source(&source),
            MirTerminatorKind::Branch {
                condition,
                then_edge: MirEdge::new(then_entry, []),
                else_edge: MirEdge::new(else_entry, []),
            },
        )?;

        self.yield_targets.push(YieldTarget {
            syntax: expression.origin().source_anchor().syntax(),
            block: join,
            result_type: ty,
        });

        let then_completion = self.lower_block(*then_block, then_entry)?;
        self.finish_conditional_branch(then_completion, join, ty)?;

        let else_completion = match expression.blocks().get(1).copied() {
            Some(block) => self.lower_block(block, else_entry)?,
            None => LoweredExpression::continuing(
                else_entry,
                Some(self.unit_operand(ty)?),
                Self::retained_source(&source),
            ),
        };

        self.finish_conditional_branch(else_completion, join, ty)?;

        self.yield_targets.pop();

        Ok(LoweredExpression::continuing(
            join,
            Some(MirOperand::Value(result)),
            source,
        ))
    }

    fn finish_conditional_branch(
        &mut self,
        completion: LoweredExpression,
        join: MirBlockId,
        result_type: TypeId,
    ) -> Result<(), LoweringError> {
        let Some(block) = completion.block else {
            return Ok(());
        };

        let value = match completion.value {
            Some(value) => value,
            None => self.unit_operand(result_type)?,
        };

        self.builder.set_terminator(
            block,
            completion.source,
            MirTerminatorKind::Goto(MirEdge::new(join, [value])),
        )?;

        Ok(())
    }

    pub(super) fn lower_short_circuit(
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

        // The short edge and branch condition both own the checked left operand.
        let short_edge = MirEdge::new(join, [left.clone()]);
        let right_edge = MirEdge::new(right_entry, []);

        let (then_edge, else_edge) = match operator {
            BoundOperator::LogicalOr => (short_edge, right_edge),
            BoundOperator::LogicalAnd => (right_edge, short_edge),
            _ => return Err(LoweringError::UnsupportedOperator(operator)),
        };

        self.builder.set_terminator(
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

            self.builder.set_terminator(
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

    pub(super) fn lower_control_transfer(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundControlTransferExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let source = self.source(expression.origin());

        let value = match expression.operand() {
            Some(operand) => {
                let lowered = self.lower_expression(operand, current)?;

                let Some(current) = lowered.block else {
                    return Ok(lowered);
                };

                let Some(value) = lowered.value else {
                    return Err(LoweringError::MissingOperationResult(operand));
                };

                return self.finish_control_transfer(id, expression, current, source, Some(value));
            }
            None => None,
        };

        self.finish_control_transfer(id, expression, current, source, value)
    }

    fn finish_control_transfer(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundControlTransferExpression,
        current: MirBlockId,
        source: MirSourceAnchor,
        value: Option<MirOperand>,
    ) -> Result<LoweredExpression, LoweringError> {
        let terminator = match expression.kind() {
            BoundControlTransferKind::Return => MirTerminatorKind::Return(value),
            BoundControlTransferKind::Yield => {
                let target = match expression.target() {
                    Some(syntax) => self
                        .yield_targets
                        .iter()
                        .rev()
                        .find(|target| target.syntax == syntax),
                    None => self.yield_targets.last(),
                }
                .ok_or(LoweringError::UnsupportedExpression(id))?;

                let block = target.block;
                let result_type = target.result_type;

                let value = match value {
                    Some(value) => value,
                    None => self.unit_operand(result_type)?,
                };

                MirTerminatorKind::Goto(MirEdge::new(block, [value]))
            }
            BoundControlTransferKind::Break | BoundControlTransferKind::Continue => {
                return Err(LoweringError::UnsupportedExpression(id));
            }
        };

        self.builder
            .set_terminator(current, Self::retained_source(&source), terminator)?;

        Ok(LoweredExpression::terminated(source))
    }
}
