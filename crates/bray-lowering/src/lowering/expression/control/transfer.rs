use bray_bound_tree::{
    BoundControlTransferExpression, BoundControlTransferKind, BoundExpressionId,
};
use bray_ir::{MirBlockId, MirGeneratorOperation, MirOperand, MirOperationKind, MirSourceAnchor};

use super::super::super::LoweringError;
use super::super::super::block::LoweredExpression;
use super::super::super::lowerer::{Lowerer, YieldTarget};

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
        if expression.kind() == BoundControlTransferKind::Yield {
            return self.finish_yield(id, expression, current, source, value);
        }

        match expression.kind() {
            BoundControlTransferKind::Return => {
                let value = match (value, expression.operand()) {
                    (Some(value), Some(operand)) => {
                        let operand_type = self.expression_type(operand)?;

                        let value = match self.input.expression_types().callable_result_type() {
                            Some(result_type) => self.adapt_nullable_present(
                                operand,
                                current,
                                Self::retained_source(&source),
                                value,
                                operand_type,
                                result_type,
                            )?,
                            None => (value, operand_type),
                        };

                        Some(value)
                    }
                    (None, None) => None,
                    _ => return Err(LoweringError::UnsupportedExpression(id)),
                };

                self.finish_return(current, &source, value, id.into())?;
            }
            BoundControlTransferKind::Break => {
                let (target, result_type, scope_depth) = {
                    let target = self.loop_target(id, expression)?;

                    (target.break_block, target.result_type, target.scope_depth)
                };

                let value = self.adapt_control_transfer_value(
                    expression,
                    current,
                    &source,
                    value,
                    result_type,
                )?;

                self.finish_exit_to_block(
                    current,
                    &source,
                    scope_depth,
                    target,
                    Some((value, result_type)),
                    id.into(),
                )?;
            }
            BoundControlTransferKind::Continue => {
                let (target, scope_depth) = {
                    let target = self.loop_target(id, expression)?;

                    (target.continue_block, target.scope_depth)
                };

                self.finish_exit_to_block(current, &source, scope_depth, target, None, id.into())?;
            }
            BoundControlTransferKind::Yield => {
                return Err(LoweringError::UnsupportedExpression(id));
            }
        }

        Ok(LoweredExpression::terminated(source))
    }

    fn finish_yield(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundControlTransferExpression,
        current: MirBlockId,
        source: MirSourceAnchor,
        value: Option<MirOperand>,
    ) -> Result<LoweredExpression, LoweringError> {
        let target = match expression.target() {
            Some(syntax) => self
                .yield_targets
                .iter()
                .rev()
                .find(|target| target.syntax() == syntax),
            None => self.yield_targets.last(),
        }
        // Lowering mutates the MIR builder after releasing the target-stack borrow.
        .cloned()
        .ok_or(LoweringError::UnsupportedExpression(id))?;

        match target {
            YieldTarget::Result {
                block,
                result_type,
                scope_depth,
                ..
            } => {
                let value = self.adapt_control_transfer_value(
                    expression,
                    current,
                    &source,
                    value,
                    result_type,
                )?;

                self.finish_exit_to_block(
                    current,
                    &source,
                    scope_depth,
                    block,
                    Some((value, result_type)),
                    id.into(),
                )?;

                Ok(LoweredExpression::terminated(source))
            }
            YieldTarget::Generator {
                destination,
                element_type,
                ..
            } => {
                let value = self.adapt_control_transfer_value(
                    expression,
                    current,
                    &source,
                    value,
                    element_type,
                )?;

                self.push_operation(
                    current,
                    Self::retained_source(&source),
                    MirOperationKind::Generator(MirGeneratorOperation::Push { destination, value }),
                    None,
                )?;

                let result = self.unit_operand(self.expression_type(id)?);

                Ok(LoweredExpression::continuing(current, Some(result), source))
            }
        }
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

    fn adapt_control_transfer_value(
        &mut self,
        expression: &BoundControlTransferExpression,
        current: MirBlockId,
        source: &MirSourceAnchor,
        value: Option<MirOperand>,
        destination_type: bray_symbols::TypeId,
    ) -> Result<MirOperand, LoweringError> {
        let (Some(value), Some(operand)) = (value, expression.operand()) else {
            return Ok(self.unit_operand(destination_type));
        };

        let operand_type = self.expression_type(operand)?;

        self.adapt_nullable_present(
            operand,
            current,
            Self::retained_source(source),
            value,
            operand_type,
            destination_type,
        )
        .map(|(value, _)| value)
    }
}
