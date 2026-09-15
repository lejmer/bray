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
    ConstantEvaluationLimits, ConstantLiteralError, check_constant_literal,
    check_negated_integer_operand_literal, literal_diagnostic_kind,
};
use crate::diagnostic::{diagnostic_id, diagnostic_type, expression_span};
use crate::representation::type_representation;
use crate::unit::assert_unit_inputs;
use crate::{
    CheckerInfrastructureError, CheckerLiteralValueFailure, CheckerOutcome, CheckerQueryError,
    CheckerRequestContext, CheckerUnitView,
};

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
                    let span = match expression_span(request, expression) {
                        Ok(span) => span,
                        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
                    };

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
                                    Err(CheckerQueryError::Infrastructure(error)) => {
                                        return CheckerOutcome::InfrastructureFailure(error);
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
                return CheckerOutcome::InfrastructureFailure(
                    CheckerInfrastructureError::SemanticValueStore(error),
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
        Err(error) => {
            return CheckerOutcome::InfrastructureFailure(literal_value_table_error(error));
        }
    };

    CheckerOutcome::complete(values, DiagnosticBag::from(diagnostics))
}

fn literal_value_table_error(
    error: bray_bound_tree::CheckedLiteralValueTableBuildError,
) -> CheckerInfrastructureError {
    match error {
        bray_bound_tree::CheckedLiteralValueTableBuildError::ForeignExpressionTypes => {
            CheckerInfrastructureError::LiteralValue(
                CheckerLiteralValueFailure::ForeignExpressionTypes,
            )
        }
        bray_bound_tree::CheckedLiteralValueTableBuildError::InvalidLiteral(expression) => {
            CheckerInfrastructureError::LiteralValue(CheckerLiteralValueFailure::InvalidLiteral {
                expression,
            })
        }
        bray_bound_tree::CheckedLiteralValueTableBuildError::MissingExpressionType(expression) => {
            CheckerInfrastructureError::LiteralValue(
                CheckerLiteralValueFailure::MissingExpressionType { expression },
            )
        }
        bray_bound_tree::CheckedLiteralValueTableBuildError::MissingLiteralValue(expression) => {
            CheckerInfrastructureError::LiteralValue(
                CheckerLiteralValueFailure::MissingLiteralValue { expression },
            )
        }
        bray_bound_tree::CheckedLiteralValueTableBuildError::ValueTypeMismatch(expression) => {
            CheckerInfrastructureError::LiteralValue(
                CheckerLiteralValueFailure::ValueTypeMismatch { expression },
            )
        }
        bray_bound_tree::CheckedLiteralValueTableBuildError::DuplicateExpression(expression) => {
            CheckerInfrastructureError::LiteralValue(
                CheckerLiteralValueFailure::DuplicateExpression { expression },
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundLiteralKind, BoundUnitId, CheckedLiteralValueTableBuildError};

    use super::literal_value_table_error;
    use crate::test_support::{expression_unit, literal_expression};
    use crate::{CheckerInfrastructureError, CheckerLiteralValueFailure};

    #[test]
    fn literal_table_failures_preserve_expression_identity() {
        let (_, expressions) = expression_unit(BoundUnitId::new(1), |tree, origin| {
            vec![bray_bound_tree::testing::push_expression(
                tree,
                literal_expression(origin, BoundLiteralKind::Boolean, None),
            )]
        });

        assert_eq!(
            literal_value_table_error(CheckedLiteralValueTableBuildError::InvalidLiteral(
                expressions[0]
            )),
            CheckerInfrastructureError::LiteralValue(CheckerLiteralValueFailure::InvalidLiteral {
                expression: expressions[0],
            })
        );
    }
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
    let representation = type_representation(request, ty);

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
