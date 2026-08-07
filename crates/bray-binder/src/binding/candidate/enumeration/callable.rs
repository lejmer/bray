use bray_base::Cancellation;
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundUnit,
    BoundUnresolvedReferenceKind, DeclaredValueTypeTerm,
};
use bray_checker::{
    CallableCandidateTemplate, CallableCandidateTemplateState, CallableCandidateTemplates,
    CallableDeclarationCandidateTemplate, CallableParameterDefaultTemplate,
    CallableValueCandidateTemplate, CandidateAbsence, ExpressionCandidateSet,
    PredicateCandidateTemplate,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallableContractTemplateFact, CallableDefinitionId, CallableOverloadSymbolId,
    CallableOverloadTemplateFact, CallableParameterDefaultTemplateFact, CallableSignatureFact,
    CallableSymbolId, GenericArgumentTemplate, GenericDeclarationTemplate,
    GenericDeclarationTemplateFact, GenericOwnerId, MemberLookupResult, NamedTypeSymbolId,
    OverloadArmTemplate, PredicateDefinitionSymbolId, PredicateSignatureTemplateFact,
    SymbolFactContract, SymbolFactRequest,
};
use bray_syntax::{GenericArgumentSyntax, PathSyntax};

use crate::lookup::{NameAccess, ResolvedName, bind_module_path};
use crate::{
    BinderFactContext, BinderFactError, BinderFactResult, SymbolFactProvider, TypeExpressionBinder,
    TypeExpressionScope,
};

use super::constructor::{
    bind_primary_constructor_candidates, bind_type_member_candidates, type_member_subject,
};

#[derive(Clone, Copy)]
pub(super) struct CallGenericContext<'syntax> {
    pub(super) arguments: &'syntax [GenericArgumentSyntax],
    pub(super) scope: &'syntax TypeExpressionScope,
}

struct CandidateCancellation<'context, C: ?Sized>(&'context C);

