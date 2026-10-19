use std::collections::BTreeMap;

use bray_symbols::{CallableInstanceData, DependencyContractTemplateData, DependencyRequirement};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

use super::graph::{ParameterNode, Parameters, retain_parameters, solve_parameters};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::dependency) struct ResultKey {
    pub(in crate::dependency) callable: CallableInstanceData,
    pub(in crate::dependency) contextual_self:
        Option<(bray_symbols::SelfTypeContext, bray_symbols::TypeId)>,
    pub(in crate::dependency) output: bray_symbols::DependencyCallOutput,
}

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
        let node = ResultKey {
            callable: self
                .selection
                .key(request, selected.callable, selected.output)?,
            ..selected
        };

        if !self.relevant.contains_key(&node) {
            self.discover(request, node, selected.callable)?;
        }

        let callable = retain_parameters(
            request.semantic_values(),
            selected.callable,
            self.relevant.get(&node),
        ).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in key: {error:?}"));

        Ok(ResultKey {
            callable,
            ..selected
        })
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

            let template =
                request.callable_result_dependencies(callable.definition().callable_symbol())?;

            let template = values.dependency_contract_template_data(template);

            let mut edges = Vec::new();

            let local = self.local_requirements(
                request,
                key,
                template.output_requirements(key.output),
                &mut pending,
                &mut edges,
            )?;

            let local = values
                .intern_dependency_contract_template(DependencyContractTemplateData::new(local)).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in discover: {error:?}"));

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
            let (callable, requirement, inputs, output) = match item {
                DependencyRequirement::ResultCall {
                    output,
                    callable,
                    requirement,
                    inputs,
                } => (*callable, requirement, inputs, output),
                _ => return None,
            };

            Some((|| {
                let inputs =
                    super::super::equations::map_input_requirements(inputs, &mut |requirements| {
                        self.local_requirements(request, key, requirements, pending, edges)
                    })?;

                let original = values.callable_instance_data(callable);

                let target = if let Some(requirement) = requirement {
                    let Some(target) =
                        witness_target(request, key, callable, *requirement, *output)?
                    else {
                        return Ok(vec![DependencyRequirement::result_call(
                            callable,
                            Some(*requirement),
                            inputs,
                            *output,
                        )]);
                    };

                    target
                } else {
                    ResultKey {
                        callable: *original,
                        contextual_self: None,
                        output: *output,
                    }
                };

                let concrete = instantiate(
                    request,
                    key,
                    DependencyRequirement::result_call(
                        values
                            .intern_callable_instance(target.callable).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in local_requirements: {error:?}")),
                        None,
                        [], bray_symbols::DependencyCallOutput::Value, ),
                )?;

                let Some(DependencyRequirement::ResultCall {
                    requirement: None,
                    callable: concrete,
                    ..
                }) = concrete.requirements().first()
                else {
                    panic!(
                        "Semantic-selection inputs do not describe the requested bound unit or operation category. in local_requirements"
                    );
                };

                let concrete = values.callable_instance_data(*concrete);

                let target_key = ResultKey {
                    callable: self.selection.key(request, *concrete, *output)?,
                    ..target
                };

                pending.push((target_key, target.callable));
                edges.push((target.callable, target_key));

                let empty = retain_parameters(values, *original, None)
                    .and_then(|callable| values.intern_callable_instance(callable)).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in local_requirements: {error:?}"));

                Ok(vec![DependencyRequirement::result_call(
                    empty,
                    *requirement,
                    inputs,
                    *output,
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
    output: bray_symbols::DependencyCallOutput,
) -> Result<Option<ResultKey>, CheckerQueryError<C::UpstreamError>> {
    let selection = instantiate(
        request,
        key,
        DependencyRequirement::result_call(
            callable,
            Some(requirement),
            [],
            bray_symbols::DependencyCallOutput::Value,
        ),
    )?;

    let Some(DependencyRequirement::ResultCall {
        requirement: Some(requirement),
        ..
    }) = selection.requirements().first()
    else {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in witness_target"
        );
    };

    request
        .context()
        .result_witness_callable(callable, *requirement)
        .map(|selected| {
            selected.map(|(callable, context)| ResultKey {
                callable,
                contextual_self: Some((context, requirement.subject())),
                output,
            })
        })
}

fn instantiate<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    key: ResultKey,
    requirement: DependencyRequirement,
) -> Result<std::sync::Arc<DependencyContractTemplateData>, CheckerQueryError<C::UpstreamError>> {
    let template = request
        .semantic_values()
        .intern_dependency_contract_template(DependencyContractTemplateData::new([requirement])).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in instantiate: {error:?}"));

    super::super::witness::instantiate_template(request, template, key)
}
