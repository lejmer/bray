use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{SelectedArgument, SelectedCall};
use bray_symbols::{
    DependencyContractTemplateData, DependencyRequirement, DependencyRequirementKind,
    DependencySubjectRoot,
};

/// Dependencies retained by a selected call, including inputs whose call-owned storage would escape.
pub(crate) struct CallResultDependencies {
    pub(crate) template: DependencyContractTemplateData,
    pub(crate) evaluation_effects: std::sync::Arc<[bray_symbols::DependencyStorageEffect]>,
    pub(crate) evaluation_completes: bool,
    pub(crate) evaluation_reports: DependencyContractTemplateData,
    pub(crate) escaping_evaluation_inputs: Vec<DependencySubjectRoot>,
}

impl CallResultDependencies {
    pub(crate) fn immediate_reports(&self, deferred: bool) -> DependencyContractTemplateData {
        let mut result = self.evaluation_reports.clone();

        if !deferred {
            for (normal, requirements) in [
                (true, self.template.completion_reports()),
                (false, self.template.abnormal_reports()),
            ] {
                if let Some(requirements) = requirements {
                    let previous = if normal {
                        result.completion_reports()
                    } else {
                        result.abnormal_reports()
                    };

                    let merged = previous
                        .into_iter()
                        .flatten()
                        .chain(requirements)
                        .cloned()
                        .collect::<Vec<_>>();

                    result = if normal {
                        result.with_completion_reports(merged)
                    } else {
                        result.with_abnormal_reports(merged)
                    };
                }
            }
        }

        result
    }
}

/// Replaces omitted inputs with their declaration-owned provider dependencies.
pub(crate) fn expand_result_defaults<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &SelectedCall,
    template: &DependencyContractTemplateData,
) -> Result<CallResultDependencies, CheckerQueryError<C::UpstreamError>> {
    let mut defaults = std::collections::BTreeMap::new();

    for argument in call.arguments() {
        let SelectedArgument::Default {
            parameter,
            provider,
            ..
        } = argument
        else {
            continue;
        };

        let provider = request.default_result_dependencies(
            bray_bound_tree::DefaultValueProvider::CallableParameter(*provider),
        )?;

        let provider = super::resolved_result_template(
            request,
            match call.target() {
                bray_bound_tree::BoundCallableTarget::Declaration(instance) => Some(instance),
                _ => None,
            },
            provider,
        )?;

        defaults.insert(*parameter, provider);
    }

    let mut expanded = expand(request, call, template, &defaults)?;

    for (completion, requirements) in [
        (true, template.completion_reports()),
        (false, template.abnormal_reports()),
    ] {
        if let Some(requirements) = requirements {
            let reports = expand(
                request,
                call,
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

    for completion in [true, false] {
        let mut reports = None::<Vec<DependencyRequirement>>;

        for provider in defaults.values() {
            let requirements = if completion {
                provider.completion_reports()
            } else {
                provider.abnormal_reports()
            };

            for requirements in requirements.into_iter().chain(
                (!completion)
                    .then(|| provider.completion_reports())
                    .flatten(),
            ) {
                let output = expand(
                    request,
                    call,
                    &DependencyContractTemplateData::new(requirements.iter().cloned()),
                    &defaults,
                )?;

                reports
                    .get_or_insert_default()
                    .extend(output.template.requirements().iter().cloned());

                expanded
                    .escaping_evaluation_inputs
                    .extend(output.escaping_evaluation_inputs);
            }
        }

        if let Some(reports) = reports {
            expanded.evaluation_reports = if completion {
                expanded.evaluation_reports.with_completion_reports(reports)
            } else {
                expanded.evaluation_reports.with_abnormal_reports(reports)
            };
        }
    }

    expanded.escaping_evaluation_inputs.sort();
    expanded.escaping_evaluation_inputs.dedup();

    Ok(expanded)
}

type Defaults = std::collections::BTreeMap<
    bray_symbols::CallableParameterSymbolId,
    DependencyContractTemplateData,
>;

fn expand<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &SelectedCall,
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
                    expand_witness_input_defaults(request, call, definition, defaults)
                })
                .collect::<Result<Vec<_>, _>>()?;

            let roots = expand_witness_input_defaults(request, call, &roots, defaults)?;

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
                expand_call_inputs(request, call, &inputs, defaults)?,
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

        pending.extend(
            defaults[&parameter]
                .requirements()
                .iter()
                .cloned()
                .map(|requirement| (requirement, true)),
        );
    }

    // Default providers execute when arguments are evaluated, even for a lazy call.
    let evaluation_effects = bray_symbols::DependencyStorageEffect::normalize(
        defaults
            .values()
            .flat_map(|provider| provider.effects())
            .cloned(),
    );

    let evaluation_completes = defaults
        .values()
        .all(DependencyContractTemplateData::normal_completion);

    Ok(CallResultDependencies {
        // Default arguments have no caller expression to anchor an exact entry identity.
        template: template.with_requirements(result).with_borrows(
            template
                .borrows()
                .iter()
                .filter(|relation| {
                    super::result_argument(call, relation.entry().subject_root()).is_some()
                })
                .cloned(),
        ),
        evaluation_effects,
        evaluation_completes,
        evaluation_reports: DependencyContractTemplateData::new([]),
        escaping_evaluation_inputs: escaping_evaluation_inputs.into_iter().collect(),
    })
}

