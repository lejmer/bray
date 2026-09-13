use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::{
    CallableInstanceData, DependencyContractTemplateData, DependencyContractTemplateId,
    DependencyRequirement, GenericParameterSymbolId, GenericSubstitutionData, SemanticValueStore,
    SemanticValueStoreError,
};

use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitView,
};

pub(super) type Parameters = BTreeSet<GenericParameterSymbolId>;

pub(super) struct ParameterNode<K> {
    pub(super) callable: CallableInstanceData,
    pub(super) local: DependencyContractTemplateId,
    pub(super) edges: Vec<(CallableInstanceData, K)>,
}

pub(super) fn solve_parameters<C: CheckerRequestContext + ?Sized, K: Copy + Ord>(
    request: CheckerUnitView<'_, C>,
    nodes: &BTreeMap<K, ParameterNode<K>>,
    relevant: &mut BTreeMap<K, Parameters>,
) -> Result<(), CheckerQueryError<C::UpstreamError>> {
    let values = request.semantic_values();

    // Each iteration only adds parameter uses, so this finite graph reaches a fixed point.
    for key in nodes.keys() {
        relevant.insert(*key, BTreeSet::new());
    }

    loop {
        let mut changed = false;

        for (key, node) in nodes {
            if request.is_cancelled() {
                return Err(CheckerQueryError::Cancelled);
            }

            let local = values
                .dependency_contract_template_data(node.local)
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            // Extend this node's immutable local terms with the current callee approximation.
            let mut requirements = local.requirements().to_vec();

            for (callable, target) in &node.edges {
                let callable = retain_parameters(values, *callable, relevant.get(target))
                    .and_then(|callable| values.intern_callable_instance(callable))
                    .map_err(CheckerInfrastructureError::SemanticValueStore)?;

                requirements.push(DependencyRequirement::recursive_call(callable, []));
            }

            let template = values
                .intern_dependency_contract_template(DependencyContractTemplateData::new(
                    requirements,
                ))
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            let substitution = values
                .generic_substitution_data(node.callable.substitution())
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
                    changed |= relevant
                        .entry(*key)
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

pub(super) fn retain_parameters(
    values: &SemanticValueStore,
    callable: CallableInstanceData,
    parameters: Option<&Parameters>,
) -> Result<CallableInstanceData, SemanticValueStoreError> {
    let substitution = values.generic_substitution_data(callable.substitution())?;

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

    let substitution = values.intern_generic_substitution(substitution)?;

    Ok(CallableInstanceData::new(
        callable.definition(),
        substitution,
    ))
}
