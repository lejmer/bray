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

        match session.propagate()? {
            SessionProgress::Complete(()) => {}
            SessionProgress::Cancelled => return Ok(SessionProgress::Cancelled),
        }

        let types = session.preview();

        match add_candidate_expectations(request, &types, &prepared.calls, session)? {
            SessionProgress::Complete(()) => {}
            SessionProgress::Cancelled => return Ok(SessionProgress::Cancelled),
        }

        if session.revision() != revision {
            continue;
        }

        let types = session.preview();

        match apply_selected_call_evidence(request, &types, &prepared.calls, session)? {
            SessionProgress::Complete(()) => {}
            SessionProgress::Cancelled => return Ok(SessionProgress::Cancelled),
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
