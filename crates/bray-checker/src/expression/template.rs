use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundCallResult, BoundCallableTarget, BoundFutureConstruction, BoundResolvedCall,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableAbi, CallableConstness, CallableDependencyContracts, CallableExecution,
    CallableInstanceData, CallableParameterData, CallableParameterMode, CallablePosition,
    CallableSignature, CallableSignatureTemplate, CallableTrust, CallableTypeData, GenericArgument,
    GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData, GenericSubstitutionId,
    PredicateInstanceData, TypeData, TypeExpressionTemplate, TypeId,
};

use crate::{
    CallableCandidate, CallableCandidateState, CallableCandidateTemplateState,
    CallableDeclarationCandidateTemplate, CheckedConstantTerms, CheckerInfrastructureError,
    CheckerQueryError, CheckerQueryResult, PredicateCandidateTemplate,
    normalize_callable_signature_type_valued_members, resolve_callable_signature_template,
    resolve_type_expression_template,
};

pub(crate) enum TemplateResolution<T> {
    Resolved(T),
    Unsupported,
}

pub(super) fn resolve_predicate_candidate<C>(
    request: crate::CheckerUnitView<'_, C>,
    template: &PredicateCandidateTemplate,
    diagnostics: &mut DiagnosticBag,
) -> Result<TemplateResolution<CallableCandidate>, CheckerQueryError<C::UpstreamError>>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    if template.generic_arguments().len() != template.generic().parameters().len() {
        return Ok(TemplateResolution::Unsupported);
    }

    let arguments =
        match resolve_generic_arguments(request, template.generic_arguments(), diagnostics)? {
            TemplateResolution::Resolved(arguments) => arguments,
            TemplateResolution::Unsupported => return Ok(TemplateResolution::Unsupported),
        };

    resolve_predicate_candidate_with_arguments(request, template, arguments, diagnostics)
}

pub(super) fn resolve_open_predicate_candidate<C>(
    request: crate::CheckerUnitView<'_, C>,
    template: &PredicateCandidateTemplate,
    explicit: &[GenericArgument],
    diagnostics: &mut DiagnosticBag,
) -> Result<CallableCandidate, CheckerQueryError<C::UpstreamError>>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    let arguments =
        open_generic_arguments_with_prefix(request, explicit, template.generic().parameters())?;

    match resolve_predicate_candidate_with_arguments(request, template, arguments, diagnostics)? {
        TemplateResolution::Resolved(candidate) => Ok(candidate),
        TemplateResolution::Unsupported => {
            Err(CheckerInfrastructureError::InvalidSemanticSelectionInput.into())
        }
    }
}

pub(super) fn resolve_predicate_candidate_with_arguments<C>(
    request: crate::CheckerUnitView<'_, C>,
    template: &PredicateCandidateTemplate,
    arguments: Vec<GenericArgument>,
    diagnostics: &mut DiagnosticBag,
) -> Result<TemplateResolution<CallableCandidate>, CheckerQueryError<C::UpstreamError>>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    let substitution = GenericSubstitutionData::try_new(
        template.generic().owner(),
        template.generic().parameters().iter().copied(),
        arguments,
    )
    .map_err(CheckerInfrastructureError::GenericSubstitution)?;

    let substitution = request
        .semantic_values()
        .intern_generic_substitution(substitution)
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    let mut parameters = Vec::with_capacity(template.signature().parameters().len());

    for parameter in template.signature().parameters() {
        let TemplateResolution::Resolved(ty) =
            resolve_type_template(request, parameter.ty(), diagnostics)?
        else {
            return Ok(TemplateResolution::Unsupported);
        };

        let ty = request
            .semantic_values()
            .substitute_type(ty, substitution)
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

        parameters.push(CallableParameterData::new(
            parameter.name().clone(),
            CallablePosition::PositionalOrNamed,
            CallableParameterMode::Immutable,
            ty,
        ));
    }

    let result =
        crate::representation::representation_type(request, RepresentationRole::ScalarBool)?;

    let dependencies = request
        .semantic_values()
        .empty_dependency_contract_template()
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    let trust = if template.signature().is_trusted() {
        CallableTrust::Trusted
    } else {
        CallableTrust::Safe
    };

    let callable = CallableTypeData::new(
        parameters,
        result,
        CallableConstness::Constant,
        trust,
        CallableAbi::Bray,
        CallableDependencyContracts::synchronous(dependencies),
    );

    let callable_type = request
        .semantic_values()
        .intern_type(TypeData::Callable(callable))
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    let resolution = BoundResolvedCall::new(
        BoundCallableTarget::Predicate(PredicateInstanceData::new(
            template.definition(),
            substitution,
        )),
        [],
        BoundCallResult::Immediate(result),
    );

    Ok(TemplateResolution::Resolved(
        CallableCandidate::predicate(
            template.key().clone(),
            resolution,
            callable_type,
            result,
            substitution,
            candidate_state(template.state()),
        )
        .with_generic_constraints(template.generic().constraints().iter().cloned()),
    ))
}

