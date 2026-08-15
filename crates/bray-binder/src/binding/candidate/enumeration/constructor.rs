use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundMemberSelector, BoundReferenceTarget, BoundUnit,
};
use bray_checker::{CallableCandidateTemplate, CallableCandidateTemplateState, CandidateAbsence};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, CallableContractTemplateQuery, CallableOverloadTemplateQuery,
    CallableParameterDefaultTemplateQuery, CallableSignatureQuery, GenericDeclarationTemplateQuery,
    MemberLookupResult, NamedTypeSymbolId, PredicateSignatureTemplateQuery,
    TypeAssociatedLifecycleSlot,
};

use crate::lookup::ResolvedName;
use crate::{BindingQueryContext, BindingQueryResult, SymbolQueryProvider};

use super::callable::{
    CallGenericContext, DeclarationCandidateOutcome, bind_declaration_candidate,
    bind_resolved_name_candidate, combine_recovery,
};

pub(super) fn bind_type_member_candidates<C>(
    context: &C,
    unit: &BoundUnit,
    callee: BoundExpressionId,
    state: CallableCandidateTemplateState,
    generic: CallGenericContext<'_>,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BindingQueryResult<CandidateAbsence>
where
    C: BindingQueryContext,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>
        + SymbolQueryProvider<CallableContractTemplateQuery>
        + SymbolQueryProvider<GenericDeclarationTemplateQuery>
        + SymbolQueryProvider<PredicateSignatureTemplateQuery>
        + SymbolQueryProvider<CallableParameterDefaultTemplateQuery>
        + SymbolQueryProvider<CallableOverloadTemplateQuery>,
{
    let Some(subject) = type_member_subject(unit, callee) else {
        return Ok(CandidateAbsence::UnavailableDeclarationSemantics);
    };

    let Some(BoundExpression::MemberAccess(member)) = unit.view().expression(callee) else {
        return Ok(CandidateAbsence::UnavailableDeclarationSemantics);
    };

    let Some(BoundMemberSelector::Name(name)) = member.selector() else {
        return Ok(CandidateAbsence::UnresolvedReference);
    };

    let surface = context.type_associated_surface(subject)?;
    let state = combine_recovery(state, !surface.diagnostics().is_empty());

    *diagnostics = diagnostics.merged(surface.diagnostics());

    let lookup = surface.value().lookup(name.as_str());

    let outcome = match lookup {
        MemberLookupResult::Found(symbol) => bind_resolved_name_candidate(
            context,
            ResolvedName::Surface(symbol),
            state,
            generic,
            Some(subject),
            diagnostics,
            candidates,
        )?,
        MemberLookupResult::Inaccessible(symbols) => bind_member_candidates(
            context,
            symbols,
            CallableCandidateTemplateState::Inaccessible,
            generic,
            subject,
            diagnostics,
            candidates,
        )?,
        MemberLookupResult::WrongKind(symbols)
        | MemberLookupResult::Ambiguous(symbols)
        | MemberLookupResult::Malformed(symbols) => bind_member_candidates(
            context,
            symbols,
            CallableCandidateTemplateState::Recovered,
            generic,
            subject,
            diagnostics,
            candidates,
        )?,
        MemberLookupResult::NotFound => DeclarationCandidateOutcome::Ignored,
    };

    Ok(outcome.absence())
}

pub(super) fn type_member_subject(
    unit: &BoundUnit,
    expression: BoundExpressionId,
) -> Option<NamedTypeSymbolId> {
    let Some(BoundExpression::MemberAccess(member)) = unit.view().expression(expression) else {
        return None;
    };

    let Some(BoundExpression::Name(receiver)) = unit.view().expression(member.receiver()) else {
        return None;
    };

    let BoundReferenceTarget::Surface(receiver) = receiver.target() else {
        return None;
    };

    NamedTypeSymbolId::try_from_any(receiver)
}

pub(super) fn bind_primary_constructor_candidates<C>(
    context: &C,
    subject: NamedTypeSymbolId,
    state: CallableCandidateTemplateState,
    generic: CallGenericContext<'_>,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BindingQueryResult<CandidateAbsence>
where
    C: BindingQueryContext,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>
        + SymbolQueryProvider<CallableContractTemplateQuery>
        + SymbolQueryProvider<GenericDeclarationTemplateQuery>
        + SymbolQueryProvider<CallableParameterDefaultTemplateQuery>,
{
    let surface = context.type_associated_surface(subject)?;
    let state = combine_recovery(state, !surface.diagnostics().is_empty());
    let mut outcome = DeclarationCandidateOutcome::Ignored;

    *diagnostics = diagnostics.merged(surface.diagnostics());

    for member in surface.value().lifecycle_members() {
        if member.slot() != TypeAssociatedLifecycleSlot::PrimaryConstructor {
            continue;
        }

        let candidate = bind_declaration_candidate(
            context,
            member.id(),
            state,
            generic,
            Some(subject),
            diagnostics,
            candidates,
        )?;

        outcome = outcome.merge(candidate);
    }

    Ok(outcome.absence())
}

fn bind_member_candidates<C>(
    context: &C,
    symbols: impl IntoIterator<Item = AnySymbolId>,
    state: CallableCandidateTemplateState,
    generic: CallGenericContext<'_>,
    inherited_generic: NamedTypeSymbolId,
    diagnostics: &mut DiagnosticBag,
    candidates: &mut Vec<CallableCandidateTemplate>,
) -> BindingQueryResult<DeclarationCandidateOutcome>
where
    C: BindingQueryContext,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>
        + SymbolQueryProvider<CallableContractTemplateQuery>
        + SymbolQueryProvider<GenericDeclarationTemplateQuery>
        + SymbolQueryProvider<PredicateSignatureTemplateQuery>
        + SymbolQueryProvider<CallableParameterDefaultTemplateQuery>
        + SymbolQueryProvider<CallableOverloadTemplateQuery>,
{
    let mut outcome = DeclarationCandidateOutcome::Ignored;

    for symbol in symbols {
        let candidate = bind_resolved_name_candidate(
            context,
            ResolvedName::Surface(symbol),
            state,
            generic,
            Some(inherited_generic),
            diagnostics,
            candidates,
        )?;

        outcome = outcome.merge(candidate);
    }

    Ok(outcome)
}
