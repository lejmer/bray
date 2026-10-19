use bray_bound_tree::BoundStructuredExpressionKind;
use bray_bound_tree::{BoundExpression, BoundExpressionId, IndexTarget, SelectedOperation};
use bray_ir::{MirBlockId, MirCall, MirCallTarget, MirCallableReference, MirImmediateValue};
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
        let target = match self.selected_operation(id) {
            SelectedOperation::Index { target, .. } => *target,
            _ => panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = id
            ),
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
        let target = match self.selected_operation(id) {
            SelectedOperation::Index { target, .. } => *target,
            _ => panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = id
            ),
        };

        let retained = self.input_temporaries.len();

        let result = match target {
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
            | IndexTarget::Slice => panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = id
            ),
        };

        self.input_temporaries.truncate(retained);

        result
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
            .unwrap_or_else(|| {
                panic!(
                    "lowering contract violation: MissingBoundNode {value:?}",
                    value = id
                )
            });

        let kind = expression.kind();
        let operands = expression.operands().to_vec();
        let slice_bounds = expression.slice_bounds();

        let source = self.expression_source(id);
        let mut block = current;
        let mut arguments = Vec::with_capacity(3);

        let Some(receiver) = operands.first().copied() else {
            panic!(
                "lowering contract violation: MissingBoundNode {value:?}",
                value = id
            );
        };

        let target = requirement.subject();

        let receiver_type = self.input.semantic_values().intern_type(TypeData::Borrow {
            kind: borrow_kind,
            target,
        })?;

        let lowered = self.lower_storage_borrow(
            receiver,
            id,
            block,
            borrow_kind,
            target,
            receiver_type,
            self.expression_source(receiver),
        )?;

        let lowered = if self.later_evaluation_may_change_block(operands.iter().skip(1).copied()) {
            self.materialize_typed_for_later_evaluation(receiver, lowered, receiver_type)?
        } else {
            lowered
        };

        let Some(continuation) = lowered.block else {
            return Ok(lowered);
        };

        let Some(value) = lowered.value else {
            panic!(
                "lowering contract violation: MissingOperationResult {value:?}",
                value = receiver
            );
        };

        block = continuation;
        arguments.push(value);

        match kind {
            BoundStructuredExpressionKind::ElementIndex => {
                for (index, operand) in operands.iter().copied().enumerate().skip(1) {
                    let lowered = self.lower_implicit_shared_borrow(id, operand, block)?;

                    let borrowed_type =
                        self.input.semantic_values().intern_type(TypeData::Borrow {
                            kind: bray_symbols::BorrowKind::Shared,
                            target: self.expression_type(operand),
                        })?;

                    let lowered = if self
                        .later_evaluation_may_change_block(operands[index + 1..].iter().copied())
                    {
                        self.materialize_typed_for_later_evaluation(
                            operand,
                            lowered,
                            borrowed_type,
                        )?
                    } else {
                        lowered
                    };

                    let Some(continuation) = lowered.block else {
                        return Ok(lowered);
                    };

                    let Some(value) = lowered.value else {
                        panic!(
                            "lowering contract violation: MissingOperationResult {value:?}",
                            value = operand
                        );
                    };

                    block = continuation;
                    arguments.push(value);
                }
            }
            BoundStructuredExpressionKind::SliceIndex => {
                let bounds = slice_bounds.unwrap_or_else(|| {
                    panic!(
                        "lowering contract violation: MissingBoundNode {value:?}",
                        value = id
                    )
                });

                let bound_type = self.index_bound_type(requirement);

                let nullable_type = self
                    .input
                    .semantic_values()
                    .intern_type(TypeData::Nullable(bound_type))?;

                let bounds = [bounds.lower(), bounds.upper()];

                for (index, bound) in bounds.into_iter().enumerate() {
                    let lowered = self.lower_index_bound(bound, block, nullable_type, &source)?;

                    let Some(continuation) = lowered.block else {
                        return Ok(lowered);
                    };

                    let value = lowered
                        .value
                        .expect("custom slice supplies its nullable bound");

                    block = continuation;

                    let value = if bound.is_some()
                        && self.later_evaluation_may_change_block(
                            bounds[index + 1..].iter().flatten().copied(),
                        ) {
                        self.materialize_owned_input(block, &source, value, nullable_type)?
                    } else {
                        value
                    };

                    arguments.push(value);
                }
            }
            _ => panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = id
            ),
        }

        let witnesses = witness.into_iter().map(|witness| {
            bray_bound_tree::SelectedImplementationWitness::new(requirement, witness)
        });

        let result_type = self.input.semantic_values().intern_type(TypeData::Borrow {
            kind: borrow_kind,
            target: self.expression_type(id),
        })?;

        let mut call = MirCall::protocol(
            MirCallTarget::Direct(MirCallableReference::new(callable, CallableAbi::Bray)),
            bray_bound_tree::BoundCallResult::Immediate(result_type),
            arguments,
            witnesses,
        );

        if let Some(dispatch) = dispatch {
            call = call.with_trait_dispatch(dispatch);
        }

        let (block, value) =
            self.push_checked_call(id, block, Self::retained_source(&source), call, result_type)?;

        Ok(LoweredExpression::continuing(block, Some(value), source))
    }

    fn lower_index_bound(
        &mut self,
        bound: Option<BoundExpressionId>,
        block: MirBlockId,
        nullable_type: TypeId,
        source: &bray_ir::MirSourceAnchor,
    ) -> Result<LoweredExpression, LoweringError> {
        let Some(bound) = bound else {
            return Ok(LoweredExpression::continuing(
                block,
                Some(Self::immediate_operand(
                    nullable_type,
                    MirImmediateValue::NullableAbsent,
                )),
                Self::retained_source(source),
            ));
        };

        let lowered = self.lower_expression(bound, block)?;

        let Some(block) = lowered.block else {
            return Ok(lowered);
        };

        let value = lowered
            .value
            .expect("present custom slice bound produces its value");

        let value = self.push_nullable_present(
            bound,
            block,
            Self::retained_source(source),
            value,
            nullable_type,
        )?;

        Ok(LoweredExpression::continuing(
            block,
            Some(value),
            Self::retained_source(source),
        ))
    }

    fn index_bound_type(&self, requirement: ImplementationRequirementKey) -> TypeId {
        let values = self.input.semantic_values();

        let application = values.trait_application_data(requirement.trait_application());

        let substitution = values.generic_substitution_data(application.substitution());

        let [binding] = substitution.bindings() else {
            panic!(
                "lowering index contract violated: requirement {requirement:?} must bind exactly one type, found {} bindings",
                substitution.bindings().len()
            );
        };

        match binding.argument() {
            GenericArgument::Type(bound) => bound,
            GenericArgument::Constant(constant) => panic!(
                "lowering index contract violated: requirement {requirement:?} bound constant {constant:?} instead of an index type"
            ),
        }
    }
}
