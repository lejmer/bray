use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundUnit,
    BoundUnresolvedReferenceKind, DeclaredValueTypeTerm, SelectionKind,
};
use bray_checker::{
    CallableCandidateTemplate, CallableCandidateTemplateState, CallableCandidateTemplates,
    CallableDeclarationCandidateTemplate, CallableParameterDefaultTemplate,
    CallableValueCandidateTemplate, CandidateAbsence, ExpressionCandidateSet,
};
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableOverloadSymbolId, CallableOverloadTemplateFact,
    CallableParameterDefaultTemplateFact, CallableSignatureFact, CallableSymbolId,
    GenericDeclarationTemplateFact, GenericOwnerId, MemberLookupResult, OverloadArmTemplate,
    SymbolFactContract, SymbolFactRequest,
};
use bray_syntax::PathSyntax;

use crate::lookup::{NameAccess, ResolvedName, bind_module_path};
use crate::{BinderFactContext, BinderFactError, BinderFactResult, SymbolFactProvider};

#[derive(Clone, Copy)]
enum DeclarationCandidateOutcome {
    Added,
    Ignored,
    UnavailableFacts,
}

impl DeclarationCandidateOutcome {
    const fn absence(self) -> CandidateAbsence {
        match self {
            Self::Added | Self::Ignored => CandidateAbsence::UnresolvedReference,
            Self::UnavailableFacts => CandidateAbsence::UnavailableDeclarationFacts,
        }
    }

    const fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::UnavailableFacts, _) | (_, Self::UnavailableFacts) => Self::UnavailableFacts,
            (Self::Added, _) | (_, Self::Added) => Self::Added,
            (Self::Ignored, Self::Ignored) => Self::Ignored,
        }
    }
}

pub(super) fn bind_call_candidates<C>(
    context: &C,
    unit: &BoundUnit,
    expression: BoundExpressionId,
    callee: BoundExpressionId,
) -> BinderFactResult<ExpressionCandidateSet>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<CallableParameterDefaultTemplateFact>
        + SymbolFactProvider<CallableOverloadTemplateFact>,
{
    let Some(callee_expression) = unit.view().expression(callee) else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    let mut candidates = Vec::new();

    let absence = match callee_expression {
        BoundExpression::Name(name) => bind_reference_target(
            context,
            name.target(),
            state_for_recovery(name.is_recovered()),
            &mut candidates,
        )?,
        BoundExpression::UnresolvedReference(reference) => {
            let state = state_for_unresolved_reference(reference.kind());
            let mut absence = CandidateAbsence::UnresolvedReference;

            for target in reference.candidates() {
                absence = preferred_absence(
                    absence,
                    bind_reference_target(context, *target, state, &mut candidates)?,
                );
            }

            absence
        }
        BoundExpression::MemberAccess(_) | BoundExpression::TraitQualifiedMember(_) => {
            return Ok(ExpressionCandidateSet::Unsupported {
                expression,
                kind: SelectionKind::Member,
            });
        }
        _ => {
            bind_callable_value(
                DeclaredValueTypeTerm::Expression(callee),
                state_for_recovery(callee_expression.is_recovered()),
                &mut candidates,
            );

            CandidateAbsence::UnresolvedReference
        }
    };

    if absence == CandidateAbsence::UnavailableDeclarationFacts {
        candidates.clear();
    }

    let candidate_set = CallableCandidateTemplates::present(expression, candidates)
        .unwrap_or_else(|| CallableCandidateTemplates::absent(expression, absence));

    Ok(ExpressionCandidateSet::Callable(candidate_set))
}

