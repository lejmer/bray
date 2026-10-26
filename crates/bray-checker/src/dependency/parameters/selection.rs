use std::collections::BTreeMap;

use bray_symbols::{
    CallableDefinitionId, CallableInstanceData, DependencyContractTemplateData,
    DependencyRequirement, SemanticValueStore, SemanticValueStoreError,
};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

use super::graph::{ParameterNode, Parameters, retain_parameters, solve_parameters};

#[derive(Default)]
pub(super) struct SelectionParameters {
    relevant: BTreeMap<(CallableDefinitionId, bray_symbols::DependencyCallOutput), Parameters>,
}

impl SelectionParameters {
    pub(super) fn key<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        selected: CallableInstanceData,
        output: bray_symbols::DependencyCallOutput,
    ) -> Result<CallableInstanceData, CheckerQueryError<C::UpstreamError>> {
        if !self.relevant.contains_key(&(selected.definition(), output)) {
            self.discover(request, selected, output)?;
        }

        Ok(retain_parameters(
            request.semantic_values(),
            selected,
            self.relevant.get(&(selected.definition(), output)),
        )
        .unwrap_or_else(|error| {
            panic!("key must satisfy its checked construction contract: {error:?}")
        }))
    }

    fn discover<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        selected: CallableInstanceData,
        output: bray_symbols::DependencyCallOutput,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        let values = request.semantic_values();
        let mut pending = vec![(selected, output)];
        let mut nodes = BTreeMap::new();

        while let Some((callable, output)) = pending.pop() {
            if self.relevant.contains_key(&(callable.definition(), output))
                || nodes.contains_key(&(callable.definition(), output))
            {
                continue;
            }

            if request.is_cancelled() {
                return Err(CheckerQueryError::Cancelled);
            }

            let template =
                request.callable_result_dependencies(callable.definition().callable_symbol())?;

            let template = values.dependency_contract_template_data(template);

            let mut edges = Vec::new();

            let local =
                selection_requirements(values, template.output_requirements(output), &mut pending, &mut edges).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in discover: {error:?}"));

            let local = values
                .intern_dependency_contract_template(DependencyContractTemplateData::new(local)).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in discover: {error:?}"));

            nodes.insert(
                (callable.definition(), output),
                ParameterNode {
                    callable,
                    local,
                    edges,
                },
            );
        }

        solve_parameters(request, &nodes, &mut self.relevant, None)
    }
}

fn selection_requirements(
    values: &SemanticValueStore,
    requirements: &[DependencyRequirement],
    pending: &mut Vec<(CallableInstanceData, bray_symbols::DependencyCallOutput)>,
    edges: &mut Vec<(
        CallableInstanceData,
        (CallableDefinitionId, bray_symbols::DependencyCallOutput),
    )>,
) -> Result<Vec<DependencyRequirement>, SemanticValueStoreError> {
    super::super::equations::map_requirements(requirements, 0, &mut |item, _| {
        if let DependencyRequirement::CleanupDependencies { requirements, .. } = item {
            // Cleanup presence changes retained reports, not implementation selection.
            return Some(selection_requirements(values, requirements, pending, edges));
        }

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
            let callable = values.callable_instance_data(callable);

            let inputs =
                super::super::equations::map_input_requirements(inputs, &mut |requirements| {
                    selection_requirements(values, requirements, pending, edges)
                })?;

            let empty =
                values.intern_callable_instance(retain_parameters(values, *callable, None)?)?;

            // Member arguments do not select the implementation. Its result is inspected separately.
            if requirement.is_none() {
                pending.push((*callable, *output));
                edges.push((*callable, (callable.definition(), *output)));
            }

            Ok(vec![DependencyRequirement::result_call(
                empty,
                *requirement,
                inputs,
                *output,
            )])
        })())
    })
}
