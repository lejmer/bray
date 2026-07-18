use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundExpressionId, BoundWalkControl, BoundWalkEvent, BoundWalkOutcome,
    walk_bound_unit_view,
};
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};

use crate::{CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, UnitCheckRequest};

use super::canonical::CanonicalTypes;
use super::constraints::{
    add_expectations, add_input_evidence, add_intrinsic_constraints, add_relationship_constraints,
    block_expectations, infer_tuples,
};
use super::inference::TypeInferenceContext;
use super::{ExpressionTypeCheckResult, ExpressionTypeEntry, ExpressionTypeInput};

pub(crate) fn check_expression_types<C>(
    request: UnitCheckRequest<'_, C>,
    input: &ExpressionTypeInput,
) -> CheckerOutcome<ExpressionTypeCheckResult>
where
    C: CheckerRequestContext + ?Sized,
{
    if request.is_cancelled() {
        return CheckerOutcome::Cancelled;
    }

    let nodes = match collect_nodes(request) {
        Ok(Some(nodes)) => nodes,
        Ok(None) => return CheckerOutcome::Cancelled,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    if let Err(error) = validate_input(request, input) {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    let types = match CanonicalTypes::new(request) {
        Ok(types) => types,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    let mut inference = TypeInferenceContext::new(types.error, types.never);
    let mut variables = BTreeMap::new();

    for &expression in &nodes.expressions {
        if request.is_cancelled() {
            return CheckerOutcome::Cancelled;
        }

        let Some(bound) = request.view().expression(expression) else {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidExpressionTypeInput { expression },
            );
        };

        let Some(variable) = inference.fresh(bound.is_recovered()) else {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::ExpressionTypeCapacityExceeded,
            );
        };

        variables.insert(expression, variable);

        if let Some(ty) = bound.ty() {
            if request.semantic_values().type_data(ty).is_err() {
                return CheckerOutcome::InfrastructureFailure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                );
            }

            inference.add_evidence(variable, ty, expression);
        }

        add_intrinsic_constraints(bound, expression, variable, &types, &mut inference);
    }

    add_relationship_constraints(
        request.view(),
        &nodes.expressions,
        &variables,
        &types,
        &mut inference,
    );

    let local_expectations = block_expectations(request.view(), &nodes.blocks);

    add_input_evidence(input, &variables, &mut inference);

    if let Err(error) = add_expectations(
        request,
        local_expectations
            .into_iter()
            .chain(input.expectations().iter().copied()),
        &variables,
        &mut inference,
    ) {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    if let Err(error) = infer_tuples(request, &nodes.expressions, &variables, &mut inference) {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    let ordered_variables = nodes
        .expressions
        .iter()
        .filter_map(|expression| {
            variables
                .get(expression)
                .copied()
                .map(|variable| (*expression, variable))
        })
        .collect::<Vec<_>>();

    let (results, conflicts, unresolved) = inference.finish(&ordered_variables);
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
            .with_primary_span(span),
        );
    }

    for expression in unresolved {
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

    let entries = results
        .into_iter()
        .map(|(expression, result)| ExpressionTypeEntry::new(expression, result));

    CheckerOutcome::complete(
        ExpressionTypeCheckResult::new(request.view().unit(), request.view().kind(), entries),
        DiagnosticBag::from(diagnostics),
    )
}

struct CollectedNodes {
    expressions: Vec<BoundExpressionId>,
    blocks: Vec<bray_bound_tree::BoundBlockId>,
}

fn collect_nodes<C>(
    request: UnitCheckRequest<'_, C>,
) -> Result<Option<CollectedNodes>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let root = match request.root() {
        crate::UnitCheckRoot::CallableBody(body) => AnyBoundNodeId::from(body),
        crate::UnitCheckRoot::Expression(expression) => AnyBoundNodeId::from(expression),
        crate::UnitCheckRoot::ExpressionSequence(block) => AnyBoundNodeId::from(block),
    };

    let mut expressions = BTreeSet::new();
    let mut blocks = BTreeSet::new();
    let outcome = walk_bound_unit_view(request.view(), root, |event| {
        if request.is_cancelled() {
            return BoundWalkControl::Stop;
        }

        if let BoundWalkEvent::Enter(node) = event {
            match node {
                AnyBoundNodeId::Expression(id) => {
                    expressions.insert(id);
                }
                AnyBoundNodeId::Block(id) => {
                    blocks.insert(id);
                }
                AnyBoundNodeId::Pattern(_) | AnyBoundNodeId::CallableBody(_) => {}
            }
        }

        BoundWalkControl::Continue
    });

    match outcome {
        BoundWalkOutcome::Completed => {}
        BoundWalkOutcome::Stopped => return Ok(None),
        BoundWalkOutcome::MissingNode(node) => {
            return Err(CheckerInfrastructureError::InvalidBoundNode { node });
        }
    }

    Ok(Some(CollectedNodes {
        expressions: expressions.into_iter().collect(),
        blocks: blocks.into_iter().collect(),
    }))
}

