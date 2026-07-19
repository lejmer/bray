use bray_compiler_known::RepresentationRole;
use bray_declarations::SyntaxAnchor;
use bray_symbols::{
    AnySymbolId, CallableSignatureFact, CallableSymbolId, LocalScopeBoundary, LocalScopeId,
    NamedTypeSymbolId, StructSymbolId, SymbolFactRequest, TypeData,
};

use crate::binder::Binder;
use crate::binding::BindingResult;
use crate::{BinderFactContext, BinderFactError, BinderFactResult, SymbolFactProvider};

pub(crate) fn push_contract_scope<C>(
    binder: &mut Binder<'_, C>,
    parent: LocalScopeId,
    syntax: SyntaxAnchor,
    has_result: bool,
) -> BindingResult<LocalScopeId>
where
    C: BinderFactContext + ?Sized,
{
    let scope = binder.unit_mut().push_scope(
        parent,
        LocalScopeBoundary::Contract,
        syntax,
        syntax.full_range().start(),
    )?;

    if has_result {
        binder
            .unit_mut()
            .push_postcondition_result(scope, syntax, None, syntax.is_recovered())?;
    }

    Ok(scope)
}

pub(crate) fn callable_normal_completion_has_value<C>(
    facts: &C,
    owner: AnySymbolId,
) -> BinderFactResult<bool>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>,
{
    let Some(owner) = CallableSymbolId::try_from_any(owner) else {
        return Err(BinderFactError::DependencyUnavailable);
    };

    let signature = facts
        .symbol_facts()
        .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(owner))?;

    let Some(result_type) = signature.value().result().resolved_type() else {
        return Ok(true);
    };

    let result = facts
        .semantic_values()
        .type_data(result_type)
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

    let roles = facts.symbols().compiler_known_provider().role_registry();

    let unit = roles
        .representation_symbol::<StructSymbolId>(RepresentationRole::Unit)
        .ok_or(BinderFactError::DependencyUnavailable)?;

    let never = roles
        .representation_symbol::<StructSymbolId>(RepresentationRole::Never)
        .ok_or(BinderFactError::DependencyUnavailable)?;

    let unit = NamedTypeSymbolId::Struct(unit);
    let never = NamedTypeSymbolId::Struct(never);

    Ok(!matches!(
        &*result,
        TypeData::Named { definition, .. } if *definition == unit || *definition == never
    ))
}
