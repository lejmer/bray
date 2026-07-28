use bray_bound_tree::{
    BoundExpressionId, BoundStructuredExpression, ConstructionInputId, ConstructionTarget,
};
use bray_ir::{
    MirBlockId, MirBlockKind, MirConstruction, MirConstructionInput, MirEdge, MirOperand,
    MirOperationKind, MirPanicCause, MirTerminatorKind,
};
use bray_symbols::TypeId;

use super::super::super::LoweringError;
use super::super::super::block::LoweredExpression;
use super::super::super::lowerer::{CatchTarget, Lowerer};

impl Lowerer<'_> {
    pub(super) fn lower_assertion(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let Some(condition_id) = expression.operands().first().copied() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let condition = self.lower_expression(condition_id, current)?;

        let Some(current) = condition.block else {
            return Ok(condition);
        };

        let Some(condition) = condition.value else {
            return Err(LoweringError::MissingOperationResult(condition_id));
        };

        let source = self.source(expression.origin());

        let success = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let failure = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        self.builder.set_terminator(
            current,
            Self::retained_source(&source),
            MirTerminatorKind::Branch {
                condition,
                then_edge: MirEdge::new(success, []),
                else_edge: MirEdge::new(failure, []),
            },
        )?;

        let failure_cause = match expression.operands().get(1).copied() {
            Some(message) => {
                let lowered = self.lower_expression(message, failure)?;

                let Some(failure) = lowered.block else {
                    return Ok(LoweredExpression::continuing(
                        success,
                        Some(self.unit_operand(self.expression_type(id)?)),
                        source,
                    ));
                };

                let Some(message) = lowered.value else {
                    return Err(LoweringError::MissingOperationResult(message));
                };

                Some((failure, Some(message)))
            }
            None => Some((failure, None)),
        };

        if let Some((failure, message)) = failure_cause {
            let report_type = self.panic_report_type()?;

            let report = self.push_panic_report(
                id,
                failure,
                &source,
                MirPanicCause::Assertion(message),
                report_type,
            )?;

            self.finish_panic_to_active_catch(failure, &source, report, report_type)?;
        }

        let result_type = self.expression_type(id)?;

        Ok(LoweredExpression::continuing(
            success,
            Some(self.unit_operand(result_type)),
            source,
        ))
    }

    pub(super) fn lower_panic(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let [message_id] = expression.operands() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let message = self.lower_expression(*message_id, current)?;

        let Some(current) = message.block else {
            return Ok(message);
        };

        let Some(message) = message.value else {
            return Err(LoweringError::MissingOperationResult(*message_id));
        };

        let source = self.source(expression.origin());
        let report_type = self.panic_report_type()?;

        let report = self.push_panic_report(
            id,
            current,
            &source,
            MirPanicCause::Message(message),
            report_type,
        )?;

        self.finish_panic_to_active_catch(current, &source, report, report_type)?;

        Ok(LoweredExpression::terminated(source))
    }

    pub(super) fn lower_catch(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let result_type = self.expression_type(id)?;
        let arguments = self.named_type_arguments(result_type)?;

        let [success_type, report_type] = arguments.as_slice() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let source = self.source(expression.origin());

        let success = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let success_value = self.builder.push_block_parameter(
            success,
            Self::retained_source(&source),
            *success_type,
        )?;

        let handler = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let report = self.builder.push_block_parameter(
            handler,
            Self::retained_source(&source),
            *report_type,
        )?;

        let join = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let result =
            self.builder
                .push_block_parameter(join, Self::retained_source(&source), result_type)?;

        self.catch_targets.push(CatchTarget {
            block: handler,
            report_type: *report_type,
            scope_depth: self.active_scopes.len(),
        });

        let completion = self.lower_catch_operand(id, expression, current, success, *success_type);

        self.catch_targets.pop();

        let completion = completion?;

        if let Some(block) = completion.block {
            let value = completion
                .value
                .unwrap_or_else(|| self.unit_operand(*success_type));

            self.builder.set_terminator(
                block,
                Self::retained_source(&source),
                MirTerminatorKind::Goto(MirEdge::new(success, [value])),
            )?;
        }

        let success_result = self.construct_result(
            id,
            success,
            &source,
            result_type,
            true,
            MirOperand::Value(success_value),
        )?;

        self.builder.set_terminator(
            success,
            Self::retained_source(&source),
            MirTerminatorKind::Goto(MirEdge::new(join, [success_result])),
        )?;

        let error_result = self.construct_result(
            id,
            handler,
            &source,
            result_type,
            false,
            MirOperand::Value(report),
        )?;

        self.builder.set_terminator(
            handler,
            Self::retained_source(&source),
            MirTerminatorKind::Goto(MirEdge::new(join, [error_result])),
        )?;

        Ok(LoweredExpression::continuing(
            join,
            Some(MirOperand::Value(result)),
            source,
        ))
    }

    fn lower_catch_operand(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
        success: MirBlockId,
        success_type: TypeId,
    ) -> Result<LoweredExpression, LoweringError> {
        match (expression.operands(), expression.blocks()) {
            ([operand], []) => self.lower_expression(*operand, current),
            ([], [block]) => self.lower_yielding_block(*block, current, success, success_type),
            _ => Err(LoweringError::UnsupportedExpression(id)),
        }
    }

    fn push_panic_report(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
        source: &bray_ir::MirSourceAnchor,
        cause: MirPanicCause,
        report_type: TypeId,
    ) -> Result<MirOperand, LoweringError> {
        let commit = self.builder.push_operation(
            current,
            Self::retained_source(source),
            MirOperationKind::PanicReport(cause),
            Some(report_type),
        )?;

        commit
            .result()
            .map(MirOperand::Value)
            .ok_or(LoweringError::MissingOperationResult(id))
    }

    pub(super) fn finish_panic_to_active_catch(
        &mut self,
        current: MirBlockId,
        source: &bray_ir::MirSourceAnchor,
        report: MirOperand,
        report_type: TypeId,
    ) -> Result<(), LoweringError> {
        let target = self
            .catch_targets
            .last()
            .map(|target| (target.block, target.report_type, target.scope_depth));

        let (catch, expected_report_type, scope_depth) = target
            .map_or((None, report_type, 0), |(block, ty, depth)| {
                (Some(block), ty, depth)
            });

        if report_type != expected_report_type {
            return Err(LoweringError::SemanticValueUnavailable);
        }

        self.finish_panic(current, source, report, report_type, catch, scope_depth)
    }

    pub(super) fn construct_result(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
        source: &bray_ir::MirSourceAnchor,
        result_type: TypeId,
        success: bool,
        value: MirOperand,
    ) -> Result<MirOperand, LoweringError> {
        let representation = self.result_representation()?;

        let (variant, field) = if success {
            (representation.success_variant, representation.success_field)
        } else {
            (representation.error_variant, representation.error_field)
        };

        let commit = self.builder.push_operation(
            current,
            Self::retained_source(source),
            MirOperationKind::Construct(MirConstruction::new(
                ConstructionTarget::UnionVariant(variant),
                [MirConstructionInput::Explicit {
                    input: ConstructionInputId::UnionPayloadField(field),
                    ordinal: 0,
                    value,
                }],
            )),
            Some(result_type),
        )?;

        commit
            .result()
            .map(MirOperand::Value)
            .ok_or(LoweringError::MissingOperationResult(id))
    }
}
