// rust-style: allow(module-too-large, reason = "expression candidate convergence shares one fixed-point preparation and selection state")

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

use bray_bound_tree::{
    BoundCallableTarget, BoundExpression, BoundExpressionId, BoundReferenceTarget,
    BoundResolvedCall, SelectedArgument, SelectionKind, SemanticSelection, SemanticSelectionEntry,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, CallableSignatureFact, CallableSymbolId, CheckedConstraintKind,
    GenericConstraintObligationKey, GenericConstraintsFact, GenericOwnerId, ProofOutcome,
    ReceiverMode, SymbolFactRequest, TypeId,
};

use super::built_in_operator::{self, PreparedBuiltInOperator};
use super::generic_inference::infer_call_generic_arguments;
use super::template::{
    TemplateResolution, call_result, candidate_state, resolve_declaration_candidate,
    resolve_declaration_candidate_with_arguments, resolve_generic_arguments,
    resolve_open_declaration_candidate, resolve_open_predicate_candidate,
    resolve_predicate_candidate, resolve_predicate_candidate_with_arguments,
};
use crate::type_check::{ExpressionTypeSession, SessionProgress};
use crate::{
    CallableCandidate, CallableCandidateTemplate, CallableCandidateTemplates,
    CallableSelectionRequest, CallableValueCandidateTemplate, CandidateAbsence, CandidateSelection,
    CheckerFactError, CheckerFactResult, CheckerInfrastructureError, CheckerOutcome,
    CheckerRequestContext, CheckerSemanticFactProvider, CheckerUnitView, ExpressionCandidateSet,
    NestedCallableEvidence,
};

struct PreparedCall {
    expression: BoundExpressionId,
    candidates: Vec<CallableCandidate>,
    generic_candidates: Vec<CallableCandidateTemplate>,
    values: Vec<CallableValueCandidateTemplate>,
    anonymous_target: Option<bray_symbols::AnonymousCallableSymbolId>,
}

pub(super) struct PreparedExpressions {
    calls: Vec<PreparedCall>,
    built_in_operators: Vec<PreparedBuiltInOperator>,
    member_targets: BTreeMap<BoundExpressionId, bray_bound_tree::MemberTarget>,
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

    pub(super) fn add_operation_selections(&mut self, selections: &[SemanticSelectionEntry]) {
        self.member_targets
            .extend(selections.iter().filter_map(|entry| {
                let SemanticSelection::Operation(bray_bound_tree::SelectedOperation::Member(
                    target,
                )) = entry.selection()
                else {
                    return None;
                };

                Some((entry.expression(), target.clone()))
            }));
    }

    pub(super) const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    pub(super) fn built_in_operators(&self) -> &[PreparedBuiltInOperator] {
        &self.built_in_operators
    }
}

