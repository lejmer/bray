use bray_bound_tree::{
    BoundCallableTarget, BoundExpression, BoundExpressionId, BoundOperator, ConversionTarget,
    OperatorTarget, SelectedArgument, SelectedConversion, SelectedOperation, SemanticSelection,
    StorageAccessId, StorageAccessPurpose, StorageAccessRoot, StorageIdentity, StorageIdentityId,
    StorageOperationStatus,
};
use bray_ir::{
    MirBinaryOperator, MirBlockId, MirCall, MirCallTarget, MirCallableReference, MirOperand,
    MirOperationKind, MirPlace, MirSourceAnchor, MirStorageKind, MirUnaryOperator,
};
use bray_symbols::{CallableAbi, ConstantValueData, ConstantValueKind, TypeId};

use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;
use super::super::LoweringError;

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

        match expression {
            BoundExpression::Block(expression) => self.lower_block(expression.block(), current),
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
            BoundExpression::Name(_) | BoundExpression::PatternReference(_) => {
                let source = self.source(expression.origin());
                let value = self.storage_operand(id)?;

                Ok(LoweredExpression::continuing(current, Some(value), source))
            }
            BoundExpression::Unary(expression) => {
                self.lower_unary(id, expression.operator(), expression.operands(), current)
            }
            BoundExpression::Binary(expression) => {
                self.lower_binary(id, expression.operator(), expression.operands(), current)
            }
            BoundExpression::Assignment(expression) => {
                self.lower_assignment(id, expression.operands(), current)
            }
            BoundExpression::Call(_) => self.lower_call(id, current),
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
            BoundExpression::UnresolvedReference(_)
            | BoundExpression::ErrorCall(_)
            | BoundExpression::ErrorConversion(_)
            | BoundExpression::AnonymousCallable(_)
            | BoundExpression::Await(_)
            | BoundExpression::StructConstruction(_)
            | BoundExpression::MemberAccess(_)
            | BoundExpression::LeadingDotVariant(_)
            | BoundExpression::TraitQualifiedMember(_)
            | BoundExpression::For(_)
            | BoundExpression::Match(_)
            | BoundExpression::Generator(_)
            | BoundExpression::Error(_) => Err(LoweringError::UnsupportedExpression(id)),
        }
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

        let lowered = self.lower_expression(*operand, current)?;

        let Some(current) = lowered.block else {
            return Ok(lowered);
        };

        let Some(operand) = lowered.value else {
            return Err(LoweringError::MissingOperationResult(*operand));
        };

        let source = self.expression_source(id)?;

        if operator == BoundOperator::Add {
            return Ok(LoweredExpression::continuing(
                current,
                Some(operand),
                source,
            ));
        }

        let selection = self.selected_operator(id)?;

        let value = match selection {
            OperatorTarget::BuiltIn(_) => {
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
            OperatorTarget::Trait { fulfillment, .. } => self.push_value_operation(
                id,
                current,
                Self::retained_source(&source),
                MirOperationKind::Call(MirCall::new(
                    MirCallTarget::Direct(MirCallableReference::new(
                        fulfillment,
                        CallableAbi::Bray,
                    )),
                    [operand],
                )),
            )?,
        };

        Ok(LoweredExpression::continuing(
            current,
            Some(value),
            source,
        ))
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

        let left = self.lower_expression(*left_id, current)?;

        let Some(current) = left.block else {
            return Ok(left);
        };

        let Some(left) = left.value else {
            return Err(LoweringError::MissingOperationResult(*left_id));
        };

        let right = self.lower_expression(*right_id, current)?;

        let Some(current) = right.block else {
            return Ok(right);
        };

        let Some(right) = right.value else {
            return Err(LoweringError::MissingOperationResult(*right_id));
        };

        let source = self.expression_source(id)?;
        let selection = self.selected_operator(id)?;

        let value = match selection {
            OperatorTarget::BuiltIn(_) => {
                let operator = binary_operator(operator)
                    .ok_or(LoweringError::UnsupportedOperator(operator))?;

                self.push_value_operation(
                    id,
                    current,
                    Self::retained_source(&source),
                    MirOperationKind::Binary {
                        operator,
                        left,
                        right,
                    },
                )?
            }
            OperatorTarget::Trait { fulfillment, .. } => self.push_value_operation(
                id,
                current,
                Self::retained_source(&source),
                MirOperationKind::Call(MirCall::new(
                    MirCallTarget::Direct(MirCallableReference::new(
                        fulfillment,
                        CallableAbi::Bray,
                    )),
                    [left, right],
                )),
            )?,
        };

        Ok(LoweredExpression::continuing(
            current,
            Some(value),
            source,
        ))
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

        let destination = self.expression_place(*destination, StorageAccessPurpose::Assignment)?;
        let value = self.lower_expression(*value_id, current)?;

        let Some(current) = value.block else {
            return Ok(value);
        };

        let Some(value) = value.value else {
            return Err(LoweringError::MissingOperationResult(*value_id));
        };

        let source = self.expression_source(id)?;

        self.builder.push_operation(
            current,
            Self::retained_source(&source),
            MirOperationKind::Store { destination, value },
            None,
        )?;

        let value = self.unit_operand(self.expression_type(id)?)?;

        Ok(LoweredExpression::continuing(
            current,
            Some(value),
            source,
        ))
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

                MirCallTarget::Indirect(callee)
            }
            BoundCallableTarget::Anonymous(_) => {
                return Err(LoweringError::UnsupportedExpression(id));
            }
        };

        if let Some(receiver) = selection.receiver() {
            let lowered = self.lower_expression(receiver.expression(), current)?;

            let Some(continuation) = lowered.block else {
                return Ok(lowered);
            };

            current = continuation;

            let Some(operand) = lowered.value else {
                return Err(LoweringError::MissingOperationResult(
                    receiver.expression(),
                ));
            };

            arguments.push(self.convert_operand(
                id,
                current,
                Self::retained_source(&source),
                operand,
                receiver.conversion(),
            )?);
        }

        for argument in selection.arguments() {
            let SelectedArgument::Explicit {
                expression,
                conversion,
                ..
            } = argument
            else {
                return Err(LoweringError::UnsupportedDefaultArgument(id));
            };

            let lowered = self.lower_expression(*expression, current)?;

            let Some(continuation) = lowered.block else {
                return Ok(lowered);
            };

            current = continuation;

            let Some(operand) = lowered.value else {
                return Err(LoweringError::MissingOperationResult(*expression));
            };

            arguments.push(self.convert_operand(
                id,
                current,
                Self::retained_source(&source),
                operand,
                conversion,
            )?);
        }

        let value = self.push_value_operation(
            id,
            current,
            Self::retained_source(&source),
            MirOperationKind::Call(MirCall::new(target, arguments)),
        )?;

        Ok(LoweredExpression::continuing(
            current,
            Some(value),
            source,
        ))
    }

    fn convert_operand(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: MirSourceAnchor,
        operand: MirOperand,
        conversion: &SelectedConversion,
    ) -> Result<MirOperand, LoweringError> {
        if matches!(conversion.target(), ConversionTarget::Identity) {
            return Ok(operand);
        }

        self.push_value_operation(
            expression,
            current,
            source,
            MirOperationKind::Convert {
                operand,
                target: conversion.target_type(),
            },
        )
    }

    fn selected_operation(
        &self,
        expression: BoundExpressionId,
    ) -> Result<&SelectedOperation, LoweringError> {
        self.input
            .semantic_selections()
            .expression(expression)
            .and_then(|selection| match selection {
                SemanticSelection::Operation(operation) => Some(operation),
                _ => None,
            })
            .ok_or(LoweringError::MissingSemanticSelection(expression))
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

    fn push_value_operation(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: MirSourceAnchor,
        operation: MirOperationKind,
    ) -> Result<MirOperand, LoweringError> {
        let result_type = self.expression_type(expression)?;

        let commit =
            self.builder
                .push_operation(current, source, operation, Some(result_type))?;

        commit
            .result()
            .map(MirOperand::Value)
            .ok_or(LoweringError::MissingOperationResult(expression))
    }

    pub(super) fn expression_type(
        &self,
        expression: BoundExpressionId,
    ) -> Result<TypeId, LoweringError> {
        self.input
            .expression_types()
            .expression(expression)
            .map(|result| result.ty())
            .ok_or(LoweringError::MissingExpressionType(expression))
    }

    pub(super) fn expression_source(
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

    pub(super) fn unit_operand(&self, ty: TypeId) -> Result<MirOperand, LoweringError> {
        self.constant_operand(ty, ConstantValueKind::Unit)
    }

    pub(super) fn constant_operand(
        &self,
        ty: TypeId,
        kind: ConstantValueKind,
    ) -> Result<MirOperand, LoweringError> {
        let value = self
            .input
            .semantic_values()
            .intern_constant_value(ConstantValueData::new(ty, kind))?;

        Ok(MirOperand::Constant { value, ty })
    }

    fn storage_operand(&mut self, expression: BoundExpressionId) -> Result<MirOperand, LoweringError> {
        let plan = self
            .input
            .storage_plan()
            .expression_plans(expression)
            .find(|plan| {
                matches!(
                    plan.purpose(),
                    StorageAccessPurpose::Read
                        | StorageAccessPurpose::Copy
                        | StorageAccessPurpose::Move
                        | StorageAccessPurpose::ValueTransfer
                )
            })
            .ok_or(LoweringError::MissingStorageAccess(expression))?;

        let decision = self
            .input
            .storage_flow()
            .operations()
            .iter()
            .copied()
            .find(|decision| {
                decision.expression() == expression && decision.access() == plan.access()
            })
            .ok_or(LoweringError::MissingStorageAccess(expression))?;

        if decision.status() != StorageOperationStatus::Valid {
            return Err(LoweringError::RecoveredBoundNode(expression.into()));
        }

        let place = self.place_for_access(decision.access())?;

        match decision.purpose() {
            StorageAccessPurpose::Read | StorageAccessPurpose::Copy => Ok(MirOperand::Copy(place)),
            StorageAccessPurpose::Move => Ok(MirOperand::Move(place)),
            _ => Err(LoweringError::UnsupportedStorageAccess(decision.access())),
        }
    }

    fn expression_place(
        &mut self,
        expression: BoundExpressionId,
        purpose: StorageAccessPurpose,
    ) -> Result<MirPlace, LoweringError> {
        let plan = self
            .input
            .storage_plan()
            .expression_plans(expression)
            .find(|plan| plan.purpose() == purpose)
            .ok_or(LoweringError::MissingStorageAccess(expression))?;

        let decision = self
            .input
            .storage_flow()
            .operations()
            .iter()
            .find(|decision| {
                decision.expression() == expression && decision.access() == plan.access()
            })
            .ok_or(LoweringError::MissingStorageAccess(expression))?;

        if decision.status() != StorageOperationStatus::Valid {
            return Err(LoweringError::RecoveredBoundNode(expression.into()));
        }

        self.place_for_access(plan.access())
    }

    pub(in crate::lowering) fn place_for_access(
        &mut self,
        id: StorageAccessId,
    ) -> Result<MirPlace, LoweringError> {
        let access = self
            .input
            .storage_plan()
            .access(id)
            .ok_or(LoweringError::MissingStorageAccessRecord(id))?;

        if access.is_recovered() || !access.projections().is_empty() {
            return Err(LoweringError::UnsupportedStorageAccess(id));
        }

        if !matches!(
            access.root(),
            StorageAccessRoot::Storage(_) | StorageAccessRoot::Recovery(_)
        ) {
            return Err(LoweringError::UnsupportedStorageAccess(id));
        }

        let identity = self
            .input
            .storage_plan()
            .root_identity(id)
            .ok_or(LoweringError::MissingStorageIdentity(id))?;

        self.place_for_identity(
            identity,
            access.reached_type(),
            bray_bound_tree::BoundNodeOrigin::source(access.source()),
        )
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

                let kind = storage_kind(identity);

                let storage = self
                    .builder
                    .push_storage(self.source(origin), kind, ty)?;

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

fn storage_kind(identity: StorageIdentity) -> MirStorageKind {
    match identity {
        StorageIdentity::Parameter(_)
        | StorageIdentity::Receiver(_)
        | StorageIdentity::AnonymousParameter(_)
        | StorageIdentity::PredicateParameter(_) => MirStorageKind::Parameter,
        StorageIdentity::LocalOwned(_) | StorageIdentity::Alternative { .. } => {
            MirStorageKind::Local
        }
        StorageIdentity::Result(_) => MirStorageKind::Return,
        StorageIdentity::Temporary(_)
        | StorageIdentity::IterationCursor(_)
        | StorageIdentity::IterationElement(_)
        | StorageIdentity::Allocation(_)
        | StorageIdentity::CompilerCreated(_)
        | StorageIdentity::Error(_) => MirStorageKind::Temporary,
    }
}
