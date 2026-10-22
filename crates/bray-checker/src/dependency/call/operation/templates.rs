use super::inputs::{evaluation_dependencies, nullable_inputs, operation_inputs};
use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{
    BoundExpressionId, ConstructionTarget, IndexTarget, OperatorTarget, SelectedConstructionInput,
    SelectedOperation,
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
        .map(|dispatch| request.context().dispatch_requirement(dispatch))
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
        escaping_evaluation_inputs: Vec::new(),
    }
}

fn construction_dependencies<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    construction: &bray_bound_tree::SelectedConstruction,
) -> Result<crate::dependency::defaults::CallResultDependencies, CheckerQueryError<C::UpstreamError>>
{
    let roots = construction.inputs().iter().map(|input| {
        let (SelectedConstructionInput::Explicit { ordinal, .. }
        | SelectedConstructionInput::Default { ordinal, .. }) = input;

        DependencySubjectRoot::Parameter(SymbolOrdinal::new(*ordinal))
    });

    let template = if let ConstructionTarget::TypeForm { callable, .. } = construction.target() {
        invocation_dependencies(request, callable, None, roots)?
    } else {
        DependencyContractTemplateData::new(roots.map(|root| {
            DependencyRequirement::direct(
                DependencySubject::root(root),
                DependencyRequirementKind::ValueDependencies,
            )
        }))
        .with_effects([])
    };

    Ok(direct_dependencies(template))
}
