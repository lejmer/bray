use bray_bound_tree::{
    BoundCallResult, BoundCallableTarget, BoundFutureConstruction, BoundResolvedCall,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableExecution, CallableInstanceData, CallableSignature, CallableSignatureTemplate,
    CallableTypeData, GenericArgument, GenericParameterSymbolId, GenericSubstitutionData,
    GenericSubstitutionId, NamedTypeSymbolId, StructSymbolId, SymbolProvider, TypeData,
    TypeExpressionTemplate, TypeId,
};

use crate::{
    CallableCandidate, CallableCandidateState, CallableCandidateTemplateState,
    CallableDeclarationCandidateTemplate, CheckedConstantTerms, CheckerFactError,
    CheckerInfrastructureError, resolve_callable_signature_template,
    resolve_type_expression_template,
};

pub(super) enum TemplateResolution<T> {
    Resolved(T),
    Unsupported,
}

pub(super) fn resolve_declaration_candidate<C>(
    request: crate::CheckerUnitView<'_, C>,
    template: &CallableDeclarationCandidateTemplate,
    diagnostics: &mut DiagnosticBag,
) -> Result<TemplateResolution<CallableCandidate>, CheckerInfrastructureError>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    let values = request.semantic_values();

    if template.generic_arguments().len() != template.generic().parameters().len() {
        return Ok(TemplateResolution::Unsupported);
    }

    let arguments =
        match resolve_generic_arguments(request, template.generic_arguments(), diagnostics)? {
            TemplateResolution::Resolved(arguments) => arguments,
            TemplateResolution::Unsupported => return Ok(TemplateResolution::Unsupported),
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

    let signature =
        match resolve_signature(request, template.signature(), substitution, diagnostics)? {
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

pub(super) fn resolve_type_template<C>(
    request: crate::CheckerUnitView<'_, C>,
    template: &TypeExpressionTemplate,
    diagnostics: &mut DiagnosticBag,
) -> Result<TemplateResolution<TypeId>, CheckerInfrastructureError>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    let constants = match checked_terms(request, [template], diagnostics)? {
        TemplateResolution::Resolved(constants) => constants,
        TemplateResolution::Unsupported => return Ok(TemplateResolution::Unsupported),
    };

    resolve_type_expression_template(request.semantic_values(), template, &constants).map(|ty| {
        ty.map_or(
            TemplateResolution::Unsupported,
            TemplateResolution::Resolved,
        )
    })
}

fn resolve_signature<C>(
    request: crate::CheckerUnitView<'_, C>,
    template: &CallableSignatureTemplate,
    substitution: GenericSubstitutionId,
    diagnostics: &mut DiagnosticBag,
) -> Result<TemplateResolution<CallableSignature>, CheckerInfrastructureError>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    let constants = match checked_terms(
        request,
        [template.callable_type(), template.result()],
        diagnostics,
    )? {
        TemplateResolution::Resolved(constants) => constants,
        TemplateResolution::Unsupported => return Ok(TemplateResolution::Unsupported),
    };

    resolve_callable_signature_template(
        request.semantic_values(),
        template,
        substitution,
        &constants,
    )
    .map(|signature| {
        signature.map_or(
            TemplateResolution::Unsupported,
            TemplateResolution::Resolved,
        )
    })
}

fn resolve_generic_arguments<C>(
    request: crate::CheckerUnitView<'_, C>,
    arguments: &[bray_symbols::GenericArgumentTemplate],
    diagnostics: &mut DiagnosticBag,
) -> Result<TemplateResolution<Vec<GenericArgument>>, CheckerInfrastructureError>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    let mut resolved = Vec::with_capacity(arguments.len());

    for argument in arguments {
        match argument {
            bray_symbols::GenericArgumentTemplate::Type(ty) => {
                let TemplateResolution::Resolved(ty) =
                    resolve_type_template(request, ty, diagnostics)?
                else {
                    return Ok(TemplateResolution::Unsupported);
                };

                resolved.push(GenericArgument::Type(ty));
            }
            bray_symbols::GenericArgumentTemplate::Constant(occurrence) => {
                let result = match request.checked_constant_expression(*occurrence) {
                    Ok(result) => result,
                    Err(CheckerFactError::Cancelled) => {
                        return Ok(TemplateResolution::Unsupported);
                    }
                    Err(CheckerFactError::Infrastructure(error)) => return Err(error),
                };

                // Candidate preparation owns dependency diagnostics after the fact result drops.
                diagnostics.extend(result.diagnostics().iter().cloned());
                resolved.push(GenericArgument::Constant(*result.value()));
            }
        }
    }

    Ok(TemplateResolution::Resolved(resolved))
}

fn checked_terms<'template, C>(
    request: crate::CheckerUnitView<'_, C>,
    templates: impl IntoIterator<Item = &'template TypeExpressionTemplate>,
    diagnostics: &mut DiagnosticBag,
) -> Result<TemplateResolution<CheckedConstantTerms>, CheckerInfrastructureError>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    let mut terms = BTreeMap::new();

    for template in templates {
        for occurrence in template.constant_expressions() {
            let result = match request.checked_constant_expression(occurrence) {
                Ok(result) => result,
                Err(CheckerFactError::Cancelled) => {
                    return Ok(TemplateResolution::Unsupported);
                }
                Err(CheckerFactError::Infrastructure(error)) => return Err(error),
            };

            // Template resolution owns dependency diagnostics after the fact result drops.
            diagnostics.extend(result.diagnostics().iter().cloned());
            terms.insert(occurrence.key(), *result.value());
        }
    }

    CheckedConstantTerms::try_from_terms(terms)
        .map(TemplateResolution::Resolved)
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
use std::collections::BTreeMap;
