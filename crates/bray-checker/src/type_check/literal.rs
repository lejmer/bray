use std::collections::BTreeMap;

use bray_bound_tree::{BoundExpression, BoundExpressionId, BoundLiteralKind, BoundOperator};
use bray_compiler_known::{NumericRepresentationKind, RepresentationRole};
use bray_symbols::TypeId;

use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

use crate::representation::{representation_type, type_representation};

use super::dependencies::{ExpressionTypeDependencies, numeric_kind};
use super::inference::{InferenceTypeId, TypeInferenceContext};

pub(crate) fn numeric_literal_accepts_type<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    ty: TypeId,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(BoundExpression::Literal(literal)) = request.view().expression(expression) else {
        return Ok(false);
    };

    let kind = match literal.kind() {
        BoundLiteralKind::Integer => NumericRepresentationKind::Integer,
        BoundLiteralKind::Real => NumericRepresentationKind::Real,
        BoundLiteralKind::Imaginary => NumericRepresentationKind::Complex,
        BoundLiteralKind::Boolean | BoundLiteralKind::Character | BoundLiteralKind::String => {
            return Ok(false);
        }
    };

    numeric_kind(request, ty).map(|candidate| candidate == Some(kind))
}

pub(super) fn adapt_contextual_literals<C>(
    request: CheckerUnitView<'_, C>,
    expressions: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) -> Result<Option<bool>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let before = inference.revision();

    if adapt_contextual_complex_literals(request, expressions, variables, inference)?.is_none() {
        return Ok(None);
    }

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

fn adapt_contextual_complex_literals<C>(
    request: CheckerUnitView<'_, C>,
    expressions: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    inference: &mut TypeInferenceContext,
) -> Result<Option<()>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for &expression in expressions {
        if request.is_cancelled() {
            return Ok(None);
        }

        let Some((result, real, imaginary)) = complex_literal(request, expression, variables)
        else {
            continue;
        };

        let evidence = inference.evidence(result);

        let expected = match evidence {
            Some(evidence) => Some(evidence),
            None => inference.try_unique_matching_expectation(result, |ty| {
                complex_component_type(request, ty).map(|component| component.is_some())
            })?,
        };

        let Some(expected) = expected else {
            continue;
        };

        let Some(component) = complex_component_type(request, expected)? else {
            continue;
        };

        if evidence.is_none() {
            inference.add_evidence(result, expected, expression);
        }

        inference.add_evidence(real, component, expression);
        inference.add_evidence(imaginary, component, expression);
    }

    Ok(Some(()))
}

pub(super) fn apply_literal_defaults<C>(
    request: CheckerUnitView<'_, C>,
    expressions: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    types: &ExpressionTypeDependencies,
    inference: &mut TypeInferenceContext,
) -> Option<bool>
where
    C: CheckerRequestContext + ?Sized,
{
    let before = inference.revision();

    apply_complex_literal_defaults(request, expressions, variables, types, inference)?;

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

fn apply_complex_literal_defaults<C>(
    request: CheckerUnitView<'_, C>,
    expressions: &[BoundExpressionId],
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
    types: &ExpressionTypeDependencies,
    inference: &mut TypeInferenceContext,
) -> Option<()>
where
    C: CheckerRequestContext + ?Sized,
{
    for &expression in expressions {
        if request.is_cancelled() {
            return None;
        }

        let Some((result, real, imaginary)) = complex_literal(request, expression, variables)
        else {
            continue;
        };

        if inference.evidence(result).is_some() {
            continue;
        }

        inference.add_evidence(result, types.c128, expression);
        inference.add_evidence(real, types.r64, expression);
        inference.add_evidence(imaginary, types.r64, expression);
    }

    Some(())
}

fn complex_literal<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    variables: &BTreeMap<BoundExpressionId, InferenceTypeId>,
) -> Option<(InferenceTypeId, InferenceTypeId, InferenceTypeId)>
where
    C: CheckerRequestContext + ?Sized,
{
    let BoundExpression::Binary(binary) = request.view().expression(expression)? else {
        return None;
    };

    if !matches!(
        binary.operator(),
        BoundOperator::Add | BoundOperator::Subtract
    ) {
        return None;
    }

    let [real, imaginary] = binary.operands() else {
        return None;
    };

    let BoundExpression::Literal(real_literal) = request.view().expression(*real)? else {
        return None;
    };

    let BoundExpression::Literal(imaginary_literal) = request.view().expression(*imaginary)? else {
        return None;
    };

    if real_literal.kind() != BoundLiteralKind::Real
        || imaginary_literal.kind() != BoundLiteralKind::Imaginary
    {
        return None;
    }

    Some((
        *variables.get(&expression)?,
        *variables.get(real)?,
        *variables.get(imaginary)?,
    ))
}

