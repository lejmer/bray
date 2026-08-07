// rust-style: allow(module-too-large, reason = "core expression lowering keeps one exhaustive bound-to-MIR operation dispatch")

use bray_bound_tree::{
    BoundCallResult, BoundCallableTarget, BoundExpression, BoundExpressionId, BoundOperator,
    OperatorTarget, SelectedArgument, SelectedOperation, SemanticSelection, StorageAccessPurpose,
    StorageIdentity, StorageIdentityId,
};
use bray_ir::{
    MirAggregate, MirAggregateKind, MirBinaryOperator, MirBlockId, MirBlockKind, MirCall,
    MirCallArgument, MirCallIntrinsic, MirCallTarget, MirCallableReference, MirEdge,
    MirImmediateValue, MirOperand, MirOperationKind, MirPatternPredicate, MirPlace,
    MirSourceAnchor, MirStorageKind, MirStoreKind, MirTerminatorKind, MirUnaryOperator,
};
use bray_symbols::{CallableAbi, TypeId};

use super::super::super::LoweringError;
use super::super::super::block::LoweredExpression;
use super::super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(in crate::lowering) fn lower_expression(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let expression = self
            .input
            .unit()
            .view()
            .expression(id)
            .ok_or_else(|| LoweringError::MissingBoundNode(id.into()))?;

        if expression.is_recovered() {
            return Err(LoweringError::RecoveredBoundNode(id.into()));
        }

        let lowered = match expression {
            BoundExpression::Block(expression) => {
                self.lower_block_expression(id, expression, current)
            }
            BoundExpression::Literal(expression) => {
                let source = self.source(expression.origin());

                let value = self
                    .input
                    .literal_values()
                    .expression(id)
                    .ok_or(LoweringError::MissingLiteralValue(id))?;

                let ty = self.expression_type(id)?;

                Ok(LoweredExpression::continuing(
                    current,
                    Some(MirOperand::Constant { value, ty }),
                    source,
                ))
            }
            BoundExpression::Name(_) => {
                if let Some(value) = self.input.constant_reference_value(id) {
                    let source = self.source(expression.origin());
                    let ty = self.expression_type(id)?;

                    Ok(LoweredExpression::continuing(
                        current,
                        Some(MirOperand::Constant { value, ty }),
                        source,
                    ))
                } else {
                    self.lower_storage_operand(id, current)
                }
            }
            BoundExpression::PatternReference(_) => self.lower_storage_operand(id, current),
            BoundExpression::Unary(expression) => {
                self.lower_unary(id, expression.operator(), expression.operands(), current)
            }
            BoundExpression::Binary(expression) => {
                self.lower_binary(id, expression.operator(), expression.operands(), current)
            }
            BoundExpression::Assignment(expression) => {
                self.lower_assignment(id, expression.operands(), current)
            }
            BoundExpression::Call(_) => {
                if matches!(
                    self.input.semantic_selections().expression(id),
                    Some(SemanticSelection::Operation(
                        SelectedOperation::Construction(_)
                    ))
                ) {
                    self.lower_construction(id, current)
                } else {
                    self.lower_call(id, current)
                }
            }
            BoundExpression::Conversion(expression) => {
                let lowered = self.lower_expression(expression.operand(), current)?;

                let Some(current) = lowered.block else {
                    return Ok(lowered);
                };

                let Some(operand) = lowered.value else {
                    return Err(LoweringError::MissingOperationResult(expression.operand()));
                };

                // Retain the checked selection before recursively mutating lowering state.
                let selection = self.selected_operation(id)?.clone();

                let SelectedOperation::Conversion(conversion) = selection else {
                    return Err(LoweringError::MissingSemanticSelection(id));
                };

                let source = self.source(expression.origin());

                let operand = self.convert_operand(
                    id,
                    current,
                    Self::retained_source(&source),
                    operand,
                    &conversion,
                )?;

                Ok(LoweredExpression::continuing(
                    current,
                    Some(operand),
                    source,
                ))
            }
            BoundExpression::Structured(expression) => {
                self.lower_structured(id, expression, current)
            }
            BoundExpression::ControlTransfer(expression) => {
                self.lower_control_transfer(id, expression, current)
            }
            BoundExpression::StructConstruction(_)
            | BoundExpression::LeadingDotVariant(_)
            | BoundExpression::UnqualifiedVariant(_) => self.lower_construction(id, current),
            BoundExpression::MemberAccess(_) | BoundExpression::TraitQualifiedMember(_) => {
                self.lower_member_access(id, current)
            }
            BoundExpression::For(expression) => self.lower_for(id, expression, current),
            BoundExpression::Match(expression) => self.lower_match(id, expression, current),
            BoundExpression::Generator(expression) => {
                self.lower_generator_iteration(id, expression, current)
            }
            BoundExpression::AnonymousCallable(expression) => {
                let source = self.source(expression.origin());

                let value = self.push_value_operation(
                    id,
                    current,
                    Self::retained_source(&source),
                    MirOperationKind::AnonymousCallable(
                        // MIR owns the same immutable nested-unit identity independently of HIR.
                        expression.unit().clone(),
                    ),
                )?;

                Ok(LoweredExpression::continuing(current, Some(value), source))
            }
            BoundExpression::Await(expression) => self.lower_await(id, *expression, current),
            BoundExpression::UnresolvedReference(_)
            | BoundExpression::ErrorCall(_)
            | BoundExpression::ErrorConversion(_)
            | BoundExpression::Error(_) => Err(LoweringError::UnsupportedExpression(id)),
        }?;

        self.materialize_temporary(id, lowered)
    }

    fn lower_unary(
        &mut self,
        id: BoundExpressionId,
        operator: BoundOperator,
        operands: &[BoundExpressionId],
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let [operand] = operands else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let selection = self.selected_operator(id)?;
        let lowered = self.lower_operator_operand(id, *operand, selection, current)?;

        let Some(current) = lowered.block else {
            return Ok(lowered);
        };

        let Some(operand) = lowered.value else {
            return Err(LoweringError::MissingOperationResult(*operand));
        };

        let source = self.expression_source(id)?;

        let value = match selection {
            OperatorTarget::BuiltIn(_) => {
                if operator == BoundOperator::Add {
                    return Ok(LoweredExpression::continuing(
                        current,
                        Some(operand),
                        source,
                    ));
                }

                let operator = match operator {
                    BoundOperator::Subtract => MirUnaryOperator::Negate,
                    BoundOperator::LogicalNot => MirUnaryOperator::Not,
                    BoundOperator::BitwiseNot => MirUnaryOperator::BitwiseNot,
                    _ => return Err(LoweringError::UnsupportedOperator(operator)),
                };

                self.push_value_operation(
                    id,
                    current,
                    Self::retained_source(&source),
                    MirOperationKind::Unary { operator, operand },
                )?
            }
            OperatorTarget::Trait {
                fulfillment,
                requirement,
                witness,
                ..
            } => self.push_value_operation(
                id,
                current,
                Self::retained_source(&source),
                MirOperationKind::Call(MirCall::protocol(
                    MirCallTarget::Direct(MirCallableReference::new(
                        fulfillment,
                        CallableAbi::Bray,
                    )),
                    BoundCallResult::Immediate(self.expression_type(id)?),
                    [operand],
                    [bray_bound_tree::SelectedImplementationWitness::new(
                        requirement,
                        witness,
                    )],
                )),
            )?,
            OperatorTarget::TraitConstraint {
                member, dispatch, ..
            } => {
                let intrinsic = unary_operator(operator)
                    .map(MirCallIntrinsic::Unary)
                    .ok_or(LoweringError::UnsupportedOperator(operator))?;

                self.push_value_operation(
                    id,
                    current,
                    Self::retained_source(&source),
                    MirOperationKind::Call(
                        MirCall::protocol(
                            MirCallTarget::Direct(MirCallableReference::new(
                                member,
                                CallableAbi::Bray,
                            )),
                            BoundCallResult::Immediate(self.expression_type(id)?),
                            [operand],
                            [],
                        )
                        .with_trait_dispatch(dispatch)
                        .with_intrinsic(intrinsic),
                    ),
                )?
            }
        };

        Ok(LoweredExpression::continuing(current, Some(value), source))
    }

    fn lower_binary(
        &mut self,
        id: BoundExpressionId,
        operator: BoundOperator,
        operands: &[BoundExpressionId],
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        if matches!(
            operator,
            BoundOperator::LogicalAnd | BoundOperator::LogicalOr
        ) {
            return self.lower_short_circuit(id, operator, operands, current);
        }

        let [left_id, right_id] = operands else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let selection = self.selected_operator(id)?;
        let left = self.lower_operator_operand(id, *left_id, selection, current)?;

        let Some(current) = left.block else {
            return Ok(left);
        };

        let Some(left) = left.value else {
            return Err(LoweringError::MissingOperationResult(*left_id));
        };

        let right = self.lower_operator_operand(id, *right_id, selection, current)?;

        let Some(current) = right.block else {
            return Ok(right);
        };

        let Some(right) = right.value else {
            return Err(LoweringError::MissingOperationResult(*right_id));
        };

        let source = self.expression_source(id)?;

        let (current, value) = match selection {
            OperatorTarget::BuiltIn(_) => {
                let operator = binary_operator(operator)
                    .ok_or(LoweringError::UnsupportedOperator(operator))?;

                let value = self.push_value_operation(
                    id,
                    current,
                    Self::retained_source(&source),
                    MirOperationKind::Binary {
                        operator,
                        left,
                        right,
                    },
                )?;

                (current, value)
            }
            OperatorTarget::Trait {
                fulfillment,
                requirement,
                witness,
                ..
            } => {
                let call_result_type = self.trait_binary_result_type(id, operator)?;

                let value = self.push_typed_value_operation(
                    id,
                    current,
                    Self::retained_source(&source),
                    MirOperationKind::Call(MirCall::protocol(
                        MirCallTarget::Direct(MirCallableReference::new(
                            fulfillment,
                            CallableAbi::Bray,
                        )),
                        BoundCallResult::Immediate(call_result_type),
                        [left, right],
                        [bray_bound_tree::SelectedImplementationWitness::new(
                            requirement,
                            witness,
                        )],
                    )),
                    call_result_type,
                )?;

                self.lower_trait_binary_result(id, operator, current, source.clone(), value)?
            }
            OperatorTarget::TraitConstraint {
                member, dispatch, ..
            } => {
                let intrinsic_operator = match operator {
                    BoundOperator::NotEqual => BoundOperator::Equal,
                    _ => operator,
                };

                let intrinsic = binary_operator(intrinsic_operator)
                    .map(MirCallIntrinsic::Binary)
                    .ok_or(LoweringError::UnsupportedOperator(operator))?;

                let value = self.push_value_operation(
                    id,
                    current,
                    Self::retained_source(&source),
                    MirOperationKind::Call(
                        MirCall::protocol(
                            MirCallTarget::Direct(MirCallableReference::new(
                                member,
                                CallableAbi::Bray,
                            )),
                            BoundCallResult::Immediate(self.expression_type(id)?),
                            [left, right],
                            [],
                        )
                        .with_trait_dispatch(dispatch)
                        .with_intrinsic(intrinsic),
                    ),
                )?;

                if operator == BoundOperator::NotEqual {
                    let value = self.push_value_operation(
                        id,
                        current,
                        Self::retained_source(&source),
                        MirOperationKind::Unary {
                            operator: MirUnaryOperator::Not,
                            operand: value,
                        },
                    )?;

                    (current, value)
                } else {
                    (current, value)
                }
            }
        };

        Ok(LoweredExpression::continuing(current, Some(value), source))
    }

    fn trait_binary_result_type(
        &self,
        expression: BoundExpressionId,
        operator: BoundOperator,
    ) -> Result<TypeId, LoweringError> {
        if is_relational_operator(operator) {
            return Ok(self.ordering_representation()?.ty);
        }

        self.expression_type(expression)
    }

    fn lower_trait_binary_result(
        &mut self,
        expression: BoundExpressionId,
        operator: BoundOperator,
        current: MirBlockId,
        source: MirSourceAnchor,
        value: MirOperand,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        if operator == BoundOperator::NotEqual {
            let value = self.push_value_operation(
                expression,
                current,
                Self::retained_source(&source),
                MirOperationKind::Unary {
                    operator: MirUnaryOperator::Not,
                    operand: value,
                },
            )?;

            return Ok((current, value));
        }

        if !is_relational_operator(operator) {
            return Ok((current, value));
        }

        let representation = self.ordering_representation()?;
        let result_type = self.expression_type(expression)?;

        let join = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let result =
            self.builder
                .push_block_parameter(join, Self::retained_source(&source), result_type)?;

        let (variant, matched_result) = match operator {
            BoundOperator::Less => (representation.less_variant, true),
            BoundOperator::LessEqual => (representation.greater_variant, false),
            BoundOperator::Greater => (representation.greater_variant, true),
            BoundOperator::GreaterEqual => (representation.less_variant, false),
            _ => return Err(LoweringError::UnsupportedOperator(operator)),
        };

        self.builder.set_terminator(
            current,
            source,
            MirTerminatorKind::PatternBranch {
                subject: value,
                predicate: MirPatternPredicate::ActiveUnionVariant(variant),
                matched: MirEdge::new(join, [self.boolean_operand(result_type, matched_result)]),
                unmatched: MirEdge::new(join, [self.boolean_operand(result_type, !matched_result)]),
            },
        )?;

        Ok((join, MirOperand::Value(result)))
    }

    fn lower_assignment(
        &mut self,
        id: BoundExpressionId,
        operands: &[BoundExpressionId],
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let [destination, value_id] = operands else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        let (current, destination) = match self.lower_expression_place(
            *destination,
            StorageAccessPurpose::Write,
            current,
        )? {
            super::super::access::LoweredPlace::Continuing { block, place } => (block, place),
            super::super::access::LoweredPlace::Terminated(completion) => return Ok(completion),
        };

        let value = self.lower_expression(*value_id, current)?;

        let Some(current) = value.block else {
            return Ok(value);
        };

        let Some(mut value) = value.value else {
            return Err(LoweringError::MissingOperationResult(*value_id));
        };

        let source = self.expression_source(id)?;
        let value_type = self.expression_type(*value_id)?;
        let destination_type = destination.ty();

        let destination_data = self
            .input
            .semantic_values()
            .type_data(destination_type)
            .map_err(|_| LoweringError::SemanticValueUnavailable)?;

        if matches!(destination_data.as_ref(), bray_symbols::TypeData::Nullable(element) if *element == value_type)
        {
            let commit = self.builder.push_operation(
                current,
                Self::retained_source(&source),
                MirOperationKind::Aggregate(MirAggregate::new(
                    MirAggregateKind::NullablePresent,
                    [value],
                )),
                Some(destination_type),
            )?;

            value = commit
                .result()
                .map(MirOperand::Value)
                .ok_or(LoweringError::MissingOperationResult(*value_id))?;
        }

        self.builder.push_operation(
            current,
            Self::retained_source(&source),
            MirOperationKind::Store {
                kind: MirStoreKind::Assign,
                destination,
                value,
            },
            None,
        )?;

        let value = self.unit_operand(self.expression_type(id)?);

        Ok(LoweredExpression::continuing(current, Some(value), source))
    }

    fn lower_call(
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

        if let Some(operation) = self
            .input
            .storage_flow()
            .checked_memory_operation(id)
            .cloned()
        {
            return self.lower_memory_call(id, current, source, &selection, operation);
        }

        if let Some(hook) = selection.implementation_hook()
            && let Some(kind) = super::super::text::text_operation_kind(hook)
        {
            return self.lower_text_call(id, current, source, &selection, kind);
        }

        let mut arguments = Vec::new();

        let target = match selection.target() {
            BoundCallableTarget::Declaration(instance) => {
                MirCallTarget::Direct(MirCallableReference::new(instance, selection.abi()))
            }
            BoundCallableTarget::Indirect(_) => {
                let callee = self.lower_expression(expression.callee(), current)?;

                let Some(continuation) = callee.block else {
                    return Ok(callee);
                };

                current = continuation;

                let Some(callee) = callee.value else {
                    return Err(LoweringError::MissingOperationResult(expression.callee()));
                };

                MirCallTarget::Indirect {
                    callee,
                    abi: selection.abi(),
                }
            }
            BoundCallableTarget::Predicate(_) | BoundCallableTarget::Anonymous(_) => {
                return Err(LoweringError::UnsupportedExpression(id));
            }
        };

        if let Some(receiver) = selection.receiver() {
            let (lowered, _) = self.lower_call_receiver(receiver, current)?;

            let Some(continuation) = lowered.block else {
                return Ok(lowered);
            };

            current = continuation;

            let Some(operand) = lowered.value else {
                return Err(LoweringError::MissingOperationResult(receiver.expression()));
            };

            arguments.push(MirCallArgument::Receiver {
                parameter: receiver.parameter(),
                value: operand,
            });
        }

        for argument in selection.arguments() {
            match argument {
                SelectedArgument::Default {
                    parameter,
                    ordinal,
                    provider,
                } => {
                    arguments.push(MirCallArgument::Default {
                        parameter: *parameter,
                        ordinal: *ordinal,
                        provider: *provider,
                    });
                }
                SelectedArgument::Explicit {
                    expression,
                    parameter,
                    ordinal,
                    conversion,
                } => {
                    let lowered = self.lower_expression(*expression, current)?;

                    let Some(continuation) = lowered.block else {
                        return Ok(lowered);
                    };

                    current = continuation;

                    let Some(operand) = lowered.value else {
                        return Err(LoweringError::MissingOperationResult(*expression));
                    };

                    let value = self.convert_operand(
                        id,
                        current,
                        Self::retained_source(&source),
                        operand,
                        conversion,
                    )?;

                    arguments.push(MirCallArgument::Explicit {
                        parameter: *parameter,
                        ordinal: *ordinal,
                        value,
                    });
                }
            }
        }

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

        let value = self.lower_call_operation(id, current, Self::retained_source(&source), call)?;

        Ok(LoweredExpression::continuing(current, Some(value), source))
    }

    pub(in crate::lowering::expression) fn selected_operation(
        &self,
        expression: BoundExpressionId,
    ) -> Result<&SelectedOperation, LoweringError> {
        let operation = self
            .input
            .semantic_selections()
            .expression(expression)
            .and_then(|selection| match selection {
                SemanticSelection::Operation(operation) => Some(operation),
                _ => None,
            });

        operation.ok_or(LoweringError::MissingSemanticSelection(expression))
    }

    fn selected_operator(
        &self,
        expression: BoundExpressionId,
    ) -> Result<OperatorTarget, LoweringError> {
        let SelectedOperation::Operator { target, .. } = self.selected_operation(expression)?
        else {
            return Err(LoweringError::MissingSemanticSelection(expression));
        };

        Ok(*target)
    }

    fn lower_operator_operand(
        &mut self,
        parent: BoundExpressionId,
        operand: BoundExpressionId,
        target: OperatorTarget,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        match target {
            OperatorTarget::BuiltIn(_) => self.lower_expression(operand, current),
            OperatorTarget::Trait { .. } | OperatorTarget::TraitConstraint { .. } => {
                self.lower_implicit_shared_borrow(parent, operand, current)
            }
        }
    }

    pub(in crate::lowering) fn push_value_operation(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: MirSourceAnchor,
        operation: MirOperationKind,
    ) -> Result<MirOperand, LoweringError> {
        let result_type = self.expression_type(expression)?;

        self.push_typed_value_operation(expression, current, source, operation, result_type)
    }

    fn push_typed_value_operation(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: MirSourceAnchor,
        operation: MirOperationKind,
        result_type: TypeId,
    ) -> Result<MirOperand, LoweringError> {
        let commit = self
            .builder
            .push_operation(current, source, operation, Some(result_type))?;

        commit
            .result()
            .map(MirOperand::Value)
            .ok_or(LoweringError::MissingOperationResult(expression))
    }

    pub(in crate::lowering::expression) fn expression_type(
        &self,
        expression: BoundExpressionId,
    ) -> Result<TypeId, LoweringError> {
        self.input
            .expression_types()
            .expression(expression)
            .map(|result| result.ty())
            .ok_or(LoweringError::MissingExpressionType(expression))
    }

    pub(in crate::lowering::expression) fn expression_source(
        &self,
        expression: BoundExpressionId,
    ) -> Result<MirSourceAnchor, LoweringError> {
        self.input
            .unit()
            .view()
            .expression(expression)
            .map(|expression| self.source(expression.origin()))
            .ok_or_else(|| LoweringError::MissingBoundNode(expression.into()))
    }

    pub(in crate::lowering) const fn unit_operand(&self, ty: TypeId) -> MirOperand {
        Self::immediate_operand(ty, MirImmediateValue::Unit)
    }

    pub(in crate::lowering::expression) const fn immediate_operand(
        ty: TypeId,
        value: MirImmediateValue,
    ) -> MirOperand {
        MirOperand::Immediate { value, ty }
    }

    pub(in crate::lowering) fn place_for_identity(
        &mut self,
        id: StorageIdentityId,
        ty: TypeId,
        origin: bray_bound_tree::BoundNodeOrigin,
    ) -> Result<MirPlace, LoweringError> {
        let storage = match self.storages.get(&id).copied() {
            Some(storage) => storage,
            None => {
                let identity = self
                    .input
                    .storage_plan()
                    .identity(id)
                    .ok_or(LoweringError::MissingStorageIdentityRecord(id))?;

                let kind = storage_kind(identity, self.parameter_positions.get(&id).copied());

                let storage = self.builder.push_storage(self.source(origin), kind, ty)?;

                self.storages.insert(id, storage);

                storage
            }
        };

        Ok(MirPlace::new(storage, [], ty))
    }
}

