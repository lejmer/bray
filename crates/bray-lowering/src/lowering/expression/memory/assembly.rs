use bray_bound_tree::{
    BoundCallResult, BoundExpression, BoundExpressionId, BoundStructuredExpressionKind,
    CheckedMemoryOperationKind, InlineAssemblyContract,
};
use bray_ir::{
    MirAggregate, MirAggregateKind, MirBlockId, MirBlockKind, MirCall, MirCallTarget,
    MirCallableReference, MirInlineAssemblyTerminator, MirOperand, MirOperationKind,
    MirSourceAnchor, MirTerminatorKind,
};
use bray_symbols::{TypeData, TypeId};

use crate::lowering::{LoweringError, block::LoweredExpression, lowerer::Lowerer};

pub(super) struct InlineAssemblyLabel {
    callable: MirOperand,
    ty: TypeId,
}

impl Lowerer<'_> {
    pub(super) fn lower_inline_assembly_terminator(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: &MirSourceAnchor,
        kind: CheckedMemoryOperationKind,
        operands: &[MirOperand],
        labels: Option<Vec<InlineAssemblyLabel>>,
        symbols: &[MirCallableReference],
    ) -> Result<Option<LoweredExpression>, LoweringError> {
        let CheckedMemoryOperationKind::InlineAssembly {
            inputs: inputs_type,
            output: Some(output_type),
            labels: Some(_),
            contract,
            ..
        } = kind
        else {
            return Ok(None);
        };

        let [inputs] = operands else {
            panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = expression
            );
        };

        let labels = labels.unwrap_or_else(|| {
            panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = expression
            )
        });

        let alternates = self.inline_assembly_alternates(expression, source, labels)?;

        let normal = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let output = self.builder.push_block_parameter(
            normal,
            Self::retained_source(source),
            output_type,
        )?;

        self.set_terminator(
            current,
            Self::retained_source(source),
            MirTerminatorKind::InlineAssembly(Box::new(MirInlineAssemblyTerminator::new(
                contract,
                // The terminator owns the sole compact MIR operand while the shared ordinary
                // lowering path retains the operand vector through this branch.
                inputs.clone(),
                inputs_type,
                output_type,
                normal,
                alternates,
                symbols.iter().copied(),
            ))),
        )?;

        Ok(Some(LoweredExpression::continuing(
            normal,
            Some(MirOperand::Value(output)),
            source.clone(),
        )))
    }

    pub(super) fn lower_inline_assembly_inputs(
        &mut self,
        expression: BoundExpressionId,
        mut current: MirBlockId,
        contract: InlineAssemblyContract,
    ) -> Result<LoweredExpression, LoweringError> {
        let Some(BoundExpression::Structured(tuple)) =
            self.input.unit().view().expression(expression)
        else {
            panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = expression
            );
        };

        if tuple.kind() != BoundStructuredExpressionKind::Tuple {
            panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = expression
            );
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
                panic!(
                    "lowering contract violation: MissingSemanticSelection {value:?}",
                    value = expression
                );
            }

            let Some(element) = tuple.operands().get(usize::from(source_ordinal)).copied() else {
                panic!(
                    "lowering contract violation: MissingSemanticSelection {value:?}",
                    value = expression
                );
            };

            let lowered = self.lower_expression(element, current)?;
            let lowered = self.materialize_for_later_evaluation(element, lowered)?;

            let Some(continuation) = lowered.block else {
                return Ok(lowered);
            };

            let Some(value) = lowered.value else {
                panic!(
                    "lowering contract violation: MissingOperationResult {value:?}",
                    value = element
                );
            };

            current = continuation;
            operands.push(value);
        }

        let tuple_type = self.inline_assembly_runtime_input_type(contract)?;
        let source = self.expression_source(expression);

        let value = self.push_typed_value_operation(
            expression,
            current,
            Self::retained_source(&source),
            MirOperationKind::Aggregate(MirAggregate::new(MirAggregateKind::Tuple, operands)),
            tuple_type,
        )?;

        Ok(LoweredExpression::continuing(current, Some(value), source))
    }

    pub(super) fn lower_inline_assembly_labels(
        &mut self,
        expression: BoundExpressionId,
        mut current: MirBlockId,
    ) -> Result<(MirBlockId, Vec<InlineAssemblyLabel>), LoweringError> {
        let Some(BoundExpression::Structured(tuple)) =
            self.input.unit().view().expression(expression)
        else {
            panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = expression
            );
        };

        if tuple.kind() != BoundStructuredExpressionKind::Tuple {
            panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = expression
            );
        }

        let elements = tuple.operands().to_vec();
        let mut labels = Vec::with_capacity(elements.len());

        for element in elements {
            let lowered = self.lower_expression(element, current)?;

            // Alternate trampolines run in different blocks and cannot capture local MIR values.
            let lowered = self.materialize_for_later_evaluation(element, lowered)?;

            let Some(continuation) = lowered.block else {
                panic!(
                    "lowering contract violation: MissingSemanticSelection {value:?}",
                    value = element
                );
            };

            let Some(callable) = lowered.value else {
                panic!(
                    "lowering contract violation: MissingOperationResult {value:?}",
                    value = element
                );
            };

            current = continuation;

            labels.push(InlineAssemblyLabel {
                callable,
                ty: self.expression_type(element),
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
            let data = self.input.semantic_values().type_data(label.ty);

            let TypeData::Callable(callable) = data.as_ref() else {
                panic!(
                    "lowering contract violation: MissingSemanticSelection {value:?}",
                    value = expression
                );
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

            let (completed, _) = self.push_checked_call(
                expression,
                alternate,
                Self::retained_source(source),
                call,
                callable.result(),
            )?;

            self.set_terminator(
                completed,
                Self::retained_source(source),
                MirTerminatorKind::Unreachable,
            )?;

            alternates.push(alternate);
        }

        Ok(alternates)
    }

    pub(super) fn inline_assembly_runtime_input_type(
        &self,
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
            .map_err(LoweringError::from)
    }
}

pub(super) fn lower_inline_assembly_contract(
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