fn open_generic_arguments<C>(
    request: crate::CheckerUnitView<'_, C>,
    parameters: &[GenericParameterSymbolId],
) -> Result<Vec<GenericArgument>, CheckerInfrastructureError>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    parameters
        .iter()
        .copied()
        .map(|parameter| {
            request
                .semantic_values()
                .intern_generic_parameter_argument(parameter)
                .map_err(CheckerInfrastructureError::SemanticValueStore)
        })
        .collect()
}

fn open_generic_arguments_with_prefix<C>(
    request: crate::CheckerUnitView<'_, C>,
    explicit: &[GenericArgument],
    parameters: &[GenericParameterSymbolId],
) -> Result<Vec<GenericArgument>, CheckerInfrastructureError>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    let Some(remaining) = parameters.get(explicit.len()..) else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let mut arguments = Vec::with_capacity(parameters.len());
    arguments.extend_from_slice(explicit);
    arguments.extend(open_generic_arguments(request, remaining)?);

    Ok(arguments)
}

pub(super) fn resolve_declaration_candidate<C>(
    request: crate::CheckerUnitView<'_, C>,
    template: &CallableDeclarationCandidateTemplate,
    diagnostics: &mut DiagnosticBag,
) -> CheckerQueryResult<TemplateResolution<CallableCandidate>, C::UpstreamError>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    if template.generic_arguments().len() != template.generic().parameters().len() {
        return Ok(TemplateResolution::Unsupported);
    }

    let arguments =
        match resolve_generic_arguments(request, template.generic_arguments(), diagnostics)? {
            TemplateResolution::Resolved(arguments) => arguments,
            TemplateResolution::Unsupported => return Ok(TemplateResolution::Unsupported),
        };

    resolve_declaration_candidate_with_arguments(request, template, arguments, diagnostics)
}

pub(super) fn resolve_open_declaration_candidate<C>(
    request: crate::CheckerUnitView<'_, C>,
    template: &CallableDeclarationCandidateTemplate,
    explicit: &[GenericArgument],
    diagnostics: &mut DiagnosticBag,
) -> CheckerQueryResult<CallableCandidate, C::UpstreamError>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    let arguments =
        open_generic_arguments_with_prefix(request, explicit, template.generic().parameters())?;

    match resolve_declaration_candidate_with_arguments(request, template, arguments, diagnostics)? {
        TemplateResolution::Resolved(candidate) => Ok(candidate),
        TemplateResolution::Unsupported => {
            Err(CheckerInfrastructureError::InvalidSemanticSelectionInput.into())
        }
    }
}

