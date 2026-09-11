use bray_bound_tree::{
    BoundExpressionId, BoundStructuredExpression, BoundStructuredExpressionKind,
    SelectedConstructionInput, SelectedOperation,
};
use bray_ir::{
    MirAggregate, MirAggregateKind, MirBlockId, MirConstruction, MirConstructionInput, MirOperand,
    MirOperationKind, MirSourceAnchor,
};
use bray_symbols::{TypeData, TypeId};

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

    pub(in crate::lowering) fn adapt_value(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: MirSourceAnchor,
        operand: MirOperand,
        operand_type: TypeId,
        destination_type: TypeId,
    ) -> Result<(MirOperand, TypeId), LoweringError> {
        if operand_type == destination_type {
            return Ok((operand, operand_type));
        }

        let operand = if self.nullable_contains(destination_type, operand_type)? {
            self.push_nullable_present(expression, current, source, operand, destination_type)?
        } else {
            let values = self.input.semantic_values();
            let original = values.type_data(operand_type)?;
            let destination = values.type_data(destination_type)?;

            if !matches!(
                (original.as_ref(), destination.as_ref()),
                (TypeData::Callable(_), TypeData::Callable(_))
            ) {
                return Ok((operand, operand_type));
            }

            // Type checking established substitutability. Retain the target semantic type in
            // MIR without changing the callable value or generating an execution wrapper.
            self.convert_operand(
                expression,
                current,
                source,
                operand,
                &bray_bound_tree::SelectedConversion::new(
                    operand_type,
                    destination_type,
                    bray_bound_tree::ConversionTarget::CallableContract,
                ),
            )?
            .1
        };

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
        let retained = self.construction_temporaries.len();
        let result = self.lower_construction_inputs(id, current);
        self.construction_temporaries.truncate(retained);

        result
    }

    fn lower_construction_inputs(
        &mut self,
        id: BoundExpressionId,
        mut block: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let source = self.expression_source(id)?;

        // Keep the checked selection while recursively lowering its input expressions.
        let selection = self.selected_operation(id)?.clone();
        let mut inputs: Vec<MirConstructionInput> = Vec::new();

        let (target, new_owner) = match selection {
            SelectedOperation::Construction(selection) => {
                for selected in selection.inputs() {
                    let (input, ordinal, ty, value) = match *selected {
                        SelectedConstructionInput::Explicit {
                            expression,
                            input,
                            ordinal,
                            ty,
                        } => {
                            let lowered = self.lower_expression(expression, block)?;

                            let Some(continued) = lowered.block else {
                                return Ok(lowered);
                            };

                            let value = lowered
                                .value
                                .ok_or(LoweringError::MissingOperationResult(expression))?;

                            block = continued;
                            let actual = self.builder.operand_type(&value)?;

                            let (value, ty) = self.adapt_value(
                                id,
                                block,
                                Self::retained_source(&source),
                                value,
                                actual,
                                ty,
                            )?;

                            (input, ordinal, ty, value)
                        }
                        SelectedConstructionInput::Default {
                            input, ordinal, ty, ..
                        } => {
                            let (continued, value) = self.lower_construction_default(
                                id, block, &source, &selection, selected, &inputs,
                            )?;

                            block = continued;

                            (input, ordinal, ty, value)
                        }
                    };

                    let value =
                        self.materialize_construction_input(id, block, &source, value, ty, None)?;

                    inputs.push(MirConstructionInput::new(input, ordinal, value));
                }

                (selection.target(), selection.new_owner_type())
            }
            _ => return Err(LoweringError::MissingSemanticSelection(id)),
        };

        if let Some(ty) = new_owner
            && self
                .input
                .storage_flow()
                .reachable_exits()
                .iter()
                .any(|exit| exit.exit() == id.into())
        {
            block = self.admit_construction_cleanup(id, block, &source, ty)?;
        }

        let value = self.push_value_operation(
            id,
            block,
            Self::retained_source(&source),
            MirOperationKind::Construct(MirConstruction::new(target, inputs)),
        )?;

        let (block, value) = if target.callable().is_some() {
            let result_type = self.expression_type(id)?;

            self.finish_typed_call_panic_check(id, block, &source, &value, result_type)?
        } else {
            (block, value)
        };

        Ok(LoweredExpression::continuing(block, Some(value), source))
    }

    fn lower_construction_default(
        &mut self,
        id: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        selection: &bray_bound_tree::SelectedConstruction,
        selected: &SelectedConstructionInput,
        inputs: &[MirConstructionInput],
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let SelectedConstructionInput::Default {
            ordinal,
            provider,
            ty,
            ..
        } = *selected
        else {
            return Err(LoweringError::MissingSemanticSelection(id));
        };

        let arguments = Self::default_arguments(
            inputs
                .iter()
                .filter(|input| {
                    selection.target().callable().is_some() && input.ordinal() < ordinal
                })
                .map(|input| (Some(input.ordinal()), input.value())),
        );

        let call = bray_ir::MirCall::protocol(
            bray_ir::MirCallTarget::ConstructionDefault {
                target: selection.target(),
                owner_type: selection.result_type(),
                provider,
            },
            bray_bound_tree::BoundCallResult::Immediate(ty),
            arguments,
            [],
        );

        self.push_checked_value_operation(
            id,
            block,
            Self::retained_source(source),
            MirOperationKind::Call(call),
            ty,
        )
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
