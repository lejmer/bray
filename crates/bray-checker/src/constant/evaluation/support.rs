use bray_bound_tree::{BoundExpressionId, BoundOperator, ExpressionTypeStatus};
use bray_compiler_known::IntegerRepresentation;
use bray_diagnostics::DiagnosticConstantOperation;
use bray_symbols::{
    ConstantBinaryOperation, ConstantTermData, ConstantTermId, ConstantUnaryOperation,
    ConstantValueData, ConstantValueId, ConstantValueKind, GenericArgument, TypeData, TypeId,
};

use crate::constant::diagnostic::ConstantDiagnostic;
use crate::constant::integer::fits_integer_representation;
use crate::constant::literal::ConstantLiteralError;
use crate::constant::operation::ConstantOperationError;
use crate::representation::type_representation;
use crate::{CheckerInfrastructureError, CheckerRequestContext};

use super::engine::Evaluator;

impl<'view, 'input, 'types, C> Evaluator<'view, 'input, 'types, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn expression_type(
        &self,
        expression: BoundExpressionId,
    ) -> Result<TypeId, EvaluationFailure> {
        let Some(result) = self.input.expression_types().expression(expression) else {
            return Err(EvaluationFailure::invalid_input());
        };

        if result.status() == ExpressionTypeStatus::Recovered {
            return Err(EvaluationFailure::invalid_expression(expression));
        }

        Ok(result.ty())
    }

    pub(super) fn recovery_value(
        &self,
        ty: TypeId,
    ) -> Result<ConstantValueId, CheckerInfrastructureError> {
        self.request
            .semantic_values()
            .intern_error_constant_value(ty)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
    }

    pub(super) fn recovery_term(
        &self,
        ty: TypeId,
    ) -> Result<ConstantTermId, CheckerInfrastructureError> {
        let value = self.recovery_value(ty)?;

        self.request
            .semantic_values()
            .intern_constant_term(ConstantTermData::Value(value))
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
    }

    pub(super) fn intern_value(
        &self,
        ty: TypeId,
        kind: ConstantValueKind,
    ) -> Result<ConstantValueId, EvaluationFailure> {
        self.request
            .semantic_values()
            .intern_constant_value(ConstantValueData::new(ty, kind))
            .map_err(|_| {
                EvaluationFailure::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                )
            })
    }

    pub(super) fn intern_value_term(
        &self,
        ty: TypeId,
        kind: ConstantValueKind,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let value = self.intern_value(ty, kind)?;

        self.intern_term(ConstantTermData::Value(value))
    }

    pub(super) fn intern_term(
        &self,
        data: ConstantTermData,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        self.request
            .semantic_values()
            .intern_constant_term(data)
            .map_err(|_| {
                EvaluationFailure::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                )
            })
    }

    pub(super) fn term_value(
        &self,
        term: ConstantTermId,
    ) -> Result<Option<ConstantValueId>, EvaluationFailure> {
        let data = self
            .request
            .semantic_values()
            .constant_term_data(term)
            .map_err(|_| {
                EvaluationFailure::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                )
            })?;

        Ok(match data.as_ref() {
            ConstantTermData::Value(value) => Some(*value),
            ConstantTermData::IntegerLiteral { .. }
            | ConstantTermData::Parameter(_)
            | ConstantTermData::TargetProperty(_)
            | ConstantTermData::Unary { .. }
            | ConstantTermData::Binary { .. }
            | ConstantTermData::Conversion { .. }
            | ConstantTermData::NullablePresent(_)
            | ConstantTermData::Tuple(_)
            | ConstantTermData::Array(_)
            | ConstantTermData::Product(_)
            | ConstantTermData::Union { .. }
            | ConstantTermData::DefinitionApplication { .. }
            | ConstantTermData::Call { .. }
            | ConstantTermData::PredicateCall { .. }
            | ConstantTermData::Projection(_) => None,
        })
    }

    pub(super) fn closed_value(
        &self,
        term: ConstantTermId,
        expression: BoundExpressionId,
    ) -> Result<ConstantValueId, EvaluationFailure> {
        self.term_value(term)?
            .ok_or_else(|| EvaluationFailure::invalid_expression(expression))
    }

    pub(super) fn finalize_closed_value(
        &self,
        term: ConstantTermId,
        expression: Option<BoundExpressionId>,
    ) -> Result<ConstantValueId, EvaluationFailure> {
        let Some(value) = self.term_value(term)? else {
            return match expression {
                Some(expression) => Err(EvaluationFailure::invalid_expression(expression)),
                None => Err(EvaluationFailure::invalid_input()),
            };
        };

        if let Some(expression) = expression {
            self.validate_closed_value(value, expression)?;
        }

        Ok(value)
    }

    fn validate_closed_value(
        &self,
        value: ConstantValueId,
        expression: BoundExpressionId,
    ) -> Result<(), EvaluationFailure> {
        self.observe_cancellation()?;

        let data = self.constant_value(value)?;

        match data.kind() {
            ConstantValueKind::Integer(integer) => {
                let representation = self.integer_constant_representation(data.ty(), expression)?;

                if !fits_integer_representation(integer, representation, || {
                    self.request
                        .selected_target()
                        .machine()
                        .pointer_width_bits()
                }) {
                    return Err(EvaluationFailure::operation(
                        expression,
                        DiagnosticConstantOperation::Conversion,
                        ConstantOperationError::NotRepresentable,
                    ));
                }
            }
            ConstantValueKind::NullablePresent(value) => {
                self.validate_closed_value(*value, expression)?;
            }
            ConstantValueKind::Tuple(values) | ConstantValueKind::Array(values) => {
                for value in values.iter() {
                    self.validate_closed_value(*value, expression)?;
                }
            }
            ConstantValueKind::Product(fields) => {
                for field in fields.iter() {
                    self.validate_closed_value(*field.value(), expression)?;
                }
            }
            ConstantValueKind::Union { fields, .. } => {
                for field in fields.iter() {
                    self.validate_closed_value(*field.value(), expression)?;
                }
            }
            ConstantValueKind::Error
            | ConstantValueKind::Boolean(_)
            | ConstantValueKind::Character(_)
            | ConstantValueKind::Real(_)
            | ConstantValueKind::Complex { .. }
            | ConstantValueKind::String(_)
            | ConstantValueKind::StaticAddress(_)
            | ConstantValueKind::Unit
            | ConstantValueKind::NullableAbsent => {}
        }

        Ok(())
    }

    fn integer_constant_representation(
        &self,
        ty: TypeId,
        expression: BoundExpressionId,
    ) -> Result<IntegerRepresentation, EvaluationFailure> {
        let role = type_representation(self.request, ty)
            .map_err(EvaluationFailure::Infrastructure)?
            .ok_or_else(|| EvaluationFailure::invalid_expression(expression))?;

        if let Some(representation) = role.integer_representation() {
            return Ok(representation);
        }

        if role != bray_compiler_known::RepresentationRole::Atomic {
            return Err(EvaluationFailure::invalid_expression(expression));
        }

        let values = self.request.semantic_values();
        let data = values.type_data(ty).map_err(|_| {
            EvaluationFailure::Infrastructure(CheckerInfrastructureError::SemanticValueUnavailable)
        })?;
        let TypeData::Named { substitution, .. } = data.as_ref() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };
        let substitution = values
            .generic_substitution_data(*substitution)
            .map_err(|_| {
                EvaluationFailure::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                )
            })?;
        let [binding] = substitution.bindings() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };
        let GenericArgument::Type(value_type) = binding.argument() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        type_representation(self.request, value_type)
            .map_err(EvaluationFailure::Infrastructure)?
            .and_then(bray_compiler_known::RepresentationRole::integer_representation)
            .ok_or_else(|| EvaluationFailure::invalid_expression(expression))
    }

    pub(super) fn constant_value(
        &self,
        value: ConstantValueId,
    ) -> Result<std::sync::Arc<ConstantValueData>, EvaluationFailure> {
        self.request
            .semantic_values()
            .constant_value_data(value)
            .map_err(|_| {
                EvaluationFailure::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                )
            })
    }

    pub(super) fn target_integer_width(
        &self,
        representation: Option<bray_compiler_known::RepresentationRole>,
    ) -> std::num::NonZeroU16 {
        if representation.is_some_and(|role| {
            matches!(
                role.integer_representation(),
                Some(IntegerRepresentation::TargetSigned | IntegerRepresentation::TargetUnsigned)
            )
        }) {
            return self
                .request
                .selected_target()
                .machine()
                .pointer_width_bits();
        }

        std::num::NonZeroU16::MIN
    }

    pub(super) fn observe_cancellation(&self) -> Result<(), EvaluationFailure> {
        if self.request.is_cancelled() {
            Err(EvaluationFailure::Cancelled)
        } else {
            Ok(())
        }
    }
}

