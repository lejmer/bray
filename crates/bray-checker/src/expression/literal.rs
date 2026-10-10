use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundOperator, CheckedExpressionTypes,
    CheckedLiteralValueEntry, CheckedLiteralValues,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticExpressionCategory, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, SeverityKind,
};
use bray_symbols::{ConstantValueData, ConstantValueKind};

use crate::constant::{
    ConstantEvaluationLimits, ConstantLiteralError, check_byte_string_literal,
    check_constant_literal, check_negated_integer_operand_literal, literal_diagnostic_kind,
};
use crate::diagnostic::{diagnostic_id, diagnostic_type, expression_span};
use crate::representation::type_representation;
use crate::unit::assert_unit_inputs;
use crate::{CheckerOutcome, CheckerQueryError, CheckerRequestContext, CheckerUnitView};

pub(crate) fn check_literal_values<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
) -> CheckerOutcome<CheckedLiteralValues, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    assert_unit_inputs(
        request,
        [("expression types", (types.unit(), types.kind()))],
    );

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
            panic!(
                "expression {:?} must have a committed node and inference input",
                expression
            );
        };

        let mut diagnostic = None;

        let kind = if result.is_recovered() {
            ConstantValueKind::Error
        } else {
            let source = request.source(literal.origin().source_anchor());

            let Some(spelling) = source.text_for_range(literal.spelling_range()) else {
                panic!(
                    "A bound anchor does not cover a valid UTF-8 range in its source revision. in check_literal_values, span: {:?}",
                    bray_source::SourceSpan::new(
                        source.span().source_id(),
                        literal.spelling_range(),
                    )
                );
            };

            let bytes = u64::try_from(spelling.len()).unwrap_or(u64::MAX);

            let checked = match remaining_bytes.checked_sub(bytes) {
                Some(remaining) => {
                    remaining_bytes = remaining;

                    check_literal(
                        request,
                        *literal,
                        spelling,
                        result.ty(),
                        target_width,
                        negated_operands.contains(&expression),
                    )
                }
                None => Err(ConstantLiteralError::SizeLimitExceeded {
                    actual: ConstantEvaluationLimits::default()
                        .literal_bytes()
                        .saturating_sub(remaining_bytes)
                        .saturating_add(bytes),
                    maximum: ConstantEvaluationLimits::default().literal_bytes(),
                }),
            };

            match checked {
                Ok(kind) => kind,
                Err(error) => {
                    let span = expression_span(request, expression);

                    let mut produced = Diagnostic::new(
                        diagnostic_id(diagnostics.len()),
                        literal_diagnostic_kind(error),
                        SeverityKind::Error,
                    )
                    .with_primary_span(span)
                    .with_label(DiagnosticLabel::primary(
                        DiagnosticLabelKind::InvalidConstantExpression,
                        span,
                    ));

                    produced = match error {
                        ConstantLiteralError::Invalid => produced
                            .with_arg(DiagnosticArg::expression_category(
                                DiagnosticExpressionCategory::Literal,
                            ))
                            .with_note(DiagnosticNote::new(
                                DiagnosticNoteKind::ConstantExpressionMustBeEvaluable,
                            )),
                        ConstantLiteralError::NotRepresentable => {
                            produced.with_arg(DiagnosticArg::actual_type(
                                match diagnostic_type(request.context(), result.ty()) {
                                    Ok(ty) => ty,
                                    Err(CheckerQueryError::Cancelled) => {
                                        return CheckerOutcome::Cancelled;
                                    }
                                    Err(CheckerQueryError::Upstream(error)) => {
                                        return CheckerOutcome::UpstreamFailure(error);
                                    }
                                },
                            ))
                        }
                        ConstantLiteralError::SizeLimitExceeded { actual, maximum } => produced
                            .with_arg(DiagnosticArg::actual_count(actual))
                            .with_arg(DiagnosticArg::maximum_count(maximum))
                            .with_note(DiagnosticNote::new(
                                DiagnosticNoteKind::ConstantEvaluationMustFitLimits,
                            )),
                    };

                    diagnostic = Some(produced);

                    ConstantValueKind::Error
                }
            }
        };

        let value = match request
            .semantic_values()
            .intern_constant_value(ConstantValueData::new(result.ty(), kind))
        {
            Ok(value) => value,
            Err(error) => {
                panic!(
                    "The canonical semantic value store rejected a construction or lookup operation. in check_literal_values, value0: {:?}",
                    error
                );
            }
        };

        entries.push(CheckedLiteralValueEntry::new(expression, value));

        if let Some(diagnostic) = diagnostic {
            diagnostics.push(diagnostic);
        }
    }

    let values = CheckedLiteralValues::try_new(
        request.unit(),
        types,
        request.semantic_values(),
        target_width,
        entries,
    )
    .unwrap_or_else(|error| {
        panic!(
            "literal values must agree with checked types for {:?}: {error:?}",
            request.unit().unit()
        )
    });

    CheckerOutcome::complete(values, DiagnosticBag::from(diagnostics))
}

fn check_literal<C>(
    request: CheckerUnitView<'_, C>,
    literal: bray_bound_tree::BoundLiteralExpression,
    spelling: &str,
    ty: bray_symbols::TypeId,
    target_width: std::num::NonZeroU16,
    is_negated_operand: bool,
) -> Result<ConstantValueKind, ConstantLiteralError>
where
    C: CheckerRequestContext + ?Sized,
{
    if literal.kind() == bray_bound_tree::BoundLiteralKind::ByteString {
        return check_byte_string_literal(request.semantic_values(), ty, spelling).unwrap_or_else(
            |error| {
                panic!("check_literal must satisfy its checked construction contract: {error:?}")
            },
        );
    }

    let representation = type_representation(request, ty);

    let Some(representation) = representation else {
        return Err(ConstantLiteralError::Invalid);
    };

    if is_negated_operand && literal.kind() == bray_bound_tree::BoundLiteralKind::Integer {
        return check_negated_integer_operand_literal(spelling, representation, || target_width);
    }

    check_constant_literal(literal.kind(), spelling, representation, || target_width)
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