impl<C> Cancellation for CandidateCancellation<'_, C>
where
    C: BinderFactContext,
{
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

#[derive(Clone, Copy)]
pub(super) enum DeclarationCandidateOutcome {
    Added,
    Ignored,
    UnavailableFacts,
}

impl DeclarationCandidateOutcome {
    pub(super) const fn absence(self) -> CandidateAbsence {
        match self {
            Self::Added | Self::Ignored => CandidateAbsence::UnresolvedReference,
            Self::UnavailableFacts => CandidateAbsence::UnavailableDeclarationFacts,
        }
    }

    pub(super) const fn merge(self, other: Self) -> Self {
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
    generic_arguments: &[GenericArgumentSyntax],
    type_scope: &TypeExpressionScope,
) -> BinderFactResult<DiagnosticResult<ExpressionCandidateSet>>
where
    C: BinderFactContext,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<CallableContractTemplateFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<PredicateSignatureTemplateFact>
        + SymbolFactProvider<CallableParameterDefaultTemplateFact>
        + SymbolFactProvider<CallableOverloadTemplateFact>,
{
    let Some(callee_expression) = unit.view().expression(callee) else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    let mut candidates = Vec::new();
    let mut diagnostics = DiagnosticBag::new();

    let generic = CallGenericContext {
        arguments: generic_arguments,
        scope: type_scope,
    };

    let absence = match callee_expression {
        BoundExpression::Name(name) => bind_reference_target(
            context,
            name.target(),
            state_for_recovery(name.is_recovered()),
            generic,
            &mut diagnostics,
            &mut candidates,
        )?,
        BoundExpression::UnresolvedReference(reference) => {
            let state = state_for_unresolved_reference(reference.kind());
            let mut absence = CandidateAbsence::UnresolvedReference;

            for target in reference.candidates() {
                absence = preferred_absence(
                    absence,
                    bind_reference_target(
                        context,
                        *target,
                        state,
                        generic,
                        &mut diagnostics,
                        &mut candidates,
                    )?,
                );
            }

            absence
        }
        BoundExpression::MemberAccess(_) if type_member_subject(unit, callee).is_some() => {
            bind_type_member_candidates(
                context,
                unit,
                callee,
                state_for_recovery(callee_expression.is_recovered()),
                generic,
                &mut diagnostics,
                &mut candidates,
            )?
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

    Ok(DiagnosticResult::new(
        ExpressionCandidateSet::Callable(candidate_set),
        diagnostics,
    ))
}

fn bind_reference_target<C>(
    context: &C,
    target: BoundReferenceTarget,
    state: CallableCandidateTemplateState,
    generic: CallGenericContext<'_>,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BinderFactResult<CandidateAbsence>
where
    C: BinderFactContext,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<CallableContractTemplateFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<PredicateSignatureTemplateFact>
        + SymbolFactProvider<CallableParameterDefaultTemplateFact>
        + SymbolFactProvider<CallableOverloadTemplateFact>,
{
    match target {
        BoundReferenceTarget::Surface(AnySymbolId::CallableOverload(overload)) => {
            bind_overload_candidates(context, overload, state, generic, diagnostics, candidates)
        }
        BoundReferenceTarget::Surface(symbol)
            if let Some(subject) = NamedTypeSymbolId::try_from_any(symbol) =>
        {
            bind_primary_constructor_candidates(
                context,
                subject,
                state,
                generic,
                diagnostics,
                candidates,
            )
        }
        BoundReferenceTarget::Surface(symbol) if symbol.kind().is_callable() => {
            Ok(bind_declaration_candidate(
                context,
                symbol,
                state,
                generic,
                None,
                diagnostics,
                candidates,
            )?
            .absence())
        }
        BoundReferenceTarget::Surface(symbol)
            if PredicateDefinitionSymbolId::try_from_any(symbol).is_some() =>
        {
            Ok(
                bind_predicate_candidate(context, symbol, state, generic, diagnostics, candidates)?
                    .absence(),
            )
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
    generic: CallGenericContext<'_>,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BinderFactResult<CandidateAbsence>
where
    C: BinderFactContext,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<CallableContractTemplateFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<PredicateSignatureTemplateFact>
        + SymbolFactProvider<CallableParameterDefaultTemplateFact>
        + SymbolFactProvider<CallableOverloadTemplateFact>,
{
    let (template, has_diagnostics) =
        symbol_fact_value::<_, CallableOverloadTemplateFact>(context, overload)?;

    let state = combine_recovery(state, has_diagnostics);
    let mut outcome = DeclarationCandidateOutcome::Ignored;

    for arm in template.arms() {
        let arm_outcome = match arm {
            OverloadArmTemplate::Resolved(symbol) => bind_declaration_candidate(
                context,
                *symbol,
                state,
                generic,
                None,
                diagnostics,
                candidates,
            )?,
            OverloadArmTemplate::Source(anchor) => bind_source_overload_arm(
                context,
                overload,
                *anchor,
                state,
                generic,
                diagnostics,
                candidates,
            )?,
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
    generic: CallGenericContext<'_>,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BinderFactResult<DeclarationCandidateOutcome>
where
    C: BinderFactContext,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<CallableContractTemplateFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<PredicateSignatureTemplateFact>
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
        MemberLookupResult::Found(name) => bind_resolved_name_candidate(
            context,
            name,
            state,
            generic,
            None,
            diagnostics,
            candidates,
        )?,
        MemberLookupResult::Inaccessible(names) => {
            let mut outcome = DeclarationCandidateOutcome::Ignored;

            for name in names {
                let candidate = bind_resolved_name_candidate(
                    context,
                    name,
                    CallableCandidateTemplateState::Inaccessible,
                    generic,
                    None,
                    diagnostics,
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
                    generic,
                    None,
                    diagnostics,
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

pub(super) fn bind_resolved_name_candidate<C>(
    context: &C,
    name: ResolvedName,
    state: CallableCandidateTemplateState,
    generic: CallGenericContext<'_>,
    inherited_generic: Option<NamedTypeSymbolId>,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BinderFactResult<DeclarationCandidateOutcome>
where
    C: BinderFactContext,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<CallableContractTemplateFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>
        + SymbolFactProvider<PredicateSignatureTemplateFact>
        + SymbolFactProvider<CallableParameterDefaultTemplateFact>
        + SymbolFactProvider<CallableOverloadTemplateFact>,
{
    if let ResolvedName::Surface(symbol) = name {
        if symbol.kind().is_callable() {
            return bind_declaration_candidate(
                context,
                symbol,
                state,
                generic,
                inherited_generic,
                diagnostics,
                candidates,
            );
        }

        if PredicateDefinitionSymbolId::try_from_any(symbol).is_some() {
            return bind_predicate_candidate(
                context,
                symbol,
                state,
                generic,
                diagnostics,
                candidates,
            );
        }
    }

    Ok(DeclarationCandidateOutcome::Ignored)
}

pub(super) fn bind_declaration_candidate<C>(
    context: &C,
    symbol: AnySymbolId,
    state: CallableCandidateTemplateState,
    call_generic: CallGenericContext<'_>,
    inherited_generic: Option<NamedTypeSymbolId>,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BinderFactResult<DeclarationCandidateOutcome>
where
    C: BinderFactContext,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>
        + SymbolFactProvider<CallableContractTemplateFact>
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

    // Candidate records outlive the provider borrow and therefore own the stable key.
    let Some(key) = context.symbol_key(symbol)?.cloned() else {
        return Ok(DeclarationCandidateOutcome::Ignored);
    };

    let (signature, signature_diagnostics) =
        symbol_fact_value::<_, CallableSignatureFact>(context, callable)?;

    let (contract, contract_diagnostics) =
        symbol_fact_value::<_, CallableContractTemplateFact>(context, callable)?;

    let generic_source = inherited_generic
        .and_then(|subject| GenericOwnerId::try_new(subject.into_any()))
        .unwrap_or(generic_owner);

    let (generic, generic_diagnostics) =
        symbol_fact_value::<_, GenericDeclarationTemplateFact>(context, generic_source)?;

    let Some(generic_arguments) =
        bind_generic_arguments(context, call_generic, &generic, diagnostics)?
    else {
        return Ok(DeclarationCandidateOutcome::Ignored);
    };

    let mut defaults = Vec::with_capacity(signature.parameters().len());

    let mut has_diagnostics = signature_diagnostics
        || contract_diagnostics
        || generic_diagnostics
        || generic_arguments.has_diagnostics;

    for parameter in signature.parameters() {
        let (value, default_diagnostics) =
            symbol_fact_value::<_, CallableParameterDefaultTemplateFact>(context, *parameter)?;

        has_diagnostics |= default_diagnostics;

        let provider = context.callable_parameter_default_provider(*parameter)?;

        defaults.push(CallableParameterDefaultTemplate::new(
            *parameter, value, provider,
        ));
    }

    let is_recovered = context.symbol_is_recovered(symbol)?.unwrap_or(true);
    let state = combine_recovery(state, is_recovered || has_diagnostics);

    candidates.push(CallableCandidateTemplate::Declaration(
        CallableDeclarationCandidateTemplate::new(
            key,
            definition,
            signature,
            contract,
            generic,
            generic_arguments.arguments,
            state,
        )
        .with_defaults(defaults),
    ));

    Ok(DeclarationCandidateOutcome::Added)
}

struct BoundGenericArguments {
    arguments: Vec<GenericArgumentTemplate>,
    has_diagnostics: bool,
}

fn bind_generic_arguments<C>(
    context: &C,
    call: CallGenericContext<'_>,
    declaration: &GenericDeclarationTemplate,
    diagnostics: &mut DiagnosticBag,
) -> BinderFactResult<Option<BoundGenericArguments>>
where
    C: BinderFactContext,
{
    if call.arguments.len() > declaration.parameters().len() {
        return Ok(None);
    }

    let result = if call.arguments.is_empty() {
        DiagnosticResult::without_diagnostics(Vec::new())
    } else {
        let cancellation = CandidateCancellation(context);

        // Each overload candidate owns an isolated type-expression binding scope.
        match TypeExpressionBinder::new(
            context.symbols(),
            context,
            context.semantic_values(),
            call.scope.clone(),
            &cancellation,
        )
        .bind_call_generic_arguments(call.arguments, declaration.parameters())
        {
            Ok(arguments) => arguments,
            Err(BinderFactError::Cancelled) => return Err(BinderFactError::Cancelled),
            Err(BinderFactError::DependencyUnavailable) => return Ok(None),
        }
    };

    let (arguments, argument_diagnostics) = result.into_parts();

    let has_diagnostics = !argument_diagnostics.is_empty();

    *diagnostics = diagnostics.merged(&argument_diagnostics);

    Ok(Some(BoundGenericArguments {
        arguments,
        has_diagnostics,
    }))
}

fn bind_predicate_candidate<C>(
    context: &C,
    symbol: AnySymbolId,
    state: CallableCandidateTemplateState,
    call_generic: CallGenericContext<'_>,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BinderFactResult<DeclarationCandidateOutcome>
where
    C: BinderFactContext,
    C::SymbolFacts: SymbolFactProvider<PredicateSignatureTemplateFact>
        + SymbolFactProvider<GenericDeclarationTemplateFact>,
{
    let Some(definition) = PredicateDefinitionSymbolId::try_from_any(symbol) else {
        return Ok(DeclarationCandidateOutcome::Ignored);
    };

    let Some(generic_owner) = GenericOwnerId::try_new(symbol) else {
        return Ok(DeclarationCandidateOutcome::Ignored);
    };

    let Some(key) = context.symbol_key(symbol)?.cloned() else {
        return Ok(DeclarationCandidateOutcome::Ignored);
    };

    let (signature, signature_diagnostics) =
        symbol_fact_value::<_, PredicateSignatureTemplateFact>(context, definition)?;

    let (generic, generic_diagnostics) =
        symbol_fact_value::<_, GenericDeclarationTemplateFact>(context, generic_owner)?;

    let Some(generic_arguments) =
        bind_generic_arguments(context, call_generic, &generic, diagnostics)?
    else {
        return Ok(DeclarationCandidateOutcome::Ignored);
    };

    let is_recovered = context.symbol_is_recovered(symbol)?.unwrap_or(true);

    let state = combine_recovery(
        state,
        is_recovered
            || signature_diagnostics
            || generic_diagnostics
            || generic_arguments.has_diagnostics,
    );

    candidates.push(CallableCandidateTemplate::Predicate(
        PredicateCandidateTemplate::new(
            key,
            definition,
            signature,
            generic,
            generic_arguments.arguments,
            state,
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

pub(super) const fn combine_recovery(
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
