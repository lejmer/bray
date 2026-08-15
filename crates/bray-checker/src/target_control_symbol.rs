use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, InlineAssemblySymbol,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableDefinitionId, CallableInstanceData, CallableSignatureFact, GenericOwnerId,
    GenericSubstitutionData, SymbolFactRequest, TypeData, TypeId,
};

use crate::{
    CheckerFactError, CheckerInfrastructureError, CheckerRequestContext,
    CheckerSemanticFactProvider, CheckerUnitView,
};

pub(super) fn callable_symbol<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    expected: TypeId,
) -> Result<Option<InlineAssemblySymbol>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
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
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let Some(parameters) = request
        .symbols()
        .callable_generic_parameters(definition.callable_symbol())
    else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let open_arguments = parameters
        .iter()
        .copied()
        .map(|parameter| {
            request
                .semantic_values()
                .intern_generic_parameter_argument(parameter)
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let open = intern_substitution(request, owner, &parameters, open_arguments)?;

    let template = request
        .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(
            definition.callable_symbol(),
        ))
        .map_err(fact_error)?;

    let mut diagnostics = DiagnosticBag::new();

    let crate::expression::TemplateResolution::Resolved(open_signature) =
        crate::expression::resolve_signature(request, template.value(), open, &mut diagnostics)
            .map_err(fact_error)?
    else {
        return Ok(None);
    };

    let Some(arguments) = crate::expression::infer_generic_arguments_from_type(
        request,
        open_signature.callable_type(),
        expected,
        &parameters,
    )? else {
        return Ok(None);
    };

    let substitution = intern_substitution(request, owner, &parameters, arguments)?;

    let crate::expression::TemplateResolution::Resolved(signature) =
        crate::expression::resolve_signature(
            request,
            template.value(),
            substitution,
            &mut diagnostics,
        )
        .map_err(fact_error)?
    else {
        return Ok(None);
    };

    if signature.callable_type() != expected {
        return Ok(None);
    }

    let data = request
        .semantic_values()
        .type_data(expected)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

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
) -> Result<bray_symbols::GenericSubstitutionId, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let substitution = GenericSubstitutionData::try_new(owner, parameters.iter().copied(), arguments)
        .map_err(|_| CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

    request
        .semantic_values()
        .intern_generic_substitution(substitution)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
}

fn fact_error(error: CheckerFactError) -> CheckerInfrastructureError {
    match error {
        CheckerFactError::Cancelled => CheckerInfrastructureError::InvalidSemanticSelectionInput,
        CheckerFactError::Infrastructure(error) => error,
    }
}
