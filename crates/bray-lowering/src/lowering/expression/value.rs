use bray_bound_tree::{
    BoundExpressionId, BoundStructuredExpression, BoundStructuredExpressionKind,
    ConstructionTarget, SelectedConstructionInput, SelectedOperation,
};
use bray_ir::{
    MirAggregate, MirAggregateKind, MirBlockId, MirConstruction, MirConstructionInput, MirOperand,
    MirOperationKind, MirSourceAnchor,
};
use bray_symbols::{AnySymbolId, TypeData, TypeId};

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
    pub(in crate::lowering) fn nullable_contains(
        &self,
        nullable: TypeId,
        contained: TypeId,
    ) -> Result<bool, LoweringError> {
        let data = self.input.semantic_values().type_data(nullable)?;

        Ok(matches!(data.as_ref(), TypeData::Nullable(element) if *element == contained))
    }

    pub(in crate::lowering) fn push_nullable_present(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: MirSourceAnchor,
        operand: MirOperand,
        result_type: TypeId,
    ) -> Result<MirOperand, LoweringError> {
        let commit = self.push_operation(
            current,
            source,
            MirOperationKind::Aggregate(MirAggregate::new(
                MirAggregateKind::NullablePresent,
                [operand],
            )),
            Some(result_type),
        )?;

        commit
            .result()
            .map(MirOperand::Value)
            .ok_or(LoweringError::MissingOperationResult(expression))
    }

    pub(in crate::lowering) fn adapt_nullable_present(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: MirSourceAnchor,
        operand: MirOperand,
        operand_type: TypeId,
        destination_type: TypeId,
    ) -> Result<(MirOperand, TypeId), LoweringError> {
        if !self.nullable_contains(destination_type, operand_type)? {
            return Ok((operand, operand_type));
        }

        let operand =
            self.push_nullable_present(expression, current, source, operand, destination_type)?;

        Ok((operand, destination_type))
    }

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

        let (block, mut operands) = match self.lower_operands(expression.operands(), current)? {
            LoweredOperands::Continuing { block, operands } => (block, operands),
            LoweredOperands::Terminated(completion) => return Ok(completion),
        };

        if kind == MirAggregateKind::RepeatedArray {
            if operands.len() != 2 {
                return Err(LoweringError::UnsupportedExpression(id));
            }

            // Source evaluation is preserved, but the checked array type owns the MIR extent.
            operands.truncate(1);
        }

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
        let retained = self.input_temporaries.len();
        let result = self.lower_construction_inputs(id, current);
        self.input_temporaries.truncate(retained);

        result
    }

    fn lower_construction_inputs(
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

                for input in selection.inputs() {
                    match *input {
                        SelectedConstructionInput::Explicit {
                            expression,
                            input,
                            ordinal,
                            ..
                        } => {
                            let lowered = self.lower_expression(expression, block)?;

                            let Some(continuation) = lowered.block else {
                                return Ok(lowered);
                            };

                            let Some(value) = lowered.value else {
                                return Err(LoweringError::MissingOperationResult(expression));
                            };

                            block = continuation;

                            let ty = self.builder.operand_type(&value)?;

                            let value =
                                self.materialize_owned_input(id, block, &source, value, ty)?;

                            inputs.push(MirConstructionInput::new(input, ordinal, value));
                        }
                        SelectedConstructionInput::Default {
                            input,
                            provider,
                            ordinal,
                            ty,
                        } => {
                            let arguments = if matches!(
                                selection.target(),
                                ConstructionTarget::TypeForm { .. }
                            ) {
                                self.default_value_arguments(
                                    id,
                                    block,
                                    &source,
                                    inputs
                                        .iter()
                                        .map(|input| (Some(input.ordinal()), input.value())),
                                    ordinal,
                                )?
                            } else {
                                Vec::new()
                            };

                            let call = bray_ir::MirCall::protocol(
                                bray_ir::MirCallTarget::DefaultValue {
                                    owner: match selection.target() {
                                        ConstructionTarget::TypeForm { callable, .. } => {
                                            bray_ir::MirDefaultOwner::Callable(
                                                bray_ir::MirCallableReference::new(
                                                    callable,
                                                    bray_symbols::CallableAbi::Bray,
                                                ),
                                            )
                                        }
                                        target => bray_ir::MirDefaultOwner::Type {
                                            target,
                                            ty: selection.result_type(),
                                        },
                                    },
                                    provider,
                                },
                                bray_bound_tree::BoundCallResult::Immediate(ty),
                                arguments,
                                [],
                            );

                            let (continued, value) = self.push_checked_value_operation(
                                id,
                                block,
                                Self::retained_source(&source),
                                MirOperationKind::Call(call),
                                ty,
                            )?;

                            block = continued;
                            let ty = self.builder.operand_type(&value)?;

                            let value =
                                self.materialize_owned_input(id, block, &source, value, ty)?;

                            inputs.push(MirConstructionInput::new(input, ordinal, value));
                        }
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

        let owner_type = self.expression_type(id)?;
        let block = self.admit_outgoing_owner(id, block, &source, owner_type)?;

        let value = self.push_value_operation(
            id,
            block,
            Self::retained_source(&source),
            MirOperationKind::Construct(MirConstruction::new(target, inputs)),
        )?;

        let (block, value) = if matches!(target, ConstructionTarget::TypeForm { .. }) {
            self.finish_typed_call_panic_check(
                id,
                block,
                &source,
                &value,
                owner_type,
                Some(owner_type),
            )?
        } else {
            (block, value)
        };

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
                .later_evaluation_may_change_block(expressions[index + 1..].iter().copied())?
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
