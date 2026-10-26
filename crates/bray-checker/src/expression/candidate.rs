// rust-style: allow(module-too-large, reason = "expression candidate convergence shares one fixed-point preparation and selection state")

use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
};

use bray_bound_tree::{
    BoundCallableTarget, BoundExpression, BoundExpressionId, BoundMemberSelector,
    BoundReferenceTarget, BoundResolvedCall, SelectedArgument, SelectionKind, SemanticSelection,
    SemanticSelectionEntry,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, CallableParameterMode, CallableSignatureQuery, CheckedConstraintKind,
    GenericConstraintObligationKey, GenericConstraintsQuery, GenericOwnerId,
    GenericTypeConstraintsQuery, ImplementationRequirementKey, ImplementationSelection,
    ProofOutcome, ReceiverMode, SymbolQueryRequest, TypeData, TypeExpressionTemplate, TypeId,
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
    CheckerOutcome, CheckerQueryError, CheckerQueryResult, CheckerRequestContext,
    CheckerSemanticQueryProvider, CheckerUnitView, ExpressionCandidateSet, NestedCallableEvidence,
};

fn invalid_selection_input<Upstream>() -> CheckerQueryError<Upstream> {
    panic!(
        "Semantic-selection inputs do not describe the requested bound unit or operation category. in invalid_selection_input"
    )
}

struct PreparedCall {
    expression: BoundExpressionId,
    candidates: Vec<CallableCandidate>,
    generic_candidates: Vec<CallableCandidateTemplate>,
    values: Vec<CallableValueCandidateTemplate>,
    anonymous_target: Option<(
        bray_symbols::AnonymousCallableSymbolId,
        bray_bound_tree::BoundUnitKey,
    )>,
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
) -> Result<SessionProgress<PreparedExpressions>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<GenericConstraintsQuery>
        + CheckerSemanticQueryProvider<GenericTypeConstraintsQuery>
        + ?Sized,
{
    let immutable_initializers = request.unit().tree().immutable_initializers();
    let mut calls = Vec::new();
    let mut built_in_operators = Vec::new();
    let mut deferred = BTreeSet::new();
    let mut all_diagnostics = DiagnosticBag::new();

    for candidate_set in candidate_sets {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        let Some(expression) = request.view().expression(candidate_set.expression()) else {
            panic!(
                "Semantic-selection inputs do not describe the requested bound unit or operation category. in prepare_calls"
            );
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

        let mut diagnostics = DiagnosticBag::new();

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
                            let materialized = match dependency_progress(
                                resolve_declaration_candidate(request, candidate, &mut diagnostics),
                            )? {
                                SessionProgress::Complete(materialized) => materialized,
                                SessionProgress::Cancelled => {
                                    return Ok(SessionProgress::Cancelled);
                                }
                            };

                            defer_call |= prepare_resolved_candidate(
                                request,
                                materialized,
                                &mut resolved,
                                &mut diagnostics,
                            )?;
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

                            defer_call |= prepare_resolved_candidate(
                                request,
                                materialized,
                                &mut resolved,
                                &mut diagnostics,
                            )?;
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
                            &immutable_initializers,
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
                reason: CandidateAbsence::UnavailableDeclarationSemantics,
            }) if member_callee(request, *expression).is_some() => {
                let callee = member_callee(request, *expression).unwrap_or_else(|| {
                    panic!(
                        "prepare_calls requires selected member callee, expression: {expression:?}"
                    )
                });

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
                } else if !matches!(expression, BoundExpression::BoxConstruction(_)) {
                    deferred.insert(source.expression());
                }
            }
            ExpressionCandidateSet::NotApplicable(_) => {}
        }

        crate::diagnostic::anchor_diagnostics(
            &mut diagnostics,
            bray_diagnostics::DiagnosticLabelKind::SelectionFailure,
            || {
                Some(crate::diagnostic::expression_span(
                    request,
                    candidate_set.expression(),
                ))
            },
        );

        all_diagnostics.extend(diagnostics);
    }

    calls.sort_unstable_by_key(|call| call.expression);
    built_in_operators.sort_unstable_by_key(|operation| operation.expression());

    Ok(SessionProgress::Complete(PreparedExpressions {
        calls,
        built_in_operators,
        member_targets: BTreeMap::new(),
        deferred,
        diagnostics: all_diagnostics,
    }))
}

