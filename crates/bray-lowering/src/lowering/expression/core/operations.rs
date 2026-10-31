// rust-style: allow(module-too-large, reason = "core expression lowering keeps one exhaustive bound-to-MIR operation dispatch")

use bray_bound_tree::{
    BoundAssignmentOperator, BoundCallResult, BoundCallableTarget, BoundExpression,
    BoundExpressionId, BoundNameExpression, BoundOperator, BoundReferenceTarget, ConversionTarget,
    OperatorTarget, SelectedArgument, SelectedConversion, SelectedOperation, SemanticSelection,
    StorageAccessPurpose, StorageIdentity, StorageIdentityId, StorageOperationStatus,
};
use bray_compiler_known::{CompilerKnownOperationRole, RepresentationRole};
use bray_ir::{
    MirAggregate, MirAggregateKind, MirBinaryOperator, MirBlockId, MirBlockKind, MirCall,
    MirCallArgument, MirCallIntrinsic, MirCallTarget, MirCallableReference, MirEdge,
    MirImmediateValue, MirOperand, MirOperationKind, MirPatternPredicate, MirPlace,
    MirSourceAnchor, MirStorageKind, MirTerminatorKind, MirUnaryOperator,
};
use bray_symbols::{
    BorrowKind, CallableAbi, CallableDefinitionId, CallableInstanceData, ConstantValueData,
    ConstantValueKind, GenericOwnerId, GenericSubstitutionData, TypeData, TypeId,
};