pub(super) fn prepare_calls<C>(
    request: CheckerUnitView<'_, C>,
    nested_callables: &[NestedCallableEvidence],
    candidate_sets: &[ExpressionCandidateSet],
) -> Result<SessionProgress<PreparedExpressions>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<GenericConstraintsFact> + ?Sized,
{
    let mut calls = Vec::new();
    let mut built_in_operators = Vec::new();
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
                let mut generic_candidates = Vec::new();
                let mut values = Vec::new();
                let mut defer_call = false;

                // Deferred inference retains these Arc-backed templates after this borrowed input.
                for candidate in candidates.iter() {
                    if request.is_cancelled() {
                        return Ok(SessionProgress::Cancelled);
                    }

                    match candidate {
                        CallableCandidateTemplate::Declaration(candidate)
                            if candidate.generic_arguments().len()
                                < candidate.generic().parameters().len() =>
                        {
                            generic_candidates
                                .push(CallableCandidateTemplate::Declaration(candidate.clone()));
                        }
                        CallableCandidateTemplate::Declaration(candidate) => {
                            let materialized = resolve_declaration_candidate(
                                request,
                                candidate,
                                &mut diagnostics,
                            )?;

                            match materialized {
                                TemplateResolution::Resolved(candidate) => {
                                    match generic_constraint_outcome(
                                        request,
                                        &candidate,
                                        &mut diagnostics,
                                    )? {
                                        ProofOutcome::Proven => resolved.push(candidate),
                                        ProofOutcome::Disproven | ProofOutcome::Recovered => {}
                                        ProofOutcome::Unknown => defer_call = true,
                                    }
                                }
                                TemplateResolution::Unsupported => defer_call = true,
                            }
                        }
                        CallableCandidateTemplate::Predicate(candidate)
                            if candidate.generic_arguments().len()
                                < candidate.generic().parameters().len() =>
                        {
                            generic_candidates
                                .push(CallableCandidateTemplate::Predicate(candidate.clone()));
                        }
                        CallableCandidateTemplate::Predicate(candidate) => {
                            let materialized =
                                resolve_predicate_candidate(request, candidate, &mut diagnostics)?;

                            match materialized {
                                TemplateResolution::Resolved(candidate) => {
                                    match generic_constraint_outcome(
                                        request,
                                        &candidate,
                                        &mut diagnostics,
                                    )? {
                                        ProofOutcome::Proven => resolved.push(candidate),
                                        ProofOutcome::Disproven | ProofOutcome::Recovered => {}
                                        ProofOutcome::Unknown => defer_call = true,
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
                        generic_candidates,
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
                generic_candidates: Vec::new(),
                values: Vec::new(),
                anonymous_target: None,
            }),
            ExpressionCandidateSet::Callable(CallableCandidateTemplates::Absent {
                expression,
                reason: CandidateAbsence::UnavailableDeclarationFacts,
            }) if member_callee(request, *expression).is_some() => {
                let callee = member_callee(request, *expression)
                    .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

                calls.push(PreparedCall {
                    expression: *expression,
                    candidates: Vec::new(),
                    generic_candidates: Vec::new(),
                    values: vec![CallableValueCandidateTemplate::new(
                        bray_bound_tree::DeclaredValueTypeTerm::Expression(callee),
                        crate::CallableCandidateTemplateState::Visible,
                    )],
                    anonymous_target: None,
                });
            }
            ExpressionCandidateSet::Callable(CallableCandidateTemplates::Absent {
                expression,
                ..
            }) => {
                if defer_callable_selection(request, *expression, &mut deferred)?.is_cancelled() {
                    return Ok(SessionProgress::Cancelled);
                }
            }
            ExpressionCandidateSet::Operation(source) => {
                if source.kind() == SelectionKind::Operator
                    && let Some(operator) =
                        PreparedBuiltInOperator::for_expression(source.expression(), expression)
                {
                    built_in_operators.push(operator);
                } else {
                    deferred.insert(source.expression());
                }
            }
            ExpressionCandidateSet::NotApplicable(_) => {}
        }
    }

    calls.sort_unstable_by_key(|call| call.expression);
    built_in_operators.sort_unstable_by_key(|operation| operation.expression());

    Ok(SessionProgress::Complete(PreparedExpressions {
        calls,
        built_in_operators,
        member_targets: BTreeMap::new(),
        deferred,
        diagnostics,
    }))
}

fn member_callee<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
) -> Option<BoundExpressionId>
where
    C: CheckerRequestContext + ?Sized,
{
    let BoundExpression::Call(call) = request.view().expression(expression)? else {
        return None;
    };

    matches!(
        request.view().expression(call.callee()),
        Some(BoundExpression::MemberAccess(_) | BoundExpression::TraitQualifiedMember(_))
    )
    .then_some(call.callee())
}

fn generic_constraint_outcome<C>(
    request: CheckerUnitView<'_, C>,
    candidate: &CallableCandidate,
    diagnostics: &mut DiagnosticBag,
) -> Result<ProofOutcome, CheckerInfrastructureError>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<GenericConstraintsFact> + ?Sized,
{
    if candidate.generic_constraints().is_empty() {
        return Ok(ProofOutcome::Proven);
    }

    let Some(substitution_id) = candidate.generic_substitution() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let substitution = request
        .semantic_values()
        .generic_substitution_data(substitution_id)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let obligation = GenericConstraintObligationKey::new(substitution.owner(), substitution_id);

    match active_constraints_prove(request, obligation, diagnostics) {
        Ok(true) => return Ok(ProofOutcome::Proven),
        Ok(false) => {}
        Err(CheckerFactError::Cancelled) => return Ok(ProofOutcome::Unknown),
        Err(CheckerFactError::Infrastructure(error)) => return Err(error),
    }

    let result = match request.generic_constraints(obligation) {
        Ok(result) => result,
        Err(crate::CheckerFactError::Cancelled) => return Ok(ProofOutcome::Unknown),
        Err(crate::CheckerFactError::Infrastructure(error)) => return Err(error),
    };

    diagnostics.extend(result.diagnostics().iter().cloned());

    Ok(*result.value())
}

fn active_constraints_prove<C>(
    request: CheckerUnitView<'_, C>,
    obligation: GenericConstraintObligationKey,
    diagnostics: &mut DiagnosticBag,
) -> CheckerFactResult<bool>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<GenericConstraintsFact> + ?Sized,
{
    let required = request.symbol_fact(SymbolFactRequest::<GenericConstraintsFact>::new(
        obligation.owner(),
    ))?;

    if required.diagnostics().has_errors() {
        diagnostics.extend(required.diagnostics().iter().cloned());

        return Ok(false);
    }

    let mut active = BTreeSet::new();

    let Some(mut owner) = request
        .containing_callable()
        .map(CallableSymbolId::into_any)
    else {
        return Ok(false);
    };

    loop {
        if let Some(generic_owner) = GenericOwnerId::try_new(owner) {
            let constraints = request.symbol_fact(
                SymbolFactRequest::<GenericConstraintsFact>::new(generic_owner),
            )?;

            if constraints.diagnostics().has_errors() {
                diagnostics.extend(constraints.diagnostics().iter().cloned());
            } else {
                active.extend(
                    constraints
                        .value()
                        .constraints()
                        .iter()
                        .filter_map(|constraint| match constraint.kind() {
                            CheckedConstraintKind::TraitSatisfaction {
                                subject,
                                application,
                            } => Some((subject, application)),
                            CheckedConstraintKind::Predicate(_)
                            | CheckedConstraintKind::TypeEquality { .. } => None,
                        }),
                );
            }
        }

        let Some(container) = request.symbols().containing_symbol(owner) else {
            break;
        };

        owner = container;
    }

    for constraint in required.value().constraints() {
        let CheckedConstraintKind::TraitSatisfaction {
            subject,
            application,
        } = constraint.kind()
        else {
            return Ok(false);
        };

        let subject = request
            .semantic_values()
            .substitute_type(subject, obligation.substitution())
            .map_err(|_| {
                CheckerFactError::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                )
            })?;

        let application = request
            .semantic_values()
            .substitute_trait_application(application, obligation.substitution())
            .map_err(|_| {
                CheckerFactError::Infrastructure(
                    CheckerInfrastructureError::SemanticValueUnavailable,
                )
            })?;

        if active.contains(&(subject, application)) {
            continue;
        }

        let built_in =
            crate::built_in_trait_constraint_outcome(request.context(), subject, application)
                .map_err(CheckerFactError::Infrastructure)?;

        if built_in != Some(ProofOutcome::Proven) {
            return Ok(false);
        }
    }

    Ok(true)
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
    prepared: &PreparedExpressions,
    session: &mut ExpressionTypeSession<'_, C>,
) -> Result<SessionProgress<()>, CheckerInfrastructureError>
where
    C: CheckerRequestContext
        + CheckerSemanticFactProvider<CallableSignatureFact>
        + CheckerSemanticFactProvider<GenericConstraintsFact>
        + ?Sized,
{
    for prepared_call in &prepared.calls {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        let Some(materialized) =
            materialize_call_candidates(request, types, prepared_call, &prepared.member_targets)?
        else {
            continue;
        };

        let candidates = &materialized.candidates;

        let Some(BoundExpression::Call(call)) = request.view().expression(prepared_call.expression)
        else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        let available = candidates.iter().collect::<Vec<_>>();

        for (ordinal, argument) in call.arguments().iter().enumerate() {
            if !expected_type_directed_variant(request, argument.expression()) {
                continue;
            }

            let Some(expected) =
                common_parameter_type(request, &available, call.arguments(), ordinal)?
            else {
                continue;
            };

            session.add_evidence(argument.expression(), expected)?;
        }

        let Some(candidates) = viable_candidates(
            request,
            types,
            prepared_call,
            &prepared.member_targets,
            candidates,
        )?
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
    prepared: &PreparedCall,
    member_targets: &BTreeMap<BoundExpressionId, bray_bound_tree::MemberTarget>,
    candidates: &'candidate [CallableCandidate],
) -> Result<Option<Vec<&'candidate CallableCandidate>>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    // The probe borrows the prepared candidates while owning only the source call surface.
    let input = match call_selection_request(request, types, prepared, member_targets, []) {
        Ok(input) => input,
        Err(CheckerFactError::Cancelled) => return Ok(None),
        Err(CheckerFactError::Infrastructure(error)) => return Err(error),
    };

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

fn expected_type_directed_variant<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    match request.view().expression(expression) {
        Some(BoundExpression::LeadingDotVariant(_) | BoundExpression::UnqualifiedVariant(_)) => {
            true
        }
        Some(BoundExpression::Call(call)) => matches!(
            request.view().expression(call.callee()),
            Some(BoundExpression::LeadingDotVariant(_) | BoundExpression::UnqualifiedVariant(_))
        ),
        _ => false,
    }
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

struct MaterializedCallCandidates<'prepared> {
    candidates: Cow<'prepared, [CallableCandidate]>,
    diagnostics: DiagnosticBag,
}

