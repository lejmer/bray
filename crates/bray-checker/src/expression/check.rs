use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundExpression, CheckedSemanticSelections, DeclaredValueTypeTemplates,
    SelectedIterationSource, SemanticSelection, SemanticSelectionEntry,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableSignatureQuery, GenericConstraintsQuery, StructFieldTypeQuery,
    UnionPayloadFieldTypeQuery,
};

use super::built_in_operator;
use super::candidate::{PreparedExpressions, converge, final_selections, prepare_calls};
use super::declared::{PreparedDeclaredTypes, defer_return_operands, prepare_declared_types};
use super::pattern_reference::{
    PreparedPatternReferences, expression_uses_pattern_binding,
    pattern_binding_reference_expressions, prepare_pattern_binding_references,
    resolved_pattern_binding_evidence,
};
use crate::expression::check_literal_values;
use crate::type_check::{
    ExpressionTypeSession, SessionProgress, finish_expression_types_with_deferred,
};
use crate::unit::semantic_input_failure;
use crate::{
    CheckerInfrastructureError, CheckerInputKind, CheckerOutcome, CheckerQueryError,
    CheckerRequestContext, CheckerSemanticQueryProvider, CheckerUnitView, ExpressionCandidateSet,
    ExpressionTypeEvidence, NestedCallableEvidence, PatternCheckInput,
};

pub(crate) fn check_expression_semantics<C>(
    request: CheckerUnitView<'_, C>,
    declared_types: &DeclaredValueTypeTemplates,
    nested_callables: &[NestedCallableEvidence],
    candidate_sets: &[ExpressionCandidateSet],
    pattern_input: &PatternCheckInput,
    operation_input: &crate::ExpressionTypeInput,
) -> CheckerOutcome<
    (
        bray_bound_tree::CheckedExpressionTypes,
        CheckedSemanticSelections,
        bray_bound_tree::CheckedLiteralValues,
    ),
    C::UpstreamError,
