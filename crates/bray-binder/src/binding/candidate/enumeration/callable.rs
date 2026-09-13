use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundUnit,
    BoundUnresolvedReferenceKind, DeclaredValueTypeTerm,
};
use bray_checker::{
    CallableCandidateTemplate, CallableCandidateTemplateState, CallableCandidateTemplates,
    CallableDeclarationCandidateTemplate, CallableValueCandidateTemplate, CandidateAbsence,
    ExpressionCandidateSet, PredicateCandidateTemplate,
};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallableContractTemplateQuery, CallableOverloadSymbolId,
    CallableOverloadTemplateQuery, CallableParameterDefaultTemplateQuery, CallableSignatureQuery,
    GenericArgumentTemplate, GenericDeclarationTemplate, GenericDeclarationTemplateQuery,
    GenericOwnerId, MemberLookupResult, NamedTypeSymbolId, OverloadArmTemplate,
    PredicateDefinitionSymbolId, PredicateSignatureTemplateQuery, SymbolQueryContract,
    SymbolQueryRequest,
};
use bray_syntax::{GenericArgumentSyntax, PathSyntax};

use super::template::{
    bind_generic_arguments, callable_declaration_template, combined_generic_declaration,
};

use crate::lookup::{NameAccess, ResolvedName, bind_module_path, bind_owner_path};
use crate::{
    BindingQueryContext, BindingQueryError, BindingQueryResult, SymbolQueryProvider,
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

#[derive(Clone, Copy)]
pub(super) struct InheritedGenericContext<'syntax> {
    pub(super) owner: NamedTypeSymbolId,
    pub(super) arguments: &'syntax [GenericArgumentSyntax],
}

#[derive(Clone, Copy)]
pub(super) enum DeclarationCandidateOutcome {
    Added,
    Ignored,
    UnavailableSemantics,
}

impl DeclarationCandidateOutcome {
    pub(super) const fn absence(self) -> CandidateAbsence {
        match self {
            Self::Added | Self::Ignored => CandidateAbsence::UnresolvedReference,
            Self::UnavailableSemantics => CandidateAbsence::UnavailableDeclarationSemantics,
        }
    }

