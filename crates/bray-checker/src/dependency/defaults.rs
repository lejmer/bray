use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView,
};
use bray_bound_tree::{SelectedArgument, SelectedCall};
use bray_symbols::{
    DependencyContractTemplateData, DependencyRequirement, DependencyRequirementKind,
    DependencySubjectRoot,
};

/// Dependencies retained by a selected call, including inputs whose call-owned storage would escape.
pub(crate) struct CallResultDependencies {
    pub(crate) template: DependencyContractTemplateData,
    pub(crate) escaping_default_inputs: Vec<DependencySubjectRoot>,
}

/// Replaces omitted inputs with their declaration-owned provider dependencies.
pub(crate) fn expand_result_defaults<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &SelectedCall,
    template: &DependencyContractTemplateData,
) -> Result<CallResultDependencies, CheckerQueryError<C::UpstreamError>> {
    let mut result = Vec::new();
    let mut escaping_default_inputs = std::collections::BTreeSet::new();

    let mut pending = template
        .requirements()
        .iter()
        .cloned()
        .map(|requirement| (requirement, false))
        .collect::<Vec<_>>();

    let mut visited = std::collections::BTreeSet::new();

    while let Some((requirement, from_default)) = pending.pop() {
        let DependencyRequirement::Direct { subject, kind } = &requirement else {
            if let DependencyRequirement::Guarded(guarded) = requirement {
                pending.extend(
                    guarded
                        .requirements()
                        .iter()
                        .cloned()
                        .map(|requirement| (requirement, from_default)),
                );
            }

            continue;
        };

        if from_default
            && *kind == DependencyRequirementKind::StorageAlive
            && default_borrows_argument_storage(call, subject, request)?
        {
            escaping_default_inputs.insert(subject.subject_root());
            continue;
        }

        let default = match subject.subject_root() {
            DependencySubjectRoot::Parameter(ordinal) => {
                call.arguments().iter().find_map(|argument| match argument {
                    SelectedArgument::Default {
                        parameter,
                        ordinal: actual,
                        ..
                    } if *actual == ordinal.raw() => Some(*parameter),
                    _ => None,
                })
            }
            _ => None,
        };

        let Some(parameter) = default else {
            result.push(requirement);
            continue;
        };

        if !visited.insert(parameter) {
            continue;
        }

        let (_, template) = request.context().parameter_default_result(parameter)?;

        let template = match call.target() {
            bray_bound_tree::BoundCallableTarget::Declaration(instance) => request
                .semantic_values()
                .substitute_dependency_contract(template, instance.substitution())
                .map_err(CheckerInfrastructureError::SemanticValueStore)?,
            _ => template,
        };

        let template = request
            .semantic_values()
            .dependency_contract_template_data(template)
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

        pending.extend(
            template
                .requirements()
                .iter()
                .cloned()
                .map(|requirement| (requirement, true)),
        );
    }

    Ok(CallResultDependencies {
        template: DependencyContractTemplateData::new(result),
        escaping_default_inputs: escaping_default_inputs.into_iter().collect(),
    })
}

fn default_borrows_argument_storage<C: CheckerRequestContext + ?Sized>(
    call: &SelectedCall,
    subject: &bray_symbols::DependencySubject,
    request: CheckerUnitView<'_, C>,
) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
    let root = subject.subject_root();

    if subject.projections().is_empty() && matches!(root, DependencySubjectRoot::Parameter(_)) {
        return Ok(true);
    }

    let ty = match root {
        DependencySubjectRoot::Parameter(ordinal) => {
            match call.arguments().iter().find(|argument| match argument {
                SelectedArgument::Explicit {
                    ordinal: actual, ..
                }
                | SelectedArgument::Default {
                    ordinal: actual, ..
                } => *actual == ordinal.raw(),
            }) {
                Some(SelectedArgument::Explicit { conversion, .. }) => conversion.target_type(),
                Some(SelectedArgument::Default { parameter, .. }) => {
                    let (ty, _) = request.context().parameter_default_result(*parameter)?;

                    match call.target() {
                        bray_bound_tree::BoundCallableTarget::Declaration(instance) => request
                            .semantic_values()
                            .substitute_type(ty, instance.substitution())
                            .map_err(CheckerInfrastructureError::SemanticValueStore)?,
                        _ => ty,
                    }
                }
                None => {
                    return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput.into());
                }
            }
        }
        DependencySubjectRoot::Receiver => {
            return Ok(call.receiver().is_some_and(|receiver| {
                matches!(
                    receiver.mode(),
                    bray_symbols::ReceiverMode::Consuming
                        | bray_symbols::ReceiverMode::ConsumingMutable
                )
            }));
        }
        _ => return Ok(false),
    };

    let ty = request
        .semantic_values()
        .type_data(ty)
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    Ok(!matches!(
        ty.as_ref(),
        bray_symbols::TypeData::Borrow { .. }
    ))
}

pub(super) fn escaping_default_diagnostic<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expression: bray_bound_tree::BoundExpressionId,
    call: &SelectedCall,
    root: DependencySubjectRoot,
    id: bray_diagnostics::DiagnosticId,
) -> Result<bray_diagnostics::Diagnostic, CheckerInfrastructureError> {
    let origin = crate::diagnostic::bound_node_origin(request, expression.into())
        .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

    let source = request.source(origin.source_anchor())?;

    let argument = super::result_argument(call, root)
        .or_else(|| match request.view().expression(expression) {
            Some(bray_bound_tree::BoundExpression::Call(call)) => Some(call.callee()),
            _ => None,
        })
        .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

    let argument = crate::diagnostic::bound_node_origin(request, argument.into())
        .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

    let argument = request.source(argument.source_anchor())?;

    Ok(crate::diagnostic::escaping_storage_dependency_diagnostic(
        id,
        source.span(),
        argument.span(),
    ))
}
