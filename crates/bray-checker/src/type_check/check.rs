use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, CheckedExpressionTypes, ExpressionTypeEntry,
    SelectedIterationSource,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticType, SeverityKind,
};
use bray_symbols::TypeId;

use crate::diagnostic::{diagnostic_id, expression_span};
use crate::{CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerUnitView};

use super::ExpressionTypeInput;
use super::cardinality::unproven_array_generators;
use super::session::{ExpressionTypeSession, SessionProgress};

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
struct DiagnosticConflict {
    expression: BoundExpressionId,
    expected: DiagnosticType,
    actual: DiagnosticType,
}

pub(crate) fn check_expression_types<C>(
    request: CheckerUnitView<'_, C>,
    input: &ExpressionTypeInput,
) -> CheckerOutcome<CheckedExpressionTypes>
where
    C: CheckerRequestContext + ?Sized,
{
    let session = match prepare_expression_types(request, input) {
        Ok(SessionProgress::Complete(session)) => session,
        Ok(SessionProgress::Cancelled) => return CheckerOutcome::Cancelled,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    finish_expression_types(request, session)
}

fn prepare_expression_types<'view, C>(
    request: CheckerUnitView<'view, C>,
    input: &ExpressionTypeInput,
) -> Result<SessionProgress<ExpressionTypeSession<'view, C>>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(mut session) = ExpressionTypeSession::begin(request)?.into_value() else {
        return Ok(SessionProgress::Cancelled);
    };

    session.apply_input(input)?;

    if session.propagate()?.is_cancelled()
        || session.apply_literal_defaults().is_cancelled()
        || session.propagate()?.is_cancelled()
    {
        return Ok(SessionProgress::Cancelled);
    }

    Ok(SessionProgress::Complete(session))
}

pub(crate) fn finish_expression_types<C>(
    request: CheckerUnitView<'_, C>,
    session: ExpressionTypeSession<'_, C>,
) -> CheckerOutcome<CheckedExpressionTypes>
where
    C: CheckerRequestContext + ?Sized,
{
    finish_expression_types_with_deferred(request, session, &BTreeSet::new(), &[])
}

pub(crate) fn finish_expression_types_with_deferred<C>(
    request: CheckerUnitView<'_, C>,
    session: ExpressionTypeSession<'_, C>,
    deferred: &BTreeSet<BoundExpressionId>,
    iteration_sources: &[SelectedIterationSource],
) -> CheckerOutcome<CheckedExpressionTypes>
where
    C: CheckerRequestContext + ?Sized,
{
    let finished = session.finish();
    let mut conflicts = Vec::with_capacity(finished.conflicts.len());

    for conflict in finished.conflicts {
        if deferred.contains(&conflict.expression) {
            continue;
        }

        let mut expected = match diagnostic_type(request, conflict.expected) {
            Ok(expected) => expected,
            Err(error) => return CheckerOutcome::InfrastructureFailure(error),
        };

        let mut actual = match diagnostic_type(request, conflict.actual) {
            Ok(actual) => actual,
            Err(error) => return CheckerOutcome::InfrastructureFailure(error),
        };

        if !conflict.is_directional && actual < expected {
            std::mem::swap(&mut expected, &mut actual);
        }

        conflicts.push(DiagnosticConflict {
            expression: conflict.expression,
            expected,
            actual,
        });
    }

    conflicts.sort_unstable();
    conflicts.dedup();

    let mut diagnostics = Vec::new();

    for conflict in conflicts {
        let span = match expression_span(request, conflict.expression) {
            Ok(span) => span,
            Err(error) => return CheckerOutcome::InfrastructureFailure(error),
        };

        diagnostics.push(
            Diagnostic::new(
                diagnostic_id(diagnostics.len()),
                DiagnosticKind::CheckingIncompatibleExpressionType,
                SeverityKind::Error,
            )
            .with_primary_span(span)
            .with_arg(DiagnosticArg::expected_type(conflict.expected))
            .with_arg(DiagnosticArg::actual_type(conflict.actual)),
        );
    }

    for expression in finished.unresolved {
        let Some(bound_expression) = request.view().expression(expression) else {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidExpressionTypeInput { expression },
            );
        };

        if deferred.contains(&expression) || is_compile_time_path_expression(bound_expression) {
            continue;
        }

        let span = match expression_span(request, expression) {
            Ok(span) => span,
            Err(error) => return CheckerOutcome::InfrastructureFailure(error),
        };

        diagnostics.push(
            Diagnostic::new(
                diagnostic_id(diagnostics.len()),
                DiagnosticKind::CheckingCannotInferExpressionType,
                SeverityKind::Error,
            )
            .with_primary_span(span),
        );
    }

    let entries = finished
        .results
        .iter()
        .map(|(expression, result)| ExpressionTypeEntry::new(*expression, *result));

    let checked_types =
        CheckedExpressionTypes::new(request.view().unit(), request.view().kind(), entries);

    let checked_types = match finished.callable_result_type {
        Some(ty) => checked_types.with_callable_result_type(ty),
        None => checked_types,
    };

    let unproven_generators =
        match unproven_array_generators(request, &checked_types, iteration_sources) {
            Ok(expressions) => expressions,
            Err(error) => return CheckerOutcome::InfrastructureFailure(error),
        };

    for expression in unproven_generators {
        let span = match expression_span(request, expression) {
            Ok(span) => span,
            Err(error) => return CheckerOutcome::InfrastructureFailure(error),
        };

        diagnostics.push(
            Diagnostic::new(
                diagnostic_id(diagnostics.len()),
                DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable,
                SeverityKind::Error,
            )
            .with_primary_span(span),
        );
    }

    CheckerOutcome::complete(checked_types, DiagnosticBag::from(diagnostics))
}

