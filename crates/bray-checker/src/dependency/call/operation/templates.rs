use super::inputs::{evaluation_dependencies, nullable_inputs, operation_inputs};
use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{
    BoundExpressionId, ConstructionInputId, ConstructionTarget, DefaultValueProvider, IndexTarget,
    OperatorTarget, SelectedArgument, SelectedConstructionInput, SelectedOperation,
};
use bray_symbols::{
    DependencyContractTemplateData, DependencyRequirement, DependencyRequirementKind,
    DependencySubject, DependencySubjectRoot, SymbolOrdinal,
};

pub(crate) fn operation_result_dependencies<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    operation: &SelectedOperation,
) -> Result<
    Option<crate::dependency::defaults::CallResultDependencies>,
    CheckerQueryError<C::UpstreamError>,
> {
    if let SelectedOperation::Construction(construction) = operation {
        if !matches!(construction.target(), ConstructionTarget::TypeForm { .. })
            && !construction
                .inputs()
                .iter()
                .any(|input| matches!(input, SelectedConstructionInput::Default { .. }))
        {
            return Ok(None);
        }

        return construction_dependencies(request, construction).map(Some);
    }

    if let SelectedOperation::Conversion(conversion) = operation {
        if !conversion.invokes_source() {
            return Ok(None);
        }

        let template = super::conversion::conversion_dependencies(request, conversion)?;

        return Ok(Some(direct_dependencies(template)));
    }

    let (instance, dispatch) = if let Some(target) = operation.operator_target() {
        match target {
            OperatorTarget::BuiltIn(_) => return Ok(None),
            OperatorTarget::Trait { fulfillment, .. } => (fulfillment, None),
            OperatorTarget::TraitConstraint {
                member, dispatch, ..
            } => (member, Some(dispatch)),
        }
    } else {
        match operation {
            SelectedOperation::Index {
                target: IndexTarget::Custom { fulfillment, .. },
                ..
            } => (*fulfillment, None),
            SelectedOperation::Index {
                target:
                    IndexTarget::TraitConstraint {
                        member, dispatch, ..
                    },
                ..
            } => (*member, Some(*dispatch)),
            SelectedOperation::Member(_)
            | SelectedOperation::Index { .. }
            | SelectedOperation::Implementation(_) => return Ok(None),
            SelectedOperation::Operator { .. }
            | SelectedOperation::CompoundAssignment(_)
            | SelectedOperation::Construction(_)
            | SelectedOperation::Conversion(_) => unreachable!(),
        }
    };

    let inputs = operation_inputs(request.view(), expression, operation);

    let template = invocation_dependencies(
        request,
        instance,
        dispatch,
        inputs.iter().map(|(root, _)| *root),
    )?;

    let mut escaping = std::collections::BTreeSet::new();

    let template = template
        .try_map_requirements(|requirements| nullable_inputs(requirements, &inputs))
        .expect("synthesized nullable dependency mapping is infallible");

    let template = template
        .try_map_requirements(|requirements| evaluation_dependencies(requirements, &mut escaping))
        .expect("operation evaluation dependency classification is infallible");

    let mut result = direct_dependencies(template);

    result.escaping_evaluation_inputs = escaping.into_iter().collect();

    Ok(Some(result))
}

pub(super) fn invocation_dependencies<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    instance: bray_symbols::CallableInstanceData,
    dispatch: Option<bray_symbols::TraitConstraintDispatch>,
    roots: impl IntoIterator<Item = DependencySubjectRoot>,
) -> Result<DependencyContractTemplateData, CheckerQueryError<C::UpstreamError>> {
    let requirement = dispatch
        .map(|dispatch| request.context().result_dispatch_requirement(dispatch))
        .transpose()?;

    let symbol = instance.definition().callable_symbol();

    if requirement.is_none() && !request.is_recursive_result(symbol) {
        let template = request.callable_result_dependencies(symbol)?;

        return crate::dependency::resolved_result_template(request, Some(instance), template);
    }

    crate::dependency::witness::deferred_outputs(request, instance, requirement, roots, true, true)
}

fn direct_dependencies(
    template: DependencyContractTemplateData,
) -> crate::dependency::defaults::CallResultDependencies {
    crate::dependency::defaults::CallResultDependencies {
        template,
        evaluation_effects: std::sync::Arc::from([]),
        evaluation_completes: true,
        evaluation_preserves_storage: true,
        evaluation_reports: DependencyContractTemplateData::new([]),
        escaping_evaluation_inputs: Vec::new(),
    }
}

fn construction_dependencies<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    construction: &bray_bound_tree::SelectedConstruction,
) -> Result<crate::dependency::defaults::CallResultDependencies, CheckerQueryError<C::UpstreamError>>
{
    if let ConstructionTarget::TypeForm { callable, .. } = construction.target() {
        // Normalization retains the selected conversion's immutable child plans.
        let arguments = construction
            .inputs()
            .iter()
            .map(|input| match input {
                SelectedConstructionInput::Explicit {
                    expression,
                    input: ConstructionInputId::CallableParameter(parameter),
                    conversion,
                    ordinal,
                } => SelectedArgument::Explicit {
                    expression: *expression,
                    parameter: Some(*parameter),
                    conversion: conversion.clone(),
                    ordinal: *ordinal,
                    reborrow: None,
                },
                SelectedConstructionInput::Default {
                    input: ConstructionInputId::CallableParameter(parameter),
                    provider: DefaultValueProvider::CallableParameter(provider),
                    ty,
                    ordinal,
                } => SelectedArgument::Default {
                    parameter: *parameter,
                    provider: *provider,
                    ty: *ty,
                    ordinal: *ordinal,
                },
                _ => panic!("type-form construction retains callable parameter inputs"),
            })
            .collect::<Vec<_>>();

        let template = invocation_dependencies(
            request,
            callable,
            None,
            arguments.iter().map(|argument| match argument {
                SelectedArgument::Explicit { ordinal, .. }
                | SelectedArgument::Default { ordinal, .. } => {
                    DependencySubjectRoot::Parameter(SymbolOrdinal::new(*ordinal))
                }
            }),
        )?;

        return crate::dependency::defaults::expand_result_defaults(
            request,
            &arguments,
            None,
            Some(callable),
            &template,
        );
    }

    let mut template =
        DependencyContractTemplateData::new(construction.inputs().iter().filter_map(|input| {
            match input {
                SelectedConstructionInput::Explicit { ordinal, .. } => {
                    Some(DependencyRequirement::direct(
                        DependencySubject::root(DependencySubjectRoot::Parameter(
                            SymbolOrdinal::new(*ordinal),
                        )),
                        DependencyRequirementKind::ValueDependencies,
                    ))
                }
                _ => None,
            }
        }))
        .with_effects([]);

    for input in construction.inputs() {
        let SelectedConstructionInput::Default { provider, .. } = input else {
            continue;
        };

        let default = request.default_result_dependencies(*provider)?;

        let default = if let bray_symbols::TypeData::Named { substitution, .. } = request
            .semantic_values()
            .type_data(construction.result_type())
            .as_ref()
        {
            request
                .semantic_values()
                .substitute_dependency_contract(default, *substitution)
                .expect("construction default uses its selected type substitution")
        } else {
            default
        };

        let default = crate::dependency::resolved_result_template(request, None, default)?;

        template = super::conversion::sequence_dependencies(template, default);
    }

    Ok(direct_dependencies(template))
}
