use std::collections::BTreeMap;

use bray_bound_tree::{BoundExpression, BoundExpressionId, BoundLiteralKind};
use bray_compiler_known::NumericRepresentationKind;

use crate::{CheckerInfrastructureError, CheckerRequestContext, UnitCheckRequest};

use super::dependencies::{ExpressionTypeDependencies, numeric_kind};
use super::inference::{InferenceTypeId, TypeInferenceContext};

pub(super) fn adapt_contextual_literals<C>(
    request: UnitCheckRequest<'_, C>,
    expressions: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) -> Result<Option<bool>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let before = inference.revision();

    for &expression in expressions {
        if request.is_cancelled() {
            return Ok(None);
        }

        let Some((_, kind, variable)) = numeric_literal(request, expression, variables) else {
            continue;
        };

        if inference.evidence(variable).is_some() {
            continue;
        }

        let expected = inference.try_unique_matching_expectation(variable, |ty| {
            numeric_kind(request, ty).map(|expected| expected == Some(kind))
        })?;

        if let Some(expected) = expected {
            inference.add_evidence(variable, expected, expression);
        }
    }

    Ok(Some(inference.revision() != before))
}

pub(super) fn apply_literal_defaults<C>(
    request: UnitCheckRequest<'_, C>,
    expressions: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    types: &ExpressionTypeDependencies,
    inference: &mut TypeInferenceContext,
) -> Option<bool>
where
    C: CheckerRequestContext + ?Sized,
{
    let before = inference.revision();

    for &expression in expressions {
        if request.is_cancelled() {
            return None;
        }

        let Some((literal_kind, _, variable)) = numeric_literal(request, expression, variables)
        else {
            continue;
        };

        if inference.evidence(variable).is_some() {
            continue;
        }

        let default = match literal_kind {
            BoundLiteralKind::Integer => types.i32,
            BoundLiteralKind::Real => types.r64,
            BoundLiteralKind::Imaginary => types.c128,
            BoundLiteralKind::Boolean | BoundLiteralKind::Character | BoundLiteralKind::String => {
                continue;
            }
        };

        inference.add_evidence(variable, default, expression);
    }

    Some(inference.revision() != before)
}

