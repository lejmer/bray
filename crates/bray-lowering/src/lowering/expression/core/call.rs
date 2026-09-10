use crate::lowering::LoweringError;
use crate::lowering::block::LoweredExpression;
use crate::lowering::lowerer::Lowerer;
use bray_bound_tree::{
    BoundCallableTarget, BoundExpression, BoundExpressionId, SelectedArgument, SemanticSelection,
};
use bray_compiler_known::CompilerKnownOperationRole;
use bray_ir::{
    MirBinaryOperator, MirBlockId, MirCall, MirCallArgument, MirCallIntrinsic, MirCallTarget,
    MirCallableReference, MirOperationKind, MirSourceAnchor, MirUnaryOperator,
};

impl Lowerer<'_> {
    pub(super) fn lower_call(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let retained = self.construction_temporaries.len();
        let result = self.lower_call_inputs(id, current);

        self.construction_temporaries.truncate(retained);

        result
    }

    fn lower_call_inputs(
        &mut self,
        id: BoundExpressionId,
        mut current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let expression = self
            .input
            .unit()
            .view()
            .expression(id)
            .and_then(|expression| match expression {
                BoundExpression::Call(call) => Some(call),
                _ => None,
            })
            .ok_or_else(|| LoweringError::MissingBoundNode(id.into()))?;

        // Retain the checked call before recursively mutating lowering state.
        let selection = self
            .input
            .semantic_selections()
            .expression(id)
            .and_then(|selection| match selection {
                SemanticSelection::Call(call) => Some(call),
                _ => None,
            })
            .ok_or(LoweringError::MissingSemanticSelection(id))?
            .clone();

        let source = self.source(expression.origin());

        if let Some(lowered) =
            self.lower_run_call(id, current, Self::retained_source(&source), &selection)?
        {
            return Ok(lowered);
        }

        if let Some(lowered) =
            self.lower_testing_call(id, current, Self::retained_source(&source), &selection)?
        {
            return Ok(lowered);
        }

        if let Some(lowered) =
            self.lower_numeric_call(id, current, Self::retained_source(&source), &selection)?
        {
            return Ok(lowered);
        }

        if let Some(lowered) = self.lower_range_call(id, current, &selection)? {
            return Ok(lowered);
        }

        if let Some(operation) = self
            .input
            .storage_flow()
            .checked_memory_operation(id)
            .cloned()
        {
            return self.lower_memory_call(id, current, source, &selection, operation);
        }

        if let Some(hook) = selection.implementation_hook()
            && let Some(kind) = super::super::nullable::nullable_query_kind(hook)
        {
            return self.lower_nullable_call(id, current, source, &selection, kind);
        }

        if let Some(hook) = selection.implementation_hook()
            && let Some(kind) = super::super::sequence::sequence_operation_kind(hook)
        {
            return self.lower_sequence_call(id, current, source, &selection, kind);
        }

        if let Some(hook) = selection.implementation_hook()
            && let Some(kind) = super::super::text::text_operation_kind(hook)
        {
            return self.lower_text_call(id, current, source, &selection, kind);
        }

        let mut arguments = Vec::new();

        let target = if let Some(role) =
            super::super::run::runtime_call_role(selection.implementation_hook())
        {
            MirCallTarget::Runtime(self.runtime_reference(role))
        } else {
            match selection.target() {
                BoundCallableTarget::Declaration(instance) => {
                    MirCallTarget::Direct(MirCallableReference::new(instance, selection.abi()))
                }
                BoundCallableTarget::Indirect(_) => {
                    let lowered = self.lower_expression(expression.callee(), current)?;

                    let Some(continuation) = lowered.block else {
                        return Ok(lowered);
                    };

                    current = continuation;

                    let callee = lowered
                        .value
                        .ok_or(LoweringError::MissingOperationResult(expression.callee()))?;

                    let ty = self.expression_type(expression.callee())?;

                    let callee = self.materialize_construction_input(
                        id,
                        current,
                        &source,
                        callee,
                        ty,
                        Some(bray_bound_tree::AsyncStorageCleanupRequirement::None),
                    )?;

                    MirCallTarget::Indirect {
                        callee,
                        abi: selection.abi(),
                    }
                }
                BoundCallableTarget::Predicate(_) | BoundCallableTarget::Anonymous(_) => {
                    return Err(LoweringError::UnsupportedExpression(id));
                }
            }
        };

        if let Some(receiver) = selection.receiver() {
            let (lowered, ty) = self.lower_call_receiver(receiver, current)?;

            let Some(continuation) = lowered.block else {
                return Ok(lowered);
            };

            current = continuation;

            let value = lowered
                .value
                .ok_or(LoweringError::MissingOperationResult(receiver.expression()))?;

            let cleanup = match receiver.mode() {
                bray_symbols::ReceiverMode::Shared | bray_symbols::ReceiverMode::Mutable => {
                    Some(bray_bound_tree::AsyncStorageCleanupRequirement::None)
                }
                bray_symbols::ReceiverMode::Consuming
                | bray_symbols::ReceiverMode::ConsumingMutable => None,
            };

            let value =
                self.materialize_construction_input(id, current, &source, value, ty, cleanup)?;

            arguments.push(MirCallArgument::Receiver {
                parameter: receiver.parameter(),
                value,
            });
        }

        for argument in selection.arguments() {
            let SelectedArgument::Explicit {
                expression,
                parameter,
                ordinal,
                conversion,
            } = argument
            else {
                continue;
            };

            let lowered = self.lower_expression(*expression, current)?;

            let Some(continuation) = lowered.block else {
                return Ok(lowered);
            };

            let operand = lowered
                .value
                .ok_or(LoweringError::MissingOperationResult(*expression))?;

            let (continuation, value) = self.convert_operand(
                id,
                continuation,
                Self::retained_source(&source),
                operand,
                conversion,
            )?;

            current = continuation;

            let ty = self.builder.operand_type(&value)?;

            let value =
                self.materialize_construction_input(id, current, &source, value, ty, None)?;

            arguments.push(MirCallArgument::Explicit {
                parameter: *parameter,
                ordinal: *ordinal,
                value,
            });
        }

        current = self.lower_parameter_defaults(
            id,
            current,
            &source,
            &selection,
            &target,
            &mut arguments,
        )?;

        let call = self.selected_mir_call(&selection, target, arguments);

        let (current, value) =
            self.lower_call_operation(id, current, Self::retained_source(&source), call)?;

        Ok(LoweredExpression::continuing(current, Some(value), source))
    }

    fn lower_parameter_defaults(
        &mut self,
        expression: BoundExpressionId,
        mut block: MirBlockId,
        source: &MirSourceAnchor,
        selection: &bray_bound_tree::SelectedCall,
        target: &MirCallTarget,
        arguments: &mut Vec<MirCallArgument>,
    ) -> Result<MirBlockId, LoweringError> {
        for argument in selection.parameter_arguments() {
            let SelectedArgument::Default {
                parameter,
                ordinal,
                provider,
                ty,
            } = argument
            else {
                continue;
            };

            let MirCallTarget::Direct(callable) = target else {
                return Err(LoweringError::UnsupportedExpression(expression));
            };

            let preceding = arguments
                .iter()
                .filter_map(|argument| match argument {
                    MirCallArgument::Receiver { value, .. } => Some((None, value)),
                    MirCallArgument::Explicit {
                        ordinal: position,
                        value,
                        ..
                    } if position < ordinal => Some((Some(*position), value)),
                    _ => None,
                })
                .collect::<Vec<_>>();

            let inputs = Self::default_arguments(preceding);

            let call = MirCall::protocol(
                MirCallTarget::ParameterDefault {
                    callable: *callable,
                    provider: *provider,
                },
                bray_bound_tree::BoundCallResult::Immediate(*ty),
                inputs,
                selection.witnesses().iter().copied(),
            );

            let (continued, value) = self.push_checked_value_operation(
                expression,
                block,
                Self::retained_source(source),
                MirOperationKind::Call(call),
                *ty,
            )?;

            block = continued;

            let value =
                self.materialize_construction_input(expression, block, source, value, *ty, None)?;

            arguments.push(MirCallArgument::Explicit {
                parameter: Some(*parameter),
                ordinal: *ordinal,
                value,
            });
        }

        Ok(block)
    }

    fn selected_mir_call(
        &self,
        selection: &bray_bound_tree::SelectedCall,
        target: MirCallTarget,
        arguments: Vec<MirCallArgument>,
    ) -> MirCall {
        // MIR owns the immutable checked call contract independently of the selection table.
        let call = MirCall::selected(
            target,
            selection.resolution().result(),
            arguments,
            selection.phase_behaviors().clone(),
            selection.contract().cloned(),
            selection
                .resolution()
                .implementation_witnesses()
                .iter()
                .copied(),
            selection.resolution().trait_dispatch(),
            selection.witnesses().iter().copied(),
        );

        match self.compiler_known_direct_call_intrinsic(selection) {
            Some(intrinsic) => call.with_intrinsic(intrinsic),
            None => call,
        }
    }

    fn compiler_known_direct_call_intrinsic(
        &self,
        selection: &bray_bound_tree::SelectedCall,
    ) -> Option<MirCallIntrinsic> {
        selection.resolution().trait_dispatch()?;

        let BoundCallableTarget::Declaration(instance) = selection.target() else {
            return None;
        };

        let callable = instance.definition().symbol();
        let available = self.input.available_compiler_known_symbols();

        if available
            .operation_contract(CompilerKnownOperationRole::Comparison)
            .and_then(|contract| contract.callable())
            .map(Into::into)
            == Some(callable)
        {
            let ordering = available.ordering_representation()?;

            return Some(MirCallIntrinsic::Comparison {
                less: ordering.less_variant(),
                equal: ordering.equal_variant(),
                greater: ordering.greater_variant(),
            });
        }

        [
            (
                CompilerKnownOperationRole::UnaryNegate,
                MirCallIntrinsic::Unary(MirUnaryOperator::Negate),
            ),
            (
                CompilerKnownOperationRole::UnaryBitNot,
                MirCallIntrinsic::Unary(MirUnaryOperator::BitwiseNot),
            ),
            (
                CompilerKnownOperationRole::BinaryAdd,
                MirCallIntrinsic::Binary(MirBinaryOperator::Add),
            ),
            (
                CompilerKnownOperationRole::BinarySubtract,
                MirCallIntrinsic::Binary(MirBinaryOperator::Subtract),
            ),
            (
                CompilerKnownOperationRole::BinaryMultiply,
                MirCallIntrinsic::Binary(MirBinaryOperator::Multiply),
            ),
            (
                CompilerKnownOperationRole::BinaryDivide,
                MirCallIntrinsic::Binary(MirBinaryOperator::Divide),
            ),
            (
                CompilerKnownOperationRole::BinaryRemainder,
                MirCallIntrinsic::Binary(MirBinaryOperator::Remainder),
            ),
            (
                CompilerKnownOperationRole::BinaryBitAnd,
                MirCallIntrinsic::Binary(MirBinaryOperator::BitwiseAnd),
            ),
            (
                CompilerKnownOperationRole::BinaryBitOr,
                MirCallIntrinsic::Binary(MirBinaryOperator::BitwiseOr),
            ),
            (
                CompilerKnownOperationRole::BinaryBitXor,
                MirCallIntrinsic::Binary(MirBinaryOperator::BitwiseXor),
            ),
            (
                CompilerKnownOperationRole::BinaryShiftLeft,
                MirCallIntrinsic::Binary(MirBinaryOperator::ShiftLeft),
            ),
            (
                CompilerKnownOperationRole::BinaryShiftRight,
                MirCallIntrinsic::Binary(MirBinaryOperator::ShiftRight),
            ),
            (
                CompilerKnownOperationRole::Equality,
                MirCallIntrinsic::Binary(MirBinaryOperator::Equal),
            ),
        ]
        .into_iter()
        .find_map(|(role, intrinsic)| {
            let contract = available.operation_contract(role)?;

            (contract.callable().map(Into::into) == Some(callable)).then_some(intrinsic)
        })
    }
}
