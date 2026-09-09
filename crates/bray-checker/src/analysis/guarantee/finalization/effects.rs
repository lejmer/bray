use std::collections::BTreeSet;

use bray_bound_tree::{
    AnyBoundNodeId, CallableProofDependency, CallableProofObligation, CallableProofTarget,
};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    ConstantTermId, ExecutionProperty, TypeAssociatedLifecycleSlot, TypeData, TypeId,
};

use super::super::flow::{DomainState, GuaranteeDomain};
use crate::CheckerInfrastructureError;

impl GuaranteeDomain<'_> {
    /// Retains implementation evidence that destruction completes without runtime effects.
    pub(in crate::analysis::guarantee) fn synchronous_destruction_dependencies(
        &self,
        state: &DomainState,
        ty: TypeId,
        receiver: ConstantTermId,
        exit: AnyBoundNodeId,
    ) -> Result<Option<BTreeSet<CallableProofDependency>>, CheckerInfrastructureError> {
        let mut dependencies = BTreeSet::new();

        for property in [ExecutionProperty::Pure, ExecutionProperty::Total] {
            let Some(proof) = self.destruction_dependencies(state, ty, receiver, exit, property)?
            else {
                return Ok(None);
            };

            dependencies.extend(proof);
        }

        dependencies.extend(state.dependencies.iter().copied());

        Ok(Some(dependencies))
    }

    /// Proves an execution property of destruction, including the consuming destructor's remainder.
    pub(in crate::analysis::guarantee) fn destruction_dependencies(
        &self,
        state: &DomainState,
        ty: TypeId,
        receiver: ConstantTermId,
        exit: AnyBoundNodeId,
        property: ExecutionProperty,
    ) -> Result<Option<BTreeSet<CallableProofDependency>>, CheckerInfrastructureError> {
        let domain = self;

        if let Some(destructor) = domain
            .input
            .lifecycle(ty, TypeAssociatedLifecycleSlot::Destructor)
        {
            let assumptions = state.conditions.iter().copied().collect::<Vec<_>>();

            let mut normalize = |condition| {
                let Some(condition) =
                    crate::instantiate_condition(domain.values, condition, &[receiver])?
                else {
                    return Ok(None);
                };

                domain.current_observation(state, condition, None, None)
            };

            if !crate::contract::preconditions_hold_with(
                domain.values,
                destructor.conditions(),
                &assumptions,
                &mut normalize,
            )
            .map_err(CheckerInfrastructureError::SemanticValueStore)?
            {
                return Ok(None);
            }

            let mut remaining = crate::contract::MAX_CONDITION_STEPS;

            let coverage = crate::guarantee::execution_property_coverage(
                domain.values,
                destructor.conditions(),
                property,
                &assumptions,
                &mut normalize,
                &mut remaining,
            )
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            let target = CallableProofTarget::Implicit {
                site: exit,
                callable: destructor.callable(),
            };

            return Ok(coverage.map(|coverage| {
                coverage
                    .into_iter()
                    .map(|guarantee| {
                        CallableProofDependency::new(
                            target,
                            CallableProofObligation::Execution(guarantee),
                        )
                    })
                    .collect()
            }));
        }

        let mut pending = vec![ty];
        let mut visited = BTreeSet::new();

        while let Some(current) = pending.pop() {
            if !visited.insert(current) {
                continue;
            }

            // Only the root's whole-value finalizer has already been discharged.
            if current != ty
                && (domain.input.finalizer(current).is_some()
                    || domain
                        .input
                        .lifecycle(current, TypeAssociatedLifecycleSlot::Destructor)
                        .is_some())
            {
                return Ok(None);
            }

            let data = domain
                .values
                .type_data(current)
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            match data.as_ref() {
                TypeData::Tuple(_) | TypeData::Array { .. } | TypeData::Nullable(_) => {}
                TypeData::Borrow { .. }
                | TypeData::Callable(_)
                | TypeData::Slice(_)
                | TypeData::FlexibleArray(_)
                | TypeData::TraitView(_) => continue,
                TypeData::Named { .. } => {
                    let role = crate::representation::type_representation_for_values(
                        domain.values,
                        domain.available,
                        current,
                    )?;

                    if role.is_some_and(|role| !inert_representation(role)) {
                        return Ok(None);
                    }
                }
                _ => return Ok(None),
            }

            pending.extend(domain.input.cleanup_dependencies(current));
        }

        Ok(Some(BTreeSet::new()))
    }
}

fn inert_representation(role: RepresentationRole) -> bool {
    role.numeric_kind().is_some()
        || matches!(
            role,
            RepresentationRole::ScalarBool
                | RepresentationRole::ScalarChar
                | RepresentationRole::Unit
                | RepresentationRole::Never
                | RepresentationRole::Result
                | RepresentationRole::RunResult
                | RepresentationRole::ConversionError
        )
}
