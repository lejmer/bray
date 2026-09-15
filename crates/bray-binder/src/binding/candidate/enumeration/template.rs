use bray_base::Cancellation;
use bray_bound_tree::{CallableDeclarationTemplate, CallableParameterDefaultTemplate};
use bray_diagnostics::{DiagnosticBag, DiagnosticResult};
use bray_symbols::{
    AnySymbolId, CallableContractTemplateQuery, CallableDefinitionId,
    CallableParameterDefaultTemplateQuery, CallableSignatureQuery, CallableSymbolId,
    GenericArgumentTemplate, GenericDeclarationTemplate, GenericDeclarationTemplateQuery,
    GenericOwnerId, GenericSubstitutionId,
};
use bray_syntax::GenericArgumentSyntax;

use super::callable::{CallGenericContext, resolve_symbol_query_value};
use crate::{
    BindingQueryContext, BindingQueryError, BindingQueryResult, SymbolQueryProvider,
    TypeExpressionBinder, TypeExpressionScope,
};

struct CandidateCancellation<'context, C: ?Sized>(&'context C);

impl<C> Cancellation for CandidateCancellation<'_, C>
where
    C: BindingQueryContext,
{
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

pub(super) struct BoundGenericArguments {
    pub(super) arguments: Vec<GenericArgumentTemplate>,
    pub(super) has_diagnostics: bool,
}

pub(super) fn bind_generic_arguments<C>(
    context: &C,
    call: CallGenericContext<'_>,
    declaration: &GenericDeclarationTemplate,
    diagnostics: &mut DiagnosticBag,
) -> BindingQueryResult<Option<BoundGenericArguments>, C::UpstreamError>
where
    C: BindingQueryContext,
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
            Err(BindingQueryError::Cancelled) => return Err(BindingQueryError::Cancelled),
            Err(BindingQueryError::CheckerInfrastructure(error)) => {
                return Err(BindingQueryError::CheckerInfrastructure(error));
            }
            Err(BindingQueryError::SemanticValue(error)) => {
                return Err(BindingQueryError::SemanticValue(error));
            }
            Err(BindingQueryError::Upstream(error)) => {
                return Err(BindingQueryError::Upstream(error));
            }
            Err(BindingQueryError::DependencyUnavailable) => return Ok(None),
            Err(
                error @ (BindingQueryError::MissingSyntax { .. }
                | BindingQueryError::MissingOwner { .. }
                | BindingQueryError::MissingModule { .. }
                | BindingQueryError::InvalidSurfaceName { .. }
                | BindingQueryError::Construction(_)
                | BindingQueryError::Binding(_)
                | BindingQueryError::Assembly(_)),
            ) => return Err(error),
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

pub(super) fn callable_declaration_template<C>(
    context: &C,
    symbol: AnySymbolId,
    generic: GenericDeclarationTemplate,
    arguments: impl IntoIterator<Item = GenericArgumentTemplate>,
) -> BindingQueryResult<Option<(CallableDeclarationTemplate, bool)>, C::UpstreamError>
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
) -> BindingQueryResult<DiagnosticResult<Option<CallableDeclarationTemplate>>, C::UpstreamError>
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
        .generic_substitution_data(inherited);

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
