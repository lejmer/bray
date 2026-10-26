use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::{
    CallableInstanceData, DependencyContractTemplateData, DependencyContractTemplateId,
    DependencyRequirement, GenericParameterSymbolId, GenericSubstitutionData, SemanticValueStore,
    SemanticValueStoreError,
};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

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
    cleanup_parameters: Option<&BTreeMap<K, Parameters>>,
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

            let local = values.dependency_contract_template_data(node.local);

            // Extend this node's immutable local terms with the current callee approximation.
            let mut requirements = if cleanup_parameters.is_some() {
                super::cleanup::exact_requirements(request, local.requirements())?
            } else {
                local.requirements().to_vec()
            };

            for (callable, target) in &node.edges {
                let mut retained = cleanup_parameters.map(|_| relevant.get(target)
                    .into_iter().flatten().copied().collect::<Parameters>());

                if let (Some(retained), Some(all)) = (retained.as_mut(), cleanup_parameters.and_then(|all| all.get(target))) {
                    let selected = values.generic_substitution_data(callable.substitution());

                    for binding in selected.bindings() {
                        if !all.contains(&binding.parameter()) || retained.contains(&binding.parameter()) {
                            continue;
                        }

                        // Cleanup presence is independent of constant arguments.
                        // Type projections that need them remain exact above.
                        if let bray_symbols::GenericArgument::Type(ty) = binding.argument() {
                            requirements.extend(super::cleanup::exact_predicate_requirements(request, ty)?);
                        }
                    }
                }

                let callable = retain_parameters(values, *callable, retained.as_ref().or_else(|| relevant.get(target)))
                    .and_then(|callable| values.intern_callable_instance(callable)).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in solve_parameters: {error:?}"));

                requirements.push(DependencyRequirement::result_call(
                    callable,
                    None,
                    [],
                    bray_symbols::DependencyCallOutput::Value,
                ));
            }

            let template = values
                .intern_dependency_contract_template(DependencyContractTemplateData::new(
                    requirements,
                )).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in solve_parameters: {error:?}"));

            let substitution = values.generic_substitution_data(node.callable.substitution());

            for binding in substitution.bindings() {
                if values
                    .dependency_contract_uses_parameter(
                        template,
                        substitution.owner(),
                        binding.parameter(),
                    ).unwrap_or_else(|error| panic!("The canonical semantic value store rejected a construction or lookup operation. in solve_parameters: {error:?}"))
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
    let substitution = values.generic_substitution_data(callable.substitution());

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
    .expect("substitution preserves parameter kinds and arity");

    let substitution = values.intern_generic_substitution(substitution)?;

    Ok(CallableInstanceData::new(
        callable.definition(),
        substitution,
    ))
}