fn is_compile_time_path_expression(expression: &BoundExpression) -> bool {
    let BoundExpression::Name(name) = expression else {
        return false;
    };

    name.target().is_compile_time_qualifier()
}

pub(crate) fn diagnostic_type<C>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
) -> Result<DiagnosticType, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    crate::diagnostic::diagnostic_type(
        request.semantic_values(),
        request.available_compiler_known_symbols(),
        ty,
    )
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundAssignmentExpression, BoundBlockItem, BoundControlTransferExpression,
        BoundControlTransferKind, BoundExpression, BoundForExpression, BoundGeneratorExpression,
        BoundIterationSource, BoundMatchArm, BoundMatchExpression, BoundNodeOrigin, BoundOperator,
        BoundPattern, BoundPatternKind, BoundPatternMode, BoundStructuredExpression,
        BoundStructuredExpressionKind, BoundTreeBuilder, BoundTypeReference, BoundUnitId,
        ExpressionTypeResult, ExpressionTypeStatus, IterationSourceMode,
    };
    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{GenericArgument, TypeData};

    use super::super::session::{ExpressionTypeSession, SessionProgress};
    use crate::test_support::{
        callable_entry, callable_key, callable_unit, completed_expression_check as completed_check,
        distinct_source_origins, error_type, expression_unit,
        integer_literal_expression as literal, push_block, push_callable, push_expression,
        semantic_values, test_source_origins, tuple_type, type_data,
        unselected_name_expression as unselected_name,
    };
    use crate::{
        CheckerInfrastructureError, CheckerOutcome, CheckerUnitView, DefaultExpressionTypeChecker,
        ExpressionTypeChecker, ExpressionTypeEvidence, ExpressionTypeExpectation,
        ExpressionTypeInput,
    };

    #[test]
    fn checking_publishes_one_canonical_result_for_every_expression() {
        let first_type = tuple_type([]);
        let second_type = tuple_type([first_type]);

        let (unit, expressions) = expression_unit(BoundUnitId::new(40), |tree, origin| {
            let first = push_expression(tree, literal(origin, None));
            let second = push_expression(tree, literal(origin, None));

            let tuple = push_expression(
                tree,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::Tuple,
                    [first, second],
                    [],
                    [],
                    None,
                    false,
                )),
            );

            vec![first, second, tuple]
        });

        let input = ExpressionTypeInput::new().with_evidence([
            ExpressionTypeEvidence::new(expressions[0], first_type),
            ExpressionTypeEvidence::new(expressions[1], second_type),
        ]);

        let result = completed_check(&unit, &input);

        assert!(result.diagnostics().is_empty());
        assert_eq!(result.value().entries().len(), 3);

        let Some(tuple) = result.value().expression(expressions[2]) else {
            panic!("tuple expression must have a result");
        };

        assert_eq!(tuple.status(), ExpressionTypeStatus::Valid);

        assert_eq!(
            type_data(tuple.ty()),
            TypeData::tuple([first_type, second_type])
        );
    }

    #[test]
    fn staged_selection_can_add_nested_results_before_finalization() {
        let operand_type = tuple_type([]);
        let child_type = tuple_type([operand_type]);
        let parent_type = tuple_type([child_type]);

        let (unit, expressions) = expression_unit(BoundUnitId::new(50), |tree, origin| {
            let leaf = push_expression(tree, literal(origin, None));

            let child = push_expression(
                tree,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::ElementIndex,
                    [leaf],
                    [],
                    [],
                    None,
                    false,
                )),
            );

            let parent = push_expression(
                tree,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::ElementIndex,
                    [child],
                    [],
                    [],
                    None,
                    false,
                )),
            );

            vec![leaf, child, parent]
        });

        let entry = callable_entry(unit.key());
        let context = crate::test_support::TestCheckerContext::new(false);

        let Ok(request) = CheckerUnitView::new(&unit, &entry, &context) else {
            panic!("test checker unit view must be valid");
        };

        let Ok(SessionProgress::Complete(mut session)) = ExpressionTypeSession::begin(request)
        else {
            panic!("expression type session must start");
        };

        session
            .add_evidence(expressions[0], operand_type)
            .unwrap_or_else(|error| panic!("leaf evidence must be valid: {error:?}"));

        assert!(matches!(
            session.propagate(),
            Ok(SessionProgress::Complete(()))
        ));

        assert_eq!(
            session
                .expression_type(expressions[0])
                .map(|result| result.ty()),
            Some(operand_type)
        );

        assert_eq!(session.expression_type(expressions[1]), None);

        session
            .add_evidence(expressions[1], child_type)
            .unwrap_or_else(|error| panic!("child selection must be valid: {error:?}"));

        assert!(matches!(
            session.propagate(),
            Ok(SessionProgress::Complete(()))
        ));

        assert_eq!(session.expression_type(expressions[2]), None);

        session
            .add_evidence(expressions[2], parent_type)
            .unwrap_or_else(|error| panic!("parent selection must be valid: {error:?}"));

        assert!(matches!(
            session.propagate(),
            Ok(SessionProgress::Complete(()))
        ));

        let finished = session.finish();

        assert!(finished.conflicts.is_empty());
        assert!(finished.unresolved.is_empty());
    }

    #[test]
    fn local_annotations_provide_expected_types_without_selecting_actual_types() {
        let actual = tuple_type([]);
        let expected = tuple_type([actual]);
        let key = callable_key();
        let origin = BoundNodeOrigin::source(key.source());
        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(41));
        let initializer = push_expression(&mut tree, literal(origin, Some(actual)));

        let declaration = bray_bound_tree::BoundLocalConstant::new(
            origin,
            None,
            BoundTypeReference::new(key.source().syntax(), Some(expected)),
            initializer,
            false,
        );

        let block = push_block(
            &mut tree,
            origin,
            [BoundBlockItem::LocalConstant(declaration)],
        );

        let root = push_callable(&mut tree, origin, block);
        let unit = callable_unit(&key, tree.finish(), root);

        let result = completed_check(&unit, &ExpressionTypeInput::new());

        assert_eq!(result.diagnostics().len(), 1);

        assert_eq!(
            result.diagnostics().diagnostics()[0].kind(),
            DiagnosticKind::CheckingIncompatibleExpressionType
        );

        assert_eq!(
            result.value().expression(initializer),
            Some(ExpressionTypeResult::new(
                actual,
                ExpressionTypeStatus::Recovered,
            ))
        );
    }

    #[test]
    fn nested_arrays_infer_compositional_element_types() {
        let element_type = tuple_type([]);

        let (unit, expressions) = expression_unit(BoundUnitId::new(51), |tree, origin| {
            let first = push_expression(tree, literal(origin, Some(element_type)));
            let second = push_expression(tree, literal(origin, Some(element_type)));

            let first_array = push_expression(
                tree,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::Array,
                    [first],
                    [],
                    [],
                    None,
                    false,
                )),
            );

            let second_array = push_expression(
                tree,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::Array,
                    [second],
                    [],
                    [],
                    None,
                    false,
                )),
            );

            let outer = push_expression(
                tree,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::Array,
                    [first_array, second_array],
                    [],
                    [],
                    None,
                    false,
                )),
            );

            vec![first, second, first_array, second_array, outer]
        });

        let result = completed_check(&unit, &ExpressionTypeInput::new());

        assert!(result.diagnostics().is_empty());

        let Some(first_array) = result.value().expression(expressions[2]) else {
            panic!("inner array must have a type");
        };

        let Some(outer) = result.value().expression(expressions[4]) else {
            panic!("outer array must have a type");
        };

        assert!(matches!(
            type_data(first_array.ty()),
            TypeData::Array { element, .. } if element == element_type
        ));

        assert!(matches!(
            type_data(outer.ty()),
            TypeData::Array { element, .. } if element == first_array.ty()
        ));
    }

    #[test]
    fn conditional_types_ignore_never_branches_when_merging_values() {
        let result_type = tuple_type([]);
        let iterable_type = tuple_type([result_type]);

        let [yield_origin, never_origin] = distinct_source_origins();

        let key = callable_key();
        let origin = BoundNodeOrigin::source(key.source());
        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(55));
        let source = push_expression(&mut tree, literal(origin, Some(iterable_type)));

        let condition = push_expression(
            &mut tree,
            BoundExpression::Structured(BoundStructuredExpression::new(
                origin,
                BoundStructuredExpressionKind::BooleanAllFold,
                [source],
                [],
                [],
                None,
                false,
            )),
        );

        let value = push_expression(&mut tree, literal(yield_origin, Some(result_type)));

        let yielded = push_expression(
            &mut tree,
            BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
                yield_origin,
                BoundControlTransferKind::Yield,
                Some(value),
                Some(yield_origin.source_anchor().syntax()),
                None,
                false,
            )),
        );

        let yield_block = push_block(
            &mut tree,
            yield_origin,
            [BoundBlockItem::Expression(yielded)],
        );

        let diverging = push_expression(
            &mut tree,
            BoundExpression::Structured(BoundStructuredExpression::new(
                never_origin,
                BoundStructuredExpressionKind::Panic,
                [],
                [],
                [],
                None,
                false,
            )),
        );

        let never_block = push_block(
            &mut tree,
            never_origin,
            [BoundBlockItem::Expression(diverging)],
        );

        let conditional = push_expression(
            &mut tree,
            BoundExpression::Structured(BoundStructuredExpression::new(
                origin,
                BoundStructuredExpressionKind::Conditional,
                [condition],
                [yield_block, never_block],
                [],
                None,
                false,
            )),
        );

        let root_block = push_block(&mut tree, origin, [BoundBlockItem::Expression(conditional)]);
        let root = push_callable(&mut tree, origin, root_block);
        let unit = callable_unit(&key, tree.finish(), root);

        let result = completed_check(&unit, &ExpressionTypeInput::new());

        assert!(result.diagnostics().is_empty());

        assert_eq!(
            result
                .value()
                .expression(conditional)
                .map(|result| result.ty()),
            Some(result_type)
        );
    }

    #[test]
    fn conditional_conditions_must_be_boolean() {
        let condition_type = tuple_type([]);
        let key = callable_key();
        let origin = BoundNodeOrigin::source(key.source());
        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(58));
        let condition = push_expression(&mut tree, literal(origin, Some(condition_type)));
        let branch = push_block(&mut tree, origin, []);

        let conditional = push_expression(
            &mut tree,
            BoundExpression::Structured(BoundStructuredExpression::new(
                origin,
                BoundStructuredExpressionKind::Conditional,
                [condition],
                [branch],
                [],
                None,
                false,
            )),
        );

        let root_block = push_block(&mut tree, origin, [BoundBlockItem::Expression(conditional)]);
        let root = push_callable(&mut tree, origin, root_block);
        let unit = callable_unit(&key, tree.finish(), root);

        let result = completed_check(&unit, &ExpressionTypeInput::new());

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingIncompatibleExpressionType)
                .count(),
            1
        );

        assert!(
            result
                .value()
                .expression(condition)
                .is_some_and(|condition| condition.is_recovered())
        );
    }

    #[test]
    fn expected_tuple_types_propagate_to_element_expressions() {
        let first_type = tuple_type([]);
        let second_type = tuple_type([first_type]);

        let (unit, expressions) = expression_unit(BoundUnitId::new(42), |tree, origin| {
            let first = push_expression(tree, literal(origin, None));
            let second = push_expression(tree, literal(origin, None));

            let tuple = push_expression(
                tree,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::Tuple,
                    [first, second],
                    [],
                    [],
                    None,
                    false,
                )),
            );

            vec![first, second, tuple]
        });

        let expected_tuple = tuple_type([first_type, first_type]);

        let input = ExpressionTypeInput::new()
            .with_evidence([
                ExpressionTypeEvidence::new(expressions[0], first_type),
                ExpressionTypeEvidence::new(expressions[1], second_type),
            ])
            .with_expectations([ExpressionTypeExpectation::new(
                expressions[2],
                expected_tuple,
            )]);

        let result = completed_check(&unit, &input);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingIncompatibleExpressionType)
                .count(),
            1
        );

        assert_eq!(
            result
                .value()
                .expression(expressions[1])
                .map(ExpressionTypeResult::status),
            Some(ExpressionTypeStatus::Recovered)
        );
    }

    #[test]
    fn recovered_elements_do_not_cascade_into_aggregate_mismatches() {
        let expected_element = tuple_type([]);
        let actual_element = tuple_type([expected_element]);

        let (unit, expressions) = expression_unit(BoundUnitId::new(54), |tree, origin| {
            let element = push_expression(tree, literal(origin, Some(actual_element)));

            let tuple = push_expression(
                tree,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::Tuple,
                    [element],
                    [],
                    [],
                    None,
                    false,
                )),
            );

            vec![element, tuple]
        });

        let input = ExpressionTypeInput::new().with_expectations([ExpressionTypeExpectation::new(
            expressions[1],
            tuple_type([expected_element]),
        )]);

        let result = completed_check(&unit, &input);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingIncompatibleExpressionType)
                .count(),
            1
        );

        assert!(
            result
                .value()
                .expression(expressions[1])
                .is_some_and(|tuple| tuple.ty() == error_type() && tuple.is_recovered())
        );
    }

    #[test]
    fn evidence_order_cannot_change_type_results_or_diagnostics() {
        let first_type = tuple_type([]);
        let second_type = tuple_type([first_type]);

        let (unit, expressions) = expression_unit(BoundUnitId::new(48), |tree, origin| {
            let leaf = push_expression(tree, literal(origin, None));

            let aggregate = push_expression(
                tree,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::Tuple,
                    [leaf],
                    [],
                    [],
                    None,
                    false,
                )),
            );

            vec![leaf, aggregate]
        });

        let first = ExpressionTypeEvidence::new(expressions[0], first_type);
        let second = ExpressionTypeEvidence::new(expressions[0], second_type);
        let forward = ExpressionTypeInput::new().with_evidence([first, second]);
        let reverse = ExpressionTypeInput::new().with_evidence([second, first]);

        let forward = completed_check(&unit, &forward);
        let reverse = completed_check(&unit, &reverse);

        assert_eq!(forward, reverse);

        assert!(expressions.iter().all(|expression| {
            forward
                .value()
                .expression(*expression)
                .is_some_and(|result| result.ty() == error_type() && result.is_recovered())
        }));

        assert_eq!(
            forward
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingIncompatibleExpressionType)
                .count(),
            1
        );
    }

    #[test]
    fn unresolved_and_recovered_expressions_use_the_canonical_error_type() {
        let (unit, expressions) = expression_unit(BoundUnitId::new(43), |tree, origin| {
            let unresolved = push_expression(tree, unselected_name(origin));
            let supplied_error = push_expression(tree, literal(origin, None));

            let recovered = push_expression(
                tree,
                BoundExpression::Error(bray_bound_tree::BoundErrorExpression::new(
                    origin,
                    error_type(),
                )),
            );

            vec![unresolved, supplied_error, recovered]
        });

        let input = ExpressionTypeInput::new()
            .with_evidence([ExpressionTypeEvidence::new(expressions[1], error_type())]);

        let result = completed_check(&unit, &input);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingCannotInferExpressionType)
                .count(),
            1
        );

        for expression in expressions {
            let Some(result) = result.value().expression(expression) else {
                panic!("every reached expression must have a type result");
            };

            assert_eq!(result.ty(), error_type());
            assert!(result.is_recovered());
        }
    }

    #[test]
    fn intrinsic_unit_and_control_transfer_types_do_not_need_external_evidence() {
        let (unit, expressions) = expression_unit(BoundUnitId::new(44), |tree, origin| {
            let unit = push_expression(
                tree,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::Unit,
                    [],
                    [],
                    [],
                    None,
                    false,
                )),
            );

            let transfer = push_expression(
                tree,
                BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
                    origin,
                    BoundControlTransferKind::Continue,
                    None,
                    None,
                    None,
                    false,
                )),
            );

            vec![unit, transfer]
        });

        let input = ExpressionTypeInput::new().with_expectations([ExpressionTypeExpectation::new(
            expressions[1],
            tuple_type([]),
        )]);

        let result = completed_check(&unit, &input);

        assert!(result.diagnostics().is_empty());

        assert_ne!(
            result
                .value()
                .expression(expressions[0])
                .map(|result| result.ty()),
            result
                .value()
                .expression(expressions[1])
                .map(|result| result.ty())
        );
    }

    #[test]
    fn boolean_folds_type_the_result_without_treating_the_source_as_bool() {
        let iterable_type = tuple_type([]);

        let (unit, expressions) = expression_unit(BoundUnitId::new(49), |tree, origin| {
            let source = push_expression(tree, literal(origin, Some(iterable_type)));

            let fold = push_expression(
                tree,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::BooleanAllFold,
                    [source],
                    [],
                    [],
                    None,
                    false,
                )),
            );

            vec![source, fold]
        });

        let result = completed_check(&unit, &ExpressionTypeInput::new());

        assert!(result.diagnostics().is_empty());

        assert_eq!(
            result
                .value()
                .expression(expressions[0])
                .map(|result| result.ty()),
            Some(iterable_type)
        );

        assert_ne!(
            result
                .value()
                .expression(expressions[1])
                .map(|result| result.ty()),
            Some(iterable_type)
        );
    }

    #[test]
    fn assignment_targets_directionally_constrain_values() {
        let target_type = tuple_type([]);
        let actual_type = tuple_type([target_type]);

        let (unit, expressions) = expression_unit(BoundUnitId::new(52), |tree, origin| {
            let target = push_expression(tree, literal(origin, Some(target_type)));
            let value = push_expression(tree, literal(origin, Some(actual_type)));

            let assignment = push_expression(
                tree,
                BoundExpression::Assignment(BoundAssignmentExpression::new(
                    origin,
                    BoundOperator::Assign,
                    [target, value],
                    None,
                    false,
                )),
            );

            vec![target, value, assignment]
        });

        let result = completed_check(&unit, &ExpressionTypeInput::new());

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingIncompatibleExpressionType)
                .count(),
            1
        );

        assert!(
            result
                .value()
                .expression(expressions[1])
                .is_some_and(|value| value.is_recovered())
        );

        assert!(
            result
                .value()
                .expression(expressions[2])
                .is_some_and(|assignment| !assignment.is_recovered())
        );
    }

    #[test]
    fn callable_result_types_directionally_constrain_return_operands() {
        let result_type = tuple_type([]);
        let actual_type = tuple_type([result_type]);

        let (unit, expressions) = expression_unit(BoundUnitId::new(56), |tree, origin| {
            let value = push_expression(tree, literal(origin, Some(actual_type)));

            let returned = push_expression(
                tree,
                BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
                    origin,
                    BoundControlTransferKind::Return,
                    Some(value),
                    None,
                    None,
                    false,
                )),
            );

            vec![value, returned]
        });

        let input = ExpressionTypeInput::new().with_callable_result_type(result_type);

        let result = completed_check(&unit, &input);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingIncompatibleExpressionType)
                .count(),
            1
        );

        assert!(
            result
                .value()
                .expression(expressions[0])
                .is_some_and(|value| value.is_recovered())
        );

        assert!(
            result
                .value()
                .expression(expressions[1])
                .is_some_and(|returned| !returned.is_recovered())
        );
    }

    #[test]
    fn bare_return_uses_unit_for_callable_result_compatibility() {
        let result_type = tuple_type([tuple_type([])]);

        let (unit, expressions) = expression_unit(BoundUnitId::new(57), |tree, origin| {
            vec![push_expression(
                tree,
                BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
                    origin,
                    BoundControlTransferKind::Return,
                    None,
                    None,
                    None,
                    false,
                )),
            )]
        });

        let input = ExpressionTypeInput::new().with_callable_result_type(result_type);

        let result = completed_check(&unit, &input);

        assert_eq!(
            result
                .diagnostics()
                .by_kind(DiagnosticKind::CheckingIncompatibleExpressionType)
                .count(),
            1
        );

        assert!(
            result
                .value()
                .expression(expressions[0])
                .is_some_and(|returned| returned.is_recovered())
        );
    }

    #[test]
    fn bare_break_contributes_unit_to_its_loop_result() {
        let key = callable_key();
        let origin = BoundNodeOrigin::source(key.source());
        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(59));

        let unit_expression = push_expression(
            &mut tree,
            BoundExpression::Structured(BoundStructuredExpression::new(
                origin,
                BoundStructuredExpressionKind::Unit,
                [],
                [],
                [],
                None,
                false,
            )),
        );

        let transfer = push_expression(
            &mut tree,
            BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
                origin,
                BoundControlTransferKind::Break,
                None,
                Some(origin.source_anchor().syntax()),
                None,
                false,
            )),
        );

        let body = push_block(&mut tree, origin, [BoundBlockItem::Expression(transfer)]);

        let loop_expression = push_expression(
            &mut tree,
            BoundExpression::Structured(BoundStructuredExpression::new(
                origin,
                BoundStructuredExpressionKind::Loop,
                [],
                [body],
                [],
                None,
                false,
            )),
        );

        let root_block = push_block(
            &mut tree,
            origin,
            [
                BoundBlockItem::Expression(unit_expression),
                BoundBlockItem::Expression(loop_expression),
            ],
        );

        let root = push_callable(&mut tree, origin, root_block);
        let unit = callable_unit(&key, tree.finish(), root);

        let result = completed_check(&unit, &ExpressionTypeInput::new());

        assert!(result.diagnostics().is_empty());

        let Some(unit_type) = result
            .value()
            .expression(unit_expression)
            .map(|result| result.ty())
        else {
            panic!("unit expression must have a type");
        };

        assert_eq!(
            result
                .value()
                .expression(loop_expression)
                .map(|result| result.ty()),
            Some(unit_type)
        );
    }

    #[test]
    fn match_arms_join_yielded_result_types() {
        let result_type = tuple_type([]);

        let [first_origin, second_origin, root_origin] = test_source_origins();

        let key = callable_key();
        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(60));
        let subject = push_expression(&mut tree, literal(first_origin, Some(result_type)));
        let first_pattern = push_wildcard_pattern(&mut tree, first_origin);
        let second_pattern = push_wildcard_pattern(&mut tree, second_origin);

        let first_value = push_expression(&mut tree, literal(first_origin, Some(result_type)));

        let first_yield = push_expression(
            &mut tree,
            BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
                first_origin,
                BoundControlTransferKind::Yield,
                Some(first_value),
                Some(first_origin.source_anchor().syntax()),
                None,
                false,
            )),
        );

        let first_body = push_block(
            &mut tree,
            first_origin,
            [BoundBlockItem::Expression(first_yield)],
        );

        let second_value = push_expression(&mut tree, literal(second_origin, Some(result_type)));

        let second_yield = push_expression(
            &mut tree,
            BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
                second_origin,
                BoundControlTransferKind::Yield,
                Some(second_value),
                Some(second_origin.source_anchor().syntax()),
                None,
                false,
            )),
        );

        let second_body = push_block(
            &mut tree,
            second_origin,
            [BoundBlockItem::Expression(second_yield)],
        );

        let expression = push_expression(
            &mut tree,
            BoundExpression::Match(BoundMatchExpression::new(
                first_origin,
                subject,
                [
                    BoundMatchArm::new(first_pattern, None, first_body),
                    BoundMatchArm::new(second_pattern, None, second_body),
                ],
                None,
                false,
            )),
        );

        let root_block = push_block(
            &mut tree,
            root_origin,
            [BoundBlockItem::Expression(expression)],
        );

        let root = push_callable(&mut tree, root_origin, root_block);
        let unit = callable_unit(&key, tree.finish(), root);

        let result = completed_check(&unit, &ExpressionTypeInput::new());

        assert!(result.diagnostics().is_empty());

        assert_eq!(
            result
                .value()
                .expression(expression)
                .map(|result| result.ty()),
            Some(result_type)
        );
    }

    #[test]
    fn for_results_join_break_values_with_else_results() {
        let result_type = tuple_type([]);

        let [for_origin, body_origin] = distinct_source_origins();

        let key = callable_key();
        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(61));
        let source = push_expression(&mut tree, literal(for_origin, Some(result_type)));
        let pattern = push_wildcard_pattern(&mut tree, body_origin);
        let break_value = push_expression(&mut tree, literal(body_origin, Some(result_type)));

        let transfer = push_expression(
            &mut tree,
            BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
                body_origin,
                BoundControlTransferKind::Break,
                Some(break_value),
                Some(for_origin.source_anchor().syntax()),
                None,
                false,
            )),
        );

        let body = push_block(
            &mut tree,
            body_origin,
            [BoundBlockItem::Expression(transfer)],
        );

        let else_value = push_expression(&mut tree, literal(for_origin, Some(result_type)));

        let else_yield = push_expression(
            &mut tree,
            BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
                for_origin,
                BoundControlTransferKind::Yield,
                Some(else_value),
                Some(for_origin.source_anchor().syntax()),
                None,
                false,
            )),
        );

        let else_body = push_block(
            &mut tree,
            for_origin,
            [BoundBlockItem::Expression(else_yield)],
        );

        let expression = push_expression(
            &mut tree,
            BoundExpression::For(BoundForExpression::new(
                for_origin,
                BoundIterationSource::new(source, IterationSourceMode::Shared),
                pattern,
                body,
                Some(else_body),
                None,
                false,
            )),
        );

        let root_block = push_block(
            &mut tree,
            body_origin,
            [BoundBlockItem::Expression(expression)],
        );

        let root = push_callable(&mut tree, body_origin, root_block);
        let unit = callable_unit(&key, tree.finish(), root);

        let result = completed_check(&unit, &ExpressionTypeInput::new());

        assert!(result.diagnostics().is_empty());

        assert_eq!(
            result
                .value()
                .expression(expression)
                .map(|result| result.ty()),
            Some(result_type)
        );
    }

    #[test]
    fn general_generators_retain_element_types_while_iterations_complete_as_unit() {
        let element_type = tuple_type([]);

        let fixture = generator_unit(
            BoundUnitId::new(62),
            BoundStructuredExpressionKind::GeneralGenerator,
            element_type,
            1,
        );

        let result = completed_check(&fixture.unit, &ExpressionTypeInput::new());

        assert!(result.diagnostics().is_empty());

        let Some(generator_type) = result.value().expression(fixture.generator) else {
            panic!("general generator must have a type");
        };

        assert_eq!(
            type_data(generator_type.ty()),
            TypeData::Generator(element_type)
        );

        let entry = callable_entry(fixture.unit.key());
        let context = crate::test_support::TestCheckerContext::new(false);

        let request = CheckerUnitView::new(&fixture.unit, &entry, &context)
            .unwrap_or_else(|error| panic!("test checker view must be valid: {error:?}"));

        let unit_type = crate::representation::representation_type(
            request,
            bray_compiler_known::RepresentationRole::Unit,
        )
        .unwrap_or_else(|error| panic!("unit type must be available: {error:?}"));

        assert_eq!(
            result
                .value()
                .expression(fixture.iteration)
                .map(|result| result.ty()),
            Some(unit_type)
        );

        assert_eq!(
            result
                .value()
                .expression(fixture.yields[0])
                .map(|result| result.ty()),
            Some(unit_type)
        );
    }

    #[test]
    fn array_generators_infer_fixed_array_types_from_fixed_array_sources() {
        let element_type = tuple_type([]);

        let fixture = generator_unit(
            BoundUnitId::new(63),
            BoundStructuredExpressionKind::ArrayGenerator,
            element_type,
            1,
        );

        let result = completed_check(&fixture.unit, &ExpressionTypeInput::new());

        assert!(result.diagnostics().is_empty());

        let Some(source_type) = result.value().expression(fixture.source) else {
            panic!("fixed array source must have a type");
        };

        assert_eq!(
            result
                .value()
                .expression(fixture.generator)
                .map(|result| result.ty()),
            Some(source_type.ty())
        );
    }

    #[test]
    fn array_generators_defer_cardinality_without_selected_iteration_sources() {
        let fixture = generator_unit(
            BoundUnitId::new(64),
            BoundStructuredExpressionKind::ArrayGenerator,
            tuple_type([]),
            0,
        );

        let result = completed_check(&fixture.unit, &ExpressionTypeInput::new());

        assert_eq!(
            result
                .diagnostics()
                .iter()
                .map(bray_diagnostics::Diagnostic::kind)
                .collect::<Vec<_>>(),
            [DiagnosticKind::CheckingCannotInferExpressionType]
        );
    }

    #[test]
    fn catch_wraps_success_values_in_result_with_panic_reports() {
        let success = tuple_type([]);

        let (unit, expressions) = expression_unit(BoundUnitId::new(65), |tree, origin| {
            let operand = push_expression(tree, literal(origin, Some(success)));

            let caught = push_expression(
                tree,
                BoundExpression::Structured(BoundStructuredExpression::new(
                    origin,
                    BoundStructuredExpressionKind::Catch,
                    [operand],
                    [],
                    [],
                    None,
                    false,
                )),
            );

            vec![caught]
        });

        let result = completed_check(&unit, &ExpressionTypeInput::new());

        assert!(result.diagnostics().is_empty());

        let Some(caught) = result.value().expression(expressions[0]) else {
            panic!("catch expression must have a type");
        };

        let TypeData::Named { substitution, .. } = type_data(caught.ty()) else {
            panic!("catch expression must produce a named Result type");
        };

        let substitution = semantic_values()
            .generic_substitution_data(substitution)
            .unwrap_or_else(|error| panic!("Result substitution must be available: {error:?}"));

        let arguments = substitution
            .bindings()
            .iter()
            .map(|binding| binding.argument())
            .collect::<Vec<_>>();

        assert_eq!(arguments.first(), Some(&GenericArgument::Type(success)));

        let Some(GenericArgument::Type(error)) = arguments.get(1).copied() else {
            panic!("Result error argument must be a type");
        };

        let entry = callable_entry(unit.key());
        let context = crate::test_support::TestCheckerContext::new(false);

        let request = CheckerUnitView::new(&unit, &entry, &context)
            .unwrap_or_else(|error| panic!("test checker view must be valid: {error:?}"));

        let panic_report = crate::representation::representation_type(
            request,
            bray_compiler_known::RepresentationRole::PanicReport,
        )
        .unwrap_or_else(|error| panic!("PanicReport must be available: {error:?}"));

        assert_eq!(error, panic_report);
    }

    struct GeneratorFixture {
        unit: bray_bound_tree::BoundUnit,
        source: bray_bound_tree::BoundExpressionId,
        iteration: bray_bound_tree::BoundExpressionId,
        generator: bray_bound_tree::BoundExpressionId,
        yields: Vec<bray_bound_tree::BoundExpressionId>,
    }

    fn generator_unit(
        unit: BoundUnitId,
        kind: BoundStructuredExpressionKind,
        element_type: bray_symbols::TypeId,
        yield_count: usize,
    ) -> GeneratorFixture {
        let [region_origin, body_origin] = distinct_source_origins();

        let key = callable_key();
        let mut tree = BoundTreeBuilder::new(unit);

        let source_elements = (0..2)
            .map(|_| push_expression(&mut tree, literal(body_origin, Some(element_type))))
            .collect::<Vec<_>>();

        let source = push_expression(
            &mut tree,
            BoundExpression::Structured(BoundStructuredExpression::new(
                body_origin,
                BoundStructuredExpressionKind::Array,
                source_elements,
                [],
                [],
                None,
                false,
            )),
        );

        let pattern = push_wildcard_pattern(&mut tree, body_origin);
        let mut yields = Vec::with_capacity(yield_count);

        for _ in 0..yield_count {
            let value = push_expression(&mut tree, literal(body_origin, Some(element_type)));

            let yielded = push_expression(
                &mut tree,
                BoundExpression::ControlTransfer(BoundControlTransferExpression::new(
                    body_origin,
                    BoundControlTransferKind::Yield,
                    Some(value),
                    Some(region_origin.source_anchor().syntax()),
                    None,
                    false,
                )),
            );

            yields.push(yielded);
        }

        let body = push_block(
            &mut tree,
            body_origin,
            yields.iter().copied().map(BoundBlockItem::Expression),
        );

        let iteration = push_expression(
            &mut tree,
            BoundExpression::Generator(BoundGeneratorExpression::new(
                body_origin,
                BoundIterationSource::new(source, IterationSourceMode::Shared),
                pattern,
                body,
                body_origin.source_anchor().syntax(),
                None,
                false,
            )),
        );

        let generator = push_expression(
            &mut tree,
            BoundExpression::Structured(BoundStructuredExpression::new(
                region_origin,
                kind,
                [iteration],
                [],
                [],
                None,
                false,
            )),
        );

        let root_block = push_block(
            &mut tree,
            region_origin,
            [BoundBlockItem::Expression(generator)],
        );

        let root = push_callable(&mut tree, region_origin, root_block);
        let unit = callable_unit(&key, tree.finish(), root);

        GeneratorFixture {
            unit,
            source,
            iteration,
            generator,
            yields,
        }
    }

    fn push_wildcard_pattern(
        tree: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
    ) -> bray_bound_tree::BoundPatternId {
        let pattern = BoundPattern::new(
            origin,
            error_type(),
            BoundPatternMode::Declaration,
            BoundPatternKind::Discard,
            [],
            [],
        );

        tree.push_pattern(pattern)
            .unwrap_or_else(|error| panic!("test wildcard pattern must fit: {error:?}"))
    }

    #[test]
    fn foreign_type_inputs_fail_without_publishing_partial_results() {
        let (unit, _) = expression_unit(BoundUnitId::new(45), |tree, origin| {
            vec![push_expression(tree, literal(origin, None))]
        });

        let (_, foreign) = expression_unit(BoundUnitId::new(46), |tree, origin| {
            vec![push_expression(tree, literal(origin, None))]
        });

        let input = ExpressionTypeInput::new()
            .with_evidence([ExpressionTypeEvidence::new(foreign[0], error_type())]);

        let key = unit.key();
        let entry = callable_entry(key);
        let context = crate::test_support::TestCheckerContext::new(false);

        let Ok(request) = CheckerUnitView::new(&unit, &entry, &context) else {
            panic!("test checker unit view must be valid");
        };

        let outcome = DefaultExpressionTypeChecker.check_expression_types(request, &input);

        assert_eq!(
            outcome.infrastructure_failure(),
            Some(CheckerInfrastructureError::InvalidExpressionTypeInput {
                expression: foreign[0],
            })
        );

        assert_eq!(outcome.result(), None);
    }

    #[test]
    fn cancellation_publishes_no_expression_types_or_diagnostics() {
        let (unit, _) = expression_unit(BoundUnitId::new(47), |tree, origin| {
            vec![push_expression(tree, literal(origin, None))]
        });

        let entry = callable_entry(unit.key());
        let context = crate::test_support::TestCheckerContext::new(true);

        let Ok(request) = CheckerUnitView::new(&unit, &entry, &context) else {
            panic!("test checker unit view must be valid");
        };

        let outcome = DefaultExpressionTypeChecker
            .check_expression_types(request, &ExpressionTypeInput::new());

        assert!(outcome.is_cancelled());
        assert_eq!(outcome.result(), None);
    }

    #[test]
    fn cancellation_during_constraint_construction_publishes_nothing() {
        let (unit, _) = expression_unit(BoundUnitId::new(53), |tree, origin| {
            (0..32)
                .map(|_| push_expression(tree, literal(origin, None)))
                .collect()
        });

        let entry = callable_entry(unit.key());
        let context = crate::test_support::TestCheckerContext::cancelling_after(8);

        let Ok(request) = CheckerUnitView::new(&unit, &entry, &context) else {
            panic!("test checker unit view must be valid");
        };

        let outcome = DefaultExpressionTypeChecker
            .check_expression_types(request, &ExpressionTypeInput::new());

        assert_eq!(outcome, CheckerOutcome::Cancelled);
    }
}
