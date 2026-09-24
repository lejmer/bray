use bray_bound_tree::{BoundExpression, BoundExpressionId, BoundOperator, ExpressionTypeStatus};
use bray_compiler_known::IntegerRepresentation;
use bray_diagnostics::DiagnosticConstantOperation;
use bray_symbols::{
    ConstantTermData, ConstantTermId, ConstantValueData, ConstantValueId, ConstantValueKind,
    GenericArgument, TypeData, TypeId,
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
    pub(super) fn is_complex_literal(
        &self,
        binary: &bray_bound_tree::BoundBinaryExpression,
    ) -> bool {
        if !matches!(
            binary.operator(),
            BoundOperator::Add | BoundOperator::Subtract
        ) {
            return false;
        }

        let [real, imaginary] = binary.operands() else {
            return false;
        };

        matches!(
            self.request.view().expression(*real),
            Some(BoundExpression::Literal(literal))
                if literal.kind() == bray_bound_tree::BoundLiteralKind::Real
        ) && matches!(
            self.request.view().expression(*imaginary),
            Some(BoundExpression::Literal(literal))
                if literal.kind() == bray_bound_tree::BoundLiteralKind::Imaginary
        )
    }

    pub(super) fn expression_type(
        &self,
        expression: BoundExpressionId,
    ) -> Result<TypeId, EvaluationFailure> {
        let Some(result) = self.input.expression_types().expression(expression) else {
            return Err(EvaluationFailure::constant(
                crate::CheckerConstantEvaluationFailure::MissingExpressionType { expression },
            ));
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
            .map_err(CheckerInfrastructureError::SemanticValueStore)
    }

    pub(super) fn recovery_term(
        &self,
        ty: TypeId,
    ) -> Result<ConstantTermId, CheckerInfrastructureError> {
        let value = self.recovery_value(ty)?;

        self.request
            .semantic_values()
            .intern_constant_term(ConstantTermData::Value(value))
            .map_err(CheckerInfrastructureError::SemanticValueStore)
    }

    pub(super) fn intern_value(
        &self,
        ty: TypeId,
        kind: ConstantValueKind,
    ) -> Result<ConstantValueId, EvaluationFailure> {
        self.request
            .semantic_values()
            .intern_constant_value(ConstantValueData::new(ty, kind))
            .map_err(|error| {
                EvaluationFailure::Infrastructure(CheckerInfrastructureError::SemanticValueStore(
                    error,
                ))
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
            .map_err(|error| {
                EvaluationFailure::Infrastructure(CheckerInfrastructureError::SemanticValueStore(
                    error,
                ))
            })
    }

    pub(super) fn adapt_nullable_present(
        &self,
        term: ConstantTermId,
        source_type: TypeId,
        target_type: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        if source_type == target_type {
            return Ok(term);
        }

        let target = self.request.semantic_values().type_data(target_type);

        if !matches!(target.as_ref(), TypeData::Nullable(contained) if *contained == source_type) {
            return Err(EvaluationFailure::invalid_input());
        }

        match self.term_value(term) {
            Some(value) => {
                self.intern_value_term(target_type, ConstantValueKind::NullablePresent(value))
            }
            None => self.intern_typed_term(target_type, ConstantTermData::NullablePresent(term)),
        }
    }

    pub(super) fn intern_typed_term(
        &self,
        ty: TypeId,
        data: ConstantTermData,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let term = self.intern_term(data)?;

        self.type_term(term, ty)
    }

    pub(super) fn type_term(
        &self,
        term: ConstantTermId,
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        if !self.input.retains_nested_term_types() {
            return Ok(term);
        }

        self.intern_term(ConstantTermData::typed(term, ty))
    }

    pub(super) fn term_value(&self, term: ConstantTermId) -> Option<ConstantValueId> {
        let data = self.request.semantic_values().constant_term_data(term);

        match data.as_ref() {
            ConstantTermData::Typed { term, .. } => return self.term_value(*term),
            ConstantTermData::Value(value) => Some(*value),
            ConstantTermData::IntegerLiteral { .. }
            | ConstantTermData::CallableArgument(_)
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
        }
    }

    pub(super) fn closed_value(
        &self,
        term: ConstantTermId,
        expression: BoundExpressionId,
    ) -> Result<ConstantValueId, EvaluationFailure> {
        self.term_value(term)
            .ok_or_else(|| EvaluationFailure::invalid_expression(expression))
    }

    pub(super) fn finalize_closed_value(
        &mut self,
        term: ConstantTermId,
        expression: Option<BoundExpressionId>,
    ) -> Result<ConstantValueId, EvaluationFailure> {
        let Some(value) = self.term_value(term) else {
            return match expression {
                Some(expression) => Err(EvaluationFailure::invalid_expression(expression)),
                None => Err(EvaluationFailure::invalid_input()),
            };
        };

        if let Some(expression) = expression {
            if self.input.destination() == crate::constant::input::ConstantDestination::Definition {
                self.check_closed_materialization(value, expression)?;
            }

            self.validate_closed_value(value, expression)?;
        }

        Ok(value)
    }

    fn check_closed_materialization(
        &mut self,
        value: ConstantValueId,
        expression: BoundExpressionId,
    ) -> Result<(), EvaluationFailure> {
        let result = crate::constant::materialization::nonmaterializable_value_tree(
            self.request.context(),
            value,
            &mut self.checked_materialization,
            &mut self.diagnostics,
        );

        if let Some(ty) = result.map_err(|error| self.record_query_failure(error))? {
            return Err(EvaluationFailure::Source {
                expression,
                diagnostic: ConstantDiagnostic::NonMaterializable(ty),
            });
        }

        Ok(())
    }

    fn validate_closed_value(
        &mut self,
        value: ConstantValueId,
        expression: BoundExpressionId,
    ) -> Result<(), EvaluationFailure> {
        self.observe_cancellation()?;

        let data = self.request.semantic_values().constant_value_data(value);

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
            .ok_or_else(|| EvaluationFailure::invalid_expression(expression))?;

        if let Some(representation) = role.integer_representation() {
            return Ok(representation);
        }

        if role != bray_compiler_known::RepresentationRole::Atomic {
            return Err(EvaluationFailure::invalid_expression(expression));
        }

        let values = self.request.semantic_values();

        let data = values.type_data(ty);

        let TypeData::Named { substitution, .. } = data.as_ref() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let substitution = values.generic_substitution_data(*substitution);

        let [binding] = substitution.bindings() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let GenericArgument::Type(value_type) = binding.argument() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        type_representation(self.request, value_type)
            .and_then(bray_compiler_known::RepresentationRole::integer_representation)
            .ok_or_else(|| EvaluationFailure::invalid_expression(expression))
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
    Upstream,
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

    pub(super) const fn constant(failure: crate::CheckerConstantEvaluationFailure) -> Self {
        Self::Infrastructure(CheckerInfrastructureError::ConstantEvaluation(failure))
    }

    pub(super) const fn invalid_expression(expression: BoundExpressionId) -> Self {
        Self::Source {
            expression,
            diagnostic: ConstantDiagnostic::InvalidExpression(None),
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