fn validate_input<C>(
    request: UnitCheckRequest<'_, C>,
    input: &ExpressionTypeInput,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if let Some(expression) = input
        .evidence()
        .iter()
        .map(|evidence| evidence.expression())
        .chain(
            input
                .expectations()
                .iter()
                .map(|expectation| expectation.expression()),
        )
        .find(|expression| request.view().expression(*expression).is_none())
    {
        return Err(CheckerInfrastructureError::InvalidExpressionTypeInput { expression });
    }

    for ty in input.evidence().iter().map(|evidence| evidence.ty()).chain(
        input
            .expectations()
            .iter()
            .map(|expectation| expectation.ty()),
    ) {
        request
            .semantic_values()
            .type_data(ty)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;
    }

    Ok(())
}

fn expression_span<C>(
    request: UnitCheckRequest<'_, C>,
    expression: BoundExpressionId,
) -> Result<bray_source::SourceSpan, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(expression) = request.view().expression(expression) else {
        return Err(CheckerInfrastructureError::InvalidExpressionTypeInput { expression });
    };

    request
        .source(expression.origin().source_anchor())
        .map(|source| source.span())
}

fn diagnostic_id(index: usize) -> DiagnosticId {
    DiagnosticId::new(u32::try_from(index).unwrap_or(u32::MAX))
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundBlock, BoundBlockItem, BoundCallableBody, BoundControlTransferExpression,
        BoundControlTransferKind, BoundExpression, BoundExpressionId, BoundNodeOrigin,
        BoundStructuredExpression, BoundStructuredExpressionKind, BoundTreeBuilder,
        BoundTypeReference, BoundUnit, BoundUnitId,
    };
    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{TypeData, TypeId};

    use super::ExpressionTypeCheckResult;
    use crate::test_support::{
        callable_entry, callable_key, callable_unit, error_type, semantic_values,
    };
    use crate::{
        CheckerInfrastructureError, CheckerOutcome, DefaultExpressionTypeChecker,
        ExpressionTypeChecker, ExpressionTypeEvidence, ExpressionTypeExpectation,
        ExpressionTypeInput, ExpressionTypeStatus, UnitCheckRequest,
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
            Some(crate::ExpressionTypeResult::new(
                actual,
                ExpressionTypeStatus::Recovered,
            ))
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
            2
        );
        assert_eq!(
            result
                .value()
                .expression(expressions[1])
                .map(crate::ExpressionTypeResult::status),
            Some(ExpressionTypeStatus::Recovered)
        );
    }

    #[test]
    fn evidence_order_cannot_change_type_results_or_diagnostics() {
        let first_type = tuple_type([]);
        let second_type = tuple_type([first_type]);
        let (unit, expressions) = expression_unit(BoundUnitId::new(48), |tree, origin| {
            vec![push_expression(tree, literal(origin, None))]
        });
        let first = ExpressionTypeEvidence::new(expressions[0], first_type);
        let second = ExpressionTypeEvidence::new(expressions[0], second_type);
        let forward = ExpressionTypeInput::new().with_evidence([first, second]);
        let reverse = ExpressionTypeInput::new().with_evidence([second, first]);

        let forward = completed_check(&unit, &forward);
        let reverse = completed_check(&unit, &reverse);

        assert_eq!(forward, reverse);
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
            let unresolved = push_expression(tree, literal(origin, None));
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
        let Ok(request) = UnitCheckRequest::new(&unit, &entry, &context) else {
            panic!("test checker request must be valid");
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
        let Ok(request) = UnitCheckRequest::new(&unit, &entry, &context) else {
            panic!("test checker request must be valid");
        };

        let outcome = DefaultExpressionTypeChecker
            .check_expression_types(request, &ExpressionTypeInput::new());

        assert!(outcome.is_cancelled());
        assert_eq!(outcome.result(), None);
    }

    fn completed_check(
        unit: &BoundUnit,
        input: &ExpressionTypeInput,
    ) -> bray_diagnostics::DiagnosticResult<ExpressionTypeCheckResult> {
        let entry = callable_entry(unit.key());
        let context = crate::test_support::TestCheckerContext::new(false);
        let Ok(request) = UnitCheckRequest::new(unit, &entry, &context) else {
            panic!("test checker request must be valid");
        };

        let outcome = DefaultExpressionTypeChecker.check_expression_types(request, input);

        let CheckerOutcome::Complete(result) = outcome else {
            panic!("expression type checking must complete");
        };

        result
    }

    fn expression_unit(
        unit: BoundUnitId,
        build: impl FnOnce(&mut BoundTreeBuilder, BoundNodeOrigin) -> Vec<BoundExpressionId>,
    ) -> (BoundUnit, Vec<BoundExpressionId>) {
        let key = callable_key();
        let origin = BoundNodeOrigin::source(key.source());
        let mut tree = BoundTreeBuilder::new(unit);
        let expressions = build(&mut tree, origin);
        let block = push_block(
            &mut tree,
            origin,
            expressions.iter().copied().map(BoundBlockItem::Expression),
        );
        let root = push_callable(&mut tree, origin, block);
        let unit = callable_unit(&key, tree.finish(), root);

        (unit, expressions)
    }

    fn literal(origin: BoundNodeOrigin, ty: Option<TypeId>) -> BoundExpression {
        BoundExpression::Structured(BoundStructuredExpression::new(
            origin,
            BoundStructuredExpressionKind::Literal,
            [],
            [],
            [],
            ty,
            false,
        ))
    }

    fn push_expression(
        tree: &mut BoundTreeBuilder,
        expression: BoundExpression,
    ) -> BoundExpressionId {
        match tree.push_expression(expression) {
            Ok(expression) => expression,
            Err(error) => panic!("test expression must be valid: {error:?}"),
        }
    }

    fn push_block(
        tree: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
        items: impl IntoIterator<Item = BoundBlockItem>,
    ) -> bray_bound_tree::BoundBlockId {
        match tree.push_block(BoundBlock::new(origin, items, false)) {
            Ok(block) => block,
            Err(error) => panic!("test block must be valid: {error:?}"),
        }
    }

    fn push_callable(
        tree: &mut BoundTreeBuilder,
        origin: BoundNodeOrigin,
        block: bray_bound_tree::BoundBlockId,
    ) -> bray_bound_tree::BoundCallableBodyId {
        match tree.push_callable_body(BoundCallableBody::block(origin, block)) {
            Ok(body) => body,
            Err(error) => panic!("test callable body must be valid: {error:?}"),
        }
    }

    fn tuple_type(elements: impl IntoIterator<Item = TypeId>) -> TypeId {
        match semantic_values().intern_type(TypeData::tuple(elements)) {
            Ok(ty) => ty,
            Err(error) => panic!("test tuple type must be valid: {error:?}"),
        }
    }

    fn type_data(ty: TypeId) -> TypeData {
        match semantic_values().type_data(ty) {
            Ok(data) => data.as_ref().clone(),
            Err(error) => panic!("test type must belong to the semantic store: {error:?}"),
        }
    }
}
