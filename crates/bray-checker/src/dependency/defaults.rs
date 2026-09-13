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
    pub(crate) escaping_evaluation_inputs: Vec<DependencySubjectRoot>,
}

/// Replaces omitted inputs with their declaration-owned provider dependencies.
pub(crate) fn expand_result_defaults<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &SelectedCall,
    template: &DependencyContractTemplateData,
) -> Result<CallResultDependencies, CheckerQueryError<C::UpstreamError>> {
    let mut result = Vec::new();
    let mut escaping_evaluation_inputs = std::collections::BTreeSet::new();

    let mut pending = template
        .requirements()
        .iter()
        .cloned()
        .map(|requirement| (requirement, false))
        .collect::<Vec<_>>();

    let mut visited = std::collections::BTreeSet::new();

    while let Some((requirement, from_default)) = pending.pop() {
        if let DependencyRequirement::FixedPoint {
            definitions,
            result: roots,
        } = requirement
        {
            let definitions = definitions
                .iter()
                .map(|definition| expand_witness_input_defaults(request, call, definition))
                .collect::<Result<Vec<_>, _>>()?;

            let roots = expand_witness_input_defaults(request, call, &roots)?;
            result.push(DependencyRequirement::fixed_point(definitions, roots));
            continue;
        }

        if matches!(requirement, DependencyRequirement::Variable { .. }) {
            result.push(requirement);
            continue;
        }

        if let DependencyRequirement::RecursiveCall { callable, inputs } = requirement {
            result.push(DependencyRequirement::recursive_call(
                callable,
                expand_call_inputs(request, call, &inputs)?,
            ));

            continue;
        }

        if let DependencyRequirement::WitnessCall {
            callable,
            requirement,
            inputs,
        } = requirement
        {
            let inputs = expand_call_inputs(request, call, &inputs)?;

            result.push(DependencyRequirement::witness_call(
                callable,
                requirement,
                inputs,
            ));

            continue;
        }

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

        if subject.subject_root() == DependencySubjectRoot::EvaluationStorage {
            if *kind != DependencyRequirementKind::ValueDependencies {
                escaping_evaluation_inputs.insert(DependencySubjectRoot::Result);
            }

            continue;
        }

        if from_default
            && *kind == DependencyRequirementKind::StorageAlive
            && default_borrows_argument_storage(call, subject, request)?
        {
            escaping_evaluation_inputs.insert(subject.subject_root());
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
            super::witness::resolve(request, template.requirements())?
                .into_iter()
                .map(|requirement| (requirement, true)),
        );
    }

    Ok(CallResultDependencies {
        template: DependencyContractTemplateData::new(result),
        escaping_evaluation_inputs: escaping_evaluation_inputs.into_iter().collect(),
    })
}

fn expand_call_inputs<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &SelectedCall,
    inputs: &[bray_symbols::DependencyCallInput],
) -> Result<Vec<bray_symbols::DependencyCallInput>, CheckerQueryError<C::UpstreamError>> {
    inputs
        .iter()
        .map(|input| {
            Ok(bray_symbols::DependencyCallInput::new(
                input.root(),
                expand_witness_input_defaults(request, call, input.values())?,
                expand_witness_input_defaults(request, call, input.storage())?,
            ))
        })
        .collect::<Result<Vec<_>, CheckerQueryError<C::UpstreamError>>>()
}

fn expand_witness_input_defaults<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &SelectedCall,
    requirements: &[DependencyRequirement],
) -> Result<Vec<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
    let template = DependencyContractTemplateData::new(requirements.iter().cloned());
    let expanded = expand_result_defaults(request, call, &template)?;

    let errors = expanded.escaping_evaluation_inputs.iter().map(|_| {
        DependencyRequirement::direct(
            bray_symbols::DependencySubject::root(DependencySubjectRoot::EvaluationStorage),
            DependencyRequirementKind::StorageAlive,
        )
    });

    Ok(expanded
        .template
        .requirements()
        .iter()
        .cloned()
        .chain(errors)
        .collect())
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

    let mut ty = match root {
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
            let receiver = call
                .receiver()
                .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

            if !matches!(
                receiver.mode(),
                bray_symbols::ReceiverMode::Consuming
                    | bray_symbols::ReceiverMode::ConsumingMutable
            ) {
                return Ok(false);
            }

            receiver.target_type()
        }
        _ => return Ok(false),
    };

    for projection in subject.projections() {
        let data = request
            .semantic_values()
            .type_data(ty)
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

        if matches!(data.as_ref(), bray_symbols::TypeData::Borrow { .. }) {
            return Ok(false);
        }

        let projected = crate::storage::projected_value_type(request, ty, *projection)?;

        if projected.diagnostics().has_errors() {
            return Ok(true);
        }

        let Some(projected) = *projected.value() else {
            return Ok(true);
        };

        ty = projected;
    }

    Ok(true)
}

pub(super) fn escaping_evaluation_diagnostic<C: CheckerRequestContext + ?Sized>(
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