fn expand_call_inputs<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &SelectedCall,
    inputs: &[bray_symbols::DependencyCallInput],
    defaults: &Defaults,
) -> Result<Vec<bray_symbols::DependencyCallInput>, CheckerQueryError<C::UpstreamError>> {
    inputs
        .iter()
        .map(|input| {
            Ok(bray_symbols::DependencyCallInput::new(
                input.root(),
                expand_witness_input_defaults(request, call, input.values(), defaults)?,
                expand_witness_input_defaults(request, call, input.storage(), defaults)?,
            ))
        })
        .collect::<Result<Vec<_>, CheckerQueryError<C::UpstreamError>>>()
}

fn expand_witness_input_defaults<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    call: &SelectedCall,
    requirements: &[DependencyRequirement],
    defaults: &Defaults,
) -> Result<Vec<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
    let template = DependencyContractTemplateData::new(requirements.iter().cloned());
    let expanded = expand(request, call, &template, defaults)?;

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
                Some(SelectedArgument::Default { ty, .. }) => *ty,
                None => {
                    panic!(
                        "Semantic-selection inputs do not describe the requested bound unit or operation category. in default_borrows_argument_storage"
                    );
                }
            }
        }
        DependencySubjectRoot::Receiver => {
            let receiver = call.receiver().unwrap_or_else(|| {
                panic!("default_borrows_argument_storage requires borrowed result receiver")
            });

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
    call: &SelectedCall,
    root: DependencySubjectRoot,
    id: bray_diagnostics::DiagnosticId,
) -> bray_diagnostics::Diagnostic {
    let origin = crate::diagnostic::bound_node_origin(request, expression.into()).unwrap_or_else(|| panic!("escaping_evaluation_diagnostic requires bound node source origin, expression: {expression:?}, id: {id:?}"));

    let source = request.source(origin.source_anchor());

    let argument = super::result_argument(call, root)
        .or_else(|| match request.view().expression(expression) {
            Some(bray_bound_tree::BoundExpression::Call(call)) => Some(call.callee()),
            _ => None,
        }).unwrap_or_else(|| panic!("escaping_evaluation_diagnostic requires checked expression type or node, expression: {expression:?}, id: {id:?}"));

    let argument = crate::diagnostic::bound_node_origin(request, argument.into()).unwrap_or_else(|| panic!("escaping_evaluation_diagnostic requires bound node source origin, expression: {expression:?}, id: {id:?}"));

    let argument = request.source(argument.source_anchor());

    crate::diagnostic::escaping_storage_dependency_diagnostic(id, source.span(), argument.span())
}