fn prepare_resolved_candidate<C>(
    request: CheckerUnitView<'_, C>,
    materialized: TemplateResolution<CallableCandidate>,
    resolved: &mut Vec<CallableCandidate>,
    diagnostics: &mut DiagnosticBag,
) -> Result<bool, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<GenericConstraintsQuery>
        + CheckerSemanticQueryProvider<GenericTypeConstraintsQuery>
        + ?Sized,
{
    let TemplateResolution::Resolved(candidate) = materialized else {
        return Ok(true);
    };

    match candidate_constraint_outcome(request, &candidate, diagnostics)? {
        ProofOutcome::Proven => {
            let selections =
                generic_constraint_implementation_selections(request, &candidate, diagnostics)?;

            resolved.push(candidate.with_implementation_selections(selections));

            Ok(false)
        }
        ProofOutcome::Disproven | ProofOutcome::Recovered => Ok(false),
        ProofOutcome::Unknown => Ok(true),
    }
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

fn candidate_constraint_outcome<C>(
    request: CheckerUnitView<'_, C>,
    candidate: &CallableCandidate,
    diagnostics: &mut DiagnosticBag,
) -> Result<ProofOutcome, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<GenericConstraintsQuery>
        + CheckerSemanticQueryProvider<GenericTypeConstraintsQuery>
        + ?Sized,
{
    if candidate.generic_constraints().is_empty() {
        return Ok(ProofOutcome::Proven);
    }

    let Some(substitution_id) = candidate.generic_substitution() else {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in generic_constraint_outcome"
        );
    };

    let substitution = request
        .semantic_values()
        .generic_substitution_data(substitution_id);

    let obligation = GenericConstraintObligationKey::new(substitution.owner(), substitution_id);

    generic_constraint_outcome(request, obligation, diagnostics)
}

/// Checks static obligations using established lexical premises and cached instance evaluation.
pub fn generic_constraint_outcome<C>(
    request: CheckerUnitView<'_, C>,
    obligation: GenericConstraintObligationKey,
    diagnostics: &mut DiagnosticBag,
) -> CheckerQueryResult<ProofOutcome, C::UpstreamError>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<GenericConstraintsQuery>
        + CheckerSemanticQueryProvider<GenericTypeConstraintsQuery>
        + ?Sized,
{
    if active_constraints_prove(request, obligation, diagnostics)? {
        return Ok(ProofOutcome::Proven);
    }

    let result = request.generic_constraints(obligation)?;

    diagnostics.extend(result.diagnostics().iter().cloned());

    Ok(*result.value())
}

fn active_constraints_prove<C>(
    request: CheckerUnitView<'_, C>,
    obligation: GenericConstraintObligationKey,
    diagnostics: &mut DiagnosticBag,
) -> CheckerQueryResult<bool, C::UpstreamError>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<GenericConstraintsQuery>
        + CheckerSemanticQueryProvider<GenericTypeConstraintsQuery>
        + ?Sized,
{
    let required = request.resolve_symbol_query(
        SymbolQueryRequest::<GenericConstraintsQuery>::new(obligation.owner()),
    )?;

    if required.diagnostics().has_errors() {
        diagnostics.extend(required.diagnostics().iter().cloned());

        return Ok(false);
    }

    if required.value().constraints().is_empty() {
        return Ok(true);
    }

    let mut normalizer = crate::TypeNormalizer::new(request.context(), None);
    let active = active_generic_constraints(request, diagnostics, &mut normalizer)?;
    let evidence = trait_constraint_evidence(&active);

    for constraint in required.value().constraints() {
        let (subject, application) = match constraint.kind() {
            CheckedConstraintKind::TraitSatisfaction {
                subject,
                application,
            } => (subject, application),
            CheckedConstraintKind::Predicate(predicate) => {
                let Some(condition) = predicate.condition() else {
                    return Ok(false);
                };

                let condition = request
                    .semantic_values()
                    .substitute_constant_term(condition, obligation.substitution())
                    .expect("checked generic predicate substitution belongs to its semantic store");

                let predicate = normalizer.normalize_predicate(
                    predicate.with_condition(Some(condition), predicate.domain()),
                )?;

                diagnostics.extend(normalizer.take_diagnostics());

                let condition = predicate
                    .condition()
                    .expect("normalized static predicate retains its condition");

                if active.iter().any(|constraint| matches!(constraint,
                    CheckedConstraintKind::Predicate(candidate)
                        if candidate.domain() == predicate.domain() && candidate.condition() == Some(condition))) {
                    continue;
                }

                return Ok(false);
            }
            CheckedConstraintKind::TypeEquality { left, right } => {
                let values = request.semantic_values();

                let left = values
                    .substitute_type(left, obligation.substitution())
                    .expect("checked equality substitution is valid");

                let right = values
                    .substitute_type(right, obligation.substitution())
                    .expect("checked equality substitution is valid");

                let equal = normalizer.normalize_type(left)? == normalizer.normalize_type(right)?;

                diagnostics.extend(normalizer.take_diagnostics());

                if !equal {
                    return Ok(false);
                }

                continue;
            }
        };

        let subject = request
            .semantic_values()
            .substitute_type(subject, obligation.substitution()).unwrap_or_else(|error| panic!("active_constraints_prove must satisfy its checked construction contract: {error:?}"));

        let application = request
            .semantic_values()
            .substitute_trait_application(application, obligation.substitution()).unwrap_or_else(|error| panic!("active_constraints_prove must satisfy its checked construction contract: {error:?}"));

        let subject = normalizer.normalize_type(subject)?;
        let application = normalizer.normalize_trait_application(application)?;

        diagnostics.extend(normalizer.take_diagnostics());

        if evidence.contains(&(subject, application)) {
            continue;
        }

        let built_in =
            crate::built_in_trait_constraint_outcome(request.context(), subject, application);

        if built_in == Some(ProofOutcome::Proven) {
            continue;
        }

        let requirement = ImplementationRequirementKey::new(subject, application);

        let selection =
            request.implementation_selection_with_constraint_evidence(requirement, &evidence)?;

        diagnostics.extend(selection.diagnostics().iter().cloned());

        if !matches!(selection.value(), ImplementationSelection::Selected(_)) {
            return Ok(false);
        }
    }

    Ok(true)
}