use super::super::super::LoweringError;
use super::super::super::block::LoweredExpression;
use super::super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(in crate::lowering) fn lower_expression(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let scope = self
            .input
            .unit()
            .view()
            .expression(id)
            .and_then(|expression| bray_bound_tree::StorageScope::for_iteration(id, expression));

        if let Some(scope) = scope {
            self.active_scopes.push(scope);
        }

        let lowered = self.lower_expression_inner(id, current);

        let lowered = lowered.and_then(|lowered| {
            if scope.is_some() {
                self.finish_iteration_scope(id, lowered)
            } else {
                Ok(lowered)
            }
        });

        if let Some(scope) = scope {
            assert_eq!(
                self.active_scopes.pop(),
                Some(scope),
                "iteration cleanup scopes remain nested"
            );
        }

        self.materialize_temporary(id, lowered?)
    }

    fn lower_expression_inner(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let expression = self.input.unit().view().expression(id).unwrap_or_else(|| {
            panic!(
                "lowering contract violation: MissingBoundNode {value:?}",
                value = id
            )
        });

        if expression.is_recovered() {
            panic!(
                "lowering contract violation: RecoveredBoundNode {value:?}",
                value = id
            );
        }

        if self
            .input
            .observed_generator(id)
            .is_some_and(|producer| producer != id)
        {
            assert!(
                matches!(expression, BoundExpression::Name(_)),
                "elided generator aliases retain only their eager producer identity"
            );

            return Ok(LoweredExpression::continuing(
                current,
                None,
                self.source(expression.origin()),
            ));
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
                    .unwrap_or_else(|| {
                        panic!(
                            "lowering contract violation: MissingLiteralValue {value:?}",
                            value = id
                        )
                    });

                let ty = self.expression_type(id);

                Ok(LoweredExpression::continuing(
                    current,
                    Some(MirOperand::Constant { value, ty }),
                    source,
                ))
            }
            BoundExpression::Name(name) => {
                if let Some(operand) = self.input.constant_reference_operand(id) {
                    let source = self.source(expression.origin());

                    Ok(LoweredExpression::continuing(
                        current,
                        Some(operand.clone()),
                        source,
                    ))
                } else if let Some(callable) = self.declared_callable_reference(id, *name)? {
                    let source = self.source(expression.origin());

                    let value = self.push_value_operation(
                        id,
                        current,
                        Self::retained_source(&source),
                        MirOperationKind::DeclaredCallable(callable),
                    )?;

                    Ok(LoweredExpression::continuing(current, Some(value), source))
                } else {
                    self.lower_storage_operand(id, current)
                }
            }
            BoundExpression::PatternReference(_) => self.lower_storage_operand(id, current),
            BoundExpression::Unary(expression) => {
                self.lower_unary(id, expression.operator(), expression.operands(), current)
            }
            BoundExpression::Binary(expression) => {
                if let Some(components) = self.input.unit().view().complex_literal_components(id) {
                    self.lower_complex_literal(id, expression.operator(), components, current)
                } else {
                    self.lower_binary(id, expression.operator(), expression.operands(), current)
                }
            }
            BoundExpression::Assignment(expression) => {
                self.lower_assignment(id, expression.operator(), expression.operands(), current)
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
                    panic!(
                        "lowering contract violation: MissingOperationResult {value:?}",
                        value = expression.operand()
                    );
                };

                // Retain the checked selection before recursively mutating lowering state.
                let selection = self.selected_operation(id).clone();

                let SelectedOperation::Conversion(conversion) = selection else {
                    panic!(
                        "lowering contract violation: MissingSemanticSelection {value:?}",
                        value = id
                    );
                };

                let source = self.source(expression.origin());

                let (current, operand) = self.convert_operand(
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
            | BoundExpression::BoxConstruction(_)
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
                        bray_ir::MirAnonymousCallableReference::bound(
                            // MIR owns the same immutable nested-unit identity independently of HIR.
                            expression.unit().clone(),
                        ),
                    ),
                )?;

                Ok(LoweredExpression::continuing(current, Some(value), source))
            }
            BoundExpression::Await(expression) => self.lower_await(id, *expression, current),
            BoundExpression::UnresolvedReference(_)
            | BoundExpression::ErrorCall(_)
            | BoundExpression::ErrorConversion(_)
            | BoundExpression::Error(_) => panic!(
                "lowering contract violation: UnsupportedExpression {value:?}",
                value = id
            ),
        }?;

        if let Some(current) = lowered.block
            && (self.type_representation(self.expression_type(id))
                == Some(RepresentationRole::Never)
                || (self
                    .input
                    .dependency_contracts()
                    .calls()
                    .contains_key(&id.into())
                    || matches!(expression, BoundExpression::Await(_)))
                    && !self.invocation_completes(id.into()))
        {
            self.set_terminator(
                current,
                Self::retained_source(&lowered.source),
                MirTerminatorKind::Unreachable,
            )?;

            return Ok(LoweredExpression::terminated(lowered.source));
        }

        Ok(lowered)
    }

    pub(in crate::lowering) fn invocation_completes(
        &self,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) -> bool {
        self.input
            .storage_flow()
            .completion(bray_bound_tree::StorageMutation::Operation(occurrence))
            .is_some()
    }

    fn declared_callable_reference(
        &self,
        expression: BoundExpressionId,
        name: BoundNameExpression,
    ) -> Result<Option<MirCallableReference>, LoweringError> {
        let BoundReferenceTarget::Surface(symbol) = name.target() else {
            return Ok(None);
        };

        let Some(definition) = CallableDefinitionId::try_new(symbol) else {
            return Ok(None);
        };

        let ty = self.expression_type(expression);

        let data = self.input.semantic_values().type_data(ty);

        let TypeData::Callable(callable) = data.as_ref() else {
            return Ok(None);
        };

        let instance = match self.input.semantic_selections().expression(expression) {
            Some(SemanticSelection::CallableReference(instance)) => *instance,
            Some(_) => panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = expression
            ),
            None => {
                let owner = GenericOwnerId::try_new(definition.symbol())
                    .unwrap_or_else(|| {
                        panic!(
                            "lowering callable-reference contract violated: definition {definition:?} has no generic owner"
                        )
                    });

                let substitution = GenericSubstitutionData::try_new(owner, [], [])
                    .unwrap_or_else(|error| {
                        panic!(
                            "lowering generic substitution contract violated for expression {expression:?}, callable {definition:?}, and owner {owner:?}: {error:?}"
                        )
                    });

                let substitution = self
                    .input
                    .semantic_values()
                    .intern_generic_substitution(substitution)?;

                CallableInstanceData::new(definition, substitution)
            }
        };

        Ok(Some(MirCallableReference::new(instance, callable.abi())))
    }

    fn lower_unary(
        &mut self,
        id: BoundExpressionId,
        operator: BoundOperator,
        operands: &[BoundExpressionId],
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let [operand] = operands else {
            panic!(
                "lowering contract violation: UnsupportedExpression {value:?}",
                value = id
            );
        };

        let selection = self.selected_operator(id);

        // A signed minimum's positive spelling is valid only with its literal minus.
        // Publish that checked literal as one representable MIR value at every level.
        if matches!(selection, OperatorTarget::BuiltIn(_))
            && operator == BoundOperator::Subtract
            && let Some(value) = self.input.literal_values().expression(*operand)
            && let ConstantValueKind::Integer(integer) = self
                .input
                .semantic_values()
                .constant_value_data(value)
                .kind()
        {
            let ty = self.expression_type(id);

            let value =
                self.input
                    .semantic_values()
                    .intern_constant_value(ConstantValueData::new(
                        ty,
                        ConstantValueKind::Integer(integer.negated()),
                    ))?;

            return Ok(LoweredExpression::continuing(
                current,
                Some(MirOperand::Constant { value, ty }),
                self.expression_source(id),
            ));
        }

        let lowered = self.lower_operator_operand(id, *operand, selection, current)?;

        let Some(current) = lowered.block else {
            return Ok(lowered);
        };

        let Some(operand) = lowered.value else {
            panic!(
                "lowering contract violation: MissingOperationResult {value:?}",
                value = *operand
            );
        };

        let source = self.expression_source(id);

        let (current, value) = match selection {
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
                    _ => panic!(
                        "lowering contract violation: operator {operator:?} is invalid for {id:?}"
                    ),
                };

                let value = self.push_value_operation(
                    id,
                    current,
                    Self::retained_source(&source),
                    MirOperationKind::Unary { operator, operand },
                )?;

                (current, value)
            }
            OperatorTarget::Trait {
                fulfillment,
                requirement,
                witness,
                ..
            } => self.push_checked_call(
                id,
                current,
                Self::retained_source(&source),
                MirCall::protocol(
                    MirCallTarget::Direct(MirCallableReference::new(
                        fulfillment,
                        CallableAbi::Bray,
                    )),
                    BoundCallResult::Immediate(self.expression_type(id)),
                    [operand],
                    [bray_bound_tree::SelectedImplementationWitness::new(
                        requirement,
                        witness,
                    )],
                ),
                self.expression_type(id),
            )?,
            OperatorTarget::TraitConstraint {
                member, dispatch, ..
            } => {
                let intrinsic = unary_operator(operator)
                    .map(MirCallIntrinsic::Unary)
                    .unwrap_or_else(|| {
                        panic!(
                            "lowering contract violation: operator {operator:?} is invalid for {id:?}"
                        )
                    });

                self.push_checked_call(
                    id,
                    current,
                    Self::retained_source(&source),
                    MirCall::protocol(
                        MirCallTarget::Direct(MirCallableReference::new(member, CallableAbi::Bray)),
                        BoundCallResult::Immediate(self.expression_type(id)),
                        [operand],
                        [],
                    )
                    .with_trait_dispatch(dispatch)
                    .with_intrinsic(intrinsic),
                    self.expression_type(id),
                )?
            }
        };

        Ok(LoweredExpression::continuing(current, Some(value), source))
    }

    fn lower_complex_literal(
        &mut self,
        id: BoundExpressionId,
        operator: BoundOperator,
        components: [BoundExpressionId; 2],
        mut current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let source = self.expression_source(id);
        let component_type = self.expression_type(components[0]);
        let mut operands = Vec::with_capacity(2);

        for component in components {
            let lowered = self.lower_expression(component, current)?;

            current = lowered
                .block
                .expect("complex literal component continues evaluation");

            operands.push(
                lowered
                    .value
                    .expect("checked complex literal component produces a value"),
            );
        }

        if operator == BoundOperator::Subtract {
            let imaginary = operands.pop().expect("complex literal has two components");

            let imaginary = self.push_typed_value_operation(
                id,
                current,
                Self::retained_source(&source),
                MirOperationKind::Unary {
                    operator: MirUnaryOperator::Negate,
                    operand: imaginary,
                },
                component_type,
            )?;

            operands.push(imaginary);
        }

        let tuple_type = self
            .input
            .semantic_values()
            .intern_type(TypeData::tuple([component_type; 2]))?;

        let tuple = self.push_typed_value_operation(
            id,
            current,
            Self::retained_source(&source),
            MirOperationKind::Aggregate(MirAggregate::new(MirAggregateKind::Tuple, operands)),
            tuple_type,
        )?;

        let conversion = SelectedConversion::new(
            tuple_type,
            self.expression_type(id),
            ConversionTarget::Composite(
                vec![
                    SelectedConversion::new(
                        component_type,
                        component_type,
                        ConversionTarget::Identity,
                    );
                    2
                ]
                .into(),
            ),
        );

        let (current, value) = self.convert_operand(
            id,
            current,
            Self::retained_source(&source),
            tuple,
            &conversion,
        )?;

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
            panic!(
                "lowering contract violation: UnsupportedExpression {value:?}",
                value = id
            );
        };

        let (selection, result_type) = self.selected_operator_operation(id);

        let left = self.lower_operator_operand(id, *left_id, selection, current)?;

        let left_type = match selection {
            OperatorTarget::BuiltIn(_) => self.expression_type(*left_id),
            OperatorTarget::Trait { .. } | OperatorTarget::TraitConstraint { .. } => {
                self.input.semantic_values().intern_type(TypeData::Borrow {
                    kind: BorrowKind::Shared,
                    target: self.expression_type(*left_id),
                })?
            }
        };

        let left = if self.later_evaluation_may_change_block([*right_id]) {
            self.materialize_typed_for_later_evaluation(*left_id, left, left_type)?
        } else {
            left
        };

        let Some(current) = left.block else {
            return Ok(left);
        };

        let Some(left) = left.value else {
            panic!(
                "lowering contract violation: MissingOperationResult {value:?}",
                value = *left_id
            );
        };

        let right = self.lower_operator_operand(id, *right_id, selection, current)?;

        let Some(current) = right.block else {
            return Ok(right);
        };

        let Some(right) = right.value else {
            panic!(
                "lowering contract violation: MissingOperationResult {value:?}",
                value = *right_id
            );
        };

        let source = self.expression_source(id);

        let (current, value) = self.lower_binary_operation(
            id,
            operator,
            selection,
            result_type,
            left_type,
            left,
            right,
            current,
            Self::retained_source(&source),
        )?;

        Ok(LoweredExpression::continuing(current, Some(value), source))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "binary lowering retains its selected operation, operands, block, and source"
    )]
    fn lower_binary_operation(
        &mut self,
        id: BoundExpressionId,
        operator: BoundOperator,
        selection: OperatorTarget,
        result_type: TypeId,
        operand_type: TypeId,
        left: MirOperand,
        right: MirOperand,
        current: MirBlockId,
        source: MirSourceAnchor,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        match selection {
            OperatorTarget::BuiltIn(_) => {
                if matches!(operator, BoundOperator::Equal | BoundOperator::NotEqual)
                    && self.type_representation(operand_type) == Some(RepresentationRole::String)
                {
                    return self.lower_string_equality(
                        id,
                        current,
                        source,
                        operator,
                        operand_type,
                        [left, right],
                    );
                }

                let operator = binary_operator(operator).unwrap_or_else(|| {
                    panic!(
                        "lowering contract violation: operator {operator:?} is invalid for {id:?}"
                    )
                });

                let representation = self.type_representation(operand_type);

                let operator = match representation {
                    Some(role) if role.complex_component().is_some() => operator.for_complex(),
                    Some(role) if role.real_format().is_some() => operator.for_real(),
                    _ => operator,
                };

                let value = self.push_typed_value_operation(
                    id,
                    current,
                    Self::retained_source(&source),
                    MirOperationKind::Binary {
                        operator,
                        left,
                        right,
                    },
                    result_type,
                )?;

                Ok((current, value))
            }
            OperatorTarget::Trait {
                fulfillment,
                requirement,
                witness,
                ..
            } => {
                let call_result_type = self.trait_binary_result_type(operator, result_type)?;

                let (current, value) = self.push_checked_call(
                    id,
                    current,
                    Self::retained_source(&source),
                    MirCall::protocol(
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
                    ),
                    call_result_type,
                )?;

                self.lower_trait_binary_result(id, operator, current, source, value)
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
                    .unwrap_or_else(|| {
                        panic!(
                            "lowering contract violation: operator {operator:?} is invalid for {id:?}"
                        )
                    });

                let (current, value) = self.push_checked_call(
                    id,
                    current,
                    Self::retained_source(&source),
                    MirCall::protocol(
                        MirCallTarget::Direct(MirCallableReference::new(member, CallableAbi::Bray)),
                        BoundCallResult::Immediate(result_type),
                        [left, right],
                        [],
                    )
                    .with_trait_dispatch(dispatch)
                    .with_intrinsic(intrinsic),
                    result_type,
                )?;

                if operator == BoundOperator::NotEqual {
                    let value = self.push_typed_value_operation(
                        id,
                        current,
                        Self::retained_source(&source),
                        MirOperationKind::Unary {
                            operator: MirUnaryOperator::Not,
                            operand: value,
                        },
                        result_type,
                    )?;

                    Ok((current, value))
                } else {
                    Ok((current, value))
                }
            }
        }
    }

    fn trait_binary_result_type(
        &self,
        operator: BoundOperator,
        result_type: TypeId,
    ) -> Result<TypeId, LoweringError> {
        if operator.ordering_test((), ()).is_some() {
            return Ok(self.ordering_representation()?.ty);
        }

        Ok(result_type)
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

        if operator.ordering_test((), ()).is_none() {
            return Ok((current, value));
        }

        let representation = self.ordering_representation()?;
        let result_type = self.expression_type(expression);

        let join = self
            .builder
            .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

        let result =
            self.builder
                .push_block_parameter(join, Self::retained_source(&source), result_type)?;

        let (variant, matched_result) = operator
            .ordering_test(representation.less_variant, representation.greater_variant)
            .expect("relational operator retains its Ordering test");

        self.set_terminator(
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
        operator: BoundAssignmentOperator,
        operands: &[BoundExpressionId],
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let [destination_id, value_id] = operands else {
            panic!(
                "lowering contract violation: UnsupportedExpression {value:?}",
                value = id
            );
        };

        let decision =
            self.storage_decision(id.into(), |purpose| purpose == StorageAccessPurpose::Write);

        let (current, destination) =
            match self.lower_access_place(*destination_id, decision.access(), current)? {
                super::super::access::LoweredPlace::Continuing { block, place } => (block, place),
                super::super::access::LoweredPlace::Terminated(completion) => {
                    return Ok(completion);
                }
            };

        let compound = operator.binary_operator().map(|operator| {
            let (selection, result_type) = self.selected_operator_operation(id);

            (operator, selection, result_type)
        });

        let value = match compound {
            Some((operator, selection, result_type)) => self.lower_compound_assignment_value(
                id,
                operator,
                selection,
                result_type,
                *value_id,
                Self::retained_place(&destination),
                current,
            )?,
            None => self.lower_expression(*value_id, current)?,
        };

        let Some(current) = value.block else {
            return Ok(value);
        };

        // The write occurs after the replacement value returns. Destination evaluation can
        // continue even when a non-returning replacement makes the write unreachable.
        assert_eq!(
            decision.status(),
            StorageOperationStatus::Valid,
            "executed access must be reachable: {destination_id:?}"
        );

        let Some(mut value) = value.value else {
            panic!(
                "lowering contract violation: MissingOperationResult {value:?}",
                value = *value_id
            );
        };

        let source = self.expression_source(id);

        let value_type = compound
            .map(|(_, _, result_type)| result_type)
            .unwrap_or(self.expression_type(*value_id));

        let destination_type = destination.ty();

        value = self
            .adapt_nullable_present(
                *value_id,
                current,
                Self::retained_source(&source),
                value,
                value_type,
                destination_type,
            )?
            .0;

        let current = self.replace_value(id, current, &source, destination, value)?;

        let value = self.unit_operand(self.expression_type(id));

        Ok(LoweredExpression::continuing(current, Some(value), source))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "compound assignment lowering retains checked operator selection and source and destination identities"
    )]
    fn lower_compound_assignment_value(
        &mut self,
        id: BoundExpressionId,
        operator: BoundOperator,
        selection: OperatorTarget,
        result_type: TypeId,
        value: BoundExpressionId,
        destination: MirPlace,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let source = self.expression_source(id);
        let operand_type = destination.ty();

        let left = match selection {
            OperatorTarget::BuiltIn(_) => MirOperand::Copy(destination),
            OperatorTarget::Trait { .. } | OperatorTarget::TraitConstraint { .. } => {
                let borrow_type = self.input.semantic_values().intern_type(TypeData::Borrow {
                    kind: BorrowKind::Shared,
                    target: destination.ty(),
                })?;

                self.push_typed_value_operation(
                    id,
                    current,
                    Self::retained_source(&source),
                    MirOperationKind::Borrow {
                        kind: BorrowKind::Shared,
                        place: destination,
                    },
                    borrow_type,
                )?
            }
        };

        let right = self.lower_operator_operand(id, value, selection, current)?;

        let Some(current) = right.block else {
            return Ok(right);
        };

        let Some(right) = right.value else {
            panic!(
                "lowering contract violation: MissingOperationResult {value:?}",
                value = value
            );
        };

        let (current, value) = self.lower_binary_operation(
            id,
            operator,
            selection,
            result_type,
            operand_type,
            left,
            right,
            current,
            Self::retained_source(&source),
        )?;

        Ok(LoweredExpression::continuing(current, Some(value), source))
    }

    fn lower_call(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let retained = self.input_temporaries.len();
        let result = self.lower_call_inputs(id, current);

        self.input_temporaries.truncate(retained);

        result
    }

    fn lower_call_inputs(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
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
            .unwrap_or_else(|| {
                panic!(
                    "lowering contract violation: MissingBoundNode {value:?}",
                    value = id
                )
            });

        // Retain the checked call before recursively mutating lowering state.
        let selection = self
            .input
            .semantic_selections()
            .expression(id)
            .and_then(|selection| match selection {
                SemanticSelection::Call(call) => Some(call),
                _ => None,
            })
            .unwrap_or_else(|| {
                panic!(
                    "lowering call contract violation: missing semantic selection for {id:?} in {:?} at {:?}",
                    self.input.unit().key(),
                    expression.origin().source_anchor(),
                )
            })
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

        if let Some(lowered) = self.lower_iteration_call(id, current, &selection)? {
            return Ok(lowered);
        }

        if let Some(operation) = self.input.storage_flow().checked_memory_operation(id) {
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

        self.lower_selected_call(id, expression.callee(), current, source, &selection)
    }

    fn lower_selected_call(
        &mut self,
        id: BoundExpressionId,
        callee_expression: BoundExpressionId,
        mut current: MirBlockId,
        source: MirSourceAnchor,
        selection: &bray_bound_tree::SelectedCall,
    ) -> Result<LoweredExpression, LoweringError> {
        let mut arguments = Vec::new();

        let target = if self.is_trait_view_call(selection) {
            None
        } else if let Some(role) =
            super::super::run::runtime_call_role(selection.implementation_hook()).or_else(|| {
                selection
                    .target()
                    .declaration()
                    .and_then(|declaration| self.input.runtime_call(declaration))
            })
        {
            Some(MirCallTarget::Runtime(self.runtime_reference(role)))
        } else {
            Some(match selection.target() {
                BoundCallableTarget::Declaration(instance) => {
                    MirCallTarget::Direct(MirCallableReference::new(instance, selection.abi()))
                }
                BoundCallableTarget::Indirect(_) | BoundCallableTarget::Anonymous(_) => {
                    let callee = self.lower_expression(callee_expression, current)?;

                    // Preserve the evaluated callee through argument evaluation and admission.
                    let callee =
                        self.materialize_for_later_evaluation(callee_expression, callee)?;

                    let Some(continuation) = callee.block else {
                        return Ok(callee);
                    };

                    current = continuation;

                    let Some(callee) = callee.value else {
                        panic!(
                            "lowering contract violation: MissingOperationResult {value:?}",
                            value = callee_expression
                        );
                    };

                    MirCallTarget::Indirect {
                        callee,
                        abi: selection.abi(),
                    }
                }
                BoundCallableTarget::TraitView { .. } => unreachable!(
                    "view calls resolve their table entry before ordinary callable lowering"
                ),
                BoundCallableTarget::Predicate(_) => {
                    panic!(
                        "lowering contract violation: UnsupportedExpression {value:?}",
                        value = id
                    );
                }
            })
        };

        if let Some(receiver) = selection.receiver() {
            let (lowered, receiver_type) = self.lower_call_receiver(receiver, current)?;

            let Some(continuation) = lowered.block else {
                return Ok(lowered);
            };

            current = continuation;

            let Some(operand) = lowered.value else {
                panic!(
                    "lowering contract violation: MissingOperationResult {value:?}",
                    value = receiver.expression()
                );
            };

            let input = bray_bound_tree::BoundExecutionSite::CallInput {
                expression: id,
                ordinal: bray_symbols::SymbolOrdinal::new(0),
            };

            let (continuation, operand) =
                self.consume_staged_input(input, receiver.expression(), current, &source, operand)?;

            current = continuation;

            let value =
                self.materialize_call_input(input, current, &source, operand, receiver_type)?;

            arguments.push(MirCallArgument::Receiver {
                parameter: receiver.parameter(),
                value,
            });
        }

        for argument in selection.arguments() {
            match argument {
                argument @ SelectedArgument::Default { .. } => {
                    current = self.lower_default_call_input(
                        id,
                        current,
                        &source,
                        selection,
                        argument,
                        &mut arguments,
                    )?;
                }
                SelectedArgument::Explicit {
                    expression,
                    parameter,
                    ordinal,
                    conversion,
                    reborrow,
                } => {
                    let lowered = self.lower_call_argument(*expression, *reborrow, current)?;

                    let Some(continuation) = lowered.block else {
                        return Ok(lowered);
                    };

                    current = continuation;

                    let Some(operand) = lowered.value else {
                        panic!(
                            "lowering contract violation: MissingOperationResult {value:?}",
                            value = *expression
                        );
                    };

                    let input = selection
                        .input_ordinal(bray_symbols::DependencySubjectRoot::Parameter(
                            bray_symbols::SymbolOrdinal::new(*ordinal),
                        ))
                        .expect("selected argument has its input ordinal");

                    let input = bray_bound_tree::BoundExecutionSite::CallInput {
                        expression: id,
                        ordinal: input,
                    };

                    let (continuation, operand) =
                        self.consume_staged_input(input, *expression, current, &source, operand)?;

                    let (continuation, value) = self.convert_operand(
                        id,
                        continuation,
                        Self::retained_source(&source),
                        operand,
                        conversion,
                    )?;

                    current = continuation;

                    let ty = self.builder.operand_type(&value);
                    let value = self.materialize_call_input(input, current, &source, value, ty)?;

                    arguments.push(MirCallArgument::Explicit {
                        parameter: *parameter,
                        ordinal: *ordinal,
                        value,
                    });
                }
            }
        }

        current = self.lower_call_requirements(id, current, &source, selection, &arguments)?;

        let target = if self.is_trait_view_call(selection) {
            self.lower_trait_view_call_target(
                id,
                callee_expression,
                current,
                selection,
                &mut arguments,
                &source,
            )?
        } else {
            target.expect("selected call retains its invocation target")
        };

        let call = self.selected_mir_call(id, selection, target, arguments);

        let (current, value) =
            self.lower_call_operation(id, current, Self::retained_source(&source), call)?;

        Ok(LoweredExpression::continuing(current, Some(value), source))
    }

    fn selected_mir_call(
        &self,
        expression: BoundExpressionId,
        selection: &bray_bound_tree::SelectedCall,
        target: MirCallTarget,
        arguments: Vec<MirCallArgument>,
    ) -> MirCall {
        // MIR owns the immutable checked call contract independently of the selection table.
        let call = MirCall::selected(
            target,
            selection.resolution().result(),
            arguments,
            selection
                .phase_behaviors()
                .clone()
                .with_invocation_may_unwind(
                    !self.input.call_cannot_unwind(expression)
                        && selection.phase_behaviors().invocation().may_unwind(),
                ),
            selection.contract().cloned(),
            selection
                .resolution()
                .implementation_witnesses()
                .iter()
                .copied(),
            selection.resolution().trait_dispatch(),
            selection.witnesses().iter().copied(),
        )
        .with_invocation(selection.invocation());

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

    pub(in crate::lowering::expression) fn selected_operation(
        &self,
        expression: BoundExpressionId,
    ) -> &SelectedOperation {
        let operation = self
            .input
            .semantic_selections()
            .expression(expression)
            .and_then(|selection| match selection {
                SemanticSelection::Operation(operation) => Some(operation),
                _ => None,
            });

        operation.unwrap_or_else(|| {
            panic!(
                "expression {expression:?} must have a selected operation in {:?}, source: {:?}, selection: {:?}, node: {:?}",
                self.input.unit().key(),
                self.expression_source(expression),
                self.input.semantic_selections().expression(expression),
                self.input.unit().view().expression(expression),
            )
        })
    }

    fn selected_operator(&self, expression: BoundExpressionId) -> OperatorTarget {
        self.selected_operator_operation(expression).0
    }

    fn selected_operator_operation(
        &self,
        expression: BoundExpressionId,
    ) -> (OperatorTarget, TypeId) {
        let operation = self.selected_operation(expression);

        let target = operation.operator_target().unwrap_or_else(|| {
            panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = expression
            )
        });

        let result_type = operation.operator_value_type().unwrap_or_else(|| {
            panic!(
                "lowering contract violation: MissingSemanticSelection {value:?}",
                value = expression
            )
        });

        (target, result_type)
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
        let result_type = self.expression_type(expression);

        self.push_typed_value_operation(expression, current, source, operation, result_type)
    }

    pub(in crate::lowering) fn push_typed_value_operation(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: MirSourceAnchor,
        operation: MirOperationKind,
        result_type: TypeId,
    ) -> Result<MirOperand, LoweringError> {
        let commit = self.push_operation(current, source, operation, Some(result_type))?;

        Ok(commit.result().map(MirOperand::Value).unwrap_or_else(|| {
            panic!(
                "lowering contract violation: MissingOperationResult {value:?}",
                value = expression
            )
        }))
    }

    pub(in crate::lowering) fn push_checked_call(
        &mut self,
        occurrence: impl Into<bray_bound_tree::BoundExecutionSite>,
        current: MirBlockId,
        source: MirSourceAnchor,
        call: MirCall,
        result_type: TypeId,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let occurrence = occurrence.into();

        let (current, call) =
            self.lower_protocol_requirements(occurrence, current, &source, call)?;

        self.push_checked_value_operation(
            occurrence,
            current,
            source,
            MirOperationKind::Call(call),
            result_type,
        )
    }

    pub(in crate::lowering) fn push_checked_value_operation(
        &mut self,
        occurrence: impl Into<bray_bound_tree::BoundExecutionSite>,
        current: MirBlockId,
        source: MirSourceAnchor,
        operation: MirOperationKind,
        result_type: TypeId,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let occurrence = occurrence.into();

        let expression = occurrence
            .expression()
            .expect("checked value operation has its source owner");

        let preserved = operation.input_preserved_on_rejection().and_then(|operand| match operand {
            MirOperand::Move(place) => Some(Self::retained_place(place)),
            _ => None,
        });

        let admission = operation.input_preserved_on_rejection().is_some();

        let may_propagate_panic = admission || matches!(
            &operation,
            MirOperationKind::Call(call) if call.may_propagate_panic()
        );

        let value = self.push_typed_value_operation(
            expression,
            current,
            Self::retained_source(&source),
            operation,
            result_type,
        )?;

        let (completed, value) = if may_propagate_panic {
            self.finish_typed_call_panic_check(
                occurrence,
                current,
                &source,
                &value,
                result_type,
                None,
                if admission {
                    crate::lowering::expression::CallOutcome::Runtime
                } else {
                    crate::lowering::expression::CallOutcome::Selected
                },
            )?
        } else {
            let mut value = Some(value);

            let completed =
                self.take_completion_report(occurrence, current, &source, &mut value)?;

            (completed, value.expect("normal call retains its result"))
        };

        if let Some(place) = preserved {
            self.set_storage_initialized(completed, &source, &place, false)?;
        }

        Ok((completed, value))
    }

    pub(in crate::lowering) fn expression_type(&self, expression: BoundExpressionId) -> TypeId {
        self.input
            .expression_types()
            .expression(expression)
            .map(|result| result.ty())
            .unwrap_or_else(|| {
                panic!(
                    "lowering contract violation: MissingExpressionType {value:?}",
                    value = expression
                )
            })
    }

    pub(in crate::lowering) fn expression_source(
        &self,
        expression: BoundExpressionId,
    ) -> MirSourceAnchor {
        self.input
            .unit()
            .view()
            .expression(expression)
            .map(|expression| self.source(expression.origin()))
            .unwrap_or_else(|| {
                panic!(
                    "lowering contract violation: MissingBoundNode {value:?}",
                    value = expression
                )
            })
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
        self.place_for_identity_with_static(id, ty, origin, None)
    }

    pub(in crate::lowering) fn place_for_identity_with_static(
        &mut self,
        id: StorageIdentityId,
        ty: TypeId,
        origin: bray_bound_tree::BoundNodeOrigin,
        static_reference: Option<&bray_symbols::StaticReferenceSelection>,
    ) -> Result<MirPlace, LoweringError> {
        if let Some(place) = self.guard_binding(id) {
            return Ok(Self::retained_place(place));
        }

        if let Some(reference) = static_reference {
            let storage = match self.static_storages.get(reference).copied() {
                Some(storage) => storage,
                None => {
                    let kind = if self.input.is_native_static(reference.template()) {
                        MirStorageKind::NativeStatic(reference.clone())
                    } else {
                        MirStorageKind::Static(reference.clone())
                    };

                    let storage = self.builder.push_storage(self.source(origin), kind, ty)?;

                    self.static_storages.insert(reference.clone(), storage);

                    storage
                }
            };

            return Ok(MirPlace::new(storage, [], ty));
        }

        let storage = match self.storages.get(&id).copied() {
            Some(storage) => storage,
            None => {
                let identity = self.input.storage_plan().identity(id).unwrap_or_else(|| {
                    panic!(
                        "lowering contract violation: MissingStorageIdentityRecord {value:?}",
                        value = id
                    )
                });

                let mut kind = storage_kind(identity, self.parameter_positions.get(&id).copied());

                if (identity.borrows_input_storage(self.input.unit().key().kind())
                    || self
                        .receiver_cleanup
                        .as_ref()
                        .is_some_and(|receiver| receiver.identity == id))
                    && let MirStorageKind::Parameter(position) = kind
                {
                    kind = MirStorageKind::BorrowedParameter(position);
                }

                let storage = self.builder.push_storage(self.source(origin), kind, ty)?;

                self.storages.insert(id, storage);

                storage
            }
        };

        Ok(MirPlace::new(storage, [], ty))
    }
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
        BoundOperator::Exponentiate => Some(MirBinaryOperator::Exponentiate),
        BoundOperator::Divide => Some(MirBinaryOperator::Divide),
        BoundOperator::Remainder => Some(MirBinaryOperator::Remainder),
        BoundOperator::LogicalOr
        | BoundOperator::LogicalAnd
        | BoundOperator::MatrixMultiply
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
        | StorageIdentity::PredicateParameter(_)
        | StorageIdentity::ContractParameter(_) => {
            MirStorageKind::Parameter(parameter_position.unwrap_or(u32::MAX))
        }
        StorageIdentity::Static(symbol) => {
            panic!(
                "lowering storage contract violated: static storage {symbol:?} must be initialized separately"
            )
        }
        StorageIdentity::LocalOwned(_) | StorageIdentity::Alternative { .. } => {
            MirStorageKind::Local
        }
        StorageIdentity::Result(_) => MirStorageKind::Return,
        StorageIdentity::Temporary(_)
        | StorageIdentity::InvocationInput { .. }
        | StorageIdentity::CustomIndexBorrow(_)
        | StorageIdentity::PostconditionResult(_)
        | StorageIdentity::InvocationResult(_)
        | StorageIdentity::ScopedCapability { .. }
        | StorageIdentity::Allocation(_)
        | StorageIdentity::CompilerCreated(_)
        | StorageIdentity::Error(_) => MirStorageKind::Temporary,
    }
}
