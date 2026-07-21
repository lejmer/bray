use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, CheckedExpressionTypes, ExpressionTypeEntry,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticType, SeverityKind,
};
use bray_symbols::{NamedTypeSymbolId, StructSymbolId, TypeData, TypeId};

use crate::diagnostic::{diagnostic_id, expression_span};
use crate::{CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerUnitView};

use super::ExpressionTypeInput;
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
    let mut session = match ExpressionTypeSession::begin(request) {
        Ok(SessionProgress::Complete(session)) => session,
        Ok(SessionProgress::Cancelled) => return CheckerOutcome::Cancelled,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    if let Err(error) = session.apply_input(input) {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    match session.propagate() {
        Ok(SessionProgress::Complete(())) => {}
        Ok(SessionProgress::Cancelled) => return CheckerOutcome::Cancelled,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    }

    match session.apply_literal_defaults() {
        SessionProgress::Complete(()) => {}
        SessionProgress::Cancelled => return CheckerOutcome::Cancelled,
    }

    match session.propagate() {
        Ok(SessionProgress::Complete(())) => {}
        Ok(SessionProgress::Cancelled) => return CheckerOutcome::Cancelled,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    }

    finish_expression_types(request, session)
}

pub(crate) fn finish_expression_types<C>(
    request: CheckerUnitView<'_, C>,
    session: ExpressionTypeSession<'_, C>,
) -> CheckerOutcome<CheckedExpressionTypes>
where
    C: CheckerRequestContext + ?Sized,
{
    finish_expression_types_with_deferred(request, session, &BTreeSet::new())
}

pub(crate) fn finish_expression_types_with_deferred<C>(
    request: CheckerUnitView<'_, C>,
    session: ExpressionTypeSession<'_, C>,
    deferred: &BTreeSet<BoundExpressionId>,
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
        let Some(bound) = request.view().expression(expression) else {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidExpressionTypeInput { expression },
            );
        };

        if deferred.contains(&expression) {
            continue;
        }

        if matches!(bound, BoundExpression::AnonymousCallable(_)) {
            // TODO(BRA-242): Infer anonymous callable types from their nested semantic unit.
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
        .into_iter()
        .map(|(expression, result)| ExpressionTypeEntry::new(expression, result));

    CheckerOutcome::complete(
        CheckedExpressionTypes::new(request.view().unit(), request.view().kind(), entries),
        DiagnosticBag::from(diagnostics),
    )
}

fn diagnostic_type<C>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
) -> Result<DiagnosticType, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let data = request
        .semantic_values()
        .type_data(ty)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let diagnostic = match data.as_ref() {
        TypeData::Error => DiagnosticType::Error,
        TypeData::Named { definition, .. } => diagnostic_named_type(request, *definition),
        TypeData::TypeParameter(_) => DiagnosticType::TypeParameter,
        TypeData::ContextualSelf(_) => DiagnosticType::ContextualSelf,
        TypeData::AssociatedTypeProjection { .. } => DiagnosticType::AssociatedType,
        TypeData::Tuple(elements) => {
            let count = u64::try_from(elements.len()).unwrap_or(u64::MAX);

            DiagnosticType::Tuple(count)
        }
        TypeData::Array { .. } => DiagnosticType::Array,
        TypeData::Slice(_) => DiagnosticType::Slice,
        TypeData::Nullable(_) => DiagnosticType::Nullable,
        TypeData::Borrow { .. } => DiagnosticType::Borrow,
        TypeData::TraitView(_) => DiagnosticType::TraitView,
        TypeData::OwnedIndirection { .. } => DiagnosticType::OwnedIndirection,
        TypeData::Callable(_) => DiagnosticType::Callable,
    };

    Ok(diagnostic)
}

fn diagnostic_named_type<C>(
    request: CheckerUnitView<'_, C>,
    definition: NamedTypeSymbolId,
) -> DiagnosticType
where
    C: CheckerRequestContext + ?Sized,
{
    let roles = [
        (RepresentationRole::ScalarBool, DiagnosticType::Boolean),
        (RepresentationRole::ScalarChar, DiagnosticType::Character),
        (RepresentationRole::ScalarI8, DiagnosticType::I8),
        (RepresentationRole::ScalarI16, DiagnosticType::I16),
        (RepresentationRole::ScalarI32, DiagnosticType::I32),
        (RepresentationRole::ScalarI64, DiagnosticType::I64),
        (RepresentationRole::ScalarI128, DiagnosticType::I128),
        (RepresentationRole::ScalarU8, DiagnosticType::U8),
        (RepresentationRole::ScalarU16, DiagnosticType::U16),
        (RepresentationRole::ScalarU32, DiagnosticType::U32),
        (RepresentationRole::ScalarU64, DiagnosticType::U64),
        (RepresentationRole::ScalarU128, DiagnosticType::U128),
        (RepresentationRole::ScalarIsize, DiagnosticType::Isize),
        (RepresentationRole::ScalarUsize, DiagnosticType::Usize),
        (RepresentationRole::ScalarR16, DiagnosticType::R16),
        (RepresentationRole::ScalarR32, DiagnosticType::R32),
        (RepresentationRole::ScalarR64, DiagnosticType::R64),
        (RepresentationRole::ScalarR128, DiagnosticType::R128),
        (RepresentationRole::ScalarC32, DiagnosticType::C32),
        (RepresentationRole::ScalarC64, DiagnosticType::C64),
        (RepresentationRole::ScalarC128, DiagnosticType::C128),
        (RepresentationRole::ScalarC256, DiagnosticType::C256),
        (RepresentationRole::Unit, DiagnosticType::Unit),
        (RepresentationRole::Never, DiagnosticType::Never),
        (RepresentationRole::String, DiagnosticType::String),
    ];

    for (role, diagnostic) in roles {
        let Some(candidate) = request
            .available_compiler_known_symbols()
            .representation_symbol::<StructSymbolId>(role)
        else {
            continue;
        };

        if definition == NamedTypeSymbolId::Struct(candidate) {
            return diagnostic;
        }
    }

    DiagnosticType::Named
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundAssignmentExpression, BoundBlockItem, BoundControlTransferExpression,
        BoundControlTransferKind, BoundExpression, BoundNodeOrigin, BoundOperator,
        BoundStructuredExpression, BoundStructuredExpressionKind, BoundTreeBuilder,
        BoundTypeReference, BoundUnitId, ExpressionTypeResult, ExpressionTypeStatus,
    };
    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::TypeData;

    use super::super::session::{ExpressionTypeSession, SessionProgress};
    use crate::test_support::{
        callable_entry, callable_key, callable_unit, completed_expression_check as completed_check,
        distinct_source_origins, error_type, expression_unit,
        integer_literal_expression as literal, push_block, push_callable, push_expression,
        tuple_type, type_data, unselected_name_expression as unselected_name,
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
                BoundStructuredExpressionKind::BooleanFold,
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
                    BoundStructuredExpressionKind::BooleanFold,
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
