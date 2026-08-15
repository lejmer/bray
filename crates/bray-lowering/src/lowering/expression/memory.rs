use bray_bound_tree::{
    BoundCallResult, BoundExpression, BoundExpressionId, BoundStructuredExpressionKind,
    CheckedMemoryOperation, CheckedMemoryOperationKind, InlineAssemblyContract, SelectedArgument,
};
use bray_ir::{
    MirAggregate, MirAggregateKind, MirBlockId, MirBlockKind, MirCall, MirCallTarget,
    MirCallableReference, MirInlineAssemblyTerminator, MirMemoryOperation, MirOperand,
    MirOperationKind, MirSourceAnchor, MirTerminatorKind,
};
use bray_symbols::{TypeData, TypeId};

use super::super::LoweringError;
use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;

struct InlineAssemblyLabel {
    callable: MirOperand,
    ty: TypeId,
}

impl Lowerer<'_> {
    pub(super) fn lower_memory_call(
        &mut self,
        id: BoundExpressionId,
        mut current: MirBlockId,
        source: MirSourceAnchor,
        selection: &bray_bound_tree::SelectedCall,
        operation: CheckedMemoryOperation,
    ) -> Result<LoweredExpression, LoweringError> {
        let kind = operation.kind();

        let (kind, inline_assembly_symbols) = lower_inline_assembly_contract(kind);

        let mut selected_arguments = Vec::with_capacity(operation.arguments().len());

        for argument in selection.arguments() {
            let SelectedArgument::Explicit {
                expression,
                ordinal,
                conversion,
                ..
            } = argument
            else {
                return Err(LoweringError::MissingSemanticSelection(id));
            };

            selected_arguments.push((*ordinal, *expression, conversion));
        }

        selected_arguments.sort_unstable_by_key(|(ordinal, _, _)| *ordinal);

        if selected_arguments.len() != operation.arguments().len()
            || selected_arguments
                .iter()
                .map(|(_, expression, _)| expression)
                .ne(operation.arguments())
        {
            return Err(LoweringError::MissingSemanticSelection(id));
        }

        let mut arguments = Vec::with_capacity(operation.arguments().len());
        let mut inline_assembly_labels = None;

        for (ordinal, expression, conversion) in selected_arguments {
            let ordinal = usize::try_from(ordinal)
                .map_err(|_| LoweringError::MissingSemanticSelection(id))?;

            if matches!(
                kind,
                CheckedMemoryOperationKind::InlineAssembly {
                    labels: Some(_),
                    ..
                }
            ) && ordinal == 6
            {
                let (continuation, labels) =
                    self.lower_inline_assembly_labels(expression, current)?;

                current = continuation;
                inline_assembly_labels = Some(labels);

                continue;
            }

            let Some(runtime_index) = kind.runtime_argument_index(ordinal) else {
                continue;
            };

            let assembly_inputs =
                matches!(kind, CheckedMemoryOperationKind::InlineAssembly { .. }) && ordinal == 5;

            let lowered = if assembly_inputs {
                let CheckedMemoryOperationKind::InlineAssembly { contract, .. } = kind else {
                    return Err(LoweringError::MissingSemanticSelection(id));
                };

                self.lower_inline_assembly_inputs(expression, current, contract)?
            } else {
                self.lower_expression(expression, current)?
            };

            let Some(continuation) = lowered.block else {
                return Ok(lowered);
            };

            current = continuation;

            let Some(operand) = lowered.value else {
                return Err(LoweringError::MissingOperationResult(expression));
            };

            let operand = if assembly_inputs {
                operand
            } else {
                self.convert_operand(
                    id,
                    current,
                    Self::retained_source(&source),
                    operand,
                    conversion,
                )?
            };

            let operand_type = if assembly_inputs {
                let CheckedMemoryOperationKind::InlineAssembly { contract, .. } = kind else {
                    return Err(LoweringError::MissingSemanticSelection(id));
                };

                self.inline_assembly_runtime_input_type(expression, contract)?
            } else {
                conversion.target_type()
            };

            arguments.push((runtime_index, operand, operand_type));
        }

        arguments.sort_unstable_by_key(|(runtime_index, _, _)| *runtime_index);

        if arguments.len() != kind.operand_count()
            || arguments
                .iter()
                .enumerate()
                .any(|(expected, (actual, _, _))| expected != *actual)
        {
            return Err(LoweringError::MissingSemanticSelection(id));
        }

        let (operands, operand_types): (Vec<_>, Vec<_>) = arguments
            .into_iter()
            .map(|(_, operand, ty)| (operand, ty))
            .unzip();

        let kind = match (kind, operand_types.first().copied()) {
            (
                CheckedMemoryOperationKind::InlineAssembly {
                    output,
                    labels,
                    contract,
                    ..
                },
                Some(inputs),
            ) => CheckedMemoryOperationKind::InlineAssembly {
                inputs,
                output,
                labels,
                contract,
            },
            (kind, _) => kind,
        };

        let result_type = self.expression_type(id)?;
        let result = kind.produces_value().then_some(result_type);

        if let bray_bound_tree::CheckedMemoryOperationKind::InlineAssembly {
            inputs: inputs_type,
            output: Some(output_type),
            labels: Some(_),
            contract,
            ..
        } = kind
        {
            let [inputs] = operands.as_slice() else {
                return Err(LoweringError::MissingSemanticSelection(id));
            };

            let labels =
                inline_assembly_labels.ok_or(LoweringError::MissingSemanticSelection(id))?;

            let alternates = self.inline_assembly_alternates(id, &source, labels)?;

            let normal = self
                .builder
                .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

            let output = self.builder.push_block_parameter(
                normal,
                Self::retained_source(&source),
                output_type,
            )?;

            self.builder.set_terminator(
                current,
                Self::retained_source(&source),
                MirTerminatorKind::InlineAssembly(MirInlineAssemblyTerminator::new(
                    contract,
                    // The terminator owns the sole compact MIR operand while the shared ordinary
                    // lowering path retains the operand vector through this branch.
                    inputs.clone(),
                    inputs_type,
                    output_type,
                    normal,
                    alternates,
                    inline_assembly_symbols,
                )),
            )?;

            return Ok(LoweredExpression::continuing(
                normal,
                Some(MirOperand::Value(output)),
                source,
            ));
        }

        let commit = self.builder.push_operation(
            current,
            Self::retained_source(&source),
            MirOperationKind::Memory(
                MirMemoryOperation::new(kind, operands, operand_types, result)
                    .with_inline_assembly_symbols(inline_assembly_symbols),
            ),
            result,
        )?;

        if matches!(
            kind,
            bray_bound_tree::CheckedMemoryOperationKind::CatastrophicAbort
                | bray_bound_tree::CheckedMemoryOperationKind::UnreachableTermination
                | bray_bound_tree::CheckedMemoryOperationKind::InlineAssembly { output: None, .. }
        ) {
            self.builder.set_terminator(
                current,
                Self::retained_source(&source),
                bray_ir::MirTerminatorKind::Unreachable,
            )?;

            return Ok(LoweredExpression::terminated(source));
        }

        let value = match commit.result() {
            Some(value) => MirOperand::Value(value),
            None => self.unit_operand(result_type),
        };

        Ok(LoweredExpression::continuing(current, Some(value), source))
    }

    fn lower_inline_assembly_inputs(
        &mut self,
        expression: BoundExpressionId,
        mut current: MirBlockId,
        contract: InlineAssemblyContract,
    ) -> Result<LoweredExpression, LoweringError> {
        let Some(BoundExpression::Structured(tuple)) =
            self.input.unit().view().expression(expression)
        else {
            return Err(LoweringError::MissingSemanticSelection(expression));
        };

        if tuple.kind() != BoundStructuredExpressionKind::Tuple {
            return Err(LoweringError::MissingSemanticSelection(expression));
        }

        let mut runtime = contract
            .operands()
            .filter_map(|operand| {
                operand
                    .runtime_input()
                    .zip(operand.input())
                    .map(|pair| (pair, operand.ty()))
            })
            .collect::<Vec<_>>();

        runtime.sort_unstable_by_key(|((runtime, _), _)| *runtime);

        let mut operands = Vec::with_capacity(runtime.len());

        for ((runtime_ordinal, source_ordinal), _) in runtime {
            if usize::from(runtime_ordinal) != operands.len() {
                return Err(LoweringError::MissingSemanticSelection(expression));
            }

            let Some(element) = tuple.operands().get(usize::from(source_ordinal)).copied() else {
                return Err(LoweringError::MissingSemanticSelection(expression));
            };

            let lowered = self.lower_expression(element, current)?;

            let Some(continuation) = lowered.block else {
                return Ok(lowered);
            };

            let Some(value) = lowered.value else {
                return Err(LoweringError::MissingOperationResult(element));
            };

            current = continuation;
            operands.push(value);
        }

        let tuple_type = self.inline_assembly_runtime_input_type(expression, contract)?;
        let source = self.expression_source(expression)?;

        let value = self.push_typed_value_operation(
            expression,
            current,
            Self::retained_source(&source),
            MirOperationKind::Aggregate(MirAggregate::new(MirAggregateKind::Tuple, operands)),
            tuple_type,
        )?;

        Ok(LoweredExpression::continuing(current, Some(value), source))
    }

    fn lower_inline_assembly_labels(
        &mut self,
        expression: BoundExpressionId,
        mut current: MirBlockId,
    ) -> Result<(MirBlockId, Vec<InlineAssemblyLabel>), LoweringError> {
        let Some(BoundExpression::Structured(tuple)) =
            self.input.unit().view().expression(expression)
        else {
            return Err(LoweringError::MissingSemanticSelection(expression));
        };

        if tuple.kind() != BoundStructuredExpressionKind::Tuple {
            return Err(LoweringError::MissingSemanticSelection(expression));
        }

        let elements = tuple.operands().to_vec();
        let mut labels = Vec::with_capacity(elements.len());

        for element in elements {
            let lowered = self.lower_expression(element, current)?;

            let Some(continuation) = lowered.block else {
                return Err(LoweringError::MissingSemanticSelection(element));
            };

            let Some(callable) = lowered.value else {
                return Err(LoweringError::MissingOperationResult(element));
            };

            current = continuation;

            labels.push(InlineAssemblyLabel {
                callable,
                ty: self.expression_type(element)?,
            });
        }

        Ok((current, labels))
    }

    fn inline_assembly_alternates(
        &mut self,
        expression: BoundExpressionId,
        source: &MirSourceAnchor,
        labels: Vec<InlineAssemblyLabel>,
    ) -> Result<Vec<MirBlockId>, LoweringError> {
        let mut alternates = Vec::with_capacity(labels.len());

        for label in labels {
            let data = self
                .input
                .semantic_values()
                .type_data(label.ty)
                .map_err(|_| LoweringError::MissingExpressionType(expression))?;

            let TypeData::Callable(callable) = data.as_ref() else {
                return Err(LoweringError::MissingSemanticSelection(expression));
            };

            let alternate = self
                .builder
                .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

            let call = MirCall::selected(
                MirCallTarget::Indirect {
                    callee: label.callable,
                    abi: callable.abi(),
                },
                BoundCallResult::Immediate(callable.result()),
                [],
                // The trampoline call owns the callable type's immutable phase snapshot.
                callable.phase_behaviors().clone(),
                None,
                [],
                None,
                [],
            );

            self.builder.push_operation(
                alternate,
                Self::retained_source(source),
                MirOperationKind::Call(call),
                Some(callable.result()),
            )?;

            self.builder.set_terminator(
                alternate,
                Self::retained_source(source),
                MirTerminatorKind::Unreachable,
            )?;

            alternates.push(alternate);
        }

        Ok(alternates)
    }

    fn inline_assembly_runtime_input_type(
        &self,
        expression: BoundExpressionId,
        contract: InlineAssemblyContract,
    ) -> Result<TypeId, LoweringError> {
        let mut runtime = contract
            .operands()
            .filter_map(|operand| {
                operand
                    .runtime_input()
                    .map(|ordinal| (ordinal, operand.ty()))
            })
            .collect::<Vec<_>>();

        runtime.sort_unstable_by_key(|(ordinal, _)| *ordinal);

        self.input
            .semantic_values()
            .intern_type(TypeData::tuple(runtime.into_iter().map(|(_, ty)| ty)))
            .map_err(|_| LoweringError::MissingExpressionType(expression))
    }
}

fn lower_inline_assembly_contract(
    kind: CheckedMemoryOperationKind,
) -> (CheckedMemoryOperationKind, Vec<MirCallableReference>) {
    let CheckedMemoryOperationKind::InlineAssembly {
        inputs,
        output,
        labels,
        contract,
    } = kind
    else {
        return (kind, Vec::new());
    };

    let symbols = contract
        .operands()
        .filter_map(|operand| operand.symbol())
        .map(|symbol| MirCallableReference::new(symbol.instance(), symbol.abi()))
        .collect();

    (
        CheckedMemoryOperationKind::InlineAssembly {
            inputs,
            output,
            labels,
            contract: contract.without_symbols(),
        },
        symbols,
    )
}