>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<CallableSignatureQuery>
        + CheckerSemanticQueryProvider<GenericConstraintsQuery>
        + CheckerSemanticQueryProvider<StructFieldTypeQuery>
        + CheckerSemanticQueryProvider<UnionPayloadFieldTypeQuery>
        + ?Sized,
{
    if let Some(error) = semantic_input_failure(
        request,
        [(
            CheckerInputKind::DeclaredValueTypes,
            (declared_types.unit(), declared_types.kind()),
        )],
    ) {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    // Provisional checks need an owned enrichment while the caller retains its reusable input.
    let pattern_input = pattern_input
        .clone()
        .with_declared_pattern_types(declared_types);

    let pending = match pattern_binding_reference_expressions(request) {
        Ok(pending) => pending,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    if pending.is_empty() {
        return check_expression_semantics_once(
            request,
            declared_types,
            nested_callables,
            candidate_sets,
            PreparedPatternReferences::default(),
            operation_input,
        );
    }

    let mut evidence = Vec::new();
    let mut deferred = pending.clone();

    let provisional_patterns = loop {
        let types = match check_provisional_expression_types(
            request,
            declared_types,
            nested_callables,
            candidate_sets,
            &evidence,
            &deferred,
            operation_input,
        ) {
            CheckerOutcome::Complete(result) => result.into_parts().0,
            CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
            CheckerOutcome::InfrastructureFailure(error) => {
                return CheckerOutcome::InfrastructureFailure(error);
            }
            CheckerOutcome::UpstreamFailure(error) => {
                return CheckerOutcome::UpstreamFailure(error);
            }
        };

        let patterns = match crate::pattern::check_patterns(request, &types, &pattern_input) {
            CheckerOutcome::Complete(result) => result.into_parts().0,
            CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
            CheckerOutcome::InfrastructureFailure(error) => {
                return CheckerOutcome::InfrastructureFailure(error);
            }
            CheckerOutcome::UpstreamFailure(error) => {
                return CheckerOutcome::UpstreamFailure(error);
            }
        };

        let resolved = match resolved_pattern_binding_evidence(request, &patterns, &deferred) {
            Ok(resolved) => resolved,
            Err(error) => return CheckerOutcome::InfrastructureFailure(error),
        };

        if resolved.is_empty() {
            break patterns;
        }

        for entry in resolved {
            deferred.remove(&entry.expression());
            evidence.push(entry);
        }
    };

    let prepared = match prepare_pattern_binding_references(request, &provisional_patterns, pending)
    {
        Ok(prepared) => prepared,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    check_expression_semantics_once(
        request,
        declared_types,
        nested_callables,
        candidate_sets,
        prepared,
        operation_input,
    )
}

fn check_provisional_expression_types<C>(
    request: CheckerUnitView<'_, C>,
    declared_types: &DeclaredValueTypeTemplates,
    nested_callables: &[NestedCallableEvidence],
    candidate_sets: &[ExpressionCandidateSet],
    evidence: &[ExpressionTypeEvidence],
    deferred: &BTreeSet<bray_bound_tree::BoundExpressionId>,
    operation_input: &crate::ExpressionTypeInput,
) -> CheckerOutcome<bray_bound_tree::CheckedExpressionTypes, C::UpstreamError>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<CallableSignatureQuery>
        + CheckerSemanticQueryProvider<GenericConstraintsQuery>
        + ?Sized,
{
    let (session, prepared) = match prepare_expression_check(
        request,
        declared_types,
        nested_callables,
        candidate_sets,
        evidence,
        deferred,
        operation_input,
    ) {
        Ok(SessionProgress::Complete(prepared)) => prepared,
        Ok(SessionProgress::Cancelled) => return CheckerOutcome::Cancelled,
        Err(error) => return query_outcome(error),
    };

    finish_expression_types_with_deferred(request, session, prepared.deferred(), &[])
}

fn check_expression_semantics_once<C>(
    request: CheckerUnitView<'_, C>,
    declared_types: &DeclaredValueTypeTemplates,
    nested_callables: &[NestedCallableEvidence],
    candidate_sets: &[ExpressionCandidateSet],
    supplemental: PreparedPatternReferences,
    operation_input: &crate::ExpressionTypeInput,
) -> CheckerOutcome<
    (
        bray_bound_tree::CheckedExpressionTypes,
        CheckedSemanticSelections,
        bray_bound_tree::CheckedLiteralValues,
    ),
    C::UpstreamError,
>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<CallableSignatureQuery>
        + CheckerSemanticQueryProvider<GenericConstraintsQuery>
        + ?Sized,
{
    let (session, prepared) = match prepare_expression_check(
        request,
        declared_types,
        nested_callables,
        candidate_sets,
        &supplemental.evidence,
        &supplemental.deferred,
        operation_input,
    ) {
        Ok(SessionProgress::Complete(prepared)) => prepared,
        Ok(SessionProgress::Cancelled) => return CheckerOutcome::Cancelled,
        Err(error) => return query_outcome(error),
    };

    finish_expression_check(
        request,
        session,
        prepared,
        supplemental.selections,
        supplemental.diagnostics,
        operation_input.iteration_sources(),
        operation_input.operation_selections(),
    )
}

fn prepare_expression_check<'view, C>(
    request: CheckerUnitView<'view, C>,
    declared_types: &DeclaredValueTypeTemplates,
    nested_callables: &[NestedCallableEvidence],
    candidate_sets: &[ExpressionCandidateSet],
    supplemental_evidence: &[ExpressionTypeEvidence],
    supplemental_deferred: &BTreeSet<bray_bound_tree::BoundExpressionId>,
    operation_input: &crate::ExpressionTypeInput,
) -> Result<
    SessionProgress<(ExpressionTypeSession<'view, C>, PreparedExpressions)>,
    crate::CheckerQueryError<C::UpstreamError>,
>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<CallableSignatureQuery>
        + CheckerSemanticQueryProvider<GenericConstraintsQuery>
        + ?Sized,
{
    let supplemental_types = nested_callables
        .iter()
        .map(NestedCallableEvidence::value_type)
        .cloned()
        .collect::<Vec<_>>();

    let Some(declared) =
        prepare_declared_types(request, declared_types, &supplemental_types)?.into_value()
    else {
        return Ok(SessionProgress::Cancelled);
    };

    let PreparedDeclaredTypes {
        input,
        deferred,
        unsupported_callable_result,
        diagnostics,
    } = declared;

    let Some(mut prepared) = prepare_calls(request, nested_callables, candidate_sets)?.into_value()
    else {
        return Ok(SessionProgress::Cancelled);
    };

    prepared.add_operation_selections(operation_input.operation_selections());

    prepared.add_diagnostics(&diagnostics);
    prepared.defer(deferred);
    prepared.defer(supplemental_deferred.iter().copied());

    let Some(mut session) = ExpressionTypeSession::begin(request)?.into_value() else {
        return Ok(SessionProgress::Cancelled);
    };

    if unsupported_callable_result {
        defer_return_operands(request, session.expressions(), prepared.deferred_mut());
    }

    let replaced_evidence = input
        .evidence()
        .iter()
        .filter_map(|evidence| {
            expression_uses_pattern_binding(request, evidence.expression())
                .then_some(evidence.expression())
        })
        .collect::<BTreeSet<_>>();

    session.apply_input_replacing_evidence(&input, &replaced_evidence)?;
    session.apply_input(operation_input)?;

    for evidence in supplemental_evidence {
        session.add_evidence(evidence.expression(), evidence.ty())?;
    }

    built_in_operator::apply_evidence(request, prepared.built_in_operators(), &mut session)?;

    if converge(request, &prepared, &mut session)?.is_cancelled() {
        return Ok(SessionProgress::Cancelled);
    }

    apply_await_completion_evidence(request, &mut session)?;

    if session.apply_literal_defaults().is_cancelled()
        || converge(request, &prepared, &mut session)?.is_cancelled()
    {
        return Ok(SessionProgress::Cancelled);
    }

    Ok(SessionProgress::Complete((session, prepared)))
}

fn apply_await_completion_evidence<C>(
    request: CheckerUnitView<'_, C>,
    session: &mut ExpressionTypeSession<'_, C>,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let awaits = request
        .unit()
        .tree()
        .expressions()
        .filter_map(|(expression, node)| match node {
            BoundExpression::Await(awaited) => Some((expression, awaited.operand())),
            _ => None,
        })
        .collect::<Vec<_>>();

    for (expression, operand) in awaits {
        let Some(future) = session.expression_type(operand) else {
            continue;
        };

        let Some(completion) = request
            .available_compiler_known_symbols()
            .unary_representation_argument(
                request.semantic_values(),
                RepresentationRole::Future,
                future.ty(),
            )
        else {
            continue;
        };

        session.add_evidence(expression, completion)?;
    }

    Ok(())
}

fn finish_expression_check<C>(
    request: CheckerUnitView<'_, C>,
    session: ExpressionTypeSession<'_, C>,
    prepared: PreparedExpressions,
    mut supplemental_selections: Vec<SemanticSelectionEntry>,
    supplemental_diagnostics: DiagnosticBag,
    iteration_sources: &[SelectedIterationSource],
    operation_selections: &[SemanticSelectionEntry],
) -> CheckerOutcome<
    (
        bray_bound_tree::CheckedExpressionTypes,
        CheckedSemanticSelections,
        bray_bound_tree::CheckedLiteralValues,
    ),
    C::UpstreamError,
>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<CallableSignatureQuery>
        + CheckerSemanticQueryProvider<GenericConstraintsQuery>
        + ?Sized,
{
    let type_result = match finish_expression_types_with_deferred(
        request,
        session,
        prepared.deferred(),
        iteration_sources,
    ) {
        CheckerOutcome::Complete(result) => result,
        CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
        CheckerOutcome::InfrastructureFailure(error) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        CheckerOutcome::UpstreamFailure(error) => {
            return CheckerOutcome::UpstreamFailure(error);
        }
    };

    let (types, type_diagnostics) = type_result.into_parts();

    let (mut entries, selection_diagnostics) = match final_selections(request, &types, &prepared) {
        Ok(Some(result)) => result,
        Ok(None) => return CheckerOutcome::Cancelled,
        Err(error) => return query_outcome(error),
    };

    let (mut propagation_entries, propagation_diagnostics) =
        match crate::selection::select_propagations(request, &types) {
            Ok(Some(result)) => result,
            Ok(None) => return CheckerOutcome::Cancelled,
            Err(error) => return query_outcome(error),
        };

    entries.append(&mut propagation_entries);
    entries.append(&mut supplemental_selections);

    entries.retain(|entry| {
        !operation_selections
            .iter()
            .any(|operation| operation.expression() == entry.expression())
    });

    entries.extend(operation_selections.iter().cloned());

    // Iteration discovery retains its cached selections while this table owns its entries.
    entries.extend(iteration_sources.iter().cloned().map(|selection| {
        SemanticSelectionEntry::new(
            selection.expression(),
            SemanticSelection::Iteration(selection),
        )
    }));

    let selections = match CheckedSemanticSelections::try_new(request.unit(), &types, entries) {
        Ok(selections) => selections,
        Err(error) => {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::SemanticSelection(error),
            );
        }
    };

    let literal_result = match check_literal_values(request, &types) {
        CheckerOutcome::Complete(result) => result,
        CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
        CheckerOutcome::InfrastructureFailure(error) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        CheckerOutcome::UpstreamFailure(error) => {
            return CheckerOutcome::UpstreamFailure(error);
        }
    };

    let (literal_values, literal_diagnostics) = literal_result.into_parts();

    CheckerOutcome::complete(
        (types, selections, literal_values),
        type_diagnostics
            .merged(&selection_diagnostics)
            .merged(&propagation_diagnostics)
            .merged(prepared.diagnostics())
            .merged(&supplemental_diagnostics)
            .merged(&literal_diagnostics),
    )
}

fn query_outcome<T, Upstream>(error: CheckerQueryError<Upstream>) -> CheckerOutcome<T, Upstream> {
    match error {
        CheckerQueryError::Cancelled => CheckerOutcome::Cancelled,
        CheckerQueryError::Infrastructure(error) => CheckerOutcome::InfrastructureFailure(error),
        CheckerQueryError::Upstream(error) => CheckerOutcome::UpstreamFailure(error),
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundExpressionId, BoundUnit, BoundUnitId, DeclaredValueTypeTemplates};

    use super::check_expression_semantics;
    use crate::test_support::{
        TestCheckerContext, callable_entry, expression_unit, integer_literal_expression,
        push_expression,
    };
    use crate::{CheckerInfrastructureError, CheckerUnitView, ExpressionCandidateSet};

    #[test]
    fn foreign_candidate_expressions_fail_without_partial_results() {
        let (unit, _) = literal_unit(BoundUnitId::new(91));

        let (_, foreign) = literal_unit(BoundUnitId::new(92));

        let declared =
            DeclaredValueTypeTemplates::new(unit.unit(), unit.key().kind(), [], [], None, None);

        let context = TestCheckerContext::new(false);
        let semantic_context = callable_entry(unit.key());

        let request = match CheckerUnitView::new(&unit, &semantic_context, &context) {
            Ok(request) => request,
            Err(error) => panic!("test checker unit view must be valid: {error:?}"),
        };

        let outcome = check_expression_semantics(
            request,
            &declared,
            &[],
            &[ExpressionCandidateSet::NotApplicable(foreign)],
            &crate::PatternCheckInput::new(),
            &crate::ExpressionTypeInput::new(),
        );

        assert_eq!(
            outcome.infrastructure_failure(),
            Some(CheckerInfrastructureError::InvalidSemanticSelectionInput)
        );

        assert_eq!(outcome.result(), None);
    }

    fn literal_unit(unit: BoundUnitId) -> (BoundUnit, BoundExpressionId) {
        let (unit, expressions) = expression_unit(unit, |tree, origin| {
            vec![push_expression(
                tree,
                integer_literal_expression(origin, None),
            )]
        });

        let [expression] = expressions.as_slice() else {
            panic!("literal unit must contain one expression");
        };

        (unit, *expression)
    }
}