pub(in crate::constant) enum EvaluationFailure {
    Cancelled,
    Infrastructure(CheckerInfrastructureError),
    Propagate(ConstantTermId),
    Source {
        expression: BoundExpressionId,
        diagnostic: ConstantDiagnostic,
    },
}

impl EvaluationFailure {
    pub(super) const fn invalid_input() -> Self {
        Self::Infrastructure(CheckerInfrastructureError::InvalidConstantEvaluationInput)
    }

    pub(super) const fn invalid_expression(expression: BoundExpressionId) -> Self {
        Self::Source {
            expression,
            diagnostic: ConstantDiagnostic::InvalidExpression,
        }
    }

    pub(super) const fn literal(
        expression: BoundExpressionId,
        error: ConstantLiteralError,
    ) -> Self {
        Self::Source {
            expression,
            diagnostic: ConstantDiagnostic::Literal(error),
        }
    }

    pub(super) const fn operation(
        expression: BoundExpressionId,
        operation: DiagnosticConstantOperation,
        error: ConstantOperationError,
    ) -> Self {
        Self::Source {
            expression,
            diagnostic: ConstantDiagnostic::operation(operation, error),
        }
    }
}

pub(super) const fn unary_term_operation(
    operation: BoundOperator,
) -> Option<ConstantUnaryOperation> {
    match operation {
        BoundOperator::Add => Some(ConstantUnaryOperation::Identity),
        BoundOperator::Subtract => Some(ConstantUnaryOperation::Negate),
        BoundOperator::LogicalNot => Some(ConstantUnaryOperation::LogicalNot),
        BoundOperator::BitwiseNot => Some(ConstantUnaryOperation::BitwiseNot),
        BoundOperator::LogicalOr
        | BoundOperator::LogicalAnd
        | BoundOperator::Equal
        | BoundOperator::NotEqual
        | BoundOperator::Less
        | BoundOperator::LessEqual
        | BoundOperator::Greater
        | BoundOperator::GreaterEqual
        | BoundOperator::BitwiseOr
        | BoundOperator::BitwiseXor
        | BoundOperator::BitwiseAnd
        | BoundOperator::ShiftLeft
        | BoundOperator::ShiftRight
        | BoundOperator::Multiply
        | BoundOperator::Divide
        | BoundOperator::Remainder
        | BoundOperator::MatrixMultiply
        | BoundOperator::Exponentiate => None,
    }
}

