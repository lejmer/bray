use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, SelectedArgument, SemanticSelection, SemanticSelectionEntry,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::TypeId;

use super::template::{TemplateResolution, resolve_declaration_candidate};
use crate::type_check::{ExpressionTypeSession, SessionProgress};
use crate::{
    CallableCandidate, CallableCandidateTemplate, CallableCandidateTemplates,
    CallableSelectionRequest, CandidateAbsence, CandidateSelection, CheckerInfrastructureError,
    CheckerOutcome, CheckerRequestContext, CheckerUnitView, ExpressionCandidateSet,
};

struct PreparedCall {
    expression: BoundExpressionId,
    candidates: Vec<CallableCandidate>,
}

pub(super) struct PreparedExpressions {
    calls: Vec<PreparedCall>,
    deferred: BTreeSet<BoundExpressionId>,
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
}

pub(super) fn prepare_calls<C>(
    request: CheckerUnitView<'_, C>,
    candidate_sets: &[ExpressionCandidateSet],
) -> Result<SessionProgress<PreparedExpressions>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut calls = Vec::new();
    let mut deferred = BTreeSet::new();

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
                let mut defer_call = false;

                for candidate in candidates.iter() {
                    if request.is_cancelled() {
                        return Ok(SessionProgress::Cancelled);
                    }

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
                    if defer_callable_selection(request, *expression, &mut deferred)?.is_cancelled()
                    {
                        return Ok(SessionProgress::Cancelled);
                    }
                } else {
                    calls.push(PreparedCall {
                        expression: *expression,
                        candidates: CallableSelectionRequest::canonical_candidates(resolved),
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
                if defer_callable_selection(request, *expression, &mut deferred)?.is_cancelled() {
                    return Ok(SessionProgress::Cancelled);
                }
            }
            ExpressionCandidateSet::Unsupported { expression, .. } => {
                deferred.insert(*expression);
            }
            ExpressionCandidateSet::NotApplicable(_) => {}
        }
    }

    calls.sort_unstable_by_key(|call| call.expression);

    Ok(SessionProgress::Complete(PreparedExpressions {
        calls,
        deferred,
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

        let Some(BoundExpression::Call(call)) = request.view().expression(prepared_call.expression)
        else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        if call
            .arguments()
            .iter()
            .any(|argument| argument.name().is_some())
        {
            // TODO(BRA-242): Share exact named-argument mapping with callable selection.
            continue;
        }

        let Some(candidates) = viable_candidates(
            request,
            types,
            prepared_call.expression,
            call,
            &prepared_call.candidates,
        )?
        else {
            return Ok(SessionProgress::Cancelled);
        };

        if let Some(callable_type) = common_candidate_type(&candidates) {
            session.add_evidence(call.callee(), callable_type)?;
        }

        for (ordinal, argument) in call.arguments().iter().enumerate() {
            let Some(expected) = common_parameter_type(&candidates, ordinal) else {
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
    common_candidate_value(candidates, |candidate| {
        Some(candidate.signature().callable_type())
    })
}

fn common_parameter_type(candidates: &[&CallableCandidate], ordinal: usize) -> Option<TypeId> {
    common_candidate_value(candidates, |candidate| {
        candidate
            .signature()
            .parameters()
            .get(ordinal)
            .map(|parameter| parameter.ty())
    })
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

        let outcome = select_prepared_call(request, types, prepared_call);

        match outcome {
            CheckerOutcome::Complete(result) => {
                if let CandidateSelection::Selected(selection) = result.value() {
                    apply_selected_signature(request, prepared_call, selection, session)?;
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
    selection: &bray_bound_tree::SelectedCall,
    session: &mut ExpressionTypeSession<'_, C>,
) -> Result<(), CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(BoundExpression::Call(call)) = request.view().expression(prepared.expression) else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let Some(candidate) = prepared
        .candidates
        .iter()
        .find(|candidate| candidate.resolution() == selection.resolution())
    else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    session.add_evidence(call.callee(), candidate.signature().callable_type())?;

    for argument in selection.arguments() {
        let SelectedArgument::Explicit {
            expression,
            parameter,
        } = argument
        else {
            continue;
        };

        let Some(parameter) = candidate
            .signature()
            .parameters()
            .iter()
            .find(|candidate| candidate.parameter() == *parameter)
        else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        session.add_expectation(*expression, parameter.ty())?;
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
        match select_prepared_call(request, types, prepared_call) {
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

    crate::selection::select_callable_candidates(request, types, &input, &prepared.candidates)
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

        let result = prepare_calls(request, &candidates);

        assert!(matches!(
            result,
            Ok(crate::type_check::SessionProgress::Cancelled)
        ));
    }
}