fn trait_constraint_evidence(
    constraints: &BTreeSet<CheckedConstraintKind>,
) -> Vec<(TypeId, bray_symbols::TraitApplicationId)> {
    constraints
        .iter()
        .filter_map(|constraint| match constraint {
            CheckedConstraintKind::TraitSatisfaction {
                subject,
                application,
            } => Some((*subject, *application)),
            _ => None,
        })
        .collect()
}

fn active_generic_constraints<C>(
    request: CheckerUnitView<'_, C>,
    diagnostics: &mut DiagnosticBag,
    normalizer: &mut crate::TypeNormalizer<'_, C>,
) -> CheckerQueryResult<BTreeSet<CheckedConstraintKind>, C::UpstreamError>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<GenericConstraintsQuery>
        + CheckerSemanticQueryProvider<GenericTypeConstraintsQuery>
        + ?Sized,
{
    let mut active = BTreeSet::new();

    let Some(mut owner) = request
        .symbols()
        .symbol_for_key(request.unit().key().declared_owner())
    else {
        panic!("bound generic context retains its declaration identity");
    };

    loop {
        if let Some(generic_owner) = GenericOwnerId::try_new(owner) {
            let scope = request
                .context()
                .generic_constraint_scope(request.semantic_context(), generic_owner)?;

            diagnostics.extend(scope.diagnostics().iter().cloned());

            let constraints = match scope.value() {
                crate::GenericConstraintScope::Unavailable => {
                    let Some(container) = request.symbols().containing_symbol(owner) else {
                        break;
                    };

                    owner = container;
                    continue;
                }
                crate::GenericConstraintScope::TypeHeader => request.resolve_symbol_query(
                    SymbolQueryRequest::<GenericTypeConstraintsQuery>::new(generic_owner),
                )?,
                crate::GenericConstraintScope::Complete => request.resolve_symbol_query(
                    SymbolQueryRequest::<GenericConstraintsQuery>::new(generic_owner),
                )?,
            };

            if constraints.diagnostics().has_errors() {
                diagnostics.extend(constraints.diagnostics().iter().cloned());
            } else {
                for constraint in constraints.value().constraints() {
                    active.insert(normalizer.normalize_constraint(*constraint)?.kind());
                }

                diagnostics.extend(normalizer.take_diagnostics());
            }
        }

        let Some(container) = request.symbols().containing_symbol(owner) else {
            break;
        };

        owner = container;
    }

    Ok(active)
}

fn defer_callable_selection<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    deferred: &mut BTreeSet<BoundExpressionId>,
) -> Result<SessionProgress<()>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let callee = match request.view().expression(expression) {
        Some(BoundExpression::Call(call)) => call.callee(),
        Some(BoundExpression::ErrorCall(call)) => call.callee(),
        _ => panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in defer_callable_selection"
        ),
    };

    deferred.insert(expression);

    defer_expression_tree(request, callee, deferred)
}

fn nested_callable_target<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    nested_callables: &[NestedCallableEvidence],
    initializers: &BTreeMap<bray_symbols::LocalBindingSymbolId, BoundExpressionId>,
) -> Result<
    Option<(
        bray_symbols::AnonymousCallableSymbolId,
        bray_bound_tree::BoundUnitKey,
    )>,
    CheckerQueryError<C::UpstreamError>,
>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(BoundExpression::Call(call)) = request.view().expression(expression) else {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in nested_callable_target"
        );
    };

    let mut callee = call.callee();

    while let Some(BoundExpression::Name(name)) = request.view().expression(callee) {
        let BoundReferenceTarget::Local(bray_symbols::AnyLocalSymbolId::Binding(binding)) =
            name.target()
        else {
            return Ok(None);
        };

        let Some(initializer) = initializers.get(&binding) else {
            return Ok(None);
        };

        callee = *initializer;
    }

    let Some(BoundExpression::AnonymousCallable(anonymous)) = request.view().expression(callee)
    else {
        return Ok(None);
    };

    let mut matching = nested_callables
        .iter()
        .filter(|evidence| evidence.expression() == callee);

    let Some(evidence) = matching.next() else {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in nested_callable_target"
        );
    };

    if matching.next().is_some() {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in nested_callable_target"
        );
    }

    Ok(Some((evidence.callable(), anonymous.unit().clone())))
}

