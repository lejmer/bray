use bray_bound_tree::{
    BoundExpressionId, BoundStructuredExpression, BoundStructuredExpressionKind,
    ConstructionTarget, SelectedConstructionInput, SelectedOperation,
};
use bray_ir::{
    MirAggregate, MirAggregateKind, MirBlockId, MirConstruction, MirConstructionInput, MirOperand,
    MirOperationKind,
};
use bray_symbols::AnySymbolId;

use super::super::LoweringError;
use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;

enum LoweredOperands {
    Continuing {
        block: MirBlockId,
        operands: Vec<MirOperand>,
    },
    Terminated(LoweredExpression),
}

impl Lowerer<'_> {
    pub(super) fn lower_aggregate(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let kind = match expression.kind() {
            BoundStructuredExpressionKind::Tuple => MirAggregateKind::Tuple,
            BoundStructuredExpressionKind::Array => MirAggregateKind::Array,
            BoundStructuredExpressionKind::RepeatedArray => MirAggregateKind::RepeatedArray,
            BoundStructuredExpressionKind::Range => MirAggregateKind::Range,
            _ => return Err(LoweringError::UnsupportedExpression(id)),
        };

        let source = self.source(expression.origin());

        let (block, operands) = match self.lower_operands(expression.operands(), current)? {
            LoweredOperands::Continuing { block, operands } => (block, operands),
            LoweredOperands::Terminated(completion) => return Ok(completion),
        };

        let value = self.push_value_operation(
            id,
            block,
            Self::retained_source(&source),
            MirOperationKind::Aggregate(MirAggregate::new(kind, operands)),
        )?;

        Ok(LoweredExpression::continuing(block, Some(value), source))
    }

    pub(super) fn lower_construction(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let source = self.expression_source(id)?;

        let selection = self.selected_operation(id)?.clone();

        let (target, block, inputs) = match selection {
            SelectedOperation::Construction(selection) => {
                let mut block = current;
                let mut inputs = Vec::with_capacity(selection.inputs().len());

                for (index, input) in selection.inputs().iter().enumerate() {
                    match *input {
                        SelectedConstructionInput::Explicit {
                            expression,
                            input,
                            ordinal,
                            ..
                        } => {
                            let lowered = self.lower_expression(expression, block)?;

                            let has_later_expression =
                                selection.inputs()[index + 1..].iter().any(|input| {
                                    matches!(input, SelectedConstructionInput::Explicit { .. })
                                });

                            let lowered = if has_later_expression {
                                self.materialize_for_later_evaluation(expression, lowered)?
                            } else {
                                lowered
                            };

                            let Some(continuation) = lowered.block else {
                                return Ok(lowered);
                            };

                            let Some(value) = lowered.value else {
                                return Err(LoweringError::MissingOperationResult(expression));
                            };

                            block = continuation;

                            inputs.push(MirConstructionInput::Explicit {
                                input,
                                ordinal,
                                value,
                            });
                        }
                        SelectedConstructionInput::Default {
                            input,
                            provider,
                            ordinal,
                            ..
                        } => inputs.push(MirConstructionInput::Default {
                            input,
                            ordinal,
                            provider,
                        }),
                    }
                }

                (selection.target(), block, inputs)
            }
            SelectedOperation::Member(member) => {
                let AnySymbolId::UnionVariant(variant) = member.member() else {
                    return Err(LoweringError::MissingSemanticSelection(id));
                };

                (
                    ConstructionTarget::UnionVariant(variant),
                    current,
                    Vec::new(),
                )
            }
            _ => return Err(LoweringError::MissingSemanticSelection(id)),
        };

        let value = self.push_value_operation(
            id,
            block,
            Self::retained_source(&source),
            MirOperationKind::Construct(MirConstruction::new(target, inputs)),
        )?;

        Ok(LoweredExpression::continuing(block, Some(value), source))
    }

    fn lower_operands(
        &mut self,
        expressions: &[BoundExpressionId],
        mut current: MirBlockId,
    ) -> Result<LoweredOperands, LoweringError> {
        let mut operands = Vec::with_capacity(expressions.len());

        for (index, expression) in expressions.iter().enumerate() {
            let lowered = self.lower_expression(*expression, current)?;

            let lowered = if self
                .later_evaluation_may_check_call_panic(expressions[index + 1..].iter().copied())?
            {
                self.materialize_for_later_evaluation(*expression, lowered)?
            } else {
                lowered
            };

            let Some(continuation) = lowered.block else {
                return Ok(LoweredOperands::Terminated(lowered));
            };

            let Some(value) = lowered.value else {
                return Err(LoweringError::MissingOperationResult(*expression));
            };

            current = continuation;
            operands.push(value);
        }

        Ok(LoweredOperands::Continuing {
            block: current,
            operands,
        })
    }
}
