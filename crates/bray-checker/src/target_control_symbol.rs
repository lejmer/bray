use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, InlineAssemblySymbol,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableDefinitionId, CallableInstanceData, CallableSignatureQuery, GenericOwnerId,
    GenericSubstitutionData, SymbolQueryRequest, TypeData, TypeId,
};

use crate::{
    CheckerQueryError, CheckerRequestContext, CheckerSemanticQueryProvider, CheckerUnitView,
};

pub(super) fn callable_symbol<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    expected: TypeId,
) -> Result<Option<InlineAssemblySymbol>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    let Some(BoundExpression::Name(name)) = request.view().expression(expression) else {
        return Ok(None);
    };

    let BoundReferenceTarget::Surface(symbol) = name.target() else {
        return Ok(None);
    };

    let Some(definition) = CallableDefinitionId::try_new(symbol) else {
        return Ok(None);
    };

    let Some(owner) = GenericOwnerId::try_new(definition.symbol()) else {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in callable_symbol"
        );
    };

    let Some(parameters) = request
        .symbols()
        .callable_generic_parameters(definition.callable_symbol())
    else {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in callable_symbol"
        );
    };

    let open_arguments = parameters
        .iter()
        .copied()
        .map(|parameter| {
            request
                .semantic_values()
                .intern_generic_parameter_argument(parameter)
                .unwrap_or_else(|error| {
                    panic!(
                        "callable_symbol must satisfy its checked construction contract: {error:?}"
                    )
                })
        })
        .collect::<Vec<_>>();

    let open = intern_substitution(request, owner, &parameters, open_arguments);

    let template = request.resolve_symbol_query(
        SymbolQueryRequest::<CallableSignatureQuery>::new(definition.callable_symbol()),
    )?;

    let mut diagnostics = DiagnosticBag::new();

    let crate::expression::TemplateResolution::Resolved(open_signature) =
        crate::expression::resolve_signature(request, template.value(), open, &mut diagnostics)?
    else {
        return Ok(None);
    };

    let Some(arguments) = crate::expression::infer_generic_arguments_from_type(
        request,
        open_signature.callable_type(),
        expected,
        &parameters,
    ) else {
        return Ok(None);
    };

    let substitution = intern_substitution(request, owner, &parameters, arguments);

    let crate::expression::TemplateResolution::Resolved(signature) =
        crate::expression::resolve_signature(
            request,
            template.value(),
            substitution,
            &mut diagnostics,
        )?
    else {
        return Ok(None);
    };

    if signature.callable_type() != expected {
        return Ok(None);
    }

    let data = request.semantic_values().type_data(expected);

    let TypeData::Callable(callable) = data.as_ref() else {
        return Ok(None);
    };

    Ok(Some(InlineAssemblySymbol::new(
        CallableInstanceData::new(definition, substitution),
        callable.abi(),
    )))
}
fn intern_substitution<C>(
    request: CheckerUnitView<'_, C>,
    owner: GenericOwnerId,
    parameters: &[bray_symbols::GenericParameterSymbolId],
    arguments: Vec<bray_symbols::GenericArgument>,
) -> bray_symbols::GenericSubstitutionId
where
    C: CheckerRequestContext + ?Sized,
{
    let substitution =
        GenericSubstitutionData::try_new(owner, parameters.iter().copied(), arguments).unwrap_or_else(|error| panic!("Generic substitution construction rejected an exact parameter-to-argument relationship. in intern_substitution: {error:?}"));

    request
        .semantic_values()
        .intern_generic_substitution(substitution)
        .unwrap_or_else(|error| {
            panic!("intern_substitution must satisfy its checked construction contract: {error:?}")
        })
}