fn complex_component_type<C>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
) -> Result<Option<TypeId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(component) =
        type_representation(request, ty)?.and_then(RepresentationRole::complex_component)
    else {
        return Ok(None);
    };

    representation_type(request, component).map(Some)
}

fn numeric_literal<C>(
    request: CheckerUnitView<'_, C>,
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
        BoundBinaryExpression, BoundExpression, BoundLiteralKind, BoundOperator,
        BoundStructuredExpression, BoundStructuredExpressionKind, BoundUnit, BoundUnitId,
    };
    use bray_compiler_known::RepresentationRole;
    use bray_diagnostics::{DiagnosticArg, DiagnosticKind, DiagnosticType};
    use bray_symbols::TypeId;

    use super::super::session::{ExpressionTypeSession, SessionProgress};
    use crate::representation::representation_type;
    use crate::test_support::{
        TestCheckerContext, array_type, callable_entry, completed_expression_check,
        expression_unit, literal_expression, nullable_type, push_expression, tuple_type,
    };
    use crate::{
        CheckerUnitView, ExpressionTypeEvidence, ExpressionTypeExpectation, ExpressionTypeInput,
    };

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
    fn expected_nullable_numeric_types_adapt_present_literals() {
        let kinds = [
            BoundLiteralKind::Integer,
            BoundLiteralKind::Real,
            BoundLiteralKind::Imaginary,
        ];

        let (unit, expressions) = literal_unit(BoundUnitId::new(81), kinds);

        let expected = [
            representation(&unit, RepresentationRole::ScalarU64),
            representation(&unit, RepresentationRole::ScalarR32),
            representation(&unit, RepresentationRole::ScalarC64),
        ];

        let expectations = expressions
            .iter()
            .copied()
            .zip(expected)
            .map(|(expression, ty)| ExpressionTypeExpectation::new(expression, nullable_type(ty)));

        let input = ExpressionTypeInput::new().with_expectations(expectations);
        let result = completed_expression_check(&unit, &input);

        assert!(result.diagnostics().is_empty());
        assert_expression_types(result.value(), &expressions, &expected);
    }

    #[test]
    fn expected_complex_types_adapt_literal_components() {
        let cases = [
            (
                BoundUnitId::new(76),
                RepresentationRole::ScalarC64,
                RepresentationRole::ScalarR32,
            ),
            (
                BoundUnitId::new(77),
                RepresentationRole::ScalarC128,
                RepresentationRole::ScalarR64,
            ),
        ];

        for (unit, complex_role, component_role) in cases {
            let (unit, expressions) = complex_literal_unit(unit);

            let complex = representation(&unit, complex_role);
            let component = representation(&unit, component_role);

            let input = ExpressionTypeInput::new()
                .with_expectations([ExpressionTypeExpectation::new(expressions[2], complex)]);

            let result = completed_expression_check(&unit, &input);

            assert!(result.diagnostics().is_empty());

            assert_expression_types(
                result.value(),
                &expressions,
                &[component, component, complex],
            );
        }

        let (unit, expressions) = complex_literal_unit(BoundUnitId::new(80));

        let complex = representation(&unit, RepresentationRole::ScalarC64);
        let component = representation(&unit, RepresentationRole::ScalarR32);

        let input = ExpressionTypeInput::new()
            .with_evidence([ExpressionTypeEvidence::new(expressions[2], complex)]);

        let result = completed_expression_check(&unit, &input);

        assert!(result.diagnostics().is_empty());

        assert_expression_types(
            result.value(),
            &expressions,
            &[component, component, complex],
        );
    }

    #[test]
    fn unconstrained_complex_literals_use_the_complex_default() {
        let (unit, expressions) = complex_literal_unit(BoundUnitId::new(78));

        let component = representation(&unit, RepresentationRole::ScalarR64);
        let complex = representation(&unit, RepresentationRole::ScalarC128);

        let result = completed_expression_check(&unit, &ExpressionTypeInput::new());

        assert!(result.diagnostics().is_empty());

        assert_expression_types(
            result.value(),
            &expressions,
            &[component, component, complex],
        );
    }

    #[test]
    fn staged_propagation_does_not_commit_numeric_defaults() {
        let (unit, expressions) = literal_unit(BoundUnitId::new(79), [BoundLiteralKind::Integer]);

        let entry = callable_entry(unit.key());
        let context = TestCheckerContext::new(false);

        let Ok(request) = CheckerUnitView::new(&unit, &entry, &context) else {
            panic!("literal test request must be valid");
        };

        let Ok(SessionProgress::Complete(mut session)) = ExpressionTypeSession::begin(request)
        else {
            panic!("literal type session must start");
        };

        let Ok(SessionProgress::Complete(())) = session.propagate() else {
            panic!("initial propagation must complete");
        };

        assert_eq!(session.expression_type(expressions[0]), None);

        let u64 = representation(&unit, RepresentationRole::ScalarU64);

        if let Err(error) = session.add_expectation(expressions[0], u64) {
            panic!("literal expectation must be accepted: {error:?}");
        }

        let Ok(SessionProgress::Complete(())) = session.propagate() else {
            panic!("contextual propagation must complete");
        };

        assert_eq!(
            session
                .expression_type(expressions[0])
                .map(|result| result.ty()),
            Some(u64)
        );
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
    fn array_context_reaches_literals_before_defaults_are_applied() {
        let (unit, expressions) = expression_unit(BoundUnitId::new(81), |tree, origin| {
            let first = push_expression(
                tree,
                literal_expression(origin, BoundLiteralKind::Integer, None),
            );

            let second = push_expression(
                tree,
                literal_expression(origin, BoundLiteralKind::Integer, None),
            );

            let array = push_expression(
                tree,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::Array,
                    [first, second],
                    [],
                    [],
                    None,
                    false,
                )),
            );

            vec![first, second, array]
        });

        let element = representation(&unit, RepresentationRole::ScalarU8);
        let array = array_type(element, 2);

        let input = ExpressionTypeInput::new()
            .with_evidence([ExpressionTypeEvidence::new(expressions[2], array)]);

        let result = completed_expression_check(&unit, &input);

        assert!(result.diagnostics().is_empty());
        assert_expression_types(result.value(), &expressions, &[element, element, array]);
    }

    #[test]
    fn repeated_array_context_reaches_the_value_and_count_literals() {
        let (unit, expressions) = expression_unit(BoundUnitId::new(82), |tree, origin| {
            let value = push_expression(
                tree,
                literal_expression(origin, BoundLiteralKind::Integer, None),
            );

            let count = push_expression(
                tree,
                literal_expression(origin, BoundLiteralKind::Integer, None),
            );

            let array = push_expression(
                tree,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::RepeatedArray,
                    [value, count],
                    [],
                    [],
                    None,
                    false,
                )),
            );

            vec![value, count, array]
        });

        let element = representation(&unit, RepresentationRole::ScalarU8);
        let count = representation(&unit, RepresentationRole::ScalarUsize);
        let array = array_type(element, 2);

        let input = ExpressionTypeInput::new()
            .with_evidence([ExpressionTypeEvidence::new(expressions[2], array)]);

        let result = completed_expression_check(&unit, &input);

        assert!(result.diagnostics().is_empty());
        assert_expression_types(result.value(), &expressions, &[element, count, array]);
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

    fn complex_literal_unit(
        unit: BoundUnitId,
    ) -> (BoundUnit, Vec<bray_bound_tree::BoundExpressionId>) {
        expression_unit(unit, |tree, origin| {
            let real = push_expression(
                tree,
                literal_expression(origin, BoundLiteralKind::Real, None),
            );

            let imaginary = push_expression(
                tree,
                literal_expression(origin, BoundLiteralKind::Imaginary, None),
            );

            let complex = push_expression(
                tree,
                BoundExpression::Binary(BoundBinaryExpression::new(
                    origin,
                    BoundOperator::Add,
                    [real, imaginary],
                    None,
                    false,
                )),
            );

            vec![real, imaginary, complex]
        })
    }

    fn representation(unit: &BoundUnit, role: RepresentationRole) -> TypeId {
        let entry = callable_entry(unit.key());
        let context = TestCheckerContext::new(false);

        let Ok(request) = CheckerUnitView::new(unit, &entry, &context) else {
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
