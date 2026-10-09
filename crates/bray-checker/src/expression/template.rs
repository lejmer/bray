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
    CallableDeclarationCandidateTemplate, CheckedConstantTerms, CheckerQueryError,
    CheckerQueryResult, PredicateCandidateTemplate, normalize_callable_signature,
    resolve_callable_signature_template, resolve_type_expression_template,
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
        open_generic_arguments_with_prefix(request, explicit, template.generic().parameters());

    match resolve_predicate_candidate_with_arguments(request, template, arguments, diagnostics)? {
        TemplateResolution::Resolved(candidate) => Ok(candidate),
        TemplateResolution::Unsupported => {
            panic!(
                "Semantic-selection inputs do not describe the requested bound unit or operation category. in resolve_open_predicate_candidate"
            )
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
    ).unwrap_or_else(|error| panic!("Generic substitution construction rejected an exact parameter-to-argument relationship. in resolve_predicate_candidate_with_arguments: {error:?}"));

    let substitution = request
        .semantic_values()
        .intern_generic_substitution(substitution).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in resolve_predicate_candidate_with_arguments: {error:?}"));

    let mut parameters = Vec::with_capacity(template.signature().parameters().len());

    for parameter in template.signature().parameters() {
        let TemplateResolution::Resolved(ty) =
            resolve_type_template(request, parameter.ty(), diagnostics)?
        else {
            return Ok(TemplateResolution::Unsupported);
        };

        let ty = request
            .semantic_values()
            .substitute_type(ty, substitution).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in resolve_predicate_candidate_with_arguments: {error:?}"));

        parameters.push(CallableParameterData::new(
            parameter.name().clone(),
            CallablePosition::PositionalOrNamed,
            CallableParameterMode::Immutable,
            ty,
        ));
    }

    let result =
        crate::representation::representation_type(request, RepresentationRole::ScalarBool);

    let dependencies = request
        .semantic_values()
        .empty_dependency_contract_template().unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in resolve_predicate_candidate_with_arguments: {error:?}"));

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
        .intern_type(TypeData::Callable(callable)).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in resolve_predicate_candidate_with_arguments: {error:?}"));

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
) -> Vec<GenericArgument>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    parameters
        .iter()
        .copied()
        .map(|parameter| {
            request
                .semantic_values()
                .intern_generic_parameter_argument(parameter).unwrap_or_else(|error| panic!("open_generic_arguments must satisfy its checked construction contract: {error:?}"))
        })
        .collect()
}

