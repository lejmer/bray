use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundOperator, CheckedExpressionTypes,
    CheckedLiteralValueEntry, CheckedLiteralValues,
};
use bray_diagnostics::{Diagnostic, DiagnosticBag, SeverityKind};
use bray_symbols::{ConstantValueData, ConstantValueKind};

use crate::constant::{
    ConstantEvaluationLimits, ConstantLiteralError, check_constant_literal,
    check_negated_integer_operand_literal, literal_diagnostic_kind,
};
use crate::diagnostic::{diagnostic_id, expression_span};
use crate::representation::type_representation;
use crate::{CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerUnitView};

pub(crate) fn check_literal_values<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
) -> CheckerOutcome<CheckedLiteralValues>
where
    C: CheckerRequestContext + ?Sized,
{
    if types.unit() != request.view().unit() || types.kind() != request.view().kind() {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidLiteralValueInput,
        );
    }

    let target_width = request.selected_target().machine().pointer_width_bits();
    let mut remaining_bytes = ConstantEvaluationLimits::default().literal_bytes();
    let mut entries = Vec::new();
    let mut diagnostics = Vec::new();
    let negated_operands = negated_literal_operands(request);

    for (expression, node) in request.unit().tree().expressions() {
        if request.is_cancelled() {
            return CheckerOutcome::Cancelled;
        }

        let BoundExpression::Literal(literal) = node else {
            continue;
        };

        let Some(result) = types.expression(expression) else {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidLiteralValueInput,
            );
        };

        let mut diagnostic = None;

        let kind = if result.is_recovered() {
            ConstantValueKind::Error
        } else {
            let source = match request.source(literal.origin().source_anchor()) {
                Ok(source) => source,
                Err(error) => return CheckerOutcome::InfrastructureFailure(error),
            };

            let Some(spelling) = source.text_for_range(literal.spelling_range()) else {
                return CheckerOutcome::InfrastructureFailure(
                    CheckerInfrastructureError::InvalidSourceRange {
                        span: bray_source::SourceSpan::new(
                            source.span().source_id(),
                            literal.spelling_range(),
                        ),
                    },
                );
            };

            let bytes = u64::try_from(spelling.len()).unwrap_or(u64::MAX);

            let checked = match remaining_bytes.checked_sub(bytes) {
                Some(remaining) => {
                    remaining_bytes = remaining;

                    match check_literal(
                        request,
                        *literal,
                        spelling,
                        result.ty(),
                        target_width,
                        negated_operands.contains(&expression),
                    ) {
                        Ok(checked) => checked,
                        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
                    }
                }
                None => Err(ConstantLiteralError::SizeLimitExceeded),
            };

            match checked {
                Ok(kind) => kind,
                Err(error) => {
                    let span = match expression_span(request, expression) {
                        Ok(span) => span,
                        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
                    };

                    diagnostic = Some(
                        Diagnostic::new(
                            diagnostic_id(diagnostics.len()),
                            literal_diagnostic_kind(error),
                            SeverityKind::Error,
                        )
                        .with_primary_span(span),
                    );

                    ConstantValueKind::Error
                }
            }
        };

        let value = match request
            .semantic_values()
            .intern_constant_value(ConstantValueData::new(result.ty(), kind))
        {
            Ok(value) => value,
            Err(_) => {
                return CheckerOutcome::InfrastructureFailure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                );
            }
        };

        entries.push(CheckedLiteralValueEntry::new(expression, value));

        if let Some(diagnostic) = diagnostic {
            diagnostics.push(diagnostic);
        }
    }

    let values = match CheckedLiteralValues::try_new(
        request.unit(),
        types,
        request.semantic_values(),
        target_width,
        entries,
    ) {
        Ok(values) => values,
        Err(_) => {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidLiteralValueInput,
            );
        }
    };

    CheckerOutcome::complete(values, DiagnosticBag::from(diagnostics))
}

fn check_literal<C>(
    request: CheckerUnitView<'_, C>,
    literal: bray_bound_tree::BoundLiteralExpression,
    spelling: &str,
    ty: bray_symbols::TypeId,
    target_width: std::num::NonZeroU16,
    is_negated_operand: bool,
) -> Result<Result<ConstantValueKind, ConstantLiteralError>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let representation = type_representation(request, ty)?;

    let Some(representation) = representation else {
        return Ok(Err(ConstantLiteralError::Invalid));
    };

    if is_negated_operand && literal.kind() == bray_bound_tree::BoundLiteralKind::Integer {
        return Ok(check_negated_integer_operand_literal(
            spelling,
            representation,
            || target_width,
        ));
    }

    Ok(check_constant_literal(
        literal.kind(),
        spelling,
        representation,
        || target_width,
    ))
}

fn negated_literal_operands<C>(request: CheckerUnitView<'_, C>) -> BTreeSet<BoundExpressionId>
where
    C: CheckerRequestContext + ?Sized,
{
    request
        .unit()
        .tree()
        .expressions()
        .filter_map(|(_, expression)| match expression {
            BoundExpression::Unary(unary) if unary.operator() == BoundOperator::Subtract => {
                unary.operands().first().copied()
            }
            _ => None,
        })
        .collect()
}
