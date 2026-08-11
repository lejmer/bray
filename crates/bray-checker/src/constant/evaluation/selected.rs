use bray_bound_tree::{
    BoundCallResult, BoundCallableTarget, BoundExpression, BoundExpressionId, BoundMemberSelector,
    BoundOperator, ConstructionInputId, ConstructionTarget, ConversionTarget, IndexTarget,
    OperatorTarget, SelectedArgument, SelectedConstructionInput, SelectedOperation,
    SemanticSelection,
};
use bray_compiler_known::NumericRepresentationKind;
use bray_diagnostics::DiagnosticConstantOperation;
use bray_symbols::{
    AnySymbolId, ConstantField, ConstantProjection, ConstantProjectionKind, ConstantTermData,
    ConstantTermId, ConstantValueId, ConstantValueKind, SymbolOrdinal, TypeId,
};

use crate::CheckerRequestContext;
use crate::constant::conversion::convert_scalar;
use crate::constant::diagnostic::diagnostic_operation;
use crate::constant::operation::{fold_binary, fold_unary};
use crate::representation::type_representation;

use super::engine::Evaluator;
use super::support::{EvaluationFailure, binary_term_operation, unary_term_operation};

impl<'view, 'input, 'types, C> Evaluator<'view, 'input, 'types, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn evaluate_selected_call(
        &mut self,
        expression: BoundExpressionId,
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let Some(SemanticSelection::Call(call)) =
            self.input.semantic_selections().expression(expression)
        else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        if !matches!(call.resolution().result(), BoundCallResult::Immediate(_)) {
            return Err(EvaluationFailure::invalid_expression(expression));
        }

        let mut arguments = Vec::with_capacity(call.arguments().len());

        for argument in call.arguments() {
            let SelectedArgument::Explicit {
                expression,
                ordinal,
                ..
            } = argument
            else {
                return Err(EvaluationFailure::invalid_expression(expression));
            };

            arguments.push((*ordinal, *expression));
        }

        arguments.sort_unstable_by_key(|(ordinal, _)| *ordinal);

        let arguments = arguments
            .into_iter()
            .map(|(_, expression)| expression)
            .collect::<Vec<_>>();

        if let BoundCallableTarget::Predicate(predicate) = call.target() {
            let arguments = arguments
                .into_iter()
                .map(|argument| self.evaluate(argument))
                .collect::<Result<Vec<_>, _>>()?;

            return self.intern_term(ConstantTermData::predicate_call(predicate, arguments));
        }

        let BoundCallableTarget::Declaration(callable) = call.target() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let witnesses = call.resolution().implementation_witnesses();

        let selected_implementation = match witnesses {
            [] => None,
            [witness] => Some(*witness),
            _ => return Err(EvaluationFailure::invalid_expression(expression)),
        };

        self.evaluate_call(
            expression,
            callable,
            selected_implementation,
            &arguments,
            ty,
        )
    }

    pub(super) fn evaluate_operator(
        &mut self,
        expression: BoundExpressionId,
        operation: BoundOperator,
        operands: &[BoundExpressionId],
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let Some(SemanticSelection::Operation(SelectedOperation::Operator { target, .. })) =
            self.input.semantic_selections().expression(expression)
        else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        match target {
            OperatorTarget::BuiltIn(selected) if *selected != operation => {
                return Err(EvaluationFailure::invalid_input());
            }
            OperatorTarget::BuiltIn(_) => {}
            OperatorTarget::Trait {
                operator,
                fulfillment,
                witness,
                ..
            } if *operator == operation => {
                return self.evaluate_call(expression, *fulfillment, Some(*witness), operands, ty);
            }
            OperatorTarget::Trait { .. } => {
                return Err(EvaluationFailure::invalid_input());
            }
            OperatorTarget::TraitConstraint { operator, .. } if *operator == operation => {
                return Err(EvaluationFailure::invalid_expression(expression));
            }
            OperatorTarget::TraitConstraint { .. } => {
                return Err(EvaluationFailure::invalid_input());
            }
        }

        match operands {
            [operand] => self.evaluate_unary_operator(expression, operation, *operand, ty),
            [left, right] => {
                self.evaluate_binary_operator(expression, operation, *left, *right, ty)
            }
            _ => Err(EvaluationFailure::invalid_input()),
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
                return Err(EvaluationFailure::invalid_expression(expression));
            };

            return self.intern_term(ConstantTermData::Unary { operation, operand });
        };

        let value = self.constant_value(value)?;

        let representation =
            type_representation(self.request, ty).map_err(EvaluationFailure::Infrastructure)?;

        let target_width = self.target_integer_width(representation);

        let kind = fold_unary(operation, value.kind(), representation, target_width)
            .map_err(|error| {
                EvaluationFailure::operation(expression, diagnostic_operation(operation), error)
            })?;

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
        .map_err(|error| {
            EvaluationFailure::operation(expression, diagnostic_operation(operation), error)
        })?;

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

        if let ConversionTarget::Trait {
            fulfillment,
            witness,
            ..
        } = conversion.target()
        {
            return self.evaluate_call(
                expression,
                *fulfillment,
                Some(*witness),
                std::slice::from_ref(&source.operand()),
                ty,
            );
        }

        let operand = self.evaluate(source.operand())?;

        if matches!(conversion.target(), ConversionTarget::Identity) {
            return Ok(operand);
        }

        let Some(operand) = self.term_value(operand)? else {
            return match conversion.target() {
                ConversionTarget::Identity => Ok(operand),
                ConversionTarget::BuiltInScalar => self.intern_term(ConstantTermData::Conversion {
                    operand,
                    target: conversion.target_type(),
                }),
                ConversionTarget::Composite(_) => self.intern_term(ConstantTermData::Conversion {
                    operand,
                    target: conversion.target_type(),
                }),
                ConversionTarget::Trait { .. } | ConversionTarget::TraitConstraint { .. } => {
                    Err(EvaluationFailure::invalid_expression(expression))
                }
            };
        };

        let value = self.convert_value(expression, &conversion, operand)?;
        let data = self.constant_value(value)?;

        if data.ty() != ty {
            return Err(EvaluationFailure::invalid_input());
        }

        self.intern_term(ConstantTermData::Value(value))
    }

    pub(super) fn convert_value(
        &mut self,
        expression: BoundExpressionId,
        conversion: &bray_bound_tree::SelectedConversion,
        value: ConstantValueId,
    ) -> Result<ConstantValueId, EvaluationFailure> {
        let data = self.constant_value(value)?;

        if data.ty() != conversion.source_type() {
            return Err(EvaluationFailure::invalid_input());
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
                .map_err(|error| {
                    EvaluationFailure::operation(
                        expression,
                        DiagnosticConstantOperation::Conversion,
                        error,
                    )
                })?
            }
            ConversionTarget::Composite(children) => {
                self.convert_composite(expression, conversion.target_type(), data.kind(), children)?
            }
            ConversionTarget::Trait {
                fulfillment,
                witness,
                ..
            } => {
                return self.evaluate_call_values(
                    expression,
                    *fulfillment,
                    Some(*witness),
                    [value],
                    conversion.target_type(),
                );
            }
            ConversionTarget::TraitConstraint { .. } => {
                return Err(EvaluationFailure::invalid_expression(expression));
            }
        };

        self.intern_value(conversion.target_type(), kind)
    }

    pub(super) fn convert_composite(
        &mut self,
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
        structured: &bray_bound_tree::BoundStructuredExpression,
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let Some(SemanticSelection::Operation(SelectedOperation::Index { target, .. })) =
            self.input.semantic_selections().expression(expression)
        else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        if let IndexTarget::Custom {
            fulfillment,
            witness,
            ..
        } = target
        {
            return self.evaluate_call(
                expression,
                *fulfillment,
                Some(*witness),
                structured.operands(),
                ty,
            );
        }

        if matches!(target, IndexTarget::ArraySlice | IndexTarget::Slice) {
            return self.evaluate_slice(expression, structured, ty);
        }

        if !matches!(
            target,
            IndexTarget::ArrayElement | IndexTarget::SliceElement
        ) {
            return Err(EvaluationFailure::invalid_expression(expression));
        }

        let operands = structured.operands();

        let [subject, index] = operands else {
            return Err(EvaluationFailure::invalid_input());
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

    fn evaluate_slice(
        &mut self,
        expression: BoundExpressionId,
        structured: &bray_bound_tree::BoundStructuredExpression,
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let Some(subject) = structured.operands().first().copied() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let subject = self.evaluate(subject)?;
        let subject = self.closed_value(subject, expression)?;
        let subject = self.constant_value(subject)?;

        let ConstantValueKind::Array(elements) = subject.kind() else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let bounds = structured
            .slice_bounds()
            .ok_or_else(|| EvaluationFailure::invalid_expression(expression))?;

        let lower = self
            .evaluate_slice_bound(expression, bounds.lower())?
            .unwrap_or(0);

        let upper = self
            .evaluate_slice_bound(expression, bounds.upper())?
            .unwrap_or(elements.len());

        let Some(values) = elements.get(lower..upper) else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        self.budget.charge_elements(expression, values.len())?;

        self.intern_value_term(ty, ConstantValueKind::array(values.iter().copied()))
    }

    fn evaluate_slice_bound(
        &mut self,
        expression: BoundExpressionId,
        bound: Option<BoundExpressionId>,
    ) -> Result<Option<usize>, EvaluationFailure> {
        let Some(bound) = bound else {
            return Ok(None);
        };

        let bound = self.evaluate(bound)?;
        let bound = self.closed_value(bound, expression)?;

        self.array_count(expression, bound).map(Some)
    }

    pub(super) fn evaluate_member_projection(
        &mut self,
        expression: BoundExpressionId,
        member: &bray_bound_tree::BoundMemberAccessExpression,
        ty: TypeId,
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let selected_member = match self.input.semantic_selections().expression(expression) {
            Some(SemanticSelection::Operation(SelectedOperation::Member(target))) => {
                target.member()
            }
            _ => return Err(EvaluationFailure::invalid_expression(expression)),
        };

        if matches!(selected_member, AnySymbolId::Constant(_)) {
            return self.evaluate_reference(expression, None, ty);
        }

        let selector = member
            .selector()
            .ok_or_else(|| EvaluationFailure::invalid_expression(expression))?;

        let projection = match (selector, selected_member) {
            (BoundMemberSelector::TupleElement(ordinal), _) => {
                ConstantProjectionKind::TupleElement(SymbolOrdinal::new(*ordinal))
            }
            (BoundMemberSelector::Name(_), AnySymbolId::StructField(field)) => {
                ConstantProjectionKind::ProductField(field)
            }
            (BoundMemberSelector::Name(_), AnySymbolId::UnionPayloadField(field)) => {
                ConstantProjectionKind::UnionPayloadField(field)
            }
            _ => return Err(EvaluationFailure::invalid_expression(expression)),
        };

        let receiver = self.evaluate(member.receiver())?;

        if self.term_value(receiver)?.is_none() {
            return self.intern_term(ConstantTermData::Projection(ConstantProjection::new(
                receiver, projection,
            )));
        }

        let receiver = self.closed_value(receiver, expression)?;
        let receiver = self.constant_value(receiver)?;

        let value = match (receiver.kind(), projection) {
            (ConstantValueKind::Tuple(elements), ConstantProjectionKind::TupleElement(ordinal)) => {
                elements.get(ordinal.raw() as usize).copied()
            }
            (ConstantValueKind::Product(fields), ConstantProjectionKind::ProductField(field)) => {
                fields
                    .iter()
                    .find(|entry| *entry.field() == field)
                    .map(|entry| *entry.value())
            }
            (
                ConstantValueKind::Union { fields, .. },
                ConstantProjectionKind::UnionPayloadField(field),
            ) => fields
                .iter()
                .find(|entry| *entry.field() == field)
                .map(|entry| *entry.value()),
            _ => None,
        };

        let Some(value) = value else {
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let value_data = self.constant_value(value)?;

        if value_data.ty() != ty {
            return Err(EvaluationFailure::invalid_input());
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
            return Err(EvaluationFailure::invalid_expression(expression));
        };

        let target = construction.target();
        let inputs = construction.inputs().to_vec();

        match target {
            ConstructionTarget::Struct(_) => {
                self.evaluate_product_construction(expression, ty, &inputs)
            }
            ConstructionTarget::UnionVariant(variant) => {
                self.evaluate_union_construction(expression, ty, variant, &inputs)
            }
            ConstructionTarget::TypeForm { callable, .. } => {
                let mut inputs = inputs
                    .iter()
                    .map(|input| match input {
                        SelectedConstructionInput::Explicit {
                            expression,
                            ordinal,
                            ..
                        } => Ok((*ordinal, *expression)),
                        SelectedConstructionInput::Default { .. } => {
                            Err(EvaluationFailure::invalid_expression(expression))
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                inputs.sort_unstable_by_key(|(ordinal, _)| *ordinal);

                let inputs = inputs
                    .into_iter()
                    .map(|(_, input)| input)
                    .collect::<Vec<_>>();

                self.evaluate_call(expression, callable, None, &inputs, ty)
            }
        }
    }

    fn evaluate_product_construction(
        &mut self,
        expression: BoundExpressionId,
        ty: TypeId,
        inputs: &[SelectedConstructionInput],
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let inputs = self.evaluate_construction_inputs(expression, inputs)?;
        let mut fields = Vec::with_capacity(inputs.len());

        for (input, value) in inputs {
            let ConstructionInputId::StructField(field) = input else {
                return Err(EvaluationFailure::invalid_expression(expression));
            };

            fields.push(ConstantField::new(field, value));
        }

        match self.closed_fields(&fields)? {
            Some(fields) => self.intern_value_term(ty, ConstantValueKind::product(fields)),
            None => self.intern_term(ConstantTermData::product(fields)),
        }
    }

    fn evaluate_union_construction(
        &mut self,
        expression: BoundExpressionId,
        ty: TypeId,
        variant: bray_symbols::UnionVariantSymbolId,
        inputs: &[SelectedConstructionInput],
    ) -> Result<ConstantTermId, EvaluationFailure> {
        let inputs = self.evaluate_construction_inputs(expression, inputs)?;
        let mut fields = Vec::with_capacity(inputs.len());

        for (input, value) in inputs {
            let ConstructionInputId::UnionPayloadField(field) = input else {
                return Err(EvaluationFailure::invalid_expression(expression));
            };

            fields.push(ConstantField::new(field, value));
        }

        match self.closed_fields(&fields)? {
            Some(fields) => self.intern_value_term(ty, ConstantValueKind::union(variant, fields)),
            None => self.intern_term(ConstantTermData::union(variant, fields)),
        }
    }

    fn evaluate_construction_inputs(
        &mut self,
        expression: BoundExpressionId,
        inputs: &[SelectedConstructionInput],
    ) -> Result<Vec<(ConstructionInputId, ConstantTermId)>, EvaluationFailure> {
        let mut evaluated = Vec::with_capacity(inputs.len());

        for input in inputs {
            let SelectedConstructionInput::Explicit {
                expression: value,
                input,
                ordinal,
                ..
            } = *input
            else {
                return Err(EvaluationFailure::invalid_expression(expression));
            };

            evaluated.push((ordinal, input, self.evaluate(value)?));
        }

        evaluated.sort_unstable_by_key(|(ordinal, _, _)| *ordinal);

        Ok(evaluated
            .into_iter()
            .map(|(_, input, value)| (input, value))
            .collect())
    }

    fn closed_fields<I>(
        &self,
        fields: &[ConstantField<I, ConstantTermId>],
    ) -> Result<Option<Vec<ConstantField<I, ConstantValueId>>>, EvaluationFailure>
    where
        I: Copy,
    {
        let mut values = Vec::with_capacity(fields.len());

        for field in fields {
            let Some(value) = self.term_value(*field.value())? else {
                return Ok(None);
            };

            values.push(ConstantField::new(*field.field(), value));
        }

        Ok(Some(values))
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
