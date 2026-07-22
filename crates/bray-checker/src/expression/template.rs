use bray_bound_tree::{BoundCallResult, BoundCallableTarget, BoundResolvedCall};
use bray_symbols::{
    CallableExecution, CallableInstanceData, CallableParameterSignature, CallableSignature,
    CallableSignatureTemplate, GenericSubstitutionData, SemanticValueStore, TypeData,
    TypeExpressionTemplate, TypeId,
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

pub(super) fn resolve_declaration_candidate(
    values: &SemanticValueStore,
    template: &CallableDeclarationCandidateTemplate,
) -> Result<TemplateResolution<CallableCandidate>, CheckerInfrastructureError> {
    if !template.generic().parameters().is_empty() || !template.generic().constraints().is_empty() {
        // TODO(BRA-242): Bind candidate-specific generic arguments and constraints.
        return Ok(TemplateResolution::Unsupported);
    }

    let signature = match resolve_signature(values, template.signature())? {
        TemplateResolution::Resolved(signature) => signature,
        TemplateResolution::Unsupported => return Ok(TemplateResolution::Unsupported),
    };

    let callable_type = values
        .type_data(signature.callable_type())
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let TypeData::Callable(callable_type) = callable_type.as_ref() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    if callable_type.execution() == CallableExecution::Asynchronous {
        // TODO(BRA-242): Construct the target-specific Future result for asynchronous calls.
        return Ok(TemplateResolution::Unsupported);
    }

    let substitution = GenericSubstitutionData::try_new(
        template.generic().owner(),
        std::iter::empty(),
        std::iter::empty(),
    )
    .map_err(|_| CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

    let substitution = values
        .intern_generic_substitution(substitution)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let instance = CallableInstanceData::new(template.definition(), substitution);

    let resolution = BoundResolvedCall::new(
        BoundCallableTarget::Declaration(instance),
        [],
        BoundCallResult::Immediate(signature.result()),
    );

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

    Ok(TemplateResolution::Resolved(CallableCandidate::new(
        key,
        resolution,
        signature,
        defaults,
        candidate_state(template.state()),
    )))
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
) -> Result<TemplateResolution<CallableSignature>, CheckerInfrastructureError> {
    let callable_type = match resolve_type_template(values, template.callable_type())? {
        TemplateResolution::Resolved(ty) => ty,
        TemplateResolution::Unsupported => return Ok(TemplateResolution::Unsupported),
    };

    let result = match resolve_type_template(values, template.result())? {
        TemplateResolution::Resolved(ty) => ty,
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
            TemplateResolution::Resolved(ty) => ty,
            TemplateResolution::Unsupported => return Ok(TemplateResolution::Unsupported),
        };

        parameters.push(CallableParameterSignature::new(parameter, ty));
    }

    Ok(TemplateResolution::Resolved(CallableSignature::new(
        callable_type,
        template.receiver(),
        parameters,
        result,
    )))
}

const fn candidate_state(state: CallableCandidateTemplateState) -> CallableCandidateState {
    match state {
        CallableCandidateTemplateState::Visible => CallableCandidateState::Available,
        CallableCandidateTemplateState::Inaccessible => CallableCandidateState::Inaccessible,
        CallableCandidateTemplateState::Recovered => CallableCandidateState::Recovered,
    }
}
