use std::collections::BTreeMap;

use bray_bound_tree::{
    AsyncStorageCleanupRequirement, BoundExpression, BoundExpressionId, BoundUnit, CheckedAsync,
    StorageAccessPurpose, StorageFlow, StorageOperationStatus, StoragePlan,
    StorageReplacementState,
};

use super::{LoweringPlanFailure, LoweringPlanFailureCause, LoweringPlanKind};

pub(super) fn verify_replacements(
    unit: &BoundUnit,
    storage: &StoragePlan,
    flow: &StorageFlow,
    analysis: &CheckedAsync,
) -> Result<BTreeMap<BoundExpressionId, usize>, LoweringPlanFailure> {
    let failure = |expression, cause| {
        LoweringPlanFailure::for_expression(LoweringPlanKind::Replacement, cause, expression)
    };

    let mut expected = flow
        .operations()
        .iter()
        .filter(|operation| {
            operation.purpose() == StorageAccessPurpose::Assignment
                && operation.status() == StorageOperationStatus::Valid
        })
        .map(|operation| (operation.expression(), operation.access()))
        .collect::<BTreeMap<_, _>>();

    let mut decisions = BTreeMap::new();

    for decision in flow.replacements() {
        let expression = decision.expression();

        if !matches!(
            unit.view().expression(expression),
            Some(BoundExpression::Assignment(_))
        ) || expected.remove(&expression) != Some(decision.access())
        {
            return Err(failure(expression, LoweringPlanFailureCause::Unexpected));
        }

        if decision.is_recovered() {
            return Err(failure(expression, LoweringPlanFailureCause::Recovered));
        }

        if decision.moved().iter().any(|access| {
            storage.root_identity(*access) != storage.root_identity(decision.access())
        }) {
            return Err(failure(expression, LoweringPlanFailureCause::Contradictory));
        }

        if decision.state() == StorageReplacementState::Present
            && decision.moved().iter().any(|moved| {
                storage.access_contains(decision.access(), *moved)
                    || storage.access_contains(*moved, decision.access())
            })
        {
            return Err(failure(expression, LoweringPlanFailureCause::Contradictory));
        }

        decisions.insert(expression, decision);
    }

    if let Some((&expression, _)) = expected.first_key_value() {
        return Err(failure(expression, LoweringPlanFailureCause::Missing));
    }

    let types = super::partition::cleanup_type_index(analysis)?;
    let mut verified = BTreeMap::new();

    for (index, plan) in analysis.replacements().iter().enumerate() {
        let expression = plan.expression();

        let decision = decisions
            .remove(&expression)
            .ok_or_else(|| failure(expression, LoweringPlanFailureCause::Unexpected))?;

        if plan.access() != decision.access() {
            return Err(failure(expression, LoweringPlanFailureCause::Contradictory));
        }

        let access = storage
            .access(plan.access())
            .ok_or_else(|| failure(expression, LoweringPlanFailureCause::Missing))?;

        let shape = types
            .get(&access.reached_type())
            .ok_or_else(|| failure(expression, LoweringPlanFailureCause::Missing))?;

        let expected = if decision.state() == StorageReplacementState::Absent {
            AsyncStorageCleanupRequirement::None
        } else {
            shape.cleanup()
        };

        if let AsyncStorageCleanupRequirement::Recovered(cause) = plan.cleanup() {
            return Err(failure(
                expression,
                LoweringPlanFailureCause::StorageRecovery(cause),
            ));
        }

        if plan.cleanup() != expected
            || decision.state() == StorageReplacementState::Absent && plan.parts().is_some()
        {
            return Err(failure(expression, LoweringPlanFailureCause::Contradictory));
        }

        let partial = decision.moved().iter().any(|moved| {
            storage.access_contains(plan.access(), *moved)
                && !storage.access_contains(*moved, plan.access())
        });

        if partial && decision.state() != StorageReplacementState::Absent && plan.parts().is_none()
        {
            return Err(failure(expression, LoweringPlanFailureCause::Missing));
        }

        if let Some(parts) = plan.parts() {
            super::partition::verify_partition(&types, access.reached_type(), parts, false)
                .map_err(|cause| failure(expression, cause))?;
        }

        verified.insert(expression, index);
    }

    if let Some((&expression, _)) = decisions.first_key_value() {
        return Err(failure(expression, LoweringPlanFailureCause::Missing));
    }

    Ok(verified)
}
