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
use bray_syntax::{GenericArgumentListSyntax, GenericArgumentSyntax};

use crate::lookup::ResolvedName;
use crate::{BindingQueryContext, BindingQueryResult, SymbolQueryProvider};

use super::callable::{
    CallGenericContext, DeclarationCandidateOutcome, InheritedGenericContext,
    bind_declaration_candidate, bind_resolved_name_candidate, combine_recovery,
};

pub(super) fn bind_type_member_candidates<C>(
    context: &C,
    unit: &BoundUnit,
    callee: BoundExpressionId,
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
    let Some(subject) = type_member_subject(context, unit, callee)? else {
        return Ok(CandidateAbsence::UnavailableDeclarationSemantics);
    };

    let Some(BoundExpression::MemberAccess(member)) = unit.view().expression(callee) else {
        return Ok(CandidateAbsence::UnavailableDeclarationSemantics);
    };

    let inherited_arguments = receiver_generic_arguments(context, unit, member.receiver())?;

    let inherited = match unit.view().expression(member.receiver()) {
        Some(BoundExpression::Name(name))
            if matches!(name.target(), BoundReferenceTarget::TypeQualifier(_)) =>
        {
            let BoundReferenceTarget::TypeQualifier(ty) = name.target() else {
                return Err(crate::BindingQueryError::DependencyUnavailable);
            };

            let data = context
                .semantic_values()
                .type_data(ty)
                .map_err(crate::BindingQueryError::SemanticValue)?;

            let bray_symbols::TypeData::Named { substitution, .. } = data.as_ref() else {
                return Err(crate::BindingQueryError::DependencyUnavailable);
            };

            InheritedGenericContext::Instantiated(*substitution)
        }
        _ => InheritedGenericContext::Written {
            owner: subject,
            arguments: &inherited_arguments,
        },
    };

    let Some(BoundMemberSelector::Name(name)) = member.selector() else {
        return Ok(CandidateAbsence::UnresolvedReference);
    };

    let surface = context.type_associated_surface(subject)?;
    let state = combine_recovery(state, !surface.diagnostics().is_empty());

    *diagnostics = diagnostics.merged(surface.diagnostics());

    let lookup = surface.value().lookup(name.as_str());
    let first_candidate = candidates.len();

    let outcome = match lookup {
        MemberLookupResult::Found(symbol) => bind_resolved_name_candidate(
            context,
            ResolvedName::Surface(symbol),
            state,
            generic,
            Some(inherited),
            diagnostics,
            candidates,
        )?,
        MemberLookupResult::Inaccessible(symbols) => bind_member_candidates(
            context,
            symbols,
            CallableCandidateTemplateState::Inaccessible,
            generic,
            inherited,
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
            inherited,
            diagnostics,
            candidates,
        )?,
        MemberLookupResult::NotFound => DeclarationCandidateOutcome::Ignored,
    };

    for candidate in &mut candidates[first_candidate..] {
        let CallableCandidateTemplate::Declaration(declaration) = candidate else {
            continue;
        };

        let Some(member) = surface.value().member(declaration.definition().symbol()) else {
            continue;
        };

        let bray_symbols::TypeAssociatedMemberOrigin::InherentImplementation(owner) =
            member.origin()
        else {
            continue;
        };

        let parameters = surface.value().generic().parameters();

        let arguments = parameters
            .iter()
            .copied()
            .map(|parameter| {
                context
                    .semantic_values()
                    .intern_generic_parameter_argument(parameter)
                    .map(bray_symbols::GenericArgumentTemplate::Resolved)
            })
            .collect::<Result<Vec<_>, _>>()
            .map_err(crate::BindingQueryError::SemanticValue)?;

        let replacement = bray_symbols::TypeExpressionTemplate::Named {
            definition: subject,
            parameters: parameters.into(),
            arguments: arguments.into(),
        };

        // The candidate keeps its shared declaration while attaching this lookup's subject.
        *declaration = declaration.clone().with_contextual_self(
            bray_symbols::SelfTypeContext::Implementation(owner.into()),
            replacement,
        );
    }

    Ok(outcome.absence())
}

fn receiver_generic_arguments<C>(
    context: &C,
    unit: &BoundUnit,
    receiver: BoundExpressionId,
) -> BindingQueryResult<Vec<GenericArgumentSyntax>, C::UpstreamError>
where
    C: BindingQueryContext + ?Sized,
{
    let Some(BoundExpression::Name(receiver)) = unit.view().expression(receiver) else {
        return Ok(Vec::new());
    };

    let Some(anchor) = receiver.generic_argument_list() else {
        return Ok(Vec::new());
    };

    let arguments = anchor
        .find_descendant::<GenericArgumentListSyntax>(context.syntax())
        .ok_or(crate::BindingQueryError::DependencyUnavailable)?;

    Ok(arguments.generic_arguments().collect())
}

pub(super) fn type_member_subject<C>(
    context: &C,
    unit: &BoundUnit,
    expression: BoundExpressionId,
) -> BindingQueryResult<Option<NamedTypeSymbolId>, C::UpstreamError>
where
    C: BindingQueryContext + ?Sized,
{
    let Some(BoundExpression::MemberAccess(member)) = unit.view().expression(expression) else {
        return Ok(None);
    };

    let Some(BoundExpression::Name(receiver)) = unit.view().expression(member.receiver()) else {
        return Ok(None);
    };

    let owner = match receiver.target() {
        BoundReferenceTarget::Surface(owner) => Some(owner),
        BoundReferenceTarget::TypeQualifier(ty) => context
            .semantic_values()
            .type_data(ty)
            .map_err(crate::BindingQueryError::SemanticValue)?
            .declaration_owner(),
        BoundReferenceTarget::Local(_) => None,
    };

    Ok(owner.and_then(NamedTypeSymbolId::try_from_any))
}

pub(super) fn bind_primary_constructor_candidates<C>(
    context: &C,
    subject: NamedTypeSymbolId,
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
            CallGenericContext {
                arguments: &[],
                scope: generic.scope,
            },
            Some(InheritedGenericContext::Written {
                owner: subject,
                arguments: generic.arguments,
            }),
            diagnostics,
            candidates,
        )?;

        outcome = outcome.merge(candidate);
    }

    Ok(match outcome {
        DeclarationCandidateOutcome::Added => CandidateAbsence::UnresolvedReference,
        DeclarationCandidateOutcome::Ignored => CandidateAbsence::EmptyOverload,
        DeclarationCandidateOutcome::UnavailableSemantics => {
            CandidateAbsence::UnavailableDeclarationSemantics
        }
    })
}

fn bind_member_candidates<C>(
    context: &C,
    symbols: impl IntoIterator<Item = AnySymbolId>,
    state: CallableCandidateTemplateState,
    generic: CallGenericContext<'_>,
    inherited_generic: InheritedGenericContext<'_>,
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
