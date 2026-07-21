use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, CheckedSemanticSelections, DeclaredValueTypeTemplates,
    DeclaredValueTypeTerm, SemanticSelection, SemanticSelectionEntry,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::TypeId;

use super::template::{TemplateResolution, resolve_declaration_candidate, resolve_type_template};
use crate::type_check::{
    ExpressionTypeSession, SessionProgress, finish_expression_types_with_deferred,
};
use crate::{
    CallableCandidate, CallableCandidateState, CallableCandidateTemplate,
    CallableCandidateTemplates, CallableSelectionRequest, CandidateAbsence, CandidateSelection,
    CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerUnitView,
    ExpressionCandidateSet, ExpressionTypeEvidence, ExpressionTypeInput,
};

struct PreparedCall {
    expression: BoundExpressionId,
    candidates: Vec<CallableCandidate>,
}

struct PreparedExpressions {
    calls: Vec<PreparedCall>,
    deferred: BTreeSet<BoundExpressionId>,
}

pub(crate) fn check_expression_semantics<C>(
    request: CheckerUnitView<'_, C>,
    declared_types: &DeclaredValueTypeTemplates,
    candidate_sets: &[ExpressionCandidateSet],
) -> CheckerOutcome<(
    bray_bound_tree::CheckedExpressionTypes,
    CheckedSemanticSelections,
)>
where
    C: CheckerRequestContext + ?Sized,
{
    if declared_types.unit() != request.view().unit()
        || declared_types.kind() != request.view().kind()
    {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        );
    }

    let input = match declared_type_input(request, declared_types) {
        Ok(input) => input,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    let prepared = match prepare_calls(request, candidate_sets) {
        Ok(prepared) => prepared,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    let mut session = match ExpressionTypeSession::begin(request) {
        Ok(SessionProgress::Complete(session)) => session,
        Ok(SessionProgress::Cancelled) => return CheckerOutcome::Cancelled,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    if let Err(error) = session.apply_input(&input) {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    if let Err(error) = add_candidate_expectations(request, &prepared.calls, &mut session) {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    match converge(request, &prepared.calls, &mut session) {
        Ok(SessionProgress::Complete(())) => {}
        Ok(SessionProgress::Cancelled) => return CheckerOutcome::Cancelled,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    }

    match session.apply_literal_defaults() {
        SessionProgress::Complete(()) => {}
        SessionProgress::Cancelled => return CheckerOutcome::Cancelled,
    }

    match converge(request, &prepared.calls, &mut session) {
        Ok(SessionProgress::Complete(())) => {}
        Ok(SessionProgress::Cancelled) => return CheckerOutcome::Cancelled,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    }

    let type_result =
        match finish_expression_types_with_deferred(request, session, &prepared.deferred) {
            CheckerOutcome::Complete(result) => result,
            CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
            CheckerOutcome::InfrastructureFailure(error) => {
                return CheckerOutcome::InfrastructureFailure(error);
            }
        };

    let (types, type_diagnostics) = type_result.into_parts();

    let (entries, selection_diagnostics) = match final_selections(request, &types, &prepared.calls)
    {
        Ok(Some(result)) => result,
        Ok(None) => return CheckerOutcome::Cancelled,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    let selections = match CheckedSemanticSelections::try_new(request.unit(), &types, entries) {
        Ok(selections) => selections,
        Err(_) => {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            );
        }
    };

    CheckerOutcome::complete(
        (types, selections),
        type_diagnostics.merged(&selection_diagnostics),
    )
}

fn declared_type_input<C>(
    request: CheckerUnitView<'_, C>,
    declared: &DeclaredValueTypeTemplates,
) -> Result<ExpressionTypeInput, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut term_types = BTreeMap::<DeclaredValueTypeTerm, BTreeSet<TypeId>>::new();

    for evidence in declared.evidence() {
        if let TemplateResolution::Resolved(ty) =
            resolve_type_template(request.semantic_values(), evidence.template())?
        {
            term_types.entry(evidence.term()).or_default().insert(ty);
        }
    }

    loop {
        let mut changed = false;

        for constraint in declared.constraints() {
            changed |= copy_term_types(&mut term_types, constraint.left(), constraint.right());
            changed |= copy_term_types(&mut term_types, constraint.right(), constraint.left());
        }

        if !changed {
            break;
        }
    }

    let evidence = term_types
        .into_iter()
        .filter_map(|(term, types)| match term {
            DeclaredValueTypeTerm::Expression(expression) => Some((expression, types)),
            DeclaredValueTypeTerm::Value(_) | DeclaredValueTypeTerm::Pattern(_) => None,
        })
        .flat_map(|(expression, types)| {
            types
                .into_iter()
                .map(move |ty| ExpressionTypeEvidence::new(expression, ty))
        });

    let mut input = ExpressionTypeInput::new().with_evidence(evidence);

    if let Some(result) = declared.callable_result()
        && let TemplateResolution::Resolved(result) =
            resolve_type_template(request.semantic_values(), result)?
    {
        input = input.with_callable_result_type(result);
    }

    Ok(input)
}

fn copy_term_types(
    term_types: &mut BTreeMap<DeclaredValueTypeTerm, BTreeSet<TypeId>>,
    source: DeclaredValueTypeTerm,
    target: DeclaredValueTypeTerm,
) -> bool {
    let values = term_types
        .get(&source)
        .into_iter()
        .flat_map(|types| types.iter().copied())
        .collect::<Vec<_>>();

    let target = term_types.entry(target).or_default();
    let original_length = target.len();

    target.extend(values);

    target.len() != original_length
}

fn prepare_calls<C>(
    request: CheckerUnitView<'_, C>,
    candidate_sets: &[ExpressionCandidateSet],
) -> Result<PreparedExpressions, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut calls = Vec::new();
    let mut deferred = BTreeSet::new();

    for candidate_set in candidate_sets {
        if request
            .view()
            .expression(candidate_set.expression())
            .is_none()
        {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        }

        match candidate_set {
            ExpressionCandidateSet::Callable(CallableCandidateTemplates::Present {
                expression,
                candidates,
            }) => {
                let mut resolved = Vec::with_capacity(candidates.len());
                let mut defer_call = false;

                for candidate in candidates.iter() {
                    match candidate {
                        CallableCandidateTemplate::Declaration(candidate) => {
                            match resolve_declaration_candidate(
                                request.semantic_values(),
                                candidate,
                            )? {
                                TemplateResolution::Resolved(candidate) => {
                                    resolved.push(candidate);
                                }
                                TemplateResolution::Unsupported => defer_call = true,
                            }
                        }
                        CallableCandidateTemplate::Value(_) => {
                            // TODO(BRA-242): Classify callable values from their converged value type.
                            defer_call = true;
                        }
                    }
                }

                if defer_call {
                    defer_callable_selection(request, *expression, &mut deferred)?;
                } else {
                    calls.push(PreparedCall {
                        expression: *expression,
                        candidates: resolved,
                    });
                }
            }
            ExpressionCandidateSet::Callable(CallableCandidateTemplates::Absent {
                expression,
                reason: CandidateAbsence::EmptyOverload,
            }) => calls.push(PreparedCall {
                expression: *expression,
                candidates: Vec::new(),
            }),
            ExpressionCandidateSet::Callable(CallableCandidateTemplates::Absent {
                expression,
                ..
            }) => {
                defer_callable_selection(request, *expression, &mut deferred)?;
            }
            ExpressionCandidateSet::Unsupported { expression, .. } => {
                deferred.insert(*expression);
            }
            ExpressionCandidateSet::NotApplicable(_) => {}
        }
    }

    calls.sort_unstable_by_key(|call| call.expression);

    Ok(PreparedExpressions { calls, deferred })
}

fn defer_callable_selection<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    deferred: &mut BTreeSet<BoundExpressionId>,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(BoundExpression::Call(call)) = request.view().expression(expression) else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    deferred.insert(expression);
    defer_expression_tree(request, call.callee(), deferred)?;

    Ok(())
}

fn defer_expression_tree<C>(
    request: CheckerUnitView<'_, C>,
    root: BoundExpressionId,
    deferred: &mut BTreeSet<BoundExpressionId>,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut pending = vec![root];

    while let Some(expression) = pending.pop() {
        if !deferred.insert(expression) {
            continue;
        }

        let Some(bound) = request.view().expression(expression) else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        pending.extend(bound.child_expressions());
    }

    Ok(())
}

fn add_candidate_expectations<C>(
    request: CheckerUnitView<'_, C>,
    prepared: &[PreparedCall],
    session: &mut ExpressionTypeSession<'_, C>,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for prepared_call in prepared {
        let Some(BoundExpression::Call(call)) = request.view().expression(prepared_call.expression)
        else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        if let Some(callable_type) = common_candidate_type(&prepared_call.candidates) {
            session.add_evidence(call.callee(), callable_type)?;
        }

        if call
            .arguments()
            .iter()
            .any(|argument| argument.name().is_some())
        {
            // TODO(BRA-242): Share exact named-argument mapping with callable selection.
            continue;
        }

        for (ordinal, argument) in call.arguments().iter().enumerate() {
            let Some(expected) = common_parameter_type(&prepared_call.candidates, ordinal) else {
                continue;
            };

            session.add_expectation(argument.expression(), expected)?;
        }
    }

    Ok(())
}

