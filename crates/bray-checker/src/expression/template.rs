use bray_bound_tree::{
    BoundCallResult, BoundCallableTarget, BoundFutureConstruction, BoundResolvedCall,
};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    CallableExecution, CallableInstanceData, CallableParameterSignature, CallableSignature,
    CallableSignatureTemplate, CallableTypeData, GenericArgument, GenericParameterSymbolId,
    GenericSubstitutionData, GenericSubstitutionId, NamedTypeSymbolId, SemanticValueStore,
    StructSymbolId, SymbolProvider, TypeData, TypeExpressionTemplate, TypeId,
};

use crate::{
    CallableCandidate, CallableCandidateState, CallableCandidateTemplateState,
    CallableDeclarationCandidateTemplate, CheckedConstantTerms, CheckerInfrastructureError,
    resolve_type_expression_template,
};

pub(super) enum TemplateResolution<T> {
    Resolved(T),
    Unsupported,
}

pub(super) fn resolve_declaration_candidate<C>(
    request: crate::CheckerUnitView<'_, C>,
    template: &CallableDeclarationCandidateTemplate,
) -> Result<TemplateResolution<CallableCandidate>, CheckerInfrastructureError>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    let values = request.semantic_values();

    if template.generic_arguments().len() != template.generic().parameters().len() {
        return Ok(TemplateResolution::Unsupported);
    }

    let arguments = template
        .generic_arguments()
        .iter()
        .map(bray_symbols::GenericArgumentTemplate::resolved_argument)
        .collect::<Option<Vec<_>>>();

    let Some(arguments) = arguments else {
        return Ok(TemplateResolution::Unsupported);
    };

    let substitution = GenericSubstitutionData::try_new(
        template.generic().owner(),
        template.generic().parameters().iter().copied(),
        arguments,
    )
    .map_err(|_| CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

    let substitution = values
        .intern_generic_substitution(substitution)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let signature = match resolve_signature(values, template.signature(), substitution)? {
        TemplateResolution::Resolved(signature) => signature,
        TemplateResolution::Unsupported => return Ok(TemplateResolution::Unsupported),
    };

    let callable_type = values
        .type_data(signature.callable_type())
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let TypeData::Callable(callable_type) = callable_type.as_ref() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let instance = CallableInstanceData::new(template.definition(), substitution);

    let result = call_result(request, callable_type, signature.result())?;

    let resolution = BoundResolvedCall::new(BoundCallableTarget::Declaration(instance), [], result);

    let mut defaults = Vec::new();

    for default in template.defaults() {
        if !default.value().is_present() {
            continue;
        }

        let Some(provider) = default.provider() else {
            return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
        };

        defaults.push((default.parameter(), provider));
    }

    // Selection owns its candidate key while binder templates remain reusable.
    let key = template.key().clone();

    Ok(TemplateResolution::Resolved(
        CallableCandidate::new(
            key,
            resolution,
            signature,
            defaults,
            candidate_state(template.state()),
        )
        .with_generic_constraints(template.generic().constraints().iter().copied()),
    ))
}

pub(super) fn resolve_type_template(
    values: &SemanticValueStore,
    template: &TypeExpressionTemplate,
) -> Result<TemplateResolution<TypeId>, CheckerInfrastructureError> {
    resolve_type_expression_template(values, template, &CheckedConstantTerms::new()).map(|ty| {
        ty.map_or(
            TemplateResolution::Unsupported,
            TemplateResolution::Resolved,
        )
    })
}