fn materialize_call_candidates<'prepared, C>(
    request: CheckerUnitView<'_, C>,
    types: &bray_bound_tree::CheckedExpressionTypes,
    prepared: &'prepared PreparedCall,
    member_targets: &BTreeMap<BoundExpressionId, bray_bound_tree::MemberTarget>,
) -> Result<Option<MaterializedCallCandidates<'prepared>>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<GenericConstraintsFact> + ?Sized,
{
    if prepared.generic_candidates.is_empty() && prepared.values.is_empty() {
        return Ok(Some(MaterializedCallCandidates {
            candidates: Cow::Borrowed(&prepared.candidates),
            diagnostics: DiagnosticBag::new(),
        }));
    }

    let mut candidates = prepared.candidates.clone();
    let mut diagnostics = DiagnosticBag::new();

    for template in &prepared.generic_candidates {
        let explicit = match template {
            CallableCandidateTemplate::Declaration(template) => {
                resolve_generic_arguments(request, template.generic_arguments(), &mut diagnostics)?
            }
            CallableCandidateTemplate::Predicate(template) => {
                resolve_generic_arguments(request, template.generic_arguments(), &mut diagnostics)?
            }
            CallableCandidateTemplate::Value(_) => {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
            }
        };

        let TemplateResolution::Resolved(mut explicit) = explicit else {
            continue;
        };

        let (parameters, open) = match template {
            CallableCandidateTemplate::Declaration(template) => (
                template.generic().parameters(),
                resolve_open_declaration_candidate(request, template, &explicit, &mut diagnostics)?,
            ),
            CallableCandidateTemplate::Predicate(template) => (
                template.generic().parameters(),
                resolve_open_predicate_candidate(request, template, &explicit, &mut diagnostics)?,
            ),
            CallableCandidateTemplate::Value(_) => {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
            }
        };

        let Some(parameters) = parameters.get(explicit.len()..) else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        let Some(inferred) =
            infer_call_generic_arguments(request, prepared.expression, &open, parameters, types)?
        else {
            continue;
        };

        explicit.extend(inferred);

        let resolved = match template {
            CallableCandidateTemplate::Declaration(template) => {
                resolve_declaration_candidate_with_arguments(
                    request,
                    template,
                    explicit,
                    &mut diagnostics,
                )?
            }
            CallableCandidateTemplate::Predicate(template) => {
                resolve_predicate_candidate_with_arguments(
                    request,
                    template,
                    explicit,
                    &mut diagnostics,
                )?
            }
            CallableCandidateTemplate::Value(_) => {
                return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
            }
        };

        let TemplateResolution::Resolved(candidate) = resolved else {
            continue;
        };

        if generic_constraint_outcome(request, &candidate, &mut diagnostics)?
            == ProofOutcome::Proven
        {
            candidates.push(candidate);
        }
    }

    if prepared.values.is_empty() {
        return Ok(Some(MaterializedCallCandidates {
            candidates: Cow::Owned(CallableSelectionRequest::canonical_candidates(candidates)),
            diagnostics,
        }));
    }

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
        return Ok(Some(MaterializedCallCandidates {
            candidates: Cow::Owned(candidates),
            diagnostics,
        }));
    };

    let result = call_result(request, callable, callable.result())?;

    let member = member_targets.get(&call.callee());

    let target = match prepared.anonymous_target {
        Some(anonymous) => BoundCallableTarget::Anonymous(anonymous),
        None => member
            .and_then(bray_bound_tree::MemberTarget::callable_instance)
            .map(BoundCallableTarget::Declaration)
            .unwrap_or(BoundCallableTarget::Indirect(callee_type.ty())),
    };

    let witnesses = member
        .into_iter()
        .flat_map(bray_bound_tree::MemberTarget::witnesses)
        .map(|witness| witness.witness());

    let mut resolution = BoundResolvedCall::new(target, witnesses, result);

    if let Some(dispatch) = member.and_then(bray_bound_tree::MemberTarget::trait_dispatch) {
        resolution = resolution.with_trait_dispatch(dispatch);
    }

    // Each materialized value candidate owns its reusable resolved-call description.
    candidates.extend(prepared.values.iter().map(|template| {
        let state = candidate_state(template.state());

        let candidate = match member.and_then(bray_bound_tree::MemberTarget::callable_signature) {
            Some(signature) => CallableCandidate::value(
                template.value(),
                resolution.clone(),
                callee_type.ty(),
                callable.result(),
                state,
            )
            .with_declaration_signature(signature.clone()),
            None => CallableCandidate::value(
                template.value(),
                resolution.clone(),
                callee_type.ty(),
                callable.result(),
                state,
            ),
        };

        candidate.with_implementation_selections(
            member
                .into_iter()
                .flat_map(bray_bound_tree::MemberTarget::witnesses)
                .map(|witness| {
                    crate::ImplementationSelectionEvidence::new(
                        witness.requirement(),
                        bray_symbols::ImplementationSelection::Selected(witness.witness()),
                    )
                }),
        )
    }));

    Ok(Some(MaterializedCallCandidates {
        candidates: Cow::Owned(CallableSelectionRequest::canonical_candidates(candidates)),
        diagnostics,
    }))
}