    pub(super) const fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::UnavailableSemantics, _) | (_, Self::UnavailableSemantics) => {
                Self::UnavailableSemantics
            }
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
) -> BindingQueryResult<DiagnosticResult<ExpressionCandidateSet>, C::UpstreamError>
where
    C: BindingQueryContext,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>
        + SymbolQueryProvider<CallableContractTemplateQuery>
        + SymbolQueryProvider<GenericDeclarationTemplateQuery>
        + SymbolQueryProvider<PredicateSignatureTemplateQuery>
        + SymbolQueryProvider<CallableParameterDefaultTemplateQuery>
        + SymbolQueryProvider<CallableOverloadTemplateQuery>,
{
    let Some(callee_expression) = unit.view().expression(callee) else {
        return Err(BindingQueryError::DependencyUnavailable);
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

    if absence == CandidateAbsence::UnavailableDeclarationSemantics {
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
) -> BindingQueryResult<CandidateAbsence, C::UpstreamError>
where
    C: BindingQueryContext,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>
        + SymbolQueryProvider<CallableContractTemplateQuery>
        + SymbolQueryProvider<GenericDeclarationTemplateQuery>
        + SymbolQueryProvider<PredicateSignatureTemplateQuery>
        + SymbolQueryProvider<CallableParameterDefaultTemplateQuery>
        + SymbolQueryProvider<CallableOverloadTemplateQuery>,
{
    match target {
        BoundReferenceTarget::Surface(AnySymbolId::CallableOverload(overload)) => {
            bind_overload_candidates(
                context,
                overload,
                state,
                generic,
                None,
                diagnostics,
                candidates,
            )
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
    inherited_generic: Option<InheritedGenericContext<'_>>,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BindingQueryResult<CandidateAbsence, C::UpstreamError>
where
    C: BindingQueryContext,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>
        + SymbolQueryProvider<CallableContractTemplateQuery>
        + SymbolQueryProvider<GenericDeclarationTemplateQuery>
        + SymbolQueryProvider<PredicateSignatureTemplateQuery>
        + SymbolQueryProvider<CallableParameterDefaultTemplateQuery>
        + SymbolQueryProvider<CallableOverloadTemplateQuery>,
{
    let (template, has_diagnostics) =
        resolve_symbol_query_value::<_, CallableOverloadTemplateQuery>(context, overload)?;

    let state = combine_recovery(state, has_diagnostics);
    let mut outcome = DeclarationCandidateOutcome::Ignored;

    for arm in template.arms() {
        let arm_outcome = match arm {
            OverloadArmTemplate::Resolved(symbol) => bind_declaration_candidate(
                context,
                *symbol,
                state,
                generic,
                inherited_generic,
                diagnostics,
                candidates,
            )?,
            OverloadArmTemplate::Source(anchor) => bind_source_overload_arm(
                context,
                overload,
                *anchor,
                state,
                generic,
                inherited_generic,
                diagnostics,
                candidates,
            )?,
        };

        outcome = outcome.merge(arm_outcome);
    }

    Ok(match outcome {
        DeclarationCandidateOutcome::UnavailableSemantics => {
            CandidateAbsence::UnavailableDeclarationSemantics
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
    inherited_generic: Option<InheritedGenericContext<'_>>,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BindingQueryResult<DeclarationCandidateOutcome, C::UpstreamError>
where
    C: BindingQueryContext,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>
        + SymbolQueryProvider<CallableContractTemplateQuery>
        + SymbolQueryProvider<GenericDeclarationTemplateQuery>
        + SymbolQueryProvider<PredicateSignatureTemplateQuery>
        + SymbolQueryProvider<CallableParameterDefaultTemplateQuery>
        + SymbolQueryProvider<CallableOverloadTemplateQuery>,
{
    let Some(owner) = context.symbols().containing_symbol(overload.into()) else {
        return Ok(DeclarationCandidateOutcome::Ignored);
    };

    let Some(path) = anchor.find_descendant::<PathSyntax>(context.syntax()) else {
        return Ok(DeclarationCandidateOutcome::Ignored);
    };

    let lookup = match owner {
        AnySymbolId::Module(module) => {
            bind_module_path(context, module, &path, NameAccess::Internal)?
        }
        owner => bind_owner_path(context, owner, &path, NameAccess::Internal)?,
    };

    let outcome = match lookup {
        MemberLookupResult::Found(name) => bind_resolved_name_candidate(
            context,
            name,
            state,
            generic,
            inherited_generic,
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
                    inherited_generic,
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
                    inherited_generic,
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
    inherited_generic: Option<InheritedGenericContext<'_>>,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BindingQueryResult<DeclarationCandidateOutcome, C::UpstreamError>
where
    C: BindingQueryContext,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>
        + SymbolQueryProvider<CallableContractTemplateQuery>
        + SymbolQueryProvider<GenericDeclarationTemplateQuery>
        + SymbolQueryProvider<PredicateSignatureTemplateQuery>
        + SymbolQueryProvider<CallableParameterDefaultTemplateQuery>
        + SymbolQueryProvider<CallableOverloadTemplateQuery>,
{
    if let ResolvedName::Surface(AnySymbolId::CallableOverload(overload)) = name {
        let candidate_count = candidates.len();

        return Ok(
            if bind_overload_candidates(
                context,
                overload,
                state,
                generic,
                inherited_generic,
                diagnostics,
                candidates,
            )? == CandidateAbsence::UnavailableDeclarationSemantics
            {
                DeclarationCandidateOutcome::UnavailableSemantics
            } else if candidates.len() == candidate_count {
                DeclarationCandidateOutcome::Ignored
            } else {
                DeclarationCandidateOutcome::Added
            },
        );
    }

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
    inherited_generic: Option<InheritedGenericContext<'_>>,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BindingQueryResult<DeclarationCandidateOutcome, C::UpstreamError>
where
    C: BindingQueryContext,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>
        + SymbolQueryProvider<CallableContractTemplateQuery>
        + SymbolQueryProvider<GenericDeclarationTemplateQuery>
        + SymbolQueryProvider<CallableParameterDefaultTemplateQuery>,
{
    let Some(generic_owner) = GenericOwnerId::try_new(symbol) else {
        return Ok(DeclarationCandidateOutcome::Ignored);
    };

    let Some(generics) = bind_declaration_generics(
        context,
        generic_owner,
        call_generic,
        inherited_generic,
        diagnostics,
    )?
    else {
        return Ok(DeclarationCandidateOutcome::Ignored);
    };

    let Some((declaration, declaration_diagnostics)) =
        callable_declaration_template(context, symbol, generics.declaration, generics.arguments)?
    else {
        return Ok(DeclarationCandidateOutcome::Ignored);
    };

    let is_recovered = context.symbol_is_recovered(symbol)?.unwrap_or(true);

    let state = combine_recovery(
        state,
        is_recovered || generics.has_diagnostics || declaration_diagnostics,
    );

    candidates.push(CallableCandidateTemplate::Declaration(
        CallableDeclarationCandidateTemplate::from_declaration(declaration, state),
    ));

    Ok(DeclarationCandidateOutcome::Added)
}

struct BoundDeclarationGenerics {
    declaration: GenericDeclarationTemplate,
    arguments: Vec<GenericArgumentTemplate>,
    has_diagnostics: bool,
}

fn bind_declaration_generics<C>(
    context: &C,
    owner: GenericOwnerId,
    call: CallGenericContext<'_>,
    inherited: Option<InheritedGenericContext<'_>>,
    diagnostics: &mut DiagnosticBag,
) -> BindingQueryResult<Option<BoundDeclarationGenerics>, C::UpstreamError>
where
    C: BindingQueryContext,
    C::SymbolSemantics: SymbolQueryProvider<GenericDeclarationTemplateQuery>,
{
    let Some(inherited) = inherited else {
        let (declaration, declaration_diagnostics) =
            resolve_symbol_query_value::<_, GenericDeclarationTemplateQuery>(context, owner)?;

        // Keep direct candidates for the checker's rejection of the full source argument count.
        // Type binding only visits arguments with corresponding declaration parameters.
        let argument_count = call.arguments.len().min(declaration.parameters().len());

        let call = CallGenericContext {
            arguments: &call.arguments[..argument_count],
            scope: call.scope,
        };

        let Some(arguments) = bind_generic_arguments(context, call, &declaration, diagnostics)?
        else {
            return Ok(None);
        };

        return Ok(Some(BoundDeclarationGenerics {
            declaration,
            arguments: arguments.arguments,
            has_diagnostics: declaration_diagnostics || arguments.has_diagnostics,
        }));
    };

    let inherited_owner = GenericOwnerId::try_new(inherited.owner.into_any())
        .ok_or(BindingQueryError::DependencyUnavailable)?;

    let (inherited_declaration, inherited_diagnostics) =
        resolve_symbol_query_value::<_, GenericDeclarationTemplateQuery>(context, inherited_owner)?;

    let (direct_declaration, direct_diagnostics) =
        resolve_symbol_query_value::<_, GenericDeclarationTemplateQuery>(context, owner)?;

    let declaration =
        combined_generic_declaration(owner, &inherited_declaration, &direct_declaration);

    let argument_syntax = inherited
        .arguments
        .iter()
        .chain(call.arguments)
        .cloned()
        .collect::<Vec<_>>();

    let combined_call = CallGenericContext {
        arguments: &argument_syntax,
        scope: call.scope,
    };

    let Some(arguments) =
        bind_generic_arguments(context, combined_call, &declaration, diagnostics)?
    else {
        return Ok(None);
    };

    Ok(Some(BoundDeclarationGenerics {
        declaration,
        arguments: arguments.arguments,
        has_diagnostics: inherited_diagnostics || direct_diagnostics || arguments.has_diagnostics,
    }))
}

fn bind_predicate_candidate<C>(
    context: &C,
    symbol: AnySymbolId,
    state: CallableCandidateTemplateState,
    call_generic: CallGenericContext<'_>,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BindingQueryResult<DeclarationCandidateOutcome, C::UpstreamError>
where
    C: BindingQueryContext,
    C::SymbolSemantics: SymbolQueryProvider<PredicateSignatureTemplateQuery>
        + SymbolQueryProvider<GenericDeclarationTemplateQuery>,
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
        resolve_symbol_query_value::<_, PredicateSignatureTemplateQuery>(context, definition)?;

    let (generic, generic_diagnostics) =
        resolve_symbol_query_value::<_, GenericDeclarationTemplateQuery>(context, generic_owner)?;

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

pub(super) fn resolve_symbol_query_value<C, F>(
    context: &C,
    owner: F::Owner,
) -> BindingQueryResult<(F::Value, bool), C::UpstreamError>
where
    C: BindingQueryContext + ?Sized,
    F: SymbolQueryContract,
    F::Value: Clone,
    C::SymbolSemantics: SymbolQueryProvider<F>,
{
    let result = context
        .symbol_semantics()
        .resolve_symbol_query(SymbolQueryRequest::<F>::new(owner))?;

    let has_diagnostics = !result.diagnostics().is_empty();

    // The candidate owns the query value while recursive semantic storage remains Arc-shared.
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
        (_, CandidateAbsence::UnavailableDeclarationSemantics) => {
            CandidateAbsence::UnavailableDeclarationSemantics
        }
        (CandidateAbsence::UnavailableDeclarationSemantics, _) => {
            CandidateAbsence::UnavailableDeclarationSemantics
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
