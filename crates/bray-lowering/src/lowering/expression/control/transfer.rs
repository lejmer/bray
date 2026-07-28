use bray_bound_tree::{
    BoundControlTransferExpression, BoundControlTransferKind, BoundExpressionId,
};
use bray_ir::{MirBlockId, MirEdge, MirOperand, MirSourceAnchor, MirTerminatorKind};

use super::super::super::LoweringError;
use super::super::super::block::LoweredExpression;
use super::super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(in crate::lowering::expression) fn lower_control_transfer(
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
            BoundControlTransferKind::Yield => self.yield_terminator(id, expression, value)?,
            BoundControlTransferKind::Break => self.break_terminator(id, expression, value)?,
            BoundControlTransferKind::Continue => {
                let target = self.loop_target(id, expression)?;

                MirTerminatorKind::Goto(MirEdge::new(target.continue_block, []))
            }
        };

        self.builder
            .set_terminator(current, Self::retained_source(&source), terminator)?;

        Ok(LoweredExpression::terminated(source))
    }

    fn yield_terminator(
        &self,
        id: BoundExpressionId,
        expression: &BoundControlTransferExpression,
        value: Option<MirOperand>,
    ) -> Result<MirTerminatorKind, LoweringError> {
        let target = match expression.target() {
            Some(syntax) => self
                .yield_targets
                .iter()
                .rev()
                .find(|target| target.syntax == syntax),
            None => self.yield_targets.last(),
        }
        .ok_or(LoweringError::UnsupportedExpression(id))?;

        let value = value.unwrap_or_else(|| self.unit_operand(target.result_type));

        Ok(MirTerminatorKind::Goto(MirEdge::new(target.block, [value])))
    }

    fn break_terminator(
        &self,
        id: BoundExpressionId,
        expression: &BoundControlTransferExpression,
        value: Option<MirOperand>,
    ) -> Result<MirTerminatorKind, LoweringError> {
        let target = self.loop_target(id, expression)?;
        let value = value.unwrap_or_else(|| self.unit_operand(target.result_type));

        Ok(MirTerminatorKind::Goto(MirEdge::new(
            target.break_block,
            [value],
        )))
    }

    fn loop_target(
        &self,
        id: BoundExpressionId,
        expression: &BoundControlTransferExpression,
    ) -> Result<&super::super::super::lowerer::LoopTarget, LoweringError> {
        match expression.target() {
            Some(syntax) => self
                .loop_targets
                .iter()
                .rev()
                .find(|target| target.syntax == syntax),
            None => self.loop_targets.last(),
        }
        .ok_or(LoweringError::UnsupportedExpression(id))
    }
}
