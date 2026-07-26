use bray_diagnostics::DiagnosticKind;
use bray_symbols::{
    ConstantField, ConstantInstanceKey, ConstantProjectionKind, ConstantTermData, ConstantTermId,
    ConstantValueId, ConstantValueKind, TypeId,
};

use super::super::ConstantReferenceResolution;
use super::super::call::ConstantCallRequest;
use super::super::operation::fold_binary;
use super::evaluator::TemplateEvaluator;
use super::support::{
    TemplateEvaluationFailure, binary_operator, fact_failure, integer_index, operation_failure,
};
use crate::CheckerRequestContext;

pub(super) fn evaluate_term<C>(
    evaluator: &mut TemplateEvaluator<'_, C>,
    term: ConstantTermId,
    ty: TypeId,
) -> Result<ConstantValueId, TemplateEvaluationFailure>
where
    C: CheckerRequestContext + ?Sized,
{
    evaluator.observe_cancellation()?;

    evaluator
        .budget
        .try_charge_step()
        .map_err(TemplateEvaluationFailure::Diagnostic)?;

    let data = evaluator
        .context
        .semantic_values()
        .constant_term_data(term)
        .map_err(|_| TemplateEvaluationFailure::semantic_value())?;

    match data.as_ref() {
        ConstantTermData::Value(value) => Ok(*value),
        ConstantTermData::Unary { operation, operand } => {
            let operand = evaluator.evaluate_term(*operand, ty)?;

            evaluator.evaluate_unary(*operation, operand, ty)
        }
        ConstantTermData::Binary {
            operation,
            left,
            right,
        } => {
            let left = evaluator.evaluate_term(*left, ty)?;
            let right = evaluator.evaluate_term(*right, ty)?;
            let left = evaluator.constant_value(left)?;
            let right = evaluator.constant_value(right)?;

            let kind = fold_binary(
                binary_operator(*operation),
                left.kind(),
                right.kind(),
                evaluator.limits.integer_bits(),
            )
            .map_err(operation_failure)?;

            evaluator.intern_value(ty, kind)
        }
        ConstantTermData::Conversion { operand, target } => {
            let operand = evaluator.evaluate_term(*operand, ty)?;

            let target = evaluator
                .context
                .semantic_values()
                .substitute_type(*target, evaluator.substitution.substitution())
                .map_err(|_| TemplateEvaluationFailure::semantic_value())?;

            evaluator.evaluate_conversion(operand, target)
        }
        ConstantTermData::NullablePresent(value) => {
            let value = evaluator.evaluate_term(*value, ty)?;

            evaluator.intern_value(ty, ConstantValueKind::NullablePresent(value))
        }
        ConstantTermData::Tuple(values) => {
            let values = values
                .iter()
                .map(|value| evaluator.evaluate_term(*value, ty))
                .collect::<Result<Vec<_>, _>>()?;

            evaluator
                .budget
                .try_charge_elements(values.len())
                .map_err(TemplateEvaluationFailure::Diagnostic)?;

            evaluator.intern_value(ty, ConstantValueKind::Tuple(values.into()))
        }
        ConstantTermData::Array(values) => {
            let values = values
                .iter()
                .map(|value| evaluator.evaluate_term(*value, ty))
                .collect::<Result<Vec<_>, _>>()?;

            evaluator
                .budget
                .try_charge_elements(values.len())
                .map_err(TemplateEvaluationFailure::Diagnostic)?;

            evaluator.intern_value(ty, ConstantValueKind::Array(values.into()))
        }
        ConstantTermData::Product(fields) => {
            let fields = fields
                .iter()
                .map(|field| {
                    evaluator
                        .evaluate_term(*field.value(), ty)
                        .map(|value| ConstantField::new(*field.field(), value))
                })
                .collect::<Result<Vec<_>, _>>()?;

            evaluator
                .budget
                .try_charge_elements(fields.len())
                .map_err(TemplateEvaluationFailure::Diagnostic)?;

            evaluator.intern_value(ty, ConstantValueKind::Product(fields.into()))
        }
        ConstantTermData::Union { variant, fields } => {
            let fields = fields
                .iter()
                .map(|field| {
                    evaluator
                        .evaluate_term(*field.value(), ty)
                        .map(|value| ConstantField::new(*field.field(), value))
                })
                .collect::<Result<Vec<_>, _>>()?;

            evaluator
                .budget
                .try_charge_elements(fields.len())
                .map_err(TemplateEvaluationFailure::Diagnostic)?;

            evaluator.intern_value(
                ty,
                ConstantValueKind::Union {
                    variant: *variant,
                    fields: fields.into(),
                },
            )
        }
        ConstantTermData::DefinitionApplication {
            definition,
            substitution,
            selected_implementation,
        } => {
            let substitution = evaluator
                .context
                .semantic_values()
                .require_concrete_substitution(*substitution)
                .map_err(|_| TemplateEvaluationFailure::semantic_value())?;

            let result = evaluator
                .resolver
                .resolve_constant(ConstantInstanceKey::new(
                    *definition,
                    substitution,
                    *selected_implementation,
                ))
                .map_err(fact_failure)?;

            evaluator.diagnostics = evaluator.diagnostics.merged(result.diagnostics());

            match result.value() {
                ConstantReferenceResolution::Value(value) => Ok(*value),
                ConstantReferenceResolution::Term(term) => evaluator.evaluate_term(*term, ty),
                ConstantReferenceResolution::Cycle => Err(TemplateEvaluationFailure::Diagnostic(
                    DiagnosticKind::CheckingCyclicConstantDefinition,
                )),
                ConstantReferenceResolution::Invalid => {
                    Err(TemplateEvaluationFailure::invalid_input())
                }
            }
        }
        ConstantTermData::Call {
            callable,
            selected_implementation,
            arguments,
        } => {
            let callable = evaluator
                .context
                .semantic_values()
                .callable_instance_data(*callable)
                .map_err(|_| TemplateEvaluationFailure::semantic_value())?;

            let arguments = arguments
                .iter()
                .map(|argument| evaluator.evaluate_term(*argument, ty))
                .collect::<Result<Vec<_>, _>>()?;

            let limits = evaluator.budget.remaining_limits(evaluator.limits);

            let Some(limits) = limits.nested_call() else {
                return Err(TemplateEvaluationFailure::Diagnostic(
                    DiagnosticKind::CheckingConstantEvaluationStepLimitExceeded,
                ));
            };

            let request = ConstantCallRequest::new(
                *callable,
                *selected_implementation,
                arguments,
                ty,
                limits,
            );

            evaluator.resolve_call(&request, ty)
        }
        ConstantTermData::Projection(projection) => {
            let subject = evaluator.evaluate_term(projection.subject(), ty)?;
            let subject = evaluator.constant_value(subject)?;

            let value = match (subject.kind(), projection.kind()) {
                (
                    ConstantValueKind::Tuple(elements),
                    ConstantProjectionKind::TupleElement(ordinal),
                ) => ordinal
                    .to_index()
                    .and_then(|index| elements.get(index))
                    .copied(),
                (
                    ConstantValueKind::Array(elements),
                    ConstantProjectionKind::ArrayElement(index),
                ) => {
                    let index = evaluator.evaluate_term(index, ty)?;
                    let index = evaluator.constant_value(index)?;

                    integer_index(index.kind())
                        .and_then(|index| elements.get(index))
                        .copied()
                }
                (
                    ConstantValueKind::Product(fields),
                    ConstantProjectionKind::ProductField(field),
                ) => fields
                    .iter()
                    .find(|entry| *entry.field() == field)
                    .map(|entry| *entry.value()),
                (
                    ConstantValueKind::Union { fields, .. },
                    ConstantProjectionKind::UnionPayloadField(field),
                ) => fields
                    .iter()
                    .find(|entry| *entry.field() == field)
                    .map(|entry| *entry.value()),
                (
                    ConstantValueKind::NullablePresent(value),
                    ConstantProjectionKind::NullableValue,
                ) => Some(*value),
                _ => None,
            };

            value.ok_or_else(TemplateEvaluationFailure::invalid_input)
        }
        ConstantTermData::IntegerLiteral { .. }
        | ConstantTermData::Parameter(_)
        | ConstantTermData::TargetFact(_) => Err(TemplateEvaluationFailure::invalid_input()),
    }
}
