use std::{borrow::Cow, collections::BTreeSet};

use bray_bound_tree::{
    BoundCallableTarget, BoundExpression, BoundExpressionId, BoundResolvedCall, SelectedArgument,
    SemanticSelection, SemanticSelectionEntry,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::TypeId;

use super::template::{
    TemplateResolution, call_result, candidate_state, resolve_declaration_candidate,
};
use crate::type_check::{ExpressionTypeSession, SessionProgress};
use crate::{
    CallableCandidate, CallableCandidateTemplate, CallableCandidateTemplates,
    CallableSelectionRequest, CallableValueCandidateTemplate, CandidateAbsence, CandidateSelection,
    CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerUnitView,
    ExpressionCandidateSet, NestedCallableEvidence,
};

struct PreparedCall {
    expression: BoundExpressionId,
    candidates: Vec<CallableCandidate>,
    values: Vec<CallableValueCandidateTemplate>,
    anonymous_target: Option<bray_symbols::AnonymousCallableSymbolId>,
}

pub(super) struct PreparedExpressions {
    calls: Vec<PreparedCall>,
    deferred: BTreeSet<BoundExpressionId>,
    diagnostics: DiagnosticBag,
}

impl PreparedExpressions {
    pub(super) fn defer(&mut self, expressions: impl IntoIterator<Item = BoundExpressionId>) {
        self.deferred.extend(expressions);
    }

    pub(super) const fn deferred(&self) -> &BTreeSet<BoundExpressionId> {
        &self.deferred
    }

    pub(super) fn deferred_mut(&mut self) -> &mut BTreeSet<BoundExpressionId> {
        &mut self.deferred
    }

    pub(super) fn add_diagnostics(&mut self, diagnostics: &DiagnosticBag) {
        // Prepared expression state outlives the dependency bag it aggregates.
        self.diagnostics.extend(diagnostics.iter().cloned());
    }

    pub(super) const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }
}

pub(super) fn prepare_calls<C>(
    request: CheckerUnitView<'_, C>,
    nested_callables: &[NestedCallableEvidence],
    candidate_sets: &[ExpressionCandidateSet],
) -> Result<SessionProgress<PreparedExpressions>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut calls = Vec::new();
    let mut deferred = BTreeSet::new();
    let mut diagnostics = DiagnosticBag::new();

    for candidate_set in candidate_sets {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        let Some(expression) = request.view().expression(candidate_set.expression()) else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        if matches!(candidate_set, ExpressionCandidateSet::Callable(_))
            && matches!(expression, BoundExpression::ErrorCall(_))
        {
            if defer_callable_selection(request, candidate_set.expression(), &mut deferred)?
                .is_cancelled()
            {
                return Ok(SessionProgress::Cancelled);
            }

            continue;
        }

        match candidate_set {
            ExpressionCandidateSet::Callable(CallableCandidateTemplates::Present {
                expression,
                candidates,
            }) => {
                let mut resolved = Vec::with_capacity(candidates.len());
                let mut values = Vec::new();
                let mut defer_call = false;

                for candidate in candidates.iter() {
                    if request.is_cancelled() {
                        return Ok(SessionProgress::Cancelled);
                    }

                    match candidate {
                        CallableCandidateTemplate::Declaration(candidate) => {
                            let materialized = resolve_declaration_candidate(
                                request,
                                candidate,
                                &mut diagnostics,
                            )?;

                            match materialized {
                                TemplateResolution::Resolved(candidate) => {
                                    if candidate.generic_constraints().is_empty() {
                                        resolved.push(candidate);
                                    } else {
                                        // TODO(BRA-233): Select constrained candidates from checked predicate facts.
                                        defer_call = true;
                                    }
                                }
                                TemplateResolution::Unsupported => defer_call = true,
                            }
                        }
                        CallableCandidateTemplate::Value(candidate) => values.push(*candidate),
                    }
                }

                if defer_call {
                    if defer_callable_selection(request, *expression, &mut deferred)?.is_cancelled()
                    {
                        return Ok(SessionProgress::Cancelled);
                    }
                } else {
                    calls.push(PreparedCall {
                        expression: *expression,
                        candidates: CallableSelectionRequest::canonical_candidates(resolved),
                        values,
                        anonymous_target: nested_callable_target(
                            request,
                            *expression,
                            nested_callables,
                        )?,
                    });
                }
            }
            ExpressionCandidateSet::Callable(CallableCandidateTemplates::Absent {
                expression,
                reason: CandidateAbsence::EmptyOverload,
            }) => calls.push(PreparedCall {
                expression: *expression,
                candidates: Vec::new(),
                values: Vec::new(),
                anonymous_target: None,
            }),
            ExpressionCandidateSet::Callable(CallableCandidateTemplates::Absent {
                expression,
                ..
            }) => {
                if defer_callable_selection(request, *expression, &mut deferred)?.is_cancelled() {
                    return Ok(SessionProgress::Cancelled);
                }
            }
            ExpressionCandidateSet::Operation(source) => {
                deferred.insert(source.expression());
            }
            ExpressionCandidateSet::NotApplicable(_) => {}
        }
    }

    calls.sort_unstable_by_key(|call| call.expression);

    Ok(SessionProgress::Complete(PreparedExpressions {
        calls,
        deferred,
        diagnostics,
    }))
}

