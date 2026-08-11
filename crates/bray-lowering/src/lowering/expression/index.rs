use bray_bound_tree::BoundStructuredExpressionKind;
use bray_bound_tree::{BoundExpression, BoundExpressionId, IndexTarget, SelectedOperation};
use bray_ir::{
    MirAggregate, MirAggregateKind, MirBlockId, MirCall, MirCallTarget, MirCallableReference,
    MirImmediateValue, MirOperand, MirOperationKind,
};
use bray_symbols::{CallableAbi, GenericArgument, ImplementationRequirementKey, TypeData, TypeId};

use super::super::LoweringError;
use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn lower_index(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let target = match self.selected_operation(id)? {
            SelectedOperation::Index { target, .. } => *target,
            _ => return Err(LoweringError::MissingSemanticSelection(id)),
        };

        match target {
            IndexTarget::ArrayElement
            | IndexTarget::SliceElement
            | IndexTarget::ArraySlice
            | IndexTarget::Slice
            | IndexTarget::Custom { .. }
            | IndexTarget::TraitConstraint { .. } => self.lower_storage_operand(id, current),
        }
    }

    pub(in crate::lowering) fn lower_custom_index_borrow(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let target = match self.selected_operation(id)? {
            SelectedOperation::Index { target, .. } => *target,
            _ => return Err(LoweringError::MissingSemanticSelection(id)),
        };

        match target {
            IndexTarget::Custom {
                borrow_kind,
                fulfillment,
                requirement,
                witness,
                ..
            } => self.lower_custom_index(
                id,
                current,
                borrow_kind,
                fulfillment,
                requirement,
                None,
                Some(witness),
            ),
            IndexTarget::TraitConstraint {
                borrow_kind,
                member,
                requirement,
                dispatch,
            } => self.lower_custom_index(
                id,
                current,
                borrow_kind,
                member,
                requirement,
                Some(dispatch),
                None,
            ),
            IndexTarget::ArrayElement
            | IndexTarget::SliceElement
            | IndexTarget::ArraySlice
            | IndexTarget::Slice => Err(LoweringError::MissingSemanticSelection(id)),
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "custom indexing retains its selected callable, dispatch, witness, and access capability"
    )]
    fn lower_custom_index(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
        borrow_kind: bray_symbols::BorrowKind,
        callable: bray_symbols::CallableInstanceData,
        requirement: ImplementationRequirementKey,
        dispatch: Option<bray_symbols::TraitConstraintDispatch>,
        witness: Option<bray_symbols::ImplementationInstanceId>,
    ) -> Result<LoweredExpression, LoweringError> {
        let expression = self
            .input
            .unit()
            .view()
            .expression(id)
            .and_then(|expression| match expression {
                BoundExpression::Structured(expression) => Some(expression),
                _ => None,
            })
            .ok_or_else(|| LoweringError::MissingBoundNode(id.into()))?;

        let kind = expression.kind();
        let operands = expression.operands().to_vec();
        let slice_bounds = expression.slice_bounds();

        let source = self.expression_source(id)?;
        let mut block = current;
        let mut arguments = Vec::with_capacity(3);

        let Some(receiver) = operands.first().copied() else {
            return Err(LoweringError::MissingBoundNode(id.into()));
        };

        let lowered = self.lower_implicit_borrow(id, receiver, block, borrow_kind)?;

        let Some(continuation) = lowered.block else {
            return Ok(lowered);
        };

        let Some(value) = lowered.value else {
            return Err(LoweringError::MissingOperationResult(receiver));
        };

        block = continuation;
        arguments.push(value);

        match kind {
            BoundStructuredExpressionKind::ElementIndex => {
                for operand in operands.into_iter().skip(1) {
                    let lowered = self.lower_implicit_shared_borrow(id, operand, block)?;

                    let Some(continuation) = lowered.block else {
                        return Ok(lowered);
                    };

                    let Some(value) = lowered.value else {
                        return Err(LoweringError::MissingOperationResult(operand));
                    };

                    block = continuation;
                    arguments.push(value);
                }
            }
            BoundStructuredExpressionKind::SliceIndex => {
                let bounds =
                    slice_bounds.ok_or_else(|| LoweringError::MissingBoundNode(id.into()))?;

                let bound_type = self.index_bound_type(requirement)?;

                let nullable_type = self
                    .input
                    .semantic_values()
                    .intern_type(TypeData::Nullable(bound_type))
                    .map_err(|_| LoweringError::SemanticValueUnavailable)?;

                for bound in [bounds.lower(), bounds.upper()] {
                    let Some(bound) = bound else {
                        arguments.push(Self::immediate_operand(
                            nullable_type,
                            MirImmediateValue::NullableAbsent,
                        ));

                        continue;
                    };

                    let lowered = self.lower_expression(bound, block)?;

                    let Some(continuation) = lowered.block else {
                        return Ok(lowered);
                    };

                    let Some(value) = lowered.value else {
                        return Err(LoweringError::MissingOperationResult(bound));
                    };

                    block = continuation;

                    let commit = self.builder.push_operation(
                        block,
                        Self::retained_source(&source),
                        MirOperationKind::Aggregate(MirAggregate::new(
                            MirAggregateKind::NullablePresent,
                            [value],
                        )),
                        Some(nullable_type),
                    )?;

                    let value = commit
                        .result()
                        .map(MirOperand::Value)
                        .ok_or(LoweringError::MissingOperationResult(bound))?;

                    arguments.push(value);
                }
            }
            _ => return Err(LoweringError::MissingSemanticSelection(id)),
        }

        let witnesses = witness.into_iter().map(|witness| {
            bray_bound_tree::SelectedImplementationWitness::new(requirement, witness)
        });

        let result_type = self
            .input
            .semantic_values()
            .intern_type(TypeData::Borrow {
                kind: borrow_kind,
                target: self.expression_type(id)?,
            })
            .map_err(|_| LoweringError::SemanticValueUnavailable)?;

        let mut call = MirCall::protocol(
            MirCallTarget::Direct(MirCallableReference::new(callable, CallableAbi::Bray)),
            bray_bound_tree::BoundCallResult::Immediate(result_type),
            arguments,
            witnesses,
        );

        if let Some(dispatch) = dispatch {
            call = call.with_trait_dispatch(dispatch);
        }

        let value = self.push_typed_value_operation(
            id,
            block,
            Self::retained_source(&source),
            MirOperationKind::Call(call),
            result_type,
        )?;

        Ok(LoweredExpression::continuing(block, Some(value), source))
    }

    fn index_bound_type(
        &self,
        requirement: ImplementationRequirementKey,
    ) -> Result<TypeId, LoweringError> {
        let values = self.input.semantic_values();

        let application = values
            .trait_application_data(requirement.trait_application())
            .map_err(|_| LoweringError::SemanticValueUnavailable)?;

        let substitution = values
            .generic_substitution_data(application.substitution())
            .map_err(|_| LoweringError::SemanticValueUnavailable)?;

        let [binding] = substitution.bindings() else {
            return Err(LoweringError::SemanticValueUnavailable);
        };

        match binding.argument() {
            GenericArgument::Type(bound) => Ok(bound),
            GenericArgument::Constant(_) => Err(LoweringError::SemanticValueUnavailable),
        }
    }
}
