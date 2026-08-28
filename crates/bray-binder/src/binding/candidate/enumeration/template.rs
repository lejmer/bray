use bray_bound_tree::{CallableDeclarationTemplate, CallableParameterDefaultTemplate};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallableContractTemplateQuery, CallableDefinitionId,
    CallableParameterDefaultTemplateQuery, CallableSignatureQuery, CallableSymbolId,
    GenericArgumentTemplate, GenericDeclarationTemplate, GenericDeclarationTemplateQuery,
    GenericOwnerId, GenericSubstitutionId,
};
use bray_syntax::GenericArgumentSyntax;

use super::callable::{CallGenericContext, bind_generic_arguments, resolve_symbol_query_value};
use crate::{
    BindingQueryContext, BindingQueryError, BindingQueryResult, SymbolQueryProvider,
    TypeExpressionScope,
};

pub(super) fn callable_declaration_template<C>(
    context: &C,
    symbol: AnySymbolId,
    generic: GenericDeclarationTemplate,
    arguments: impl IntoIterator<Item = GenericArgumentTemplate>,
) -> BindingQueryResult<Option<(CallableDeclarationTemplate, bool)>>
where
    C: BindingQueryContext + ?Sized,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>
        + SymbolQueryProvider<CallableContractTemplateQuery>
        + SymbolQueryProvider<CallableParameterDefaultTemplateQuery>,
{
    let Some(definition) = CallableDefinitionId::try_new(symbol) else {
        return Ok(None);
    };

    let Some(callable) = CallableSymbolId::try_from_any(symbol) else {
        return Ok(None);
    };

    // Candidate records outlive the provider borrow and therefore own the stable key.
    let Some(key) = context.symbol_key(symbol)?.cloned() else {
        return Ok(None);
    };

    let (signature, signature_diagnostics) =
        resolve_symbol_query_value::<_, CallableSignatureQuery>(context, callable)?;

    let (contract, contract_diagnostics) =
        resolve_symbol_query_value::<_, CallableContractTemplateQuery>(context, callable)?;

    let mut defaults = Vec::with_capacity(signature.parameters().len());
    let mut has_diagnostics = signature_diagnostics || contract_diagnostics;

    for parameter in signature.parameters() {
        let (value, default_diagnostics) = resolve_symbol_query_value::<
            _,
            CallableParameterDefaultTemplateQuery,
        >(context, *parameter)?;

        has_diagnostics |= default_diagnostics;

        defaults.push(CallableParameterDefaultTemplate::new(
            *parameter,
            value,
            context.callable_parameter_default_provider(*parameter)?,
        ));
    }

    Ok(Some((
        CallableDeclarationTemplate::new(key, definition, signature, contract, generic, arguments)
            .with_defaults(defaults),
        has_diagnostics,
    )))
}

/// Binds one receiver-selected callable with inherited generic arguments.
pub fn bind_member_callable_template<C>(
    context: &C,
    symbol: AnySymbolId,
    inherited: GenericSubstitutionId,
    arguments: &[GenericArgumentSyntax],
    scope: &TypeExpressionScope,
) -> BindingQueryResult<DiagnosticResult<Option<CallableDeclarationTemplate>>>
where
    C: BindingQueryContext,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>
        + SymbolQueryProvider<CallableContractTemplateQuery>
        + SymbolQueryProvider<GenericDeclarationTemplateQuery>
        + SymbolQueryProvider<CallableParameterDefaultTemplateQuery>,
{
    let Some(owner) = GenericOwnerId::try_new(symbol) else {
        return Ok(DiagnosticResult::without_diagnostics(None));
    };

    let inherited = context
        .semantic_values()
        .generic_substitution_data(inherited)
        .map_err(|_| BindingQueryError::DependencyUnavailable)?;

    let (inherited_declaration, inherited_diagnostics) = resolve_symbol_query_value::<
        _,
        GenericDeclarationTemplateQuery,
    >(context, inherited.owner())?;

    let (direct_declaration, direct_diagnostics) =
        resolve_symbol_query_value::<_, GenericDeclarationTemplateQuery>(context, owner)?;

    let generic = combined_generic_declaration(owner, &inherited_declaration, &direct_declaration);
    let mut diagnostics = DiagnosticBag::new();

    let Some(direct_arguments) = bind_generic_arguments(
        context,
        CallGenericContext { arguments, scope },
        &direct_declaration,
        &mut diagnostics,
    )?
    else {
        return Ok(DiagnosticResult::new(None, diagnostics));
    };

    if inherited.bindings().len() != inherited_declaration.parameters().len() {
        return Err(BindingQueryError::DependencyUnavailable);
    }

    let generic_arguments = inherited
        .bindings()
        .iter()
        .map(|binding| GenericArgumentTemplate::Resolved(binding.argument()))
        .chain(direct_arguments.arguments);

    let declaration = callable_declaration_template(context, symbol, generic, generic_arguments)?
        .and_then(|(declaration, declaration_diagnostics)| {
            (!inherited_diagnostics
                && !direct_diagnostics
                && !direct_arguments.has_diagnostics
                && !declaration_diagnostics)
                .then_some(declaration)
        });

    Ok(DiagnosticResult::new(declaration, diagnostics))
}

pub(super) fn combined_generic_declaration(
    owner: GenericOwnerId,
    inherited: &GenericDeclarationTemplate,
    direct: &GenericDeclarationTemplate,
) -> GenericDeclarationTemplate {
    GenericDeclarationTemplate::new(
        owner,
        inherited
            .parameters()
            .iter()
            .chain(direct.parameters())
            .copied(),
        inherited
            .constraints()
            .iter()
            .chain(direct.constraints())
            .cloned(),
    )
}
