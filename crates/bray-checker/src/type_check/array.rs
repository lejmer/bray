use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundLiteralKind, BoundStructuredExpressionKind,
    CheckedExpressionTypes,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind, DiagnosticType,
    SeverityKind,
};
use bray_symbols::{
    ConstantTermData, ConstantTermId, IntegerConstant, TargetSizedIntegerType, TypeData, TypeId,
};

use crate::constant::{integer_to_usize, normalize_integer_literal};
use crate::diagnostic::{diagnostic_id, expression_span};
use crate::representation::{representation_type, type_representation};
use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView,
};

use super::diagnostic_type;

pub(super) fn array_length<C>(
    request: CheckerUnitView<'_, C>,
    length: usize,
) -> Result<ConstantTermId, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let length = u64::try_from(length)
        .map_err(|_| CheckerInfrastructureError::ConstantArrayLengthCapacityExceeded { length })?;

    request
        .semantic_values()
        .intern_constant_term(ConstantTermData::IntegerLiteral {
            ty: TargetSizedIntegerType::Usize,
            value: IntegerConstant::from_u64(length),
        })
        .map_err(CheckerInfrastructureError::SemanticValueStore)
}

pub(super) fn inferred_byte_array_type<C>(
    request: CheckerUnitView<'_, C>,
    initializer: BoundExpressionId,
) -> Result<Option<TypeId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(length) = inferred_byte_array_length(request, initializer)? else {
        return Ok(None);
    };

    let element = representation_type(request, RepresentationRole::ScalarU8)?;
    let length = array_length(request, length)?;

    let ty = request
        .semantic_values()
        .intern_type(TypeData::Array { element, length })
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    Ok(Some(ty))
}

fn inferred_byte_array_length<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
) -> Result<Option<usize>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(BoundExpression::Structured(array)) = request.view().expression(expression) else {
        return Ok(None);
    };

    match array.kind() {
        BoundStructuredExpressionKind::Array => Ok(Some(array.operands().len())),
        BoundStructuredExpressionKind::RepeatedArray => {
            let Some(count) = array.operands().get(1).copied() else {
                return Ok(None);
            };

            let Some(BoundExpression::Literal(literal)) = request.view().expression(count) else {
                return Ok(None);
            };

            if literal.kind() != BoundLiteralKind::Integer || literal.is_recovered() {
                return Ok(None);
            }

            let source = request.source(literal.origin().source_anchor())?;

            let spelling = source.text_for_range(literal.spelling_range()).unwrap_or_else(|| {
                panic!("array count literal {:?} has an invalid source range", count)
            });

            Ok(normalize_integer_literal(spelling)
                .ok()
                .as_ref()
                .and_then(integer_to_usize))
        }
        _ => Ok(None),
    }
}

pub(super) fn append_inferred_bytes_diagnostics<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    initializers: &[BoundExpressionId],
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    for &initializer in initializers {
        let result = types.expression(initializer).unwrap_or_else(|| {
            panic!("inferred byte initializer {initializer:?} must have a checked type")
        });

        if result.is_recovered() {
            continue;
        }

        let data = request.semantic_values().type_data(result.ty());

        let (expected, actual) = match data.as_ref() {
            TypeData::Array { element, .. }
                if type_representation(request, *element) == Some(RepresentationRole::ScalarU8) => continue,
            TypeData::Array { element, .. } => {
                (DiagnosticType::U8, diagnostic_type(request, *element)?)
            }
            _ => (DiagnosticType::Array, diagnostic_type(request, result.ty())?),
        };

        let span = expression_span(request, initializer)?;

        diagnostics.push(
            Diagnostic::new(
                diagnostic_id(diagnostics.len()),
                DiagnosticKind::CheckingIncompatibleExpressionType,
                SeverityKind::Error,
            )
            .with_primary_span(span)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::IncompatibleExpressionType,
                span,
            ))
            .with_arg(DiagnosticArg::expected_type(expected))
            .with_arg(DiagnosticArg::actual_type(actual)),
        );
    }

    Ok(())
}
