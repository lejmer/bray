use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::{
    CallableDefinitionId, CallableInstanceData, DependencyContractTemplateData,
    DependencyRequirement, GenericParameterSymbolId, GenericSubstitutionData,
    GenericSubstitutionId, SemanticValueStore, SemanticValueStoreError,
};

use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView,
};

type RelevantParameters = BTreeMap<CallableDefinitionId, BTreeSet<GenericParameterSymbolId>>;

#[derive(Default)]
pub(super) struct ResultParameters {
    relevant: RelevantParameters,
}

impl ResultParameters {
    pub(super) fn key<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        selected: CallableInstanceData,
    ) -> Result<CallableInstanceData, CheckerQueryError<C::UpstreamError>> {
        if !self.relevant.contains_key(&selected.definition()) {
            self.discover(request, selected)?;
        }

        let values = request.semantic_values();

        let substitution = retain_parameters(values, selected, &self.relevant)
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

        Ok(CallableInstanceData::new(
            selected.definition(),
            substitution,
        ))
    }

    fn discover<C: CheckerRequestContext + ?Sized>(
        &mut self,
        request: CheckerUnitView<'_, C>,
        selected: CallableInstanceData,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        let values = request.semantic_values();
        let mut pending = vec![selected];
        let mut templates = BTreeMap::new();

        while let Some(callable) = pending.pop() {
            if self.relevant.contains_key(&callable.definition())
                || templates.contains_key(&callable.definition())
            {
                continue;
            }

            if request.is_cancelled() {
                return Err(CheckerQueryError::Cancelled);
            }

            let template = request
                .context()
                .callable_result_dependencies(callable.definition().callable_symbol())?;

            let data = values
                .dependency_contract_template_data(template)
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            super::equations::map_requirements(data.requirements(), 0, &mut |item, _| {
                if let DependencyRequirement::RecursiveCall { callable, .. } = item {
                    match values.callable_instance_data(*callable) {
                        Ok(callable) => pending.push(*callable),
                        Err(error) => return Some(Err(error)),
                    }
                }

                None
            })
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            templates.insert(callable.definition(), (callable, data));
        }

        // Start with no recursive argument uses, then propagate uses backwards through the graph.
        for definition in templates.keys() {
            self.relevant.insert(*definition, BTreeSet::new());
        }

        loop {
            let mut changed = false;

            for (definition, (callable, template)) in &templates {
                if request.is_cancelled() {
                    return Err(CheckerQueryError::Cancelled);
                }

                let requirements = prune_calls(values, template.requirements(), &self.relevant)
                    .map_err(CheckerInfrastructureError::SemanticValueStore)?;

                let template = values
                    .intern_dependency_contract_template(DependencyContractTemplateData::new(
                        requirements,
                    ))
                    .map_err(CheckerInfrastructureError::SemanticValueStore)?;

                let substitution = values
                    .generic_substitution_data(callable.substitution())
                    .map_err(CheckerInfrastructureError::SemanticValueStore)?;

                for binding in substitution.bindings() {
                    if values
                        .dependency_contract_uses_parameter(
                            template,
                            substitution.owner(),
                            binding.parameter(),
                        )
                        .map_err(CheckerInfrastructureError::SemanticValueStore)?
                    {
                        changed |= self
                            .relevant
                            .entry(*definition)
                            .or_default()
                            .insert(binding.parameter());
                    }
                }
            }

            if !changed {
                return Ok(());
            }
        }
    }
}

fn retain_parameters(
    values: &SemanticValueStore,
    callable: CallableInstanceData,
    relevant: &RelevantParameters,
) -> Result<GenericSubstitutionId, SemanticValueStoreError> {
    let substitution = values.generic_substitution_data(callable.substitution())?;
    let parameters = relevant.get(&callable.definition());

    let retained = substitution
        .bindings()
        .iter()
        .filter(|binding| {
            parameters.is_some_and(|parameters| parameters.contains(&binding.parameter()))
        })
        .collect::<Vec<_>>();

    let substitution = GenericSubstitutionData::try_new(
        substitution.owner(),
        retained.iter().map(|binding| binding.parameter()),
        retained.iter().map(|binding| binding.argument()),
    )
    .map_err(|_| SemanticValueStoreError::OpenSubstitution)?;

    values.intern_generic_substitution(substitution)
}

fn prune_calls(
    values: &SemanticValueStore,
    requirements: &[DependencyRequirement],
    relevant: &RelevantParameters,
) -> Result<Vec<DependencyRequirement>, SemanticValueStoreError> {
    super::equations::map_requirements(requirements, 0, &mut |item, _| {
        let DependencyRequirement::RecursiveCall { callable, inputs } = item else {
            return None;
        };

        Some((|| {
            let callable = values.callable_instance_data(*callable)?;
            let substitution = retain_parameters(values, *callable, relevant)?;

            let callable = values.intern_callable_instance(CallableInstanceData::new(
                callable.definition(),
                substitution,
            ))?;

            let inputs = inputs
                .iter()
                .map(|input| {
                    Ok(bray_symbols::DependencyCallInput::new(
                        input.root(),
                        prune_calls(values, input.values(), relevant)?,
                        prune_calls(values, input.storage(), relevant)?,
                    ))
                })
                .collect::<Result<Vec<_>, SemanticValueStoreError>>()?;

            Ok(vec![DependencyRequirement::recursive_call(
                callable, inputs,
            )])
        })())
    })
}