fn defer_expression_tree<C>(
    request: CheckerUnitView<'_, C>,
    root: BoundExpressionId,
    deferred: &mut BTreeSet<BoundExpressionId>,
) -> Result<SessionProgress<()>, CheckerQueryError<C::UpstreamError>>
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
            panic!(
                "Semantic-selection inputs do not describe the requested bound unit or operation category. in defer_expression_tree"
            );
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
) -> Result<SessionProgress<()>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<CallableSignatureQuery>
        + CheckerSemanticQueryProvider<GenericConstraintsQuery>
        + CheckerSemanticQueryProvider<GenericTypeConstraintsQuery>
        + ?Sized,
{
    for prepared_call in &prepared.calls {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        let materialized = match dependency_progress(materialize_call_candidates(
            request,
            types,
            prepared_call,
            &prepared.member_targets,
        ))? {
            SessionProgress::Complete(materialized) => materialized,
            SessionProgress::Cancelled => return Ok(SessionProgress::Cancelled),
        };

        let Some(materialized) = materialized else {
            continue;
        };

        let candidates = &materialized.candidates;

        let Some(BoundExpression::Call(call)) = request.view().expression(prepared_call.expression)
        else {
            panic!(
                "Semantic-selection inputs do not describe the requested bound unit or operation category. in add_candidate_expectations"
            );
        };

        let available = candidates.iter().collect::<Vec<_>>();

        for (ordinal, argument) in call.arguments().iter().enumerate() {
            let expected = common_parameter_type(request, &available, call.arguments(), ordinal)?;

            let Some(expected) = expected else {
                continue;
            };

            session.add_expectation(argument.expression(), expected);
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
            session.add_evidence(call.callee(), callable_type);
        }

        for (ordinal, argument) in call.arguments().iter().enumerate() {
            let Some(expected) =
                common_parameter_type(request, &candidates, call.arguments(), ordinal)?
            else {
                continue;
            };

            session.add_expectation(argument.expression(), expected);
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
) -> Result<Option<Vec<&'candidate CallableCandidate>>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    // The probe borrows the prepared candidates while owning only the source call surface.
    let input = match call_selection_request(request, types, prepared, member_targets, []) {
        Ok(input) => input,
        Err(CheckerQueryError::Cancelled) => return Ok(None),
        Err(CheckerQueryError::Upstream(error)) => {
            return Err(CheckerQueryError::Upstream(error));
        }
    };

    let Some(indices) =
        crate::selection::viable_candidate_indices(request, types, &input, candidates)
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
) -> Result<Option<TypeId>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut expected = None;

    for candidate in candidates {
        let callable = request
            .semantic_values()
            .type_data(candidate.callable_type());

        let TypeData::Callable(callable) = callable.as_ref() else {
            panic!(
                "Semantic-selection inputs do not describe the requested bound unit or operation category. in common_parameter_type"
            );
        };

        let Some(mapping) = crate::selection::map_argument_parameter_indices(
            arguments,
            callable.parameters(),
            callable.is_variadic(),
        ) else {
            return Ok(None);
        };

        let Some(parameter_index) = mapping.get(ordinal).copied() else {
            panic!(
                "Semantic-selection inputs do not describe the requested bound unit or operation category. in common_parameter_type"
            );
        };

        let Some(parameter_index) = parameter_index else {
            return Ok(None);
        };

        let Some(parameter) = callable.parameters().get(parameter_index) else {
            panic!(
                "Semantic-selection inputs do not describe the requested bound unit or operation category. in common_parameter_type"
            );
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
    value: impl FnMut(&CallableCandidate) -> Option<T>,
) -> Option<T> {
    common_value(candidates.iter().copied().map(value))
}

fn common_value<T: Copy + Eq>(values: impl IntoIterator<Item = Option<T>>) -> Option<T> {
    let mut values = values.into_iter();

    let first = values.next()??;

    values.all(|value| value == Some(first)).then_some(first)
}

struct MaterializedCallCandidates<'prepared> {
    candidates: Cow<'prepared, [CallableCandidate]>,
    diagnostics: DiagnosticBag,
}

impl<'prepared> MaterializedCallCandidates<'prepared> {
    fn new<C: CheckerRequestContext + ?Sized>(
        request: CheckerUnitView<'_, C>,
        expression: BoundExpressionId,
        candidates: Cow<'prepared, [CallableCandidate]>,
        mut diagnostics: DiagnosticBag,
    ) -> Self {
        crate::diagnostic::anchor_diagnostics(
            &mut diagnostics,
            bray_diagnostics::DiagnosticLabelKind::SelectionFailure,
            || Some(crate::diagnostic::expression_span(request, expression)),
        );

        Self {
            candidates,
            diagnostics,
        }
    }
}

fn dependency_progress<T, Upstream>(
    result: CheckerQueryResult<T, Upstream>,
) -> Result<SessionProgress<T>, CheckerQueryError<Upstream>> {
    match result {
        Ok(value) => Ok(SessionProgress::Complete(value)),
        Err(CheckerQueryError::Cancelled) => Ok(SessionProgress::Cancelled),
        Err(CheckerQueryError::Upstream(error)) => Err(CheckerQueryError::Upstream(error)),
    }
}

fn materialize_call_candidates<'prepared, C>(
    request: CheckerUnitView<'_, C>,
    types: &bray_bound_tree::CheckedExpressionTypes,
    prepared: &'prepared PreparedCall,
    member_targets: &BTreeMap<BoundExpressionId, bray_bound_tree::MemberTarget>,
) -> CheckerQueryResult<Option<MaterializedCallCandidates<'prepared>>, C::UpstreamError>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<GenericConstraintsQuery>
        + CheckerSemanticQueryProvider<GenericTypeConstraintsQuery>
        + ?Sized,
{
    let Some(BoundExpression::Call(call)) = request.view().expression(prepared.expression) else {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in materialize_call_candidates"
        );
    };

    let member = member_targets.get(&call.callee());

    let member_template = member
        .and_then(bray_bound_tree::MemberTarget::callable_template)
        .cloned()
        .map(|template| {
            CallableCandidateTemplate::Declaration(
                crate::CallableDeclarationCandidateTemplate::from_declaration(
                    template,
                    crate::CallableCandidateTemplateState::Visible,
                ),
            )
        });

    if prepared.generic_candidates.is_empty()
        && prepared.values.is_empty()
        && member_template.is_none()
    {
        return Ok(Some(MaterializedCallCandidates::new(
            request,
            prepared.expression,
            Cow::Borrowed(&prepared.candidates),
            DiagnosticBag::new(),
        )));
    }

    let mut candidates = prepared.candidates.clone();
    let mut diagnostics = DiagnosticBag::new();

    for template in prepared
        .generic_candidates
        .iter()
        .chain(member_template.iter())
    {
        let explicit = match template {
            CallableCandidateTemplate::Declaration(template) => {
                resolve_generic_arguments(request, template.generic_arguments(), &mut diagnostics)?
            }
            CallableCandidateTemplate::Predicate(template) => {
                resolve_generic_arguments(request, template.generic_arguments(), &mut diagnostics)?
            }
            CallableCandidateTemplate::Value(_) => {
                panic!(
                    "Semantic-selection inputs do not describe the requested bound unit or operation category. in materialize_call_candidates"
                );
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
                panic!(
                    "Semantic-selection inputs do not describe the requested bound unit or operation category. in materialize_call_candidates"
                );
            }
        };

        let Some(parameters) = parameters.get(explicit.len()..) else {
            panic!(
                "Semantic-selection inputs do not describe the requested bound unit or operation category. in materialize_call_candidates"
            );
        };

        let Some(inferred) =
            infer_call_generic_arguments(request, prepared.expression, &open, parameters, types)
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
                panic!(
                    "Semantic-selection inputs do not describe the requested bound unit or operation category. in materialize_call_candidates"
                );
            }
        };

        let TemplateResolution::Resolved(candidate) = resolved else {
            continue;
        };

        let outcome = candidate_constraint_outcome(request, &candidate, &mut diagnostics)?;

        if outcome == ProofOutcome::Proven {
            let selections = generic_constraint_implementation_selections(
                request,
                &candidate,
                &mut diagnostics,
            )?;

            let candidate = match member.and_then(bray_bound_tree::MemberTarget::trait_dispatch) {
                Some(dispatch) => candidate.with_trait_dispatch(dispatch),
                None => candidate,
            };

            candidates.push(candidate.with_implementation_selections(selections));
        }
    }

    if prepared.values.is_empty() || member_template.is_some() {
        return Ok(Some(MaterializedCallCandidates::new(
            request,
            prepared.expression,
            Cow::Owned(CallableSelectionRequest::canonical_candidates(candidates)),
            diagnostics,
        )));
    }

    let Some(callee_type) = types.expression(call.callee()) else {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in materialize_call_candidates"
        );
    };

    if callee_type.is_recovered() {
        return Ok(None);
    }

    let data = request.semantic_values().type_data(callee_type.ty());

    let TypeData::Callable(callable) = data.as_ref() else {
        return Ok(Some(MaterializedCallCandidates::new(
            request,
            prepared.expression,
            Cow::Owned(candidates),
            diagnostics,
        )));
    };

    let result = call_result(request, callable, callable.result());

    let target = match &prepared.anonymous_target {
        Some((anonymous, _)) => BoundCallableTarget::Anonymous(*anonymous),
        None => member
            .and_then(bray_bound_tree::MemberTarget::callable_instance)
            .map(|instance| member_call_target(instance, member, callee_type.ty()))
            .unwrap_or(BoundCallableTarget::Indirect(callee_type.ty())),
    };

    let witnesses = member
        .into_iter()
        .flat_map(bray_bound_tree::MemberTarget::witnesses)
        .map(|witness| witness.witness());

    let mut resolution = BoundResolvedCall::new(target, witnesses, result);

    if let Some((_, unit)) = &prepared.anonymous_target {
        resolution = resolution.with_anonymous_body(unit.clone(), callee_type.ty());
    }

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
            .with_declaration(
                signature.clone(),
                member
                    .into_iter()
                    .flat_map(bray_bound_tree::MemberTarget::callable_defaults)
                    .copied(),
            ),
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
                        ImplementationSelection::Selected(witness.witness()),
                    )
                }),
        )
    }));

    Ok(Some(MaterializedCallCandidates::new(
        request,
        prepared.expression,
        Cow::Owned(CallableSelectionRequest::canonical_candidates(candidates)),
        diagnostics,
    )))
}