fn defer_callable_selection<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    deferred: &mut BTreeSet<BoundExpressionId>,
) -> Result<SessionProgress<()>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let callee = match request.view().expression(expression) {
        Some(BoundExpression::Call(call)) => call.callee(),
        Some(BoundExpression::ErrorCall(call)) => call.callee(),
        _ => return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput),
    };

    deferred.insert(expression);

    defer_expression_tree(request, callee, deferred)
}

fn nested_callable_target<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    nested_callables: &[NestedCallableEvidence],
) -> Result<Option<bray_symbols::AnonymousCallableSymbolId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(BoundExpression::Call(call)) = request.view().expression(expression) else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    if !matches!(
        request.view().expression(call.callee()),
        Some(BoundExpression::AnonymousCallable(_))
    ) {
        return Ok(None);
    }

    let mut matching = nested_callables
        .iter()
        .filter(|evidence| evidence.expression() == call.callee());

    let Some(evidence) = matching.next() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    if matching.next().is_some() {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    Ok(Some(evidence.callable()))
}

fn defer_expression_tree<C>(
    request: CheckerUnitView<'_, C>,
    root: BoundExpressionId,
    deferred: &mut BTreeSet<BoundExpressionId>,
) -> Result<SessionProgress<()>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut pending = vec![root];

    while let Some(expression) = pending.pop() {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        if !deferred.insert(expression) {
            continue;
        }

        let Some(bound) = request.view().expression(expression) else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        pending.extend(bound.child_expressions());
    }

    Ok(SessionProgress::Complete(()))
}

fn add_candidate_expectations<C>(
    request: CheckerUnitView<'_, C>,
    types: &bray_bound_tree::CheckedExpressionTypes,
    prepared: &[PreparedCall],
    session: &mut ExpressionTypeSession<'_, C>,
) -> Result<SessionProgress<()>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for prepared_call in prepared {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        let Some(candidates) = materialize_call_candidates(request, types, prepared_call)? else {
            continue;
        };

        let Some(BoundExpression::Call(call)) = request.view().expression(prepared_call.expression)
        else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        let Some(candidates) =
            viable_candidates(request, types, prepared_call.expression, call, &candidates)?
        else {
            return Ok(SessionProgress::Cancelled);
        };

        if let Some(callable_type) = common_candidate_type(&candidates) {
            session.add_evidence(call.callee(), callable_type)?;
        }

        for (ordinal, argument) in call.arguments().iter().enumerate() {
            let Some(expected) =
                common_parameter_type(request, &candidates, call.arguments(), ordinal)?
            else {
                continue;
            };

            session.add_expectation(argument.expression(), expected)?;
        }
    }

    Ok(SessionProgress::Complete(()))
}

fn viable_candidates<'candidate, C>(
    request: CheckerUnitView<'_, C>,
    types: &bray_bound_tree::CheckedExpressionTypes,
    expression: BoundExpressionId,
    call: &bray_bound_tree::BoundCallExpression,
    candidates: &'candidate [CallableCandidate],
) -> Result<Option<Vec<&'candidate CallableCandidate>>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    // The probe borrows the prepared candidates while owning only the source call surface.
    let input = CallableSelectionRequest::new(
        expression,
        None,
        None,
        call.generic_arguments().iter().copied(),
        call.arguments().iter().cloned(),
        [],
    );

    let Some(indices) =
        crate::selection::viable_candidate_indices(request, types, &input, candidates)?
    else {
        return Ok(None);
    };

    let viable = indices
        .into_iter()
        .map(|index| &candidates[index])
        .collect();

    Ok(Some(viable))
}

