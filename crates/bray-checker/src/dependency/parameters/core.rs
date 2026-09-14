use std::collections::BTreeMap;

use bray_symbols::{CallableInstanceData, DependencyContractTemplateData, DependencyRequirement};

use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView,
};

use super::graph::{ParameterNode, Parameters, retain_parameters, solve_parameters};

pub(in crate::dependency) type ResultKey = (
    CallableInstanceData,
    Option<(bray_symbols::SelfTypeContext, bray_symbols::TypeId)>,
);

#[derive(Default)]
pub(in crate::dependency) struct ResultParameters {
    selection: super::selection::SelectionParameters,
    relevant: BTreeMap<ResultKey, Parameters>,
}

impl ResultParameters {
    pub(in crate::dependency) fn key<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        selected: ResultKey,
    ) -> Result<ResultKey, CheckerQueryError<C::UpstreamError>> {
        // Reuse a relevance graph only when its implementation-selection arguments agree.
        let node = (self.selection.key(request, selected.0)?, selected.1);

        if !self.relevant.contains_key(&node) {
            self.discover(request, node, selected.0)?;
        }

        let callable = retain_parameters(
            request.semantic_values(),
            selected.0,
            self.relevant.get(&node),
        )
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

        Ok((callable, selected.1))
    }

    fn discover<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        root: ResultKey,
        callable: CallableInstanceData,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        let values = request.semantic_values();
        let mut pending = vec![(root, callable)];
        let mut nodes = BTreeMap::new();

        while let Some((key, callable)) = pending.pop() {
            if self.relevant.contains_key(&key) || nodes.contains_key(&key) {
                continue;
            }

            if request.is_cancelled() {
                return Err(CheckerQueryError::Cancelled);
            }

            let template = request
                .context()
                .callable_result_dependencies(callable.definition().callable_symbol())?;

            let template = values
                .dependency_contract_template_data(template)
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            let mut edges = Vec::new();

            let local = self.local_requirements(
                request,
                key,
                template.requirements(),
                &mut pending,
                &mut edges,
            )?;

            let local = values
                .intern_dependency_contract_template(DependencyContractTemplateData::new(local))
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            nodes.insert(
                key,
                ParameterNode {
                    callable,
                    local,
                    edges,
                },
            );
        }

        solve_parameters(request, &nodes, &mut self.relevant)
    }

    fn local_requirements<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        key: ResultKey,
        requirements: &[DependencyRequirement],
        pending: &mut Vec<(ResultKey, CallableInstanceData)>,
        edges: &mut Vec<(CallableInstanceData, ResultKey)>,
    ) -> Result<Vec<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
        let values = request.semantic_values();

        super::super::equations::map_requirements(requirements, 0, &mut |item, _| {
            let (callable, requirement, inputs) = match item {
                DependencyRequirement::ResultCall {
                    callable,
                    requirement,
                    inputs,
                } => (*callable, requirement, inputs),
                _ => return None,
            };

            Some((|| {
                let inputs =
                    super::super::equations::map_input_requirements(inputs, &mut |requirements| {
                        self.local_requirements(request, key, requirements, pending, edges)
                    })?;

                let original = values
                    .callable_instance_data(callable)
                    .map_err(CheckerInfrastructureError::SemanticValueStore)?;

                let target = if let Some(requirement) = requirement {
                    let Some(target) = witness_target(request, key, callable, *requirement)? else {
                        return Ok(vec![DependencyRequirement::result_call(
                            callable,
                            Some(*requirement),
                            inputs,
                        )]);
                    };

                    target
                } else {
                    (*original, None)
                };

                let concrete = instantiate(
                    request,
                    key,
                    DependencyRequirement::result_call(
                        values
                            .intern_callable_instance(target.0)
                            .map_err(CheckerInfrastructureError::SemanticValueStore)?,
                        None,
                        [],
                    ),
                )?;

                let Some(DependencyRequirement::ResultCall {
                    requirement: None,
                    callable: concrete,
                    ..
                }) = concrete.requirements().first()
                else {
                    return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput.into());
                };

                let concrete = values
                    .callable_instance_data(*concrete)
                    .map_err(CheckerInfrastructureError::SemanticValueStore)?;

                let target_key = (self.selection.key(request, *concrete)?, target.1);

                pending.push((target_key, target.0));
                edges.push((target.0, target_key));

                let empty = retain_parameters(values, *original, None)
                    .and_then(|callable| values.intern_callable_instance(callable))
                    .map_err(CheckerInfrastructureError::SemanticValueStore)?;

                Ok(vec![DependencyRequirement::result_call(
                    empty,
                    *requirement,
                    inputs,
                )])
            })())
        })
    }
}

fn witness_target<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    key: ResultKey,
    callable: bray_symbols::CallableInstanceId,
    requirement: bray_symbols::ImplementationRequirementKey,
) -> Result<Option<ResultKey>, CheckerQueryError<C::UpstreamError>> {
    let selection = instantiate(
        request,
        key,
        DependencyRequirement::result_call(callable, Some(requirement), []),
    )?;

    let Some(DependencyRequirement::ResultCall {
        requirement: Some(requirement),
        ..
    }) = selection.requirements().first()
    else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput.into());
    };

    request
        .context()
        .result_witness_callable(callable, *requirement)
        .map(|selected| {
            selected.map(|(target, context)| (target, Some((context, requirement.subject()))))
        })
}

fn instantiate<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    key: ResultKey,
    requirement: DependencyRequirement,
) -> Result<std::sync::Arc<DependencyContractTemplateData>, CheckerQueryError<C::UpstreamError>> {
    let template = request
        .semantic_values()
        .intern_dependency_contract_template(DependencyContractTemplateData::new([requirement]))
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    super::super::witness::instantiate_template(request, template, key)
}