pub(super) fn resolve_declaration_candidate_with_arguments<C>(
    request: crate::CheckerUnitView<'_, C>,
    template: &CallableDeclarationCandidateTemplate,
    arguments: Vec<GenericArgument>,
    diagnostics: &mut DiagnosticBag,
) -> CheckerQueryResult<TemplateResolution<CallableCandidate>, C::UpstreamError>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    let values = request.semantic_values();

    let substitution = GenericSubstitutionData::try_new(
        template.generic().owner(),
        template.generic().parameters().iter().copied(),
        arguments,
    )
    .map_err(CheckerInfrastructureError::GenericSubstitution)?;

    let substitution = values
        .intern_generic_substitution(substitution)
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    let signature =
        match resolve_signature(request, template.signature(), substitution, diagnostics)? {
            TemplateResolution::Resolved(signature) => signature,
            TemplateResolution::Unsupported => return Ok(TemplateResolution::Unsupported),
        };

    let callable_type = values.type_data(signature.callable_type());

    let TypeData::Callable(callable_type) = callable_type.as_ref() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput.into());
    };

    let callable_owner = GenericOwnerId::try_new(template.definition().symbol())
        .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

    let callable_substitution = if callable_owner == template.generic().owner() {
        substitution
    } else {
        values
            .inherit_generic_substitution(substitution, callable_owner)
            .map_err(|error| {
                CheckerQueryError::Infrastructure(CheckerInfrastructureError::SemanticValueStore(
                    error,
                ))
            })?
    };

    let instance = CallableInstanceData::new(template.definition(), callable_substitution);

    let result = call_result(request, callable_type, signature.result())
        .map_err(CheckerQueryError::Infrastructure)?;

    let resolution = BoundResolvedCall::new(BoundCallableTarget::Declaration(instance), [], result);

    let mut defaults = Vec::new();

    for default in template.defaults() {
        if !default.value().is_present() {
            continue;
        }

        let Some(provider) = default.provider() else {
            return Err(CheckerQueryError::Infrastructure(
                CheckerInfrastructureError::InvalidSemanticSelectionInput,
            ));
        };

        defaults.push((default.parameter(), provider));
    }

    // Selection owns Arc-backed candidate data while binder templates remain reusable.
    let key = template.key().clone();

    Ok(TemplateResolution::Resolved(
        CallableCandidate::new(
            key,
            resolution,
            signature,
            defaults,
            candidate_state(template.state()),
        )
        .with_generic_substitution(substitution)
        .with_parameter_borrows(
            template
                .signature()
                .parameter_type_templates(values)
                .expect("callable candidate must have parameter type templates")
                .iter()
                .map(|template| match template {
                    TypeExpressionTemplate::Borrow { kind, .. } => Some(*kind),
                    TypeExpressionTemplate::Resolved(ty) => match values.type_data(*ty).as_ref() {
                        TypeData::Borrow { kind, .. } => Some(*kind),
                        _ => None,
                    },
                    _ => None,
                }),
        )
        .with_contract(template.contract().clone())
        .with_generic_constraints(template.generic().constraints().iter().cloned()),
    ))
}

pub(crate) fn resolve_type_template<C>(
    request: crate::CheckerUnitView<'_, C>,
    template: &TypeExpressionTemplate,
    diagnostics: &mut DiagnosticBag,
) -> Result<TemplateResolution<TypeId>, CheckerQueryError<C::UpstreamError>>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    let constants = match checked_terms(request, [template], diagnostics)? {
        TemplateResolution::Resolved(constants) => constants,
        TemplateResolution::Unsupported => return Ok(TemplateResolution::Unsupported),
    };

    resolve_type_expression_template(request.semantic_values(), template, &constants)
        .map(|ty| {
            ty.map_or(
                TemplateResolution::Unsupported,
                TemplateResolution::Resolved,
            )
        })
        .map_err(CheckerQueryError::Infrastructure)
}

pub(crate) fn resolve_signature<C>(
    request: crate::CheckerUnitView<'_, C>,
    template: &CallableSignatureTemplate,
    substitution: GenericSubstitutionId,
    diagnostics: &mut DiagnosticBag,
) -> CheckerQueryResult<TemplateResolution<CallableSignature>, C::UpstreamError>
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

    let Some(signature) = resolve_callable_signature_template(
        request.semantic_values(),
        template,
        substitution,
        &constants,
    )
    .map_err(CheckerQueryError::Infrastructure)?
    else {
        return Ok(TemplateResolution::Unsupported);
    };

    normalize_callable_signature_type_valued_members(request.context(), signature, diagnostics)
        .map(TemplateResolution::Resolved)
}