pub(super) const fn binary_term_operation(
    operation: BoundOperator,
) -> Option<ConstantBinaryOperation> {
    match operation {
        BoundOperator::LogicalOr => Some(ConstantBinaryOperation::LogicalOr),
        BoundOperator::LogicalAnd => Some(ConstantBinaryOperation::LogicalAnd),
        BoundOperator::Equal => Some(ConstantBinaryOperation::Equal),
        BoundOperator::NotEqual => Some(ConstantBinaryOperation::NotEqual),
        BoundOperator::Less => Some(ConstantBinaryOperation::Less),
        BoundOperator::LessEqual => Some(ConstantBinaryOperation::LessOrEqual),
        BoundOperator::Greater => Some(ConstantBinaryOperation::Greater),
        BoundOperator::GreaterEqual => Some(ConstantBinaryOperation::GreaterOrEqual),
        BoundOperator::BitwiseOr => Some(ConstantBinaryOperation::BitwiseOr),
        BoundOperator::BitwiseXor => Some(ConstantBinaryOperation::BitwiseXor),
        BoundOperator::BitwiseAnd => Some(ConstantBinaryOperation::BitwiseAnd),
        BoundOperator::ShiftLeft => Some(ConstantBinaryOperation::ShiftLeft),
        BoundOperator::ShiftRight => Some(ConstantBinaryOperation::ShiftRight),
        BoundOperator::Add => Some(ConstantBinaryOperation::Add),
        BoundOperator::Subtract => Some(ConstantBinaryOperation::Subtract),
        BoundOperator::Multiply => Some(ConstantBinaryOperation::Multiply),
        BoundOperator::Divide => Some(ConstantBinaryOperation::Divide),
        BoundOperator::Remainder => Some(ConstantBinaryOperation::Remainder),
        BoundOperator::Exponentiate => Some(ConstantBinaryOperation::Exponentiate),
        BoundOperator::MatrixMultiply | BoundOperator::BitwiseNot | BoundOperator::LogicalNot => {
            None
        }
    }
}
