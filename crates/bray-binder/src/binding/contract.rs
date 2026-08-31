use bray_compiler_known::RepresentationRole;
use bray_declarations::SyntaxAnchor;
use bray_symbols::{
    AnySymbolId, CallableSignatureQuery, CallableSymbolId, LocalScopeBoundary, LocalScopeId,
    NamedTypeSymbolId, StructSymbolId, SymbolQueryRequest, TypeData,
};

use crate::binder::Binder;
use crate::binding::BindingResult;
use crate::{BindingQueryContext, BindingQueryError, BindingQueryResult, SymbolQueryProvider};

pub(crate) fn push_contract_scope<C>(
    binder: &mut Binder<'_, C>,
    parent: LocalScopeId,
    syntax: SyntaxAnchor,
    has_result: bool,
) -> BindingResult<LocalScopeId, C::UpstreamError>
where
    C: BindingQueryContext + ?Sized,
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
    binding_context: &C,
    owner: AnySymbolId,
) -> BindingQueryResult<bool, C::UpstreamError>
where
    C: BindingQueryContext + ?Sized,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>,
{
    let Some(owner) = CallableSymbolId::try_from_any(owner) else {
        return Err(BindingQueryError::DependencyUnavailable);
    };

    let signature = binding_context
        .symbol_semantics()
        .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(owner))?;

    let Some(result_type) = signature.value().result().resolved_type() else {
        return Ok(true);
    };

    let result = binding_context
        .semantic_values()
        .type_data(result_type)
        .map_err(|_| BindingQueryError::DependencyUnavailable)?;

    let roles = binding_context
        .symbols()
        .compiler_known_provider()
        .role_registry();

    let unit = roles
        .representation_symbol::<StructSymbolId>(RepresentationRole::Unit)
        .ok_or(BindingQueryError::DependencyUnavailable)?;

    let never = roles
        .representation_symbol::<StructSymbolId>(RepresentationRole::Never)
        .ok_or(BindingQueryError::DependencyUnavailable)?;

    let unit = NamedTypeSymbolId::Struct(unit);
    let never = NamedTypeSymbolId::Struct(never);

    Ok(!matches!(
        &*result,
        TypeData::Named { definition, .. } if *definition == unit || *definition == never
    ))
}