fn member_call_target(
    instance: bray_symbols::CallableInstanceData,
    member: Option<&bray_bound_tree::MemberTarget>,
    ty: TypeId,
) -> BoundCallableTarget {
    match member.and_then(bray_bound_tree::MemberTarget::trait_view_dispatch) {
        Some((slot, defaults)) => BoundCallableTarget::TraitView {
            member: instance,
            slot,
            defaults,
            ty,
        },
        None => BoundCallableTarget::Declaration(instance),
    }
}

pub(super) fn converge<C>(
    request: CheckerUnitView<'_, C>,
    prepared: &PreparedExpressions,
    session: &mut ExpressionTypeSession<'_, C>,
) -> Result<SessionProgress<()>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<CallableSignatureQuery>
        + CheckerSemanticQueryProvider<GenericConstraintsQuery>
        + CheckerSemanticQueryProvider<GenericTypeConstraintsQuery>
        + ?Sized,
{
    loop {
        let revision = session.revision();

        if session.propagate().is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        built_in_operator::apply_evidence(request, prepared.built_in_operators(), session);

        let types = session.preview();

        built_in_operator::apply_operand_expectations(
            request,
            &types,
            prepared.built_in_operators(),
            session,
        );

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
) -> Result<SessionProgress<()>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<CallableSignatureQuery>
        + CheckerSemanticQueryProvider<GenericConstraintsQuery>
        + CheckerSemanticQueryProvider<GenericTypeConstraintsQuery>
        + ?Sized,
{
    for prepared_call in &prepared.calls {
        if request.is_cancelled() {
            return Ok(SessionProgress::Cancelled);
        }

        let materialized = match dependency_progress(materialize_call_candidates(
            request,
            types,
            prepared_call,
            &prepared.member_targets,
        ))? {
            SessionProgress::Complete(materialized) => materialized,
            SessionProgress::Cancelled => return Ok(SessionProgress::Cancelled),
        };

        let Some(materialized) = materialized else {
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
            CheckerOutcome::UpstreamFailure(error) => {
                return Err(CheckerQueryError::Upstream(error));
            }
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
) -> Result<(), CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(BoundExpression::Call(call)) = request.view().expression(prepared.expression) else {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in apply_selected_signature"
        );
    };

    let Some(candidate) = candidates
        .iter()
        .find(|candidate| candidate.resolution() == selection.resolution())
    else {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in apply_selected_signature"
        );
    };

    session.add_evidence(call.callee(), candidate.callable_type());

    let callable = request
        .semantic_values()
        .type_data(candidate.callable_type());

    let TypeData::Callable(callable) = callable.as_ref() else {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in apply_selected_signature"
        );
    };

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

        let Some(selected) = callable.parameters().get(*ordinal as usize) else {
            if callable.is_variadic() && parameter.is_none() {
                continue;
            }

            panic!(
                "Semantic-selection inputs do not describe the requested bound unit or operation category. in apply_selected_signature"
            );
        };

        if let Some(parameter) = parameter {
            let Some(signature) = candidate.declaration_signature() else {
                panic!(
                    "Semantic-selection inputs do not describe the requested bound unit or operation category. in apply_selected_signature"
                );
            };

            if signature
                .parameters()
                .get(*ordinal as usize)
                .is_none_or(|signature| signature.parameter() != *parameter)
            {
                panic!(
                    "Semantic-selection inputs do not describe the requested bound unit or operation category. in apply_selected_signature"
                );
            }
        }

        session.add_expectation(*expression, selected.ty());
    }

    session.add_evidence(prepared.expression, selection.resolution().result().ty());

    Ok(())
}