fn bind_reference_target<C>(
    context: &C,
    target: BoundReferenceTarget,
    state: CallableCandidateTemplateState,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BinderFactResult<CandidateAbsence>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<CallableParameterDefaultTemplateFact>
        + SymbolFactProvider<CallableOverloadTemplateFact>,
{
    match target {
        BoundReferenceTarget::Surface(AnySymbolId::CallableOverload(overload)) => {
            bind_overload_candidates(context, overload, state, candidates)
        }
        BoundReferenceTarget::Surface(symbol) if symbol.kind().is_callable() => {
            Ok(bind_declaration_candidate(context, symbol, state, candidates)?.absence())
        }
        BoundReferenceTarget::Local(_) | BoundReferenceTarget::Surface(_) => {
            bind_callable_value(DeclaredValueTypeTerm::Value(target), state, candidates);

            Ok(CandidateAbsence::UnresolvedReference)
        }
    }
}

fn bind_callable_value(
    value: DeclaredValueTypeTerm,
    state: CallableCandidateTemplateState,
    candidates: &mut Vec<CallableCandidateTemplate>,
) {
    let candidate = CallableValueCandidateTemplate::new(value, state);

    candidates.push(CallableCandidateTemplate::Value(candidate));
}

fn bind_overload_candidates<C>(
    context: &C,
    overload: CallableOverloadSymbolId,
    state: CallableCandidateTemplateState,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BinderFactResult<CandidateAbsence>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<CallableParameterDefaultTemplateFact>
        + SymbolFactProvider<CallableOverloadTemplateFact>,
{
    let (template, has_diagnostics) =
        symbol_fact_value::<_, CallableOverloadTemplateFact>(context, overload)?;

    let state = combine_recovery(state, has_diagnostics);
    let mut outcome = DeclarationCandidateOutcome::Ignored;

    for arm in template.arms() {
        let arm_outcome = match arm {
            OverloadArmTemplate::Resolved(symbol) => {
                bind_declaration_candidate(context, *symbol, state, candidates)?
            }
            OverloadArmTemplate::Source(anchor) => {
                bind_source_overload_arm(context, overload, *anchor, state, candidates)?
            }
        };

        outcome = outcome.merge(arm_outcome);
    }

    Ok(match outcome {
        DeclarationCandidateOutcome::UnavailableFacts => {
            CandidateAbsence::UnavailableDeclarationFacts
        }
        DeclarationCandidateOutcome::Added | DeclarationCandidateOutcome::Ignored => {
            CandidateAbsence::EmptyOverload
        }
    })
}

fn bind_source_overload_arm<C>(
    context: &C,
    overload: CallableOverloadSymbolId,
    anchor: bray_declarations::SyntaxAnchor,
    state: CallableCandidateTemplateState,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BinderFactResult<DeclarationCandidateOutcome>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<CallableParameterDefaultTemplateFact>
        + SymbolFactProvider<CallableOverloadTemplateFact>,
{
    let Some(module) = context.symbols().containing_module(overload.into()) else {
        return Ok(DeclarationCandidateOutcome::Ignored);
    };

    let Some(path) = anchor.find_descendant::<PathSyntax>(context.syntax()) else {
        return Ok(DeclarationCandidateOutcome::Ignored);
    };

    let lookup = bind_module_path(context, module.id(), &path, NameAccess::Internal)?;

    let outcome = match lookup {
        MemberLookupResult::Found(name) => {
            bind_resolved_name_candidate(context, name, state, candidates)?
        }
        MemberLookupResult::Inaccessible(names) => {
            let mut outcome = DeclarationCandidateOutcome::Ignored;

            for name in names {
                let candidate = bind_resolved_name_candidate(
                    context,
                    name,
                    CallableCandidateTemplateState::Inaccessible,
                    candidates,
                )?;

                outcome = outcome.merge(candidate);
            }

            outcome
        }
        MemberLookupResult::WrongKind(names)
        | MemberLookupResult::Ambiguous(names)
        | MemberLookupResult::Malformed(names) => {
            let mut outcome = DeclarationCandidateOutcome::Ignored;

            for name in names {
                let candidate = bind_resolved_name_candidate(
                    context,
                    name,
                    CallableCandidateTemplateState::Recovered,
                    candidates,
                )?;

                outcome = outcome.merge(candidate);
            }

            outcome
        }
        MemberLookupResult::NotFound => DeclarationCandidateOutcome::Ignored,
    };

    Ok(outcome)
}

fn bind_resolved_name_candidate<C>(
    context: &C,
    name: ResolvedName,
    state: CallableCandidateTemplateState,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BinderFactResult<DeclarationCandidateOutcome>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<CallableParameterDefaultTemplateFact>
        + SymbolFactProvider<CallableOverloadTemplateFact>,
{
    if let ResolvedName::Surface(symbol) = name
        && symbol.kind().is_callable()
    {
        return bind_declaration_candidate(context, symbol, state, candidates);
    }

    Ok(DeclarationCandidateOutcome::Ignored)
}

fn bind_declaration_candidate<C>(
    context: &C,
    symbol: AnySymbolId,
    state: CallableCandidateTemplateState,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BinderFactResult<DeclarationCandidateOutcome>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<CallableParameterDefaultTemplateFact>,
{
    let Some(definition) = CallableDefinitionId::try_new(symbol) else {
        return Ok(DeclarationCandidateOutcome::Ignored);
    };

    let Some(callable) = CallableSymbolId::try_from_any(symbol) else {
        return Ok(DeclarationCandidateOutcome::Ignored);
    };

    let Some(generic_owner) = GenericOwnerId::try_new(symbol) else {
        return Ok(DeclarationCandidateOutcome::Ignored);
    };

    if context.symbols().symbol_key(symbol).is_none() {
        // TODO(BRA-235): Enumerate imported callables after interfaces publish signature facts.
        return Ok(if context.symbol_key(symbol)?.is_some() {
            DeclarationCandidateOutcome::UnavailableFacts
        } else {
            DeclarationCandidateOutcome::Ignored
        });
    }

    // Candidate records outlive the provider borrow and therefore own the stable key.
    let Some(key) = context.symbol_key(symbol)?.cloned() else {
        return Ok(DeclarationCandidateOutcome::Ignored);
    };

    let (signature, signature_diagnostics) =
        symbol_fact_value::<_, CallableSignatureFact>(context, callable)?;

    let (generic, generic_diagnostics) =
        symbol_fact_value::<_, GenericDeclarationTemplateFact>(context, generic_owner)?;

    let mut defaults = Vec::with_capacity(signature.parameters().len());
    let mut has_diagnostics = signature_diagnostics || generic_diagnostics;

    for parameter in signature.parameters() {
        let (value, default_diagnostics) =
            symbol_fact_value::<_, CallableParameterDefaultTemplateFact>(context, *parameter)?;

        has_diagnostics |= default_diagnostics;
        defaults.push(CallableParameterDefaultTemplate::new(*parameter, value));
    }

    let is_recovered = context.symbol_is_recovered(symbol)?.unwrap_or(true);

    let state = combine_recovery(state, is_recovered || has_diagnostics);

    candidates.push(CallableCandidateTemplate::Declaration(
        CallableDeclarationCandidateTemplate::new(
            key, definition, signature, generic, defaults, state,
        ),
    ));

    Ok(DeclarationCandidateOutcome::Added)
}

fn symbol_fact_value<C, F>(context: &C, owner: F::Owner) -> BinderFactResult<(F::Value, bool)>
where
    C: BinderFactContext + ?Sized,
    F: SymbolFactContract,
    F::Value: Clone,
    C::SymbolFacts: SymbolFactProvider<F>,
{
    let result = context
        .symbol_facts()
        .symbol_fact(SymbolFactRequest::<F>::new(owner))?;

    let has_diagnostics = !result.diagnostics().is_empty();

    // The candidate owns the fact value while recursive semantic storage remains Arc-shared.
    Ok((result.value().clone(), has_diagnostics))
}

const fn state_for_recovery(is_recovered: bool) -> CallableCandidateTemplateState {
    if is_recovered {
        CallableCandidateTemplateState::Recovered
    } else {
        CallableCandidateTemplateState::Visible
    }
}

const fn state_for_unresolved_reference(
    kind: BoundUnresolvedReferenceKind,
) -> CallableCandidateTemplateState {
    match kind {
        BoundUnresolvedReferenceKind::Inaccessible => CallableCandidateTemplateState::Inaccessible,
        BoundUnresolvedReferenceKind::NotFound
        | BoundUnresolvedReferenceKind::WrongKind
        | BoundUnresolvedReferenceKind::Ambiguous
        | BoundUnresolvedReferenceKind::Malformed => CallableCandidateTemplateState::Recovered,
    }
}

const fn combine_recovery(
    state: CallableCandidateTemplateState,
    is_recovered: bool,
) -> CallableCandidateTemplateState {
    match state {
        CallableCandidateTemplateState::Inaccessible => {
            CallableCandidateTemplateState::Inaccessible
        }
        CallableCandidateTemplateState::Visible | CallableCandidateTemplateState::Recovered
            if is_recovered =>
        {
            CallableCandidateTemplateState::Recovered
        }
        CallableCandidateTemplateState::Visible | CallableCandidateTemplateState::Recovered => {
            state
        }
    }
}

const fn preferred_absence(
    current: CandidateAbsence,
    candidate: CandidateAbsence,
) -> CandidateAbsence {
    match (current, candidate) {
        (_, CandidateAbsence::UnavailableDeclarationFacts) => {
            CandidateAbsence::UnavailableDeclarationFacts
        }
        (CandidateAbsence::UnavailableDeclarationFacts, _) => {
            CandidateAbsence::UnavailableDeclarationFacts
        }
        (_, CandidateAbsence::EmptyOverload) => CandidateAbsence::EmptyOverload,
        (CandidateAbsence::EmptyOverload, _) => CandidateAbsence::EmptyOverload,
        (CandidateAbsence::UnresolvedReference, CandidateAbsence::UnresolvedReference) => {
            CandidateAbsence::UnresolvedReference
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundUnresolvedReferenceKind;
    use bray_checker::CallableCandidateTemplateState;

    use super::state_for_unresolved_reference;

    #[test]
    fn unresolved_reference_state_preserves_inaccessible_calls() {
        assert_eq!(
            state_for_unresolved_reference(BoundUnresolvedReferenceKind::Inaccessible),
            CallableCandidateTemplateState::Inaccessible
        );

        for kind in [
            BoundUnresolvedReferenceKind::NotFound,
            BoundUnresolvedReferenceKind::WrongKind,
            BoundUnresolvedReferenceKind::Ambiguous,
            BoundUnresolvedReferenceKind::Malformed,
        ] {
            assert_eq!(
                state_for_unresolved_reference(kind),
                CallableCandidateTemplateState::Recovered
            );
        }
    }
}
