use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView,
};
use bray_bound_tree::{BoundCallableTarget, SelectedArgument, SelectedCall};
use bray_symbols::{
    CallableSymbolId, DependencyContractTemplateData, DependencyContractTemplateId,
    DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot,
    SymbolOrdinal,
};

pub(crate) fn opaque_result(call: &SelectedCall) -> bool {
    call.implementation_hook()
        .and_then(super::implementation_dependency_source)
        .is_none()
        && !matches!(call.target(), BoundCallableTarget::Declaration(_))
}

/// Applies the selected implementation, generic substitution and parameter defaults once.
pub(crate) fn call_result_template<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &SelectedCall,
    declaration: impl FnOnce(
        CallableSymbolId,
    ) -> Result<
        DependencyContractTemplateId,
        CheckerQueryError<C::UpstreamError>,
    >,
) -> Result<DependencyContractTemplateData, CheckerQueryError<C::UpstreamError>> {
    let store = request.semantic_values();

    let template = if let Some((parameter, _)) = call
        .implementation_hook()
        .and_then(super::implementation_dependency_source)
    {
        store
            .intern_dependency_contract_template(DependencyContractTemplateData::new([
                value_requirement(DependencySubjectRoot::Parameter(parameter)),
            ]))
            .map_err(CheckerInfrastructureError::SemanticValueStore)?
    } else if !opaque_result(call)
        && let BoundCallableTarget::Declaration(instance) = call.target()
    {
        declaration(instance.definition().callable_symbol())?
    } else {
        // An opaque target can retain any input value contract, including its receiver.
        let requirements = call
            .arguments()
            .iter()
            .map(|argument| match argument {
                SelectedArgument::Explicit { ordinal, .. }
                | SelectedArgument::Default { ordinal, .. } => value_requirement(
                    DependencySubjectRoot::Parameter(SymbolOrdinal::new(*ordinal)),
                ),
            })
            .chain(
                call.receiver()
                    .map(|_| value_requirement(DependencySubjectRoot::Receiver)),
            );

        store
            .intern_dependency_contract_template(DependencyContractTemplateData::new(requirements))
            .map_err(CheckerInfrastructureError::SemanticValueStore)?
    };

    let template = match call.target() {
        BoundCallableTarget::Declaration(instance) => store
            .substitute_dependency_contract(template, instance.substitution())
            .map_err(CheckerInfrastructureError::SemanticValueStore)?,
        _ => template,
    };

    let template = store
        .dependency_contract_template_data(template)
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    super::defaults::expand_result_defaults(request, call, &template)
}

fn value_requirement(root: DependencySubjectRoot) -> DependencyRequirement {
    DependencyRequirement::direct(
        DependencySubject::root(root),
        DependencyRequirementKind::ValueDependencies,
    )
}