fn common_candidate_type(candidates: &[CallableCandidate]) -> Option<TypeId> {
    common_candidate_value(candidates, |candidate| {
        Some(candidate.signature().callable_type())
    })
}

fn common_parameter_type(candidates: &[CallableCandidate], ordinal: usize) -> Option<TypeId> {
    common_candidate_value(candidates, |candidate| {
        candidate
            .signature()
            .parameters()
            .get(ordinal)
            .map(|parameter| parameter.ty())
    })
}

fn common_candidate_value<T: Copy + Eq>(
    candidates: &[CallableCandidate],
    mut value: impl FnMut(&CallableCandidate) -> Option<T>,
) -> Option<T> {
    let mut available = candidates
        .iter()
        .filter(|candidate| candidate.state() == CallableCandidateState::Available);
    let first = value(available.next()?)?;

    available
        .all(|candidate| value(candidate) == Some(first))
        .then_some(first)
}

fn converge<C>(
    request: CheckerUnitView<'_, C>,
    prepared: &[PreparedCall],
    session: &mut ExpressionTypeSession<'_, C>,
) -> Result<SessionProgress<()>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    loop {
        let revision = session.revision();

        match session.propagate()? {
            SessionProgress::Complete(()) => {}
            SessionProgress::Cancelled => return Ok(SessionProgress::Cancelled),
        }

        let types = session.preview();
        let Some(selections) = selected_calls(request, &types, prepared)? else {
            return Ok(SessionProgress::Cancelled);
        };

        for (expression, selection) in selections {
            session.add_evidence(expression, selection.resolution().result().ty())?;
        }

        if session.revision() == revision {
            return Ok(SessionProgress::Complete(()));
        }
    }
}