fn common_candidate_type(candidates: &[&CallableCandidate]) -> Option<TypeId> {
    common_candidate_value(candidates, |candidate| Some(candidate.callable_type()))
}

fn common_parameter_type<C>(
    request: CheckerUnitView<'_, C>,
    candidates: &[&CallableCandidate],
    arguments: &[bray_bound_tree::BoundArgument],
    ordinal: usize,
) -> Result<Option<TypeId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut expected = None;

    for candidate in candidates {
        let callable = request
            .semantic_values()
            .type_data(candidate.callable_type())
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        let bray_symbols::TypeData::Callable(callable) = callable.as_ref() else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        let Some(mapping) =
            crate::selection::map_argument_parameter_indices(arguments, callable.parameters())
        else {
            return Ok(None);
        };

        let Some(parameter_index) = mapping.get(ordinal).copied() else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        let Some(parameter) = callable.parameters().get(parameter_index) else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        match expected {
            Some(expected) if expected != parameter.ty() => return Ok(None),
            Some(_) => {}
            None => expected = Some(parameter.ty()),
        }
    }

    Ok(expected)
}

fn common_candidate_value<T: Copy + Eq>(
    candidates: &[&CallableCandidate],
    mut value: impl FnMut(&CallableCandidate) -> Option<T>,
) -> Option<T> {
    let mut candidates = candidates.iter().copied();

    let first = value(candidates.next()?)?;

    candidates
        .all(|candidate| value(candidate) == Some(first))
        .then_some(first)
}

fn materialize_call_candidates<'prepared, C>(
    request: CheckerUnitView<'_, C>,
    types: &bray_bound_tree::CheckedExpressionTypes,
    prepared: &'prepared PreparedCall,
) -> Result<Option<Cow<'prepared, [CallableCandidate]>>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if prepared.values.is_empty() {
        return Ok(Some(Cow::Borrowed(&prepared.candidates)));
    }

    // Value materialization appends to a task-local candidate list without mutating preparation.
    let mut candidates = prepared.candidates.clone();

    let Some(BoundExpression::Call(call)) = request.view().expression(prepared.expression) else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let Some(callee_type) = types.expression(call.callee()) else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    if callee_type.is_recovered() {
        return Ok(None);
    }

    let data = request
        .semantic_values()
        .type_data(callee_type.ty())
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let bray_symbols::TypeData::Callable(callable) = data.as_ref() else {
        return Ok(Some(Cow::Owned(candidates)));
    };

    let result = call_result(request, callable, callable.result())?;

    let target = prepared.anonymous_target.map_or(
        BoundCallableTarget::Indirect(callee_type.ty()),
        BoundCallableTarget::Anonymous,
    );

    let resolution = BoundResolvedCall::new(target, [], result);

    // Each materialized value candidate owns its reusable resolved-call description.
    candidates.extend(prepared.values.iter().map(|template| {
        CallableCandidate::value(
            template.value(),
            resolution.clone(),
            callee_type.ty(),
            callable.result(),
            candidate_state(template.state()),
        )
    }));

    Ok(Some(Cow::Owned(
        CallableSelectionRequest::canonical_candidates(candidates),
    )))
}

pub(super) fn converge<C>(
    request: CheckerUnitView<'_, C>,
    prepared: &PreparedExpressions,
    session: &mut ExpressionTypeSession<'_, C>,
) -> Result<SessionProgress<()>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    loop {
        let revision = session.revision();

        if session.propagate()?.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        let types = session.preview();

        if add_candidate_expectations(request, &types, &prepared.calls, session)?.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        if session.revision() != revision {
            continue;
        }

        if apply_selected_call_evidence(request, &types, &prepared.calls, session)?.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        if session.revision() == revision {
            return Ok(SessionProgress::Complete(()));
        }
    }
}