pub(super) fn converge<C>(
    request: CheckerUnitView<'_, C>,
    prepared: &PreparedExpressions,
    session: &mut ExpressionTypeSession<'_, C>,
) -> Result<SessionProgress<()>, CheckerInfrastructureError>
where
    C: CheckerRequestContext
        + CheckerSemanticFactProvider<CallableSignatureFact>
        + CheckerSemanticFactProvider<GenericConstraintsFact>
        + ?Sized,
{
    loop {
        let revision = session.revision();

        if session.propagate()?.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        built_in_operator::apply_evidence(request, prepared.built_in_operators(), session)?;

        let types = session.preview();

        built_in_operator::apply_operand_expectations(
            request,
            &types,
            prepared.built_in_operators(),
            session,
        )?;

        if add_candidate_expectations(request, &types, prepared, session)?.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        if session.revision() != revision {
            continue;
        }

        if apply_selected_call_evidence(request, &types, prepared, session)?.is_cancelled() {
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
    prepared: &PreparedExpressions,
    session: &mut ExpressionTypeSession<'_, C>,
) -> Result<SessionProgress<()>, CheckerInfrastructureError>
where
    C: CheckerRequestContext
        + CheckerSemanticFactProvider<CallableSignatureFact>
        + CheckerSemanticFactProvider<GenericConstraintsFact>
        + ?Sized,
{
    for prepared_call in &prepared.calls {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        let Some(materialized) =
            materialize_call_candidates(request, types, prepared_call, &prepared.member_targets)?
        else {
            continue;
        };

        let candidates = &materialized.candidates;

        let outcome = select_prepared_call(
            request,
            types,
            prepared_call,
            &prepared.member_targets,
            candidates,
        );

        match outcome {
            CheckerOutcome::Complete(result) => {
                if let CandidateSelection::Selected(selection) = result.value() {
                    apply_selected_signature(
                        request,
                        prepared_call,
                        candidates,
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
            ..
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
    C: CheckerRequestContext
        + CheckerSemanticFactProvider<CallableSignatureFact>
        + CheckerSemanticFactProvider<GenericConstraintsFact>
        + ?Sized,
{
    let mut entries = Vec::new();
    let mut diagnostics = DiagnosticBag::new();

    for prepared_call in &prepared.calls {
        let Some(materialized) =
            materialize_call_candidates(request, types, prepared_call, &prepared.member_targets)?
        else {
            continue;
        };

        diagnostics.add_range(materialized.diagnostics);

        match select_prepared_call(
            request,
            types,
            prepared_call,
            &prepared.member_targets,
            &materialized.candidates,
        ) {
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

    entries.extend(built_in_operator::selections(
        request,
        types,
        prepared.built_in_operators(),
    )?);

    Ok(Some((entries, diagnostics)))
}

fn select_prepared_call<C>(
    request: CheckerUnitView<'_, C>,
    types: &bray_bound_tree::CheckedExpressionTypes,
    prepared: &PreparedCall,
    member_targets: &BTreeMap<BoundExpressionId, bray_bound_tree::MemberTarget>,
    candidates: &[CallableCandidate],
) -> CheckerOutcome<CandidateSelection<bray_bound_tree::SelectedCall>>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    let input = match call_selection_request(request, types, prepared, member_targets, []) {
        Ok(input) => input,
        Err(CheckerFactError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(CheckerFactError::Infrastructure(error)) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
    };

    crate::selection::select_callable_candidates(request, types, &input, candidates)
}

fn call_selection_request<C>(
    request: CheckerUnitView<'_, C>,
    types: &bray_bound_tree::CheckedExpressionTypes,
    prepared: &PreparedCall,
    member_targets: &BTreeMap<BoundExpressionId, bray_bound_tree::MemberTarget>,
    candidates: impl IntoIterator<Item = CallableCandidate>,
) -> CheckerFactResult<CallableSelectionRequest>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    let Some(BoundExpression::Call(call)) = request.view().expression(prepared.expression) else {
        return Err(CheckerFactError::Infrastructure(
            CheckerInfrastructureError::InvalidSemanticSelectionInput,
        ));
    };

    let member = member_targets.get(&call.callee()).cloned();

    let receiver = member
        .as_ref()
        .and_then(|_| match request.view().expression(call.callee()) {
            Some(BoundExpression::MemberAccess(member)) => Some(member.receiver()),
            Some(BoundExpression::TraitQualifiedMember(member)) => Some(member.receiver()),
            _ => None,
        });

    let receiver = receiver
        .map(|receiver| {
            receiver_capability(request, types, member_targets, receiver)
                .map(|capability| crate::ReceiverSelection::new(receiver, capability))
        })
        .transpose()?;

    Ok(CallableSelectionRequest::new(
        prepared.expression,
        member,
        receiver,
        call.generic_arguments().iter().copied(),
        call.arguments().iter().cloned(),
        candidates,
    ))
}

fn receiver_capability<C>(
    request: CheckerUnitView<'_, C>,
    types: &bray_bound_tree::CheckedExpressionTypes,
    member_targets: &BTreeMap<BoundExpressionId, bray_bound_tree::MemberTarget>,
    receiver: BoundExpressionId,
) -> CheckerFactResult<crate::ReceiverCapability>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    let mut expression = receiver;
    let mut mutable_projection = true;

    loop {
        let ty = types.expression(expression).ok_or_else(|| {
            CheckerFactError::Infrastructure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            )
        })?;

        let data = request.semantic_values().type_data(ty.ty()).map_err(|_| {
            CheckerFactError::Infrastructure(CheckerInfrastructureError::SemanticValueUnavailable)
        })?;

        if let bray_symbols::TypeData::Borrow { kind, .. } = data.as_ref() {
            return Ok(match kind {
                bray_symbols::BorrowKind::Shared => crate::ReceiverCapability::Shared,
                bray_symbols::BorrowKind::Mutable if mutable_projection => {
                    crate::ReceiverCapability::Mutable
                }
                bray_symbols::BorrowKind::Mutable => crate::ReceiverCapability::Shared,
            });
        }

        match request.view().expression(expression) {
            Some(BoundExpression::MemberAccess(member)) => {
                let target = member_targets.get(&expression).ok_or_else(|| {
                    CheckerFactError::Infrastructure(
                        CheckerInfrastructureError::InvalidSemanticSelectionInput,
                    )
                })?;

                mutable_projection &= member_allows_mutation(request, target.member());
                expression = member.receiver();
            }
            Some(BoundExpression::Name(name)) => {
                let capability = name_receiver_capability(request, name.target())?;

                return Ok(projected_receiver_capability(
                    capability,
                    mutable_projection,
                ));
            }
            Some(_) => return Ok(crate::ReceiverCapability::Owned),
            None => {
                return Err(CheckerFactError::Infrastructure(
                    CheckerInfrastructureError::InvalidSemanticSelectionInput,
                ));
            }
        }
    }
}

fn name_receiver_capability<C>(
    request: CheckerUnitView<'_, C>,
    target: BoundReferenceTarget,
) -> CheckerFactResult<crate::ReceiverCapability>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    match target {
        BoundReferenceTarget::Local(bray_symbols::AnyLocalSymbolId::Binding(binding)) => {
            let is_mutable =
                request.unit().tree().patterns().any(|(_, pattern)| {
                    pattern.bindings().contains(&binding) && pattern.is_mutable()
                });

            Ok(if is_mutable {
                crate::ReceiverCapability::OwnedMutable
            } else {
                crate::ReceiverCapability::Owned
            })
        }
        BoundReferenceTarget::Surface(AnySymbolId::ReceiverParameter(parameter)) => {
            let receiver = request
                .symbols()
                .receiver_parameter(parameter)
                .ok_or_else(|| {
                    CheckerFactError::Infrastructure(
                        CheckerInfrastructureError::InvalidSemanticSelectionInput,
                    )
                })?;

            let signature = request.symbol_fact(
                SymbolFactRequest::<CallableSignatureFact>::new(receiver.owner()),
            )?;

            let mode = signature
                .value()
                .receiver()
                .filter(|signature| signature.parameter() == parameter)
                .map(bray_symbols::ReceiverParameterSignature::mode)
                .ok_or_else(|| {
                    CheckerFactError::Infrastructure(
                        CheckerInfrastructureError::InvalidSemanticSelectionInput,
                    )
                })?;

            Ok(receiver_mode_capability(mode))
        }
        BoundReferenceTarget::Local(_) | BoundReferenceTarget::Surface(_) => {
            Ok(crate::ReceiverCapability::Owned)
        }
    }
}

fn member_allows_mutation<C>(request: CheckerUnitView<'_, C>, member: AnySymbolId) -> bool
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    match member {
        AnySymbolId::StructField(field) => request
            .symbols()
            .struct_field(field)
            .is_some_and(bray_symbols::StructFieldSymbol::allows_mutation),
        AnySymbolId::UnionPayloadField(field) => request
            .symbols()
            .union_payload_field(field)
            .is_some_and(bray_symbols::UnionPayloadFieldSymbol::allows_mutation),
        _ => false,
    }
}

const fn receiver_mode_capability(mode: ReceiverMode) -> crate::ReceiverCapability {
    match mode {
        ReceiverMode::Shared => crate::ReceiverCapability::Shared,
        ReceiverMode::Mutable => crate::ReceiverCapability::Mutable,
        ReceiverMode::Consuming => crate::ReceiverCapability::Owned,
        ReceiverMode::ConsumingMutable => crate::ReceiverCapability::OwnedMutable,
    }
}

const fn projected_receiver_capability(
    capability: crate::ReceiverCapability,
    allows_mutation: bool,
) -> crate::ReceiverCapability {
    match (capability, allows_mutation) {
        (crate::ReceiverCapability::Mutable, false) => crate::ReceiverCapability::Shared,
        (crate::ReceiverCapability::OwnedMutable, false) => crate::ReceiverCapability::Owned,
        (capability, _) => capability,
    }
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