fn selected_calls<C>(
    request: CheckerUnitView<'_, C>,
    types: &bray_bound_tree::CheckedExpressionTypes,
    prepared: &[PreparedCall],
) -> Result<
    Option<Vec<(BoundExpressionId, bray_bound_tree::SelectedCall)>>,
    CheckerInfrastructureError,
>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut selections = Vec::new();

    for prepared_call in prepared {
        let outcome = select_prepared_call(request, types, prepared_call);

        match outcome {
            CheckerOutcome::Complete(result) => {
                if let CandidateSelection::Selected(selection) = result.value() {
                    // Fixed-point evidence keeps an owned selection while the interim result is discarded.
                    selections.push((prepared_call.expression, selection.clone()));
                }
            }
            CheckerOutcome::Cancelled => return Ok(None),
            CheckerOutcome::InfrastructureFailure(error) => return Err(error),
        }
    }

    Ok(Some(selections))
}

fn final_selections<C>(
    request: CheckerUnitView<'_, C>,
    types: &bray_bound_tree::CheckedExpressionTypes,
    prepared: &[PreparedCall],
) -> Result<Option<(Vec<SemanticSelectionEntry>, DiagnosticBag)>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut entries = Vec::new();
    let mut diagnostics = DiagnosticBag::new();

    for prepared_call in prepared {
        match select_prepared_call(request, types, prepared_call) {
            CheckerOutcome::Complete(result) => {
                let (selection, selection_diagnostics) = result.into_parts();

                diagnostics.add_range(selection_diagnostics.diagnostics().iter().cloned());

                if let CandidateSelection::Selected(selection) = selection {
                    entries.push(SemanticSelectionEntry::new(
                        prepared_call.expression,
                        SemanticSelection::Call(selection),
                    ));
                }
            }
            CheckerOutcome::Cancelled => return Ok(None),
            CheckerOutcome::InfrastructureFailure(error) => return Err(error),
        }
    }

    Ok(Some((entries, diagnostics)))
}

fn select_prepared_call<C>(
    request: CheckerUnitView<'_, C>,
    types: &bray_bound_tree::CheckedExpressionTypes,
    prepared: &PreparedCall,
) -> CheckerOutcome<CandidateSelection<bray_bound_tree::SelectedCall>>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(BoundExpression::Call(call)) = request.view().expression(prepared.expression) else {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        );
    };

    // Selection consumes and canonicalizes its working candidates while the fixed point retains
    // the prepared set for subsequent rounds.
    let candidates = prepared.candidates.clone();

    // The request owns source argument records independently of the immutable bound unit.
    let arguments = call.arguments().iter().cloned();

    crate::selection::select_callable(
        request,
        types,
        CallableSelectionRequest::new(
            prepared.expression,
            None,
            None,
            call.generic_arguments().iter().copied(),
            arguments,
            candidates,
        ),
    )
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
            DeclaredValueTypeTemplates::new(unit.unit(), unit.key().kind(), [], [], None);

        let context = TestCheckerContext::new(false);
        let semantic_context = callable_entry(unit.key());

        let request = match CheckerUnitView::new(&unit, &semantic_context, &context) {
            Ok(request) => request,
            Err(error) => panic!("test checker unit view must be valid: {error:?}"),
        };

        let outcome = check_expression_semantics(
            request,
            &declared,
            &[ExpressionCandidateSet::NotApplicable(foreign)],
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
