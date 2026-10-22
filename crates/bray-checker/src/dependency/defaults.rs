use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::SemanticSelection;
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
    selection: &SemanticSelection,
    template: &DependencyContractTemplateData,
) -> Result<CallResultDependencies, CheckerQueryError<C::UpstreamError>> {
    let mut defaults = std::collections::BTreeMap::new();

    for input in selection
        .inputs()
        .filter(|input| input.default_provider().is_some())
    {
        let ordinal = selection
            .input_ordinal(input.root())
            .expect("default input has its ordinal");

        let provider = super::call::selected_input_template(request, selection, ordinal)?;

        defaults.insert(input.root(), provider);
    }

    let mut expanded = expand(request, selection, template, &defaults)?;

    for (completion, requirements) in [
        (true, template.completion_reports()),
        (false, template.abnormal_reports()),
    ] {
        if let Some(requirements) = requirements {
            let reports = expand(
                request,
                selection,
                &DependencyContractTemplateData::new(requirements.iter().cloned()),
                &defaults,
            )?;

            expanded
                .escaping_evaluation_inputs
                .extend(reports.escaping_evaluation_inputs);

            expanded.template = if completion {
                expanded
                    .template
                    .with_completion_reports(reports.template.requirements().iter().cloned())
            } else {
                expanded
                    .template
                    .with_abnormal_reports(reports.template.requirements().iter().cloned())
            };
        }
    }

    expanded.escaping_evaluation_inputs.sort();
    expanded.escaping_evaluation_inputs.dedup();

    Ok(expanded)
}

type Defaults = std::collections::BTreeMap<DependencySubjectRoot, DependencyContractTemplateData>;

fn expand<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    selection: &SemanticSelection,
    template: &DependencyContractTemplateData,
    defaults: &Defaults,
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
                .map(|definition| {
                    expand_witness_input_defaults(request, selection, definition, defaults)
                })
                .collect::<Result<Vec<_>, _>>()?;

            let roots = expand_witness_input_defaults(request, selection, &roots, defaults)?;

            result.push(DependencyRequirement::fixed_point(definitions, roots));
            continue;
        }

        if matches!(requirement, DependencyRequirement::Variable { .. }) {
            result.push(requirement);
            continue;
        }

        if let DependencyRequirement::ResultCall {
            output,
            callable,
            requirement,
            inputs,
        } = requirement
        {
            result.push(DependencyRequirement::result_call(
                callable,
                requirement,
                expand_call_inputs(request, selection, &inputs, defaults)?,
                output,
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
            && default_borrows_argument_storage(selection, subject, request)?
        {
            escaping_evaluation_inputs.insert(subject.subject_root());
            continue;
        }

        let root = subject.subject_root();

        let Some(default) = defaults.get(&root) else {
            result.push(requirement);
            continue;
        };

        if !visited.insert(root) {
            continue;
        }

        pending.extend(
            default
                .requirements()
                .iter()
                .cloned()
                .map(|requirement| (requirement, true)),
        );
    }

    let omitted_origins = template
        .borrows()
        .iter()
        .filter(|relation| {
            selection
                .input_ordinal(relation.entry().subject_root())
                .and_then(|ordinal| selection.input(ordinal))
                .and_then(|input| input.expression())
                .is_none()
        })
        .map(|relation| relation.exit().clone())
        .collect::<std::collections::BTreeSet<_>>();

    Ok(CallResultDependencies {
        template: template.with_requirements(result).with_borrows(
            template
                .borrows()
                .iter()
                .filter(|relation| !omitted_origins.contains(relation.exit()))
                .cloned(),
        ),
        escaping_evaluation_inputs: escaping_evaluation_inputs.into_iter().collect(),
    })
}

fn expand_call_inputs<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    selection: &SemanticSelection,
    inputs: &[bray_symbols::DependencyCallInput],
    defaults: &Defaults,
) -> Result<Vec<bray_symbols::DependencyCallInput>, CheckerQueryError<C::UpstreamError>> {
    inputs
        .iter()
        .map(|input| {
            Ok(bray_symbols::DependencyCallInput::new(
                input.root(),
                expand_witness_input_defaults(request, selection, input.values(), defaults)?,
                expand_witness_input_defaults(request, selection, input.storage(), defaults)?,
            ))
        })
        .collect::<Result<Vec<_>, CheckerQueryError<C::UpstreamError>>>()
}

fn expand_witness_input_defaults<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    selection: &SemanticSelection,
    requirements: &[DependencyRequirement],
    defaults: &Defaults,
) -> Result<Vec<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
    let template = DependencyContractTemplateData::new(requirements.iter().cloned());
    let expanded = expand(request, selection, &template, defaults)?;

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
    selection: &SemanticSelection,
    subject: &bray_symbols::DependencySubject,
    request: CheckerUnitView<'_, C>,
) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
    let root = subject.subject_root();

    if subject.projections().is_empty() && matches!(root, DependencySubjectRoot::Parameter(_)) {
        return Ok(true);
    }

    let Some(input) = selection
        .input_ordinal(root)
        .and_then(|ordinal| selection.input(ordinal))
    else {
        return Ok(false);
    };

    let mut ty = input.ty(request.semantic_values());

    if let bray_bound_tree::SelectedInput::Receiver(receiver) = input
        && !matches!(
            receiver.mode(),
            bray_symbols::ReceiverMode::Consuming | bray_symbols::ReceiverMode::ConsumingMutable
        )
    {
        return Ok(false);
    }

    for projection in subject.projections() {
        let data = request.semantic_values().type_data(ty);

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
    selection: &bray_bound_tree::SemanticSelection,
    root: DependencySubjectRoot,
    id: bray_diagnostics::DiagnosticId,
) -> bray_diagnostics::Diagnostic {
    let origin = crate::diagnostic::bound_node_origin(request, expression.into()).unwrap_or_else(|| panic!("escaping_evaluation_diagnostic requires bound node source origin, expression: {expression:?}, id: {id:?}"));

    let source = request.source(origin.source_anchor());

    let argument = match selection {
        bray_bound_tree::SemanticSelection::Call(call) => {
            super::result_argument(call.arguments(), call.receiver(), root)
        }
        bray_bound_tree::SemanticSelection::Operation(operation) => {
            super::call::operation_inputs(request.view(), expression, operation)
                .into_iter()
                .find(|(actual, _)| *actual == root)
                .and_then(|(_, input)| input.expression())
        }
        _ => unreachable!("escaping evaluation belongs to the selected invocation"),
    };

    let argument = argument
        .or_else(|| match request.view().expression(expression) {
            Some(bray_bound_tree::BoundExpression::Call(call)) => Some(call.callee()),
            _ => Some(expression),
        })
        .expect("escaping evaluation has its source expression");

    let argument = crate::diagnostic::bound_node_origin(request, argument.into()).unwrap_or_else(|| panic!("escaping_evaluation_diagnostic requires bound node source origin, expression: {expression:?}, id: {id:?}"));

    let argument = request.source(argument.source_anchor());

    crate::diagnostic::escaping_storage_dependency_diagnostic(id, source.span(), argument.span())
}