fn numeric_literal<C>(
    request: UnitCheckRequest<'_, C>,
    expression: BoundExpressionId,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
) -> Option<(BoundLiteralKind, NumericRepresentationKind, InferenceTypeId)>
where
    C: CheckerRequestContext + ?Sized,
{
    let BoundExpression::Literal(literal) = request.view().expression(expression)? else {
        return None;
    };

    let family = match literal.kind() {
        BoundLiteralKind::Integer => NumericRepresentationKind::Integer,
        BoundLiteralKind::Real => NumericRepresentationKind::Real,
        BoundLiteralKind::Imaginary => NumericRepresentationKind::Complex,
        BoundLiteralKind::Boolean | BoundLiteralKind::Character | BoundLiteralKind::String => {
            return None;
        }
    };

    Some((literal.kind(), family, *variables.get(&expression)?))
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundExpression, BoundLiteralKind, BoundStructuredExpression,
        BoundStructuredExpressionKind, BoundUnit, BoundUnitId,
    };
    use bray_compiler_known::RepresentationRole;
    use bray_diagnostics::{DiagnosticArg, DiagnosticKind, DiagnosticType};
    use bray_symbols::TypeId;

    use super::super::dependencies::representation_type;
    use crate::test_support::{
        TestCheckerContext, callable_entry, completed_expression_check, expression_unit,
        literal_expression, push_expression, tuple_type,
    };
    use crate::{ExpressionTypeExpectation, ExpressionTypeInput, UnitCheckRequest};

    #[test]
    fn fixed_literal_categories_have_their_language_defined_types() {
        let kinds = [
            BoundLiteralKind::Boolean,
            BoundLiteralKind::Character,
            BoundLiteralKind::String,
        ];
        let (unit, expressions) = literal_unit(BoundUnitId::new(70), kinds);
        let expected = [
            representation(&unit, RepresentationRole::ScalarBool),
            representation(&unit, RepresentationRole::ScalarChar),
            representation(&unit, RepresentationRole::String),
        ];

        let result = completed_expression_check(&unit, &ExpressionTypeInput::new());

        assert!(result.diagnostics().is_empty());
        assert_expression_types(result.value(), &expressions, &expected);
    }

    #[test]
    fn unconstrained_numeric_literals_use_language_defaults() {
        let kinds = [
            BoundLiteralKind::Integer,
            BoundLiteralKind::Real,
            BoundLiteralKind::Imaginary,
        ];
        let (unit, expressions) = literal_unit(BoundUnitId::new(71), kinds);
        let expected = [
            representation(&unit, RepresentationRole::ScalarI32),
            representation(&unit, RepresentationRole::ScalarR64),
            representation(&unit, RepresentationRole::ScalarC128),
        ];

        let result = completed_expression_check(&unit, &ExpressionTypeInput::new());

        assert!(result.diagnostics().is_empty());
        assert_expression_types(result.value(), &expressions, &expected);
    }

    #[test]
    fn expected_numeric_types_adapt_matching_literal_families() {
        let kinds = [
            BoundLiteralKind::Integer,
            BoundLiteralKind::Real,
            BoundLiteralKind::Imaginary,
        ];
        let (unit, expressions) = literal_unit(BoundUnitId::new(72), kinds);
        let expected = [
            representation(&unit, RepresentationRole::ScalarU64),
            representation(&unit, RepresentationRole::ScalarR32),
            representation(&unit, RepresentationRole::ScalarC64),
        ];
        let expectations = expressions
            .iter()
            .copied()
            .zip(expected)
            .map(|(expression, ty)| ExpressionTypeExpectation::new(expression, ty));
        let input = ExpressionTypeInput::new().with_expectations(expectations);

        let result = completed_expression_check(&unit, &input);

        assert!(result.diagnostics().is_empty());
        assert_expression_types(result.value(), &expressions, &expected);
    }

    #[test]
    fn aggregate_context_reaches_literals_before_defaults_are_applied() {
        let (unit, expressions) = expression_unit(BoundUnitId::new(73), |tree, origin| {
            let integer = push_expression(
                tree,
                literal_expression(origin, BoundLiteralKind::Integer, None),
            );
            let real = push_expression(
                tree,
                literal_expression(origin, BoundLiteralKind::Real, None),
            );
            let tuple = push_expression(
                tree,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::Tuple,
                    [integer, real],
                    [],
                    [],
                    None,
                    false,
                )),
            );

            vec![integer, real, tuple]
        });
        let integer = representation(&unit, RepresentationRole::ScalarU16);
        let real = representation(&unit, RepresentationRole::ScalarR128);
        let tuple = tuple_type([integer, real]);
        let input = ExpressionTypeInput::new()
            .with_expectations([ExpressionTypeExpectation::new(expressions[2], tuple)]);

        let result = completed_expression_check(&unit, &input);

        assert!(result.diagnostics().is_empty());
        assert_expression_types(result.value(), &expressions, &[integer, real, tuple]);
    }

    #[test]
    fn conflicting_matching_expectations_do_not_select_an_arbitrary_literal_type() {
        let (unit, expressions) = literal_unit(BoundUnitId::new(74), [BoundLiteralKind::Integer]);
        let u16 = representation(&unit, RepresentationRole::ScalarU16);
        let u64 = representation(&unit, RepresentationRole::ScalarU64);
        let default = representation(&unit, RepresentationRole::ScalarI32);
        let input = ExpressionTypeInput::new().with_expectations([
            ExpressionTypeExpectation::new(expressions[0], u16),
            ExpressionTypeExpectation::new(expressions[0], u64),
        ]);

        let result = completed_expression_check(&unit, &input);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingIncompatibleExpressionType)
                .count(),
            2
        );
        assert_expression_types(result.value(), &expressions, &[default]);
    }

    #[test]
    fn incompatible_expected_categories_do_not_change_literal_defaults() {
        let kinds = [
            BoundLiteralKind::Integer,
            BoundLiteralKind::Real,
            BoundLiteralKind::Imaginary,
        ];
        let (unit, expressions) = literal_unit(BoundUnitId::new(75), kinds);
        let expected = [
            representation(&unit, RepresentationRole::ScalarBool),
            representation(&unit, RepresentationRole::ScalarI64),
            representation(&unit, RepresentationRole::ScalarR64),
        ];
        let defaults = [
            representation(&unit, RepresentationRole::ScalarI32),
            representation(&unit, RepresentationRole::ScalarR64),
            representation(&unit, RepresentationRole::ScalarC128),
        ];
        let expectations = expressions
            .iter()
            .copied()
            .zip(expected)
            .map(|(expression, ty)| ExpressionTypeExpectation::new(expression, ty));
        let input = ExpressionTypeInput::new().with_expectations(expectations);

        let result = completed_expression_check(&unit, &input);

        let diagnostics = result
            .diagnostics()
            .by_kind(DiagnosticKind::CheckingIncompatibleExpressionType)
            .collect::<Vec<_>>();

        assert_eq!(diagnostics.len(), 3);
        assert_eq!(
            diagnostics[0].args(),
            &[
                DiagnosticArg::expected_type(DiagnosticType::Boolean),
                DiagnosticArg::actual_type(DiagnosticType::I32),
            ]
        );
        assert_eq!(
            diagnostics[1].args(),
            &[
                DiagnosticArg::expected_type(DiagnosticType::I64),
                DiagnosticArg::actual_type(DiagnosticType::R64),
            ]
        );
        assert_eq!(
            diagnostics[2].args(),
            &[
                DiagnosticArg::expected_type(DiagnosticType::R64),
                DiagnosticArg::actual_type(DiagnosticType::C128),
            ]
        );
        assert_expression_types(result.value(), &expressions, &defaults);
    }

    fn literal_unit(
        unit: BoundUnitId,
        kinds: impl IntoIterator<Item = BoundLiteralKind>,
    ) -> (BoundUnit, Vec<bray_bound_tree::BoundExpressionId>) {
        let kinds = kinds.into_iter().collect::<Vec<_>>();

        expression_unit(unit, |tree, origin| {
            kinds
                .into_iter()
                .map(|kind| push_expression(tree, literal_expression(origin, kind, None)))
                .collect()
        })
    }

    fn representation(unit: &BoundUnit, role: RepresentationRole) -> TypeId {
        let entry = callable_entry(unit.key());
        let context = TestCheckerContext::new(false);
        let Ok(request) = UnitCheckRequest::new(unit, &entry, &context) else {
            panic!("literal test request must be valid");
        };

        match representation_type(request, role) {
            Ok(ty) => ty,
            Err(error) => panic!("literal test representation must be available: {error:?}"),
        }
    }

    fn assert_expression_types(
        types: &bray_bound_tree::CheckedExpressionTypes,
        expressions: &[bray_bound_tree::BoundExpressionId],
        expected: &[TypeId],
    ) {
        let actual = expressions
            .iter()
            .map(|expression| types.expression(*expression).map(|result| result.ty()))
            .collect::<Vec<_>>();
        let expected = expected.iter().copied().map(Some).collect::<Vec<_>>();

        assert_eq!(actual, expected);
    }
}