fn open_generic_arguments_with_prefix<C>(
    request: crate::CheckerUnitView<'_, C>,
    explicit: &[GenericArgument],
    parameters: &[GenericParameterSymbolId],
) -> Vec<GenericArgument>
where
    C: crate::CheckerRequestContext + ?Sized,
{
    let Some(remaining) = parameters.get(explicit.len()..) else {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in open_generic_arguments_with_prefix"
        );
    };

    let mut arguments = Vec::with_capacity(parameters.len());

    arguments.extend_from_slice(explicit);
    arguments.extend(open_generic_arguments(request, remaining));

    arguments
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
        open_generic_arguments_with_prefix(request, explicit, template.generic().parameters());

    match resolve_declaration_candidate_with_arguments(request, template, arguments, diagnostics)? {
        TemplateResolution::Resolved(candidate) => Ok(candidate),
        TemplateResolution::Unsupported => {
            panic!(
                "Semantic-selection inputs do not describe the requested bound unit or operation category. in resolve_open_declaration_candidate"
            )
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
    ).unwrap_or_else(|error| panic!("Generic substitution construction rejected an exact parameter-to-argument relationship. in resolve_declaration_candidate_with_arguments: {error:?}"));

    let substitution = values
        .intern_generic_substitution(substitution).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in resolve_declaration_candidate_with_arguments: {error:?}"));

    let inherited =
        request
            .context()
            .inherited_callable_substitution(CallableInstanceData::new(
                template.definition(),
                substitution,
            ))?;

    diagnostics.add_range(inherited.diagnostics().iter().cloned());

    let Some(substitution) = *inherited.value() else {
        return Ok(TemplateResolution::Unsupported);
    };

    let mut signature =
        match resolve_signature(request, template.signature(), substitution, diagnostics)? {
            TemplateResolution::Resolved(signature) => signature,
            TemplateResolution::Unsupported => return Ok(TemplateResolution::Unsupported),
        };

    let callable_type = values.type_data(signature.callable_type());

    let TypeData::Callable(callable_type) = callable_type.as_ref() else {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in resolve_declaration_candidate_with_arguments"
        );
    };

    let callable_owner =
        GenericOwnerId::try_new(template.definition().symbol()).unwrap_or_else(|| {
            panic!("resolve_declaration_candidate_with_arguments requires generic callable owner")
        });

    let callable_substitution = if callable_owner == template.generic().owner() {
        substitution
    } else {
        values
            .inherit_generic_substitution(substitution, callable_owner).unwrap_or_else(|error| panic!("resolve_declaration_candidate_with_arguments must satisfy its checked construction contract: {error:?}"))
    };

    {
        let owner = bray_symbols::CallableSymbolId::try_from_any(template.definition().symbol())
            .expect("declaration candidate must be callable");

        let predicates = request.context().callable_predicate_contracts(owner)?;

        diagnostics.add_range(predicates.diagnostics().iter().cloned());

        signature = signature
            .with_predicate_contracts(values, predicates.value(), callable_substitution).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in resolve_declaration_candidate_with_arguments: {error:?}"));
    }

    let instance = CallableInstanceData::new(template.definition(), callable_substitution);

    let result = call_result(request, callable_type, signature.result());

    let resolution = BoundResolvedCall::new(BoundCallableTarget::Declaration(instance), [], result);

    let mut defaults = Vec::new();

    for default in template.defaults() {
        if !default.value().is_present() {
            continue;
        }

        let Some(provider) = default.provider() else {
            panic!(
                "Semantic-selection inputs do not describe the requested bound unit or operation category. in resolve_declaration_candidate_with_arguments"
            );
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

    Ok(
        resolve_type_expression_template(request.semantic_values(), template, &constants).map_or(
            TemplateResolution::Unsupported,
            TemplateResolution::Resolved,
        ),
    )
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
    ) else {
        return Ok(TemplateResolution::Unsupported);
    };

    normalize_callable_signature(
        request.context(),
        signature,
        Some(substitution),
        diagnostics,
    )
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
                Err(CheckerQueryError::Upstream(error)) => {
                    return Err(CheckerQueryError::Upstream(error));
                }
            };

            // Template resolution owns dependency diagnostics after the query result drops.
            diagnostics.extend(result.diagnostics().iter().cloned());
            terms.insert(occurrence.key(), *result.value());
        }
    }

    Ok(CheckedConstantTerms::try_from_terms(terms)
        .map(TemplateResolution::Resolved)
        .unwrap_or_else(|error| {
            panic!("checked_terms must satisfy its checked construction contract: {error:?}")
        }))
}

/// Resolves the immediate or lazy result of a closed callable signature.
pub fn call_result<C>(
    request: crate::CheckerUnitView<'_, C>,
    callable: &CallableTypeData,
    result: TypeId,
) -> BoundCallResult
where
    C: crate::CheckerRequestContext + ?Sized,
{
    match callable.execution() {
        CallableExecution::Synchronous => BoundCallResult::Immediate(result),
        CallableExecution::Asynchronous => BoundCallResult::LazyFuture(
            BoundFutureConstruction::new(result, future_type(request, result)),
        ),
    }
}

fn future_type<C>(request: crate::CheckerUnitView<'_, C>, completion: TypeId) -> TypeId
where
    C: crate::CheckerRequestContext + ?Sized,
{
    request
        .available_compiler_known_symbols()
        .unary_representation_type(
            request.semantic_values(),
            RepresentationRole::Future,
            completion,
        ).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in future_type: {error:?}")).unwrap_or_else(|| panic!("compiler-known Future type must be available for async call selection"))
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
