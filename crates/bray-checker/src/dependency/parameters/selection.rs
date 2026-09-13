use std::collections::BTreeMap;

use bray_symbols::{
    CallableDefinitionId, CallableInstanceData, DependencyContractTemplateData,
    DependencyRequirement, SemanticValueStore, SemanticValueStoreError,
};

use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView,
};

use super::graph::{ParameterNode, Parameters, retain_parameters, solve_parameters};

#[derive(Default)]
pub(super) struct SelectionParameters {
    relevant: BTreeMap<CallableDefinitionId, Parameters>,
}

impl SelectionParameters {
    pub(super) fn key<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        selected: CallableInstanceData,
    ) -> Result<CallableInstanceData, CheckerQueryError<C::UpstreamError>> {
        if !self.relevant.contains_key(&selected.definition()) {
            self.discover(request, selected)?;
        }

        retain_parameters(
            request.semantic_values(),
            selected,
            self.relevant.get(&selected.definition()),
        )
        .map_err(|error| CheckerInfrastructureError::SemanticValueStore(error).into())
    }

    fn discover<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        selected: CallableInstanceData,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        let values = request.semantic_values();
        let mut pending = vec![selected];
        let mut nodes = BTreeMap::new();

        while let Some(callable) = pending.pop() {
            if self.relevant.contains_key(&callable.definition())
                || nodes.contains_key(&callable.definition())
            {
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

            let local =
                selection_requirements(values, template.requirements(), &mut pending, &mut edges)
                    .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            let local = values
                .intern_dependency_contract_template(DependencyContractTemplateData::new(local))
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            nodes.insert(
                callable.definition(),
                ParameterNode {
                    callable,
                    local,
                    edges,
                },
            );
        }

        solve_parameters(request, &nodes, &mut self.relevant)
    }
}

fn selection_requirements(
    values: &SemanticValueStore,
    requirements: &[DependencyRequirement],
    pending: &mut Vec<CallableInstanceData>,
    edges: &mut Vec<(CallableInstanceData, CallableDefinitionId)>,
) -> Result<Vec<DependencyRequirement>, SemanticValueStoreError> {
    super::super::equations::map_requirements(requirements, 0, &mut |item, _| {
        let (callable, inputs) = match item {
            DependencyRequirement::RecursiveCall { callable, inputs }
            | DependencyRequirement::WitnessCall {
                callable, inputs, ..
            } => (*callable, inputs),
            _ => return None,
        };

        Some((|| {
            let callable = values.callable_instance_data(callable)?;

            let inputs =
                super::super::equations::map_input_requirements(inputs, &mut |requirements| {
                    selection_requirements(values, requirements, pending, edges)
                })?;

            let empty =
                values.intern_callable_instance(retain_parameters(values, *callable, None)?)?;

            let local = match item {
                DependencyRequirement::WitnessCall { requirement, .. } => {
                    // Member arguments do not select the implementation. Its result is inspected separately.
                    DependencyRequirement::witness_call(empty, *requirement, inputs)
                }
                _ => {
                    pending.push(*callable);
                    edges.push((*callable, callable.definition()));

                    DependencyRequirement::recursive_call(empty, inputs)
                }
            };

            Ok(vec![local])
        })())
    })
}