pub(super) fn final_selections<C>(
    request: CheckerUnitView<'_, C>,
    types: &bray_bound_tree::CheckedExpressionTypes,
    prepared: &PreparedExpressions,
) -> crate::CheckerQueryResult<Option<(Vec<SemanticSelectionEntry>, DiagnosticBag)>, C::UpstreamError>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<CallableSignatureQuery>
        + CheckerSemanticQueryProvider<GenericConstraintsQuery>
        + CheckerSemanticQueryProvider<GenericTypeConstraintsQuery>
        + ?Sized,
{
    let mut entries = Vec::new();
    let mut diagnostics = DiagnosticBag::new();

    for prepared_call in &prepared.calls {
        let materialized = match materialize_call_candidates(
            request,
            types,
            prepared_call,
            &prepared.member_targets,
        ) {
            Ok(materialized) => materialized,
            Err(CheckerQueryError::Cancelled) => return Ok(None),
            Err(CheckerQueryError::Upstream(error)) => {
                return Err(CheckerQueryError::Upstream(error));
            }
        };

        let Some(materialized) = materialized else {
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
            CheckerOutcome::UpstreamFailure(error) => {
                return Err(CheckerQueryError::Upstream(error));
            }
        }
    }

    entries.extend(built_in_operator::selections(
        request,
        types,
        prepared.built_in_operators(),
    ));

    Ok(Some((entries, diagnostics)))
}