fn is_relational_operator(operator: BoundOperator) -> bool {
    matches!(
        operator,
        BoundOperator::Less
            | BoundOperator::LessEqual
            | BoundOperator::Greater
            | BoundOperator::GreaterEqual
    )
}

fn binary_operator(operator: BoundOperator) -> Option<MirBinaryOperator> {
    match operator {
        BoundOperator::Equal => Some(MirBinaryOperator::Equal),
        BoundOperator::NotEqual => Some(MirBinaryOperator::NotEqual),
        BoundOperator::Less => Some(MirBinaryOperator::LessThan),
        BoundOperator::LessEqual => Some(MirBinaryOperator::LessThanOrEqual),
        BoundOperator::Greater => Some(MirBinaryOperator::GreaterThan),
        BoundOperator::GreaterEqual => Some(MirBinaryOperator::GreaterThanOrEqual),
        BoundOperator::BitwiseOr => Some(MirBinaryOperator::BitwiseOr),
        BoundOperator::BitwiseXor => Some(MirBinaryOperator::BitwiseXor),
        BoundOperator::BitwiseAnd => Some(MirBinaryOperator::BitwiseAnd),
        BoundOperator::ShiftLeft => Some(MirBinaryOperator::ShiftLeft),
        BoundOperator::ShiftRight => Some(MirBinaryOperator::ShiftRight),
        BoundOperator::Add => Some(MirBinaryOperator::Add),
        BoundOperator::Subtract => Some(MirBinaryOperator::Subtract),
        BoundOperator::Multiply => Some(MirBinaryOperator::Multiply),
        BoundOperator::Divide => Some(MirBinaryOperator::Divide),
        BoundOperator::Remainder => Some(MirBinaryOperator::Remainder),
        BoundOperator::Assign
        | BoundOperator::LogicalOr
        | BoundOperator::LogicalAnd
        | BoundOperator::MatrixMultiply
        | BoundOperator::Exponentiate
        | BoundOperator::BitwiseNot
        | BoundOperator::LogicalNot => None,
    }
}