fn resolve_signature(
    values: &SemanticValueStore,
    template: &CallableSignatureTemplate,
    substitution: GenericSubstitutionId,
) -> Result<TemplateResolution<CallableSignature>, CheckerInfrastructureError> {
    let callable_type = match resolve_type_template(values, template.callable_type())? {
        TemplateResolution::Resolved(ty) => substitute_type(values, ty, substitution)?,
        TemplateResolution::Unsupported => return Ok(TemplateResolution::Unsupported),
    };

    let result = match resolve_type_template(values, template.result())? {
        TemplateResolution::Resolved(ty) => substitute_type(values, ty, substitution)?,
        TemplateResolution::Unsupported => return Ok(TemplateResolution::Unsupported),
    };

    let parameter_templates = template
        .parameter_type_templates(values)
        .map_err(|_| CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

    let mut parameters = Vec::with_capacity(parameter_templates.len());

    for (parameter, parameter_template) in template
        .parameters()
        .iter()
        .copied()
        .zip(parameter_templates)
    {
        let ty = match resolve_type_template(values, &parameter_template)? {
            TemplateResolution::Resolved(ty) => substitute_type(values, ty, substitution)?,
            TemplateResolution::Unsupported => return Ok(TemplateResolution::Unsupported),
        };

        parameters.push(CallableParameterSignature::new(parameter, ty));
    }

    let receiver = template
        .receiver()
        .map(|receiver| {
            substitute_type(values, receiver.ty(), substitution).map(|ty| {
                bray_symbols::ReceiverParameterSignature::new(
                    receiver.parameter(),
                    ty,
                    receiver.mode(),
                )
            })
        })
        .transpose()?;

    Ok(TemplateResolution::Resolved(CallableSignature::new(
        callable_type,
        receiver,
        parameters,
        result,
    )))
}

fn substitute_type(
    values: &SemanticValueStore,
    ty: TypeId,
    substitution: GenericSubstitutionId,
) -> Result<TypeId, CheckerInfrastructureError> {
    values
        .substitute_type(ty, substitution)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
}

pub(super) fn call_result<C>(
    request: crate::CheckerUnitView<'_, C>,
    callable: &CallableTypeData,
    result: TypeId,
) -> Result<BoundCallResult, CheckerInfrastructureError>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    match callable.execution() {
        CallableExecution::Synchronous => Ok(BoundCallResult::Immediate(result)),
        CallableExecution::Asynchronous => Ok(BoundCallResult::LazyFuture(
            BoundFutureConstruction::new(result, future_type(request, result)?),
        )),
    }
}

fn future_type<C>(
    request: crate::CheckerUnitView<'_, C>,
    completion: TypeId,
) -> Result<TypeId, CheckerInfrastructureError>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    let Some(definition) = request
        .available_compiler_known_symbols()
        .representation_symbol::<StructSymbolId>(RepresentationRole::Future)
    else {
        return Err(
            CheckerInfrastructureError::CompilerKnownRepresentationUnavailable {
                role: RepresentationRole::Future,
            },
        );
    };

    let Some(symbol) = request
        .available_compiler_known_symbols()
        .provider()
        .symbol(definition)
    else {
        return Err(CheckerInfrastructureError::SemanticValueUnavailable);
    };

    let [parameter] = symbol.generic_type_parameters() else {
        return Err(CheckerInfrastructureError::SemanticValueUnavailable);
    };

    let Some(owner) = bray_symbols::GenericOwnerId::try_new(definition.into()) else {
        return Err(CheckerInfrastructureError::SemanticValueUnavailable);
    };

    let substitution = GenericSubstitutionData::try_new(
        owner,
        [GenericParameterSymbolId::Type(*parameter)],
        [GenericArgument::Type(completion)],
    )
    .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let substitution = request
        .semantic_values()
        .intern_generic_substitution(substitution)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    request
        .semantic_values()
        .intern_type(TypeData::Named {
            definition: NamedTypeSymbolId::Struct(definition),
            substitution,
        })
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
}

pub(super) const fn candidate_state(
    state: CallableCandidateTemplateState,
) -> CallableCandidateState {
    match state {
        CallableCandidateTemplateState::Visible => CallableCandidateState::Available,
        CallableCandidateTemplateState::Inaccessible => CallableCandidateState::Inaccessible,
        CallableCandidateTemplateState::Recovered => CallableCandidateState::Recovered,
    }
}
