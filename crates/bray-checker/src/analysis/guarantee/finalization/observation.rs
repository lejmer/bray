use bray_bound_tree::{
    AsyncScopeExitPlan, StorageAccessId, StorageCleanupPart, StorageCleanupProjection,
    StorageCleanupProjectionKind, StoragePlan, StorageProjection,
};
use bray_symbols::{
    ConstantProjection, ConstantProjectionKind, ConstantTermData, ConstantTermId, ConstantTest,
    ProofOutcome,
};

use super::super::flow::{DomainState, GuaranteeDomain};
use crate::CheckerInfrastructureError;

#[derive(Clone, Copy)]
pub(in crate::analysis::guarantee) enum CompletionReceiver {
    Unknown,
    Absent,
    Value(ConstantTermId),
}

pub(in crate::analysis::guarantee) fn completion_receiver(
    domain: &GuaranteeDomain<'_>,
    state: &DomainState,
    access: StorageAccessId,
    path: &[StorageCleanupProjection],
) -> Result<CompletionReceiver, CheckerInfrastructureError> {
    let Some(mut receiver) = domain
        .storage
        .root_identity(access)
        .and_then(|identity| domain.input.storage_observation(identity))
    else {
        return Ok(CompletionReceiver::Unknown);
    };

    for projection in path {
        if let StorageCleanupProjectionKind::ArrayElements(length) = projection.projection() {
            let mut remaining = crate::contract::MAX_CONDITION_STEPS;

            if crate::constant::shape::observation_index(domain.values, length, &mut remaining)
                .map_err(CheckerInfrastructureError::SemanticValueStore)?
                == Some(0)
            {
                return Ok(CompletionReceiver::Absent);
            }
        }

        let (projection, active) = match projection.projection() {
            StorageCleanupProjectionKind::Component(StorageProjection::ProductField(field)) => {
                (ConstantProjectionKind::ProductField(field), None)
            }
            StorageCleanupProjectionKind::OwnedTarget(_) => {
                if !domain.input.projections_verified() {
                    return Ok(CompletionReceiver::Unknown);
                }

                (ConstantProjectionKind::OwnedTarget, None)
            }
            StorageCleanupProjectionKind::Component(StorageProjection::TupleElement(ordinal)) => {
                (ConstantProjectionKind::TupleElement(ordinal), None)
            }
            StorageCleanupProjectionKind::Component(StorageProjection::NullableValue) => (
                ConstantProjectionKind::NullableValue,
                Some(ConstantTest::NullablePresent),
            ),
            StorageCleanupProjectionKind::Component(
                StorageProjection::ActiveUnionPayloadField { variant, field },
            ) => (
                ConstantProjectionKind::UnionPayloadField(field),
                Some(ConstantTest::ActiveUnionVariant(variant)),
            ),
            StorageCleanupProjectionKind::ArrayElements(length) => {
                let Some(index) = arbitrary_element_index(domain, state, receiver, length)? else {
                    return Ok(CompletionReceiver::Unknown);
                };

                (ConstantProjectionKind::ArrayElement(index), None)
            }
            _ => return Ok(CompletionReceiver::Unknown),
        };

        if let Some(active) = active {
            let active = domain
                .values
                .intern_constant_term(ConstantTermData::Test {
                    subject: receiver,
                    kind: active,
                })
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            let observed = domain
                .current_observation(state, active, None, None)
                .map_err(CheckerInfrastructureError::SemanticValueStore)?;

            if let Some(observed) = observed
                && crate::prove_condition(
                    domain.values,
                    &state.conditions.iter().copied().collect::<Vec<_>>(),
                    observed,
                )
                .map_err(CheckerInfrastructureError::SemanticValueStore)?
                    == ProofOutcome::Disproven
            {
                return Ok(CompletionReceiver::Absent);
            }
        }

        receiver = domain
            .values
            .intern_constant_term(ConstantTermData::Projection(ConstantProjection::new(
                receiver, projection,
            )))
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;
    }

    Ok(CompletionReceiver::Value(receiver))
}

fn arbitrary_element_index(
    domain: &GuaranteeDomain<'_>,
    state: &DomainState,
    receiver: ConstantTermId,
    length: ConstantTermId,
) -> Result<Option<ConstantTermId>, CheckerInfrastructureError> {
    let mut remaining = crate::contract::MAX_CONDITION_STEPS;

    if crate::constant::shape::observation_index(domain.values, length, &mut remaining)
        .map_err(CheckerInfrastructureError::SemanticValueStore)?
        == Some(1)
    {
        return crate::constant::shape::array_index_observation(domain.values, 0)
            .map(Some)
            .map_err(CheckerInfrastructureError::SemanticValueStore);
    }

    // An unconstrained fresh input denotes an arbitrary element of this cleanup family.
    // Proofs over it apply uniformly, without enumerating the array or emitting runtime state.
    let terms = std::iter::once(receiver)
        .chain(state.conditions.iter().map(|(term, _)| *term))
        .chain(
            state
                .observations
                .iter()
                .flat_map(|(key, value)| [*key, *value]),
        )
        .chain(state.evaluated_values.values().copied());

    let Some(ordinal) = crate::contract::fresh_callable_argument(domain.values, terms)
        .map_err(CheckerInfrastructureError::SemanticValueStore)?
    else {
        return Ok(None);
    };

    domain
        .values
        .intern_constant_term(ConstantTermData::CallableArgument(ordinal))
        .map(Some)
        .map_err(CheckerInfrastructureError::SemanticValueStore)
}

pub(in crate::analysis::guarantee) fn part_was_moved(
    storage: &StoragePlan,
    plan: &AsyncScopeExitPlan,
    access: StorageAccessId,
    part: &StorageCleanupPart,
) -> bool {
    plan.moved()
        .iter()
        .filter(|moved| storage.root_identity(**moved) == storage.root_identity(access))
        .filter_map(|moved| storage.access(*moved))
        .any(|moved| {
            moved.projections().len() <= part.projections().len()
                && moved.projections().iter().zip(part.projections()).all(
                    |(moved, part)| match part.projection() {
                        StorageCleanupProjectionKind::Component(component) => component == *moved,
                        StorageCleanupProjectionKind::OwnedTarget(_) => {
                            *moved == StorageProjection::OwnedTarget
                        }
                        _ => false,
                    },
                )
        })
}