fn unary_operator(operator: BoundOperator) -> Option<MirUnaryOperator> {
    match operator {
        BoundOperator::Subtract => Some(MirUnaryOperator::Negate),
        BoundOperator::LogicalNot => Some(MirUnaryOperator::Not),
        BoundOperator::BitwiseNot => Some(MirUnaryOperator::BitwiseNot),
        _ => None,
    }
}

fn storage_kind(identity: StorageIdentity, parameter_position: Option<u32>) -> MirStorageKind {
    match identity {
        StorageIdentity::Parameter(_)
        | StorageIdentity::Receiver(_)
        | StorageIdentity::AnonymousParameter(_)
        | StorageIdentity::PredicateParameter(_) => {
            MirStorageKind::Parameter(parameter_position.unwrap_or(u32::MAX))
        }
        StorageIdentity::LocalOwned(_) | StorageIdentity::Alternative { .. } => {
            MirStorageKind::Local
        }
        StorageIdentity::Result(_) => MirStorageKind::Return,
        StorageIdentity::Temporary(_)
        | StorageIdentity::PostconditionResult(_)
        | StorageIdentity::IterationCursor(_)
        | StorageIdentity::IterationElement(_)
        | StorageIdentity::Allocation(_)
        | StorageIdentity::CompilerCreated(_)
        | StorageIdentity::Error(_) => MirStorageKind::Temporary,
    }
}
