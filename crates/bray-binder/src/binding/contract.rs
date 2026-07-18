use bray_declarations::SyntaxAnchor;
use bray_symbols::{
    AnySymbolId, CallableSignatureFact, CallableSymbolId, LocalScopeBoundary, LocalScopeId,
    SymbolFactRequest, TypeData,
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

    let result = facts
        .semantic_values()
        .type_data(signature.value().result())
        .map_err(|_| BinderFactError::DependencyUnavailable)?;

    Ok(!matches!(&*result, TypeData::Tuple(elements) if elements.is_empty()))
}