fn select_prepared_call<C>(
    request: CheckerUnitView<'_, C>,
    types: &bray_bound_tree::CheckedExpressionTypes,
    prepared: &PreparedCall,
    member_targets: &BTreeMap<BoundExpressionId, bray_bound_tree::MemberTarget>,
    candidates: &[CallableCandidate],
) -> CheckerOutcome<CandidateSelection<bray_bound_tree::SelectedCall>, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    let input = match call_selection_request(request, types, prepared, member_targets, []) {
        Ok(input) => input,
        Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Upstream(error)) => {
            return CheckerOutcome::UpstreamFailure(error);
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
) -> CheckerQueryResult<CallableSelectionRequest, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    let Some(BoundExpression::Call(call)) = request.view().expression(prepared.expression) else {
        panic!(
            "prepared callable selection {:?} must name a committed call",
            prepared.expression
        );
    };

    let member = member_targets.get(&call.callee()).cloned();

    let receiver = member
        .as_ref()
        .filter(|member| member.is_callable_declaration())
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
) -> CheckerQueryResult<crate::ReceiverCapability, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    let mut expression = receiver;
    let mut mutable_projection = true;

    loop {
        let ty = types
            .expression(expression)
            .ok_or_else(invalid_selection_input)?;

        let data = request.semantic_values().type_data(ty.ty());

        if let TypeData::Borrow { kind, .. } = data.as_ref() {
            return Ok(borrow_receiver_capability(*kind, mutable_projection));
        }

        match request.view().expression(expression) {
            Some(BoundExpression::MemberAccess(member)) => {
                match member.selector() {
                    Some(BoundMemberSelector::Name(_)) => {
                        let target = member_targets
                            .get(&expression)
                            .ok_or_else(invalid_selection_input)?;

                        mutable_projection &= request.member_allows_mutation(target.member())?;
                    }
                    Some(BoundMemberSelector::TupleElement(_)) => {}
                    None => return Err(invalid_selection_input()),
                }

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
                panic!(
                    "Semantic-selection inputs do not describe the requested bound unit or operation category. in receiver_capability"
                );
            }
        }
    }
}

fn generic_constraint_implementation_selections<C>(
    request: CheckerUnitView<'_, C>,
    candidate: &CallableCandidate,
    diagnostics: &mut DiagnosticBag,
) -> Result<Vec<crate::ImplementationSelectionEvidence>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<GenericConstraintsQuery>
        + CheckerSemanticQueryProvider<GenericTypeConstraintsQuery>
        + ?Sized,
{
    if candidate.generic_constraints().is_empty() {
        return Ok(Vec::new());
    }

    let Some(substitution) = candidate.generic_substitution() else {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in generic_constraint_implementation_selections"
        );
    };

    let substitution_data = request
        .semantic_values()
        .generic_substitution_data(substitution);

    let constraints = request.resolve_symbol_query(
        SymbolQueryRequest::<GenericConstraintsQuery>::new(substitution_data.owner()),
    )?;

    diagnostics.extend(constraints.diagnostics().iter().cloned());

    if constraints.diagnostics().has_errors() {
        return Ok(Vec::new());
    }

    let mut selections = Vec::new();
    let mut normalizer = crate::TypeNormalizer::new(request.context(), None);
    let active = active_generic_constraints(request, diagnostics, &mut normalizer)?;
    let evidence = trait_constraint_evidence(&active);

    for constraint in constraints.value().constraints() {
        let CheckedConstraintKind::TraitSatisfaction {
            subject,
            application,
        } = constraint.kind()
        else {
            continue;
        };

        let subject = request
            .semantic_values()
            .substitute_type(subject, substitution).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in generic_constraint_implementation_selections: {error:?}"));

        let application = request
            .semantic_values()
            .substitute_trait_application(application, substitution).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in generic_constraint_implementation_selections: {error:?}"));

        let subject = normalizer.normalize_type(subject)?;
        let application = normalizer.normalize_trait_application(application)?;

        diagnostics.extend(normalizer.take_diagnostics());

        if crate::built_in_trait_constraint_outcome(request.context(), subject, application)
            == Some(ProofOutcome::Proven)
        {
            continue;
        }

        let requirement = ImplementationRequirementKey::new(subject, application);

        let selection =
            request.implementation_selection_with_constraint_evidence(requirement, &evidence)?;

        diagnostics.extend(selection.diagnostics().iter().cloned());

        if matches!(selection.value(), ImplementationSelection::Selected(_)) {
            selections.push(crate::ImplementationSelectionEvidence::new(
                requirement,
                selection.value().clone(),
            ));
        }
    }

    Ok(selections)
}