fn apply_selected_call_evidence<C>(
    request: CheckerUnitView<'_, C>,
    types: &bray_bound_tree::CheckedExpressionTypes,
    prepared: &[PreparedCall],
    session: &mut ExpressionTypeSession<'_, C>,
) -> Result<SessionProgress<()>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for prepared_call in prepared {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        let Some(candidates) = materialize_call_candidates(request, types, prepared_call)? else {
            continue;
        };

        let outcome = select_prepared_call(request, types, prepared_call, &candidates);

        match outcome {
            CheckerOutcome::Complete(result) => {
                if let CandidateSelection::Selected(selection) = result.value() {
                    apply_selected_signature(
                        request,
                        prepared_call,
                        &candidates,
                        selection,
                        session,
                    )?;
                }
            }
            CheckerOutcome::Cancelled => return Ok(SessionProgress::Cancelled),
            CheckerOutcome::InfrastructureFailure(error) => return Err(error),
        }
    }

    Ok(SessionProgress::Complete(()))
}

fn apply_selected_signature<C>(
    request: CheckerUnitView<'_, C>,
    prepared: &PreparedCall,
    candidates: &[CallableCandidate],
    selection: &bray_bound_tree::SelectedCall,
    session: &mut ExpressionTypeSession<'_, C>,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(BoundExpression::Call(call)) = request.view().expression(prepared.expression) else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let Some(candidate) = candidates
        .iter()
        .find(|candidate| candidate.resolution() == selection.resolution())
    else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    session.add_evidence(call.callee(), candidate.callable_type())?;

    for argument in selection.arguments() {
        let SelectedArgument::Explicit {
            expression,
            parameter,
            ordinal,
        } = argument
        else {
            continue;
        };

        let callable = request
            .semantic_values()
            .type_data(candidate.callable_type())
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

        let bray_symbols::TypeData::Callable(callable) = callable.as_ref() else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        let Some(selected) = callable.parameters().get(*ordinal as usize) else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        if let Some(parameter) = parameter {
            let Some(signature) = candidate.declaration_signature() else {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
            };

            if signature
                .parameters()
                .get(*ordinal as usize)
                .is_none_or(|signature| signature.parameter() != *parameter)
            {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
            }
        }

        session.add_expectation(*expression, selected.ty())?;
    }

    session.add_evidence(prepared.expression, selection.resolution().result().ty())?;

    Ok(())
}

pub(super) fn final_selections<C>(
    request: CheckerUnitView<'_, C>,
    types: &bray_bound_tree::CheckedExpressionTypes,
    prepared: &PreparedExpressions,
) -> Result<Option<(Vec<SemanticSelectionEntry>, DiagnosticBag)>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut entries = Vec::new();
    let mut diagnostics = DiagnosticBag::new();

    for prepared_call in &prepared.calls {
        let Some(candidates) = materialize_call_candidates(request, types, prepared_call)? else {
            continue;
        };

        match select_prepared_call(request, types, prepared_call, &candidates) {
            CheckerOutcome::Complete(result) => {
                let (selection, selection_diagnostics) = result.into_parts();

                diagnostics.add_range(selection_diagnostics);

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
    candidates: &[CallableCandidate],
) -> CheckerOutcome<CandidateSelection<bray_bound_tree::SelectedCall>>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(BoundExpression::Call(call)) = request.view().expression(prepared.expression) else {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        );
    };

    // The request owns source argument records independently of the immutable bound unit.
    let arguments = call.arguments().iter().cloned();

    let input = CallableSelectionRequest::new(
        prepared.expression,
        None,
        None,
        call.generic_arguments().iter().copied(),
        arguments,
        [],
    );

    crate::selection::select_callable_candidates(request, types, &input, candidates)
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundUnitId;

    use super::prepare_calls;
    use crate::test_support::{
        TestCheckerContext, callable_entry, expression_unit, integer_literal_expression,
        push_expression,
    };
    use crate::{CheckerUnitView, ExpressionCandidateSet};

    #[test]
    fn candidate_preparation_observes_cancellation_between_expression_sets() {
        let (unit, expressions) = expression_unit(BoundUnitId::new(93), |tree, origin| {
            (0..32)
                .map(|_| push_expression(tree, integer_literal_expression(origin, None)))
                .collect()
        });

        let candidates = expressions
            .iter()
            .copied()
            .map(ExpressionCandidateSet::NotApplicable)
            .collect::<Vec<_>>();

        let context = TestCheckerContext::cancelling_after(8);
        let semantic_context = callable_entry(unit.key());

        let request = match CheckerUnitView::new(&unit, &semantic_context, &context) {
            Ok(request) => request,
            Err(error) => panic!("test checker unit view must be valid: {error:?}"),
        };

        let result = prepare_calls(request, &[], &candidates);

        assert!(matches!(
            result,
            Ok(crate::type_check::SessionProgress::Cancelled)
        ));
    }
}
