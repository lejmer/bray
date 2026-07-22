use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundMemberSelector, BoundOperator, ConstructionTarget,
    ConversionTarget, IndexTarget, OperatorTarget, SelectedOperation, SemanticSelection,
};
use bray_compiler_known::NumericRepresentationKind;
use bray_symbols::{
    ConstantProjection, ConstantProjectionKind, ConstantTermData, ConstantTermId, ConstantValueId,
    ConstantValueKind, SymbolOrdinal, TypeId,
};

use crate::constant::conversion::convert_scalar;
use crate::constant::operation::{fold_binary, fold_unary};
use crate::representation::type_representation;
use crate::{CheckerInfrastructureError, CheckerRequestContext};

use super::engine::Evaluator;
use super::support::{EvaluationFailure, binary_term_operation, unary_term_operation};

impl<'view, 'input, 'types, C> Evaluator<'view, 'input, 'types, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn evaluate_operator(
        &mut self,
        expression: BoundExpressionId,
        operation: BoundOperator,
        operands: &[BoundExpressionId],
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let Some(SemanticSelection::Operation(SelectedOperation::Operator {
            target: OperatorTarget::BuiltIn(selected),
            ..
        })) = self.input.semantic_selections().expression(expression)
        else {
            // TODO(BRA-245): Evaluate selected const-call operator implementations.
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        if *selected != operation {
            return Err(EvaluationFailure::Infrastructure(
                CheckerInfrastructureError::InvalidConstantEvaluationInput,
            ));
        }

        match operands {
            [operand] => self.evaluate_unary_operator(expression, operation, *operand, ty),
            [left, right] => {
                self.evaluate_binary_operator(expression, operation, *left, *right, ty)
            }
            _ => Err(EvaluationFailure::Infrastructure(
                CheckerInfrastructureError::InvalidConstantEvaluationInput,
            )),
        }
    }

    pub(super) fn evaluate_unary_operator(
        &mut self,
        expression: BoundExpressionId,
        operation: BoundOperator,
        operand: BoundExpressionId,
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let operand = self.evaluate(operand)?;

        let Some(value) = self.term_value(operand)? else {
            let Some(operation) = unary_term_operation(operation) else {
                // TODO(BRA-244): Represent additional selected unary operations as constant terms.
                return Err(EvaluationFailure::invalid_expression(expression));
            };

            return self.intern_term(ConstantTermData::Unary { operation, operand });
        };

        let value = self.constant_value(value)?;

        let representation =
            type_representation(self.request, ty).map_err(EvaluationFailure::Infrastructure)?;

        let target_width = self.target_integer_width(representation);

        let kind = fold_unary(operation, value.kind(), representation, target_width)
            .map_err(|error| EvaluationFailure::operation(expression, error))?;

        self.intern_value_term(ty, kind)
    }

    pub(super) fn evaluate_binary_operator(
        &mut self,
        expression: BoundExpressionId,
        operation: BoundOperator,
        left: BoundExpressionId,
        right: BoundExpressionId,
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let left = self.evaluate(left)?;

        if let Some(short_circuit) = self.short_circuit(expression, operation, left, ty)? {
            return Ok(short_circuit);
        }

        let right = self.evaluate(right)?;
        let left_value = self.term_value(left)?;
        let right_value = self.term_value(right)?;

        let (Some(left_value), Some(right_value)) = (left_value, right_value) else {
            let Some(operation) = binary_term_operation(operation) else {
                // TODO(BRA-244): Represent additional selected binary operations as constant terms.
                return Err(EvaluationFailure::invalid_expression(expression));
            };

            return self.intern_term(ConstantTermData::Binary {
                operation,
                left,
                right,
            });
        };

        let left_value = self.constant_value(left_value)?;
        let right_value = self.constant_value(right_value)?;

        let kind = fold_binary(
            operation,
            left_value.kind(),
            right_value.kind(),
            self.input.limits().integer_bits(),
        )
        .map_err(|error| EvaluationFailure::operation(expression, error))?;

        self.intern_value_term(ty, kind)
    }

    pub(super) fn short_circuit(
        &self,
        expression: BoundExpressionId,
        operation: BoundOperator,
        left: ConstantTermId,
        ty: TypeId,
    ) -> Result<Option<ConstantTermId>, EvaluationFailure> {
        if !matches!(
            operation,
            BoundOperator::LogicalAnd | BoundOperator::LogicalOr
        ) {
            return Ok(None);
        }

        let Some(left) = self.term_value(left)? else {
            return Ok(None);
        };

        let left = self.constant_value(left)?;

        let ConstantValueKind::Boolean(left) = left.kind() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let result = match operation {
            BoundOperator::LogicalAnd if !*left => Some(false),
            BoundOperator::LogicalOr if *left => Some(true),
            _ => None,
        };

        result
            .map(|value| self.intern_value_term(ty, ConstantValueKind::Boolean(value)))
            .transpose()
    }

    pub(super) fn evaluate_conversion(
        &mut self,
        expression: BoundExpressionId,
        source: bray_bound_tree::BoundConversionExpression,
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        // The evaluator mutates its budget while the immutable selection plan is used recursively.
        let conversion = match self.input.semantic_selections().expression(expression) {
            Some(SemanticSelection::Operation(SelectedOperation::Conversion(conversion))) => {
                conversion.clone()
            }
            _ => return Err(EvaluationFailure::invalid_expression(expression)),
        };

        let operand = self.evaluate(source.operand())?;

        if matches!(conversion.target(), ConversionTarget::Identity) {
            return Ok(operand);
        }

        let Some(operand) = self.term_value(operand)? else {
            // TODO(BRA-244): Add checked conversion operations to open constant terms.
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let value = self.convert_value(expression, &conversion, operand)?;
        let data = self.constant_value(value)?;

        if data.ty() != ty {
            return Err(EvaluationFailure::Infrastructure(
                CheckerInfrastructureError::InvalidConstantEvaluationInput,
            ));
        }

        self.intern_term(ConstantTermData::Value(value))
    }

    pub(super) fn convert_value(
        &self,
        expression: BoundExpressionId,
        conversion: &bray_bound_tree::SelectedConversion,
        value: ConstantValueId,
    ) -> Result<ConstantValueId, EvaluationFailure> {
        let data = self.constant_value(value)?;

        if data.ty() != conversion.source_type() {
            return Err(EvaluationFailure::Infrastructure(
                CheckerInfrastructureError::InvalidConstantEvaluationInput,
            ));
        }

        let kind = match conversion.target() {
            ConversionTarget::Identity => return Ok(value),
            ConversionTarget::BuiltInScalar => {
                let Some(target) = type_representation(self.request, conversion.target_type())
                    .map_err(EvaluationFailure::Infrastructure)?
                else {
                    return Err(EvaluationFailure::invalid_expression(expression));
                };

                convert_scalar(data.kind(), target, || {
                    self.request
                        .selected_target()
                        .machine()
                        .pointer_width_bits()
                })
                .map_err(|error| EvaluationFailure::operation(expression, error))?
            }
            ConversionTarget::Composite(children) => {
                self.convert_composite(expression, conversion.target_type(), data.kind(), children)?
            }
            ConversionTarget::Trait { .. } => {
                // TODO(BRA-245): Evaluate const-call conversion implementations.
                return Err(EvaluationFailure::invalid_expression(expression));
            }
        };

        self.intern_value(conversion.target_type(), kind)
    }

    pub(super) fn convert_composite(
        &self,
        expression: BoundExpressionId,
        target: TypeId,
        value: &ConstantValueKind,
        children: &[bray_bound_tree::SelectedConversion],
    ) -> Result<ConstantValueKind, EvaluationFailure> {
        let target_is_complex = type_representation(self.request, target)
            .map_err(EvaluationFailure::Infrastructure)?
            .is_some_and(|role| role.numeric_kind() == Some(NumericRepresentationKind::Complex));

        match value {
            ConstantValueKind::Tuple(values)
                if target_is_complex && values.len() == 2 && children.len() == 2 =>
            {
                let real = self.convert_value(expression, &children[0], values[0])?;
                let imaginary = self.convert_value(expression, &children[1], values[1])?;
                let real = self.real_component(expression, real)?;
                let imaginary = self.real_component(expression, imaginary)?;

                Ok(ConstantValueKind::Complex { real, imaginary })
            }
            ConstantValueKind::Tuple(values) if values.len() == children.len() => values
                .iter()
                .copied()
                .zip(children)
                .map(|(value, conversion)| self.convert_value(expression, conversion, value))
                .collect::<Result<Vec<_>, _>>()
                .map(ConstantValueKind::tuple),
            ConstantValueKind::Array(values) if children.len() == 1 => values
                .iter()
                .copied()
                .map(|value| self.convert_value(expression, &children[0], value))
                .collect::<Result<Vec<_>, _>>()
                .map(ConstantValueKind::array),
            ConstantValueKind::NullableAbsent if children.len() == 1 => {
                Ok(ConstantValueKind::NullableAbsent)
            }
            ConstantValueKind::NullablePresent(value) if children.len() == 1 => self
                .convert_value(expression, &children[0], *value)
                .map(ConstantValueKind::NullablePresent),
            _ => Err(EvaluationFailure::invalid_expression(expression)),
        }
    }

    pub(super) fn evaluate_index(
        &mut self,
        expression: BoundExpressionId,
        operands: &[BoundExpressionId],
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let Some(SemanticSelection::Operation(SelectedOperation::Index {
            target: IndexTarget::ArrayElement,
            ..
        })) = self.input.semantic_selections().expression(expression)
        else {
            // TODO(BRA-245): Evaluate slice and selected const-call indexing operations.
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let [subject, index] = operands else {
            return Err(EvaluationFailure::Infrastructure(
                CheckerInfrastructureError::InvalidConstantEvaluationInput,
            ));
        };

        let subject = self.evaluate(*subject)?;
        let index = self.evaluate(*index)?;

        let (Some(subject_value), Some(index_value)) =
            (self.term_value(subject)?, self.term_value(index)?)
        else {
            return self.intern_term(ConstantTermData::Projection(ConstantProjection::new(
                subject,
                ConstantProjectionKind::ArrayElement(index),
            )));
        };

        let subject_value = self.constant_value(subject_value)?;

        let ConstantValueKind::Array(elements) = subject_value.kind() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let index = self.array_count(expression, index_value)?;

        let Some(value) = elements.get(index).copied() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        self.intern_term(ConstantTermData::Value(value))
    }

    pub(super) fn evaluate_member_projection(
        &mut self,
        expression: BoundExpressionId,
        member: &bray_bound_tree::BoundMemberAccessExpression,
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        if !matches!(
            self.input.semantic_selections().expression(expression),
            Some(SemanticSelection::Operation(SelectedOperation::Member(_)))
        ) {
            return Err(EvaluationFailure::invalid_expression(expression));
        }

        let BoundMemberSelector::TupleElement(ordinal) = member
            .selector()
            .ok_or_else(|| EvaluationFailure::invalid_expression(expression))?
        else {
            // TODO(BRA-246): Retain product and union field identities in closed values for projection.
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let receiver = self.evaluate(member.receiver())?;

        if self.term_value(receiver)?.is_none() {
            return self.intern_term(ConstantTermData::Projection(ConstantProjection::new(
                receiver,
                ConstantProjectionKind::TupleElement(SymbolOrdinal::new(*ordinal)),
            )));
        }

        let receiver = self.closed_value(receiver, expression)?;
        let receiver = self.constant_value(receiver)?;

        let ConstantValueKind::Tuple(elements) = receiver.kind() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let index = usize::try_from(*ordinal)
            .map_err(|_| EvaluationFailure::invalid_expression(expression))?;

        let Some(value) = elements.get(index).copied() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let value_data = self.constant_value(value)?;

        if value_data.ty() != ty {
            return Err(EvaluationFailure::Infrastructure(
                CheckerInfrastructureError::InvalidConstantEvaluationInput,
            ));
        }

        self.intern_term(ConstantTermData::Value(value))
    }

    pub(super) fn evaluate_construction(
        &mut self,
        expression: BoundExpressionId,
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let Some(SemanticSelection::Operation(SelectedOperation::Construction(construction))) =
            self.input.semantic_selections().expression(expression)
        else {
            // TODO(BRA-245): Evaluate const-call construction operations.
            // TODO(BRA-246): Evaluate value-bearing construction operations.
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        if !construction.inputs().is_empty() {
            // TODO(BRA-246): Retain field identities in product and union constant values.
            return Err(EvaluationFailure::invalid_expression(expression));
        }

        let kind = match construction.target() {
            ConstructionTarget::Struct(_) => ConstantValueKind::product([]),
            ConstructionTarget::UnionVariant(variant) => ConstantValueKind::union(variant, []),
            ConstructionTarget::TypeForm(_) => {
                // TODO(BRA-245): Evaluate const type-form construction callables.
                return Err(EvaluationFailure::invalid_expression(expression));
            }
        };

        self.intern_value_term(ty, kind)
    }

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
}