pub(super) fn resolve_generic_arguments<C>(
    request: crate::CheckerUnitView<'_, C>,
    arguments: &[bray_symbols::GenericArgumentTemplate],
    diagnostics: &mut DiagnosticBag,
) -> Result<TemplateResolution<Vec<GenericArgument>>, CheckerQueryError<C::UpstreamError>>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    let mut resolved = Vec::with_capacity(arguments.len());

    for argument in arguments {
        match argument {
            bray_symbols::GenericArgumentTemplate::Resolved(argument) => {
                resolved.push(*argument);
            }
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
                    Err(CheckerQueryError::Cancelled) => {
                        return Ok(TemplateResolution::Unsupported);
                    }
                    Err(CheckerQueryError::Infrastructure(error)) => {
                        return Err(CheckerQueryError::Infrastructure(error));
                    }
                    Err(CheckerQueryError::Upstream(error)) => {
                        return Err(CheckerQueryError::Upstream(error));
                    }
                };

                // Candidate preparation owns dependency diagnostics after the query result drops.
                diagnostics.extend(result.diagnostics().iter().cloned());
                resolved.push(GenericArgument::Constant(*result.value()));
            }
        }
    }

    Ok(TemplateResolution::Resolved(resolved))
}

/// Resolves one declaration application from checked generic-argument templates.
pub fn check_generic_arguments<C>(
    request: crate::CheckerUnitView<'_, C>,
    arguments: &[bray_symbols::GenericArgumentTemplate],
) -> crate::CheckerOutcome<Option<Vec<GenericArgument>>, C::UpstreamError>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    let mut diagnostics = DiagnosticBag::new();

    match resolve_generic_arguments(request, arguments, &mut diagnostics) {
        Ok(TemplateResolution::Resolved(arguments)) => {
            crate::CheckerOutcome::complete(Some(arguments), diagnostics)
        }
        Ok(TemplateResolution::Unsupported) => crate::CheckerOutcome::complete(None, diagnostics),
        Err(CheckerQueryError::Cancelled) => crate::CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Infrastructure(error)) => {
            crate::CheckerOutcome::InfrastructureFailure(error)
        }
        Err(CheckerQueryError::Upstream(error)) => crate::CheckerOutcome::UpstreamFailure(error),
    }
}

fn checked_terms<'template, C>(
    request: crate::CheckerUnitView<'_, C>,
    templates: impl IntoIterator<Item = &'template TypeExpressionTemplate>,
    diagnostics: &mut DiagnosticBag,
) -> Result<TemplateResolution<CheckedConstantTerms>, CheckerQueryError<C::UpstreamError>>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    let mut terms = BTreeMap::new();

    for template in templates {
        for occurrence in template.constant_expressions() {
            let result = match request.checked_constant_expression(occurrence) {
                Ok(result) => result,
                Err(CheckerQueryError::Cancelled) => {
                    return Ok(TemplateResolution::Unsupported);
                }
                Err(CheckerQueryError::Infrastructure(error)) => {
                    return Err(CheckerQueryError::Infrastructure(error));
                }
                Err(CheckerQueryError::Upstream(error)) => {
                    return Err(CheckerQueryError::Upstream(error));
                }
            };

            // Template resolution owns dependency diagnostics after the query result drops.
            diagnostics.extend(result.diagnostics().iter().cloned());
            terms.insert(occurrence.key(), *result.value());
        }
    }

    CheckedConstantTerms::try_from_terms(terms)
        .map(TemplateResolution::Resolved)
        .map_err(|error| {
            CheckerQueryError::Infrastructure(CheckerInfrastructureError::CheckedConstantTerms(
                error,
            ))
        })
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
    request
        .available_compiler_known_symbols()
        .unary_representation_type(
            request.semantic_values(),
            RepresentationRole::Future,
            completion,
        )
        .map_err(CheckerInfrastructureError::SemanticValueStore)?
        .ok_or(
            CheckerInfrastructureError::CompilerKnownRepresentationUnavailable {
                role: RepresentationRole::Future,
            },
        )
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