const fn borrow_receiver_capability(
    kind: bray_symbols::BorrowKind,
    mutable_projection: bool,
) -> crate::ReceiverCapability {
    match kind {
        bray_symbols::BorrowKind::Shared => crate::ReceiverCapability::Owned,
        bray_symbols::BorrowKind::Mutable if mutable_projection => {
            crate::ReceiverCapability::OwnedMutable
        }
        bray_symbols::BorrowKind::Mutable => crate::ReceiverCapability::Owned,
    }
}

fn name_receiver_capability<C>(
    request: CheckerUnitView<'_, C>,
    target: BoundReferenceTarget,
) -> CheckerQueryResult<crate::ReceiverCapability, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
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
                .ok_or_else(invalid_selection_input)?;

            let signature = request.resolve_symbol_query(SymbolQueryRequest::<
                CallableSignatureQuery,
            >::new(receiver.owner()))?;

            let mode = signature
                .value()
                .receiver()
                .filter(|signature| signature.parameter() == parameter)
                .map(bray_symbols::ReceiverParameterSignature::mode)
                .ok_or_else(invalid_selection_input)?;

            Ok(receiver_mode_capability(mode))
        }
        BoundReferenceTarget::Surface(AnySymbolId::CallableParameter(parameter)) => {
            let parameter = request
                .symbols()
                .callable_parameter(parameter)
                .ok_or_else(invalid_selection_input)?;

            let signature = request.resolve_symbol_query(SymbolQueryRequest::<
                CallableSignatureQuery,
            >::new(parameter.owner()))?;

            let ordinal = parameter.ordinal();

            let index = usize::try_from(ordinal).unwrap_or_else(|error| panic!("name_receiver_capability must satisfy its checked construction contract: {error:?}"));

            let mode = match signature.value().callable_type() {
                TypeExpressionTemplate::Callable(callable) => callable
                    .parameters()
                    .get(index)
                    .map(bray_symbols::CallableParameterTypeTemplate::mode),
                TypeExpressionTemplate::Resolved(ty) => {
                    let data = request.semantic_values().type_data(*ty);

                    match data.as_ref() {
                        TypeData::Callable(callable) => callable
                            .parameters()
                            .get(index)
                            .map(|parameter| parameter.mode()),
                        _ => None,
                    }
                }
                _ => None,
            }
            .ok_or_else(invalid_selection_input)?;

            Ok(match mode {
                CallableParameterMode::Immutable => crate::ReceiverCapability::Owned,
                CallableParameterMode::Mutable => crate::ReceiverCapability::OwnedMutable,
            })
        }
        BoundReferenceTarget::Local(_) | BoundReferenceTarget::Surface(_) => {
            Ok(crate::ReceiverCapability::Owned)
        }
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
    use bray_symbols::BorrowKind;

    use super::{borrow_receiver_capability, common_value, prepare_calls};
    use crate::test_support::{
        TestCheckerContext, callable_entry, expression_unit, integer_literal_expression,
        push_expression,
    };
    use crate::{CheckerUnitView, ExpressionCandidateSet, ReceiverCapability};

    #[test]
    fn borrow_receivers_retain_ownership_of_the_borrow_value() {
        assert_eq!(
            borrow_receiver_capability(BorrowKind::Shared, true),
            ReceiverCapability::Owned
        );

        assert_eq!(
            borrow_receiver_capability(BorrowKind::Mutable, true),
            ReceiverCapability::OwnedMutable
        );

        assert_eq!(
            borrow_receiver_capability(BorrowKind::Mutable, false),
            ReceiverCapability::Owned
        );
    }

    #[test]
    fn common_candidate_expectations_require_structural_agreement() {
        assert_eq!(common_value([Some(7_u8), Some(7)]), Some(7));
        assert_eq!(common_value([Some(1_u8), Some(2)]), None);
        assert_eq!(common_value([Some(1_u8), None]), None);
    }

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

        let request = CheckerUnitView::new(&unit, &semantic_context, &context);

        let result = prepare_calls(request, &[], &candidates);

        assert!(matches!(
            result,
            Ok(crate::type_check::SessionProgress::Cancelled)
        ));
    }
}
