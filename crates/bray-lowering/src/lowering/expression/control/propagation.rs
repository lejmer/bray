use bray_bound_tree::{
    BoundExpressionId, BoundStructuredExpression, PatternOperation, PatternPredicate,
    PatternProjection,
};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirEdge, MirImmediateValue, MirOperand, MirOperationKind,
    MirSourceAnchor, MirTerminatorKind,
};
use bray_symbols::{TypeData, TypeId};

use super::super::super::LoweringError;
use super::super::super::block::LoweredExpression;
use super::super::super::lowerer::{Lowerer, YieldTarget};

#[derive(Clone, Copy)]
enum PropagationDestination {
    Block(MirBlockId),
    Return,
}

#[derive(Clone, Copy)]
struct PropagationTarget {
    destination: PropagationDestination,
    result_type: TypeId,
    scope_depth: usize,
}

impl Lowerer<'_> {
    pub(super) fn lower_trust_boundary(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let [operand] = expression.operands() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        self.lower_expression(*operand, current)
    }

    pub(super) fn lower_nullable_propagation(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let [operand_id] = expression.operands() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let operand_type = self.expression_type(*operand_id)?;

        let data = self
            .input
            .semantic_values()
            .type_data(operand_type)
            .map_err(|_| LoweringError::SemanticValueUnavailable)?;

        let TypeData::Nullable(value_type) = data.as_ref() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let value_type = *value_type;
        let target = self.propagation_target(|ty| self.is_nullable_type(ty))?;
        let operand = self.lower_expression(*operand_id, current)?;

        let Some(current) = operand.block else {
            return Ok(operand);
        };

        let Some(operand) = operand.value else {
            return Err(LoweringError::MissingOperationResult(*operand_id));
        };

        let source = self.source(expression.origin());

        let present = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let absent = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        self.builder.set_terminator(
            current,
            Self::retained_source(&source),
            MirTerminatorKind::PatternBranch {
                subject: Self::retained_operand(&operand),
                predicate: PatternPredicate::NullablePresent,
                matched: MirEdge::new(present, []),
                unmatched: MirEdge::new(absent, []),
            },
        )?;

        let value = self.project_pattern_value(
            id,
            present,
            &source,
            operand,
            PatternProjection::NullableValue,
            value_type,
        )?;

        let propagated =
            Self::immediate_operand(target.result_type, MirImmediateValue::NullableAbsent);

        self.finish_propagation(absent, &source, target, propagated)?;

        Ok(LoweredExpression::continuing(
            present,
            Some(value),
            source,
        ))
    }

    pub(super) fn lower_result_propagation(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let [operand_id] = expression.operands() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let operand_type = self.expression_type(*operand_id)?;

        match self.type_representation(operand_type)? {
            Some(RepresentationRole::Result) => self.lower_language_result_propagation(
                id,
                expression,
                *operand_id,
                operand_type,
                current,
            ),
            Some(RepresentationRole::RunResult) => self.lower_run_result_propagation(
                id,
                expression,
                *operand_id,
                operand_type,
                current,
            ),
            _ => Err(LoweringError::UnsupportedExpression(id)),
        }
    }

    fn lower_language_result_propagation(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        operand_id: BoundExpressionId,
        operand_type: TypeId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let arguments = self.named_type_arguments(operand_type)?;

        let [success_type, error_type] = arguments.as_slice() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let representation = self.result_representation()?;

        let target =
            self.propagation_target(|ty| self.type_has_role(ty, RepresentationRole::Result))?;

        let operand = self.lower_expression(operand_id, current)?;

        let Some(current) = operand.block else {
            return Ok(operand);
        };

        let Some(operand) = operand.value else {
            return Err(LoweringError::MissingOperationResult(operand_id));
        };

        let source = self.source(expression.origin());

        let success = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let error = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        self.builder.set_terminator(
            current,
            Self::retained_source(&source),
            MirTerminatorKind::PatternBranch {
                subject: Self::retained_operand(&operand),
                predicate: PatternPredicate::ActiveUnionVariant(representation.success_variant),
                matched: MirEdge::new(success, []),
                unmatched: MirEdge::new(error, []),
            },
        )?;

        let value = self.project_pattern_value(
            id,
            success,
            &source,
            Self::retained_operand(&operand),
            PatternProjection::ActiveUnionPayloadField {
                variant: representation.success_variant,
                field: representation.success_field,
            },
            *success_type,
        )?;

        let error_value = self.project_pattern_value(
            id,
            error,
            &source,
            operand,
            PatternProjection::ActiveUnionPayloadField {
                variant: representation.error_variant,
                field: representation.error_field,
            },
            *error_type,
        )?;

        let propagated = self.construct_result(
            id,
            error,
            &source,
            target.result_type,
            false,
            error_value,
        )?;

        self.finish_propagation(error, &source, target, propagated)?;

        Ok(LoweredExpression::continuing(
            success,
            Some(value),
            source,
        ))
    }

    fn lower_run_result_propagation(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        operand_id: BoundExpressionId,
        operand_type: TypeId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let arguments = self.named_type_arguments(operand_type)?;

        let [value_type] = arguments.as_slice() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let representation = self.run_result_representation()?;
        let report_type = self.panic_report_type()?;
        let operand = self.lower_expression(operand_id, current)?;

        let Some(current) = operand.block else {
            return Ok(operand);
        };

        let Some(operand) = operand.value else {
            return Err(LoweringError::MissingOperationResult(operand_id));
        };

        let source = self.source(expression.origin());

        let completed = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let incomplete = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let panicked = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let cancelled = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        self.builder.set_terminator(
            current,
            Self::retained_source(&source),
            MirTerminatorKind::PatternBranch {
                subject: Self::retained_operand(&operand),
                predicate: PatternPredicate::ActiveUnionVariant(
                    representation.completed_variant,
                ),
                matched: MirEdge::new(completed, []),
                unmatched: MirEdge::new(incomplete, []),
            },
        )?;

        self.builder.set_terminator(
            incomplete,
            Self::retained_source(&source),
            MirTerminatorKind::PatternBranch {
                subject: Self::retained_operand(&operand),
                predicate: PatternPredicate::ActiveUnionVariant(
                    representation.cancelled_variant,
                ),
                matched: MirEdge::new(cancelled, []),
                unmatched: MirEdge::new(panicked, []),
            },
        )?;

        let value = self.project_pattern_value(
            id,
            completed,
            &source,
            Self::retained_operand(&operand),
            PatternProjection::ActiveUnionPayloadField {
                variant: representation.completed_variant,
                field: representation.completed_field,
            },
            *value_type,
        )?;

        let report = self.project_pattern_value(
            id,
            panicked,
            &source,
            operand,
            PatternProjection::ActiveUnionPayloadField {
                variant: representation.panicked_variant,
                field: representation.panicked_field,
            },
            report_type,
        )?;

        self.finish_panic_to_active_catch(panicked, &source, report, report_type)?;
        self.finish_cancellation(cancelled, &source)?;

        Ok(LoweredExpression::continuing(
            completed,
            Some(value),
            source,
        ))
    }

    fn propagation_target(
        &self,
        accepts: impl Fn(TypeId) -> Result<bool, LoweringError>,
    ) -> Result<PropagationTarget, LoweringError> {
        for target in self.yield_targets.iter().rev() {
            let YieldTarget::Result {
                block,
                result_type,
                scope_depth,
                ..
            } = target
            else {
                continue;
            };

            if accepts(*result_type)? {
                return Ok(PropagationTarget {
                    destination: PropagationDestination::Block(*block),
                    result_type: *result_type,
                    scope_depth: *scope_depth,
                });
            }
        }

        let Some(result_type) = self.input.expression_types().callable_result_type() else {
            return Err(LoweringError::SemanticValueUnavailable);
        };

        if !accepts(result_type)? {
            return Err(LoweringError::SemanticValueUnavailable);
        }

        Ok(PropagationTarget {
            destination: PropagationDestination::Return,
            result_type,
            scope_depth: 0,
        })
    }

    fn finish_propagation(
        &mut self,
        current: MirBlockId,
        source: &MirSourceAnchor,
        target: PropagationTarget,
        value: MirOperand,
    ) -> Result<(), LoweringError> {
        match target.destination {
            PropagationDestination::Block(block) => self.finish_exit_to_block(
                current,
                source,
                target.scope_depth,
                block,
                Some((value, target.result_type)),
            ),
            PropagationDestination::Return => {
                self.finish_return(current, source, Some((value, target.result_type)))
            }
        }
    }

    fn project_pattern_value(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
        source: &MirSourceAnchor,
        subject: MirOperand,
        projection: PatternProjection,
        result_type: TypeId,
    ) -> Result<MirOperand, LoweringError> {
        let commit = self.builder.push_operation(
            current,
            Self::retained_source(source),
            MirOperationKind::PatternProjection {
                subject,
                projection,
                operation: PatternOperation::Consume,
            },
            Some(result_type),
        )?;

        commit
            .result()
            .map(MirOperand::Value)
            .ok_or(LoweringError::MissingOperationResult(id))
    }

    fn is_nullable_type(&self, ty: TypeId) -> Result<bool, LoweringError> {
        self.input
            .semantic_values()
            .type_data(ty)
            .map(|data| matches!(data.as_ref(), TypeData::Nullable(_)))
            .map_err(|_| LoweringError::SemanticValueUnavailable)
    }

    fn type_has_role(
        &self,
        ty: TypeId,
        role: RepresentationRole,
    ) -> Result<bool, LoweringError> {
        self.type_representation(ty)
            .map(|representation| representation == Some(role))
    }
}
