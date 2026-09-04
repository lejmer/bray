use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, AsyncScopeExitPlan, AsyncStorageCleanupRequirement,
    AsyncStorageExitDisposition, AsyncStorageRequirement, BoundBlockId, BoundUnit, CheckedAsync,
    StorageAccessId, StorageExitDecision, StorageExitPoint, StorageFlow, StorageIdentityId,
    StoragePlan, StorageScopeOwners, storage_identity_transfers_at_unit_exit,
};

use super::{LoweringPlanFailure, LoweringPlanFailureCause, LoweringPlanKind};

type ScopeExitVerification = (
    BTreeMap<(BoundBlockId, AnyBoundNodeId), usize>,
    BTreeSet<StorageIdentityId>,
);

pub(super) fn verify_scope_exits(
    unit: &BoundUnit,
    storage: &StoragePlan,
    flow: &StorageFlow,
    dependencies: &bray_bound_tree::CheckedDependencyContracts,
    analysis: &CheckedAsync,
) -> Result<ScopeExitVerification, LoweringPlanFailure> {
    let requirements = verify_storage_requirements(unit, storage, flow, analysis)?;

    let mut reachable = flow
        .reachable_exits()
        .iter()
        .copied()
        .collect::<BTreeSet<_>>();

    let mut expected = BTreeMap::new();

    for exit in flow.exits() {
        let key = (exit.scope(), exit.exit());
        let point = StorageExitPoint::new(exit.scope(), exit.exit());

        if unit.view().block(exit.scope()).is_none()
            || unit.view().node_is_recovered(exit.exit()).is_none()
        {
            return Err(LoweringPlanFailure::scope_exit(
                LoweringPlanKind::ScopeExit,
                LoweringPlanFailureCause::Unexpected,
                exit.scope(),
                exit.exit(),
            ));
        }

        if unit
            .view()
            .node_is_recovered(exit.exit())
            .is_some_and(|recovered| recovered)
        {
            return Err(LoweringPlanFailure::scope_exit(
                LoweringPlanKind::ScopeExit,
                LoweringPlanFailureCause::Recovered,
                exit.scope(),
                exit.exit(),
            ));
        }

        if exit.is_recovered() {
            return Err(LoweringPlanFailure::scope_exit(
                LoweringPlanKind::ScopeExit,
                LoweringPlanFailureCause::Recovered,
                exit.scope(),
                exit.exit(),
            ));
        }

        if !reachable.remove(&point) {
            return Err(LoweringPlanFailure::scope_exit(
                LoweringPlanKind::ScopeExit,
                LoweringPlanFailureCause::Unexpected,
                exit.scope(),
                exit.exit(),
            ));
        }

        if expected.insert(key, exit).is_some() {
            return Err(LoweringPlanFailure::scope_exit(
                LoweringPlanKind::ScopeExit,
                LoweringPlanFailureCause::Duplicate,
                exit.scope(),
                exit.exit(),
            ));
        }
    }

    if let Some(point) = reachable.first().copied() {
        return Err(LoweringPlanFailure::scope_exit(
            LoweringPlanKind::ScopeExit,
            LoweringPlanFailureCause::Missing,
            point.scope(),
            point.exit(),
        ));
    }

    let mut verified = BTreeMap::new();
    let mut lifecycle_storage = BTreeSet::new();

    for (index, plan) in analysis.scope_exits().iter().enumerate() {
        let key = (plan.scope(), plan.exit());

        if unit.view().block(plan.scope()).is_none()
            || unit.view().node_is_recovered(plan.exit()).is_none()
        {
            return Err(LoweringPlanFailure::scope_exit(
                LoweringPlanKind::ScopeExit,
                LoweringPlanFailureCause::Unexpected,
                plan.scope(),
                plan.exit(),
            ));
        }

        if verified.insert(key, index).is_some() {
            return Err(LoweringPlanFailure::scope_exit(
                LoweringPlanKind::ScopeExit,
                LoweringPlanFailureCause::Duplicate,
                plan.scope(),
                plan.exit(),
            ));
        }

        let Some(flow_exit) = expected.remove(&key) else {
            return Err(LoweringPlanFailure::scope_exit(
                LoweringPlanKind::ScopeExit,
                LoweringPlanFailureCause::Unexpected,
                plan.scope(),
                plan.exit(),
            ));
        };

        verify_scope_exit_storage(
            unit,
            storage,
            dependencies,
            flow_exit,
            plan,
            &requirements,
            &mut lifecycle_storage,
        )?;

        if plan.is_recovered() {
            return Err(LoweringPlanFailure::scope_exit(
                LoweringPlanKind::ScopeExit,
                LoweringPlanFailureCause::Recovered,
                plan.scope(),
                plan.exit(),
            ));
        }
    }

    if let Some((&(scope, exit), _)) = expected.first_key_value() {
        return Err(LoweringPlanFailure::scope_exit(
            LoweringPlanKind::ScopeExit,
            LoweringPlanFailureCause::Missing,
            scope,
            exit,
        ));
    }

    Ok((verified, lifecycle_storage))
}

fn verify_storage_requirements(
    unit: &BoundUnit,
    storage: &StoragePlan,
    flow: &StorageFlow,
    analysis: &CheckedAsync,
) -> Result<BTreeMap<StorageIdentityId, AsyncStorageRequirement>, LoweringPlanFailure> {
    let owners = StorageScopeOwners::collect(unit)
        .map_err(|_| LoweringPlanFailure::analysis(LoweringPlanFailureCause::Unexpected))?;

    let mut remaining = flow
        .exits()
        .iter()
        .flat_map(|exit| exit.live().iter().copied())
        .collect::<BTreeSet<_>>();

    let mut verified = BTreeMap::new();

    for requirement in analysis.storage_requirements().iter().copied() {
        let identity = requirement.identity();
        let expected_owner = owners.scope(storage.identity(identity));
        let expected_transfer = storage_identity_transfers_at_unit_exit(unit, storage, identity);

        if storage.identity(identity).is_none()
            || requirement
                .owner()
                .is_some_and(|owner| unit.view().block(owner).is_none())
            || !remaining.remove(&identity)
        {
            return Err(LoweringPlanFailure::storage_requirement(
                if verified.contains_key(&identity) {
                    LoweringPlanFailureCause::Duplicate
                } else {
                    LoweringPlanFailureCause::Unexpected
                },
                identity,
            ));
        }

        if requirement.owner() != expected_owner || requirement.transfers() != expected_transfer {
            return Err(requirement_failure(
                flow,
                requirement,
                LoweringPlanFailureCause::Contradictory,
            ));
        }

        if let AsyncStorageCleanupRequirement::Recovered(cause) = requirement.cleanup() {
            return Err(requirement_failure(
                flow,
                requirement,
                LoweringPlanFailureCause::StorageRecovery(cause),
            ));
        }

        verified.insert(identity, requirement);
    }

    if let Some(identity) = remaining.first().copied() {
        return Err(requirement_failure_for_identity(
            flow,
            identity,
            LoweringPlanFailureCause::Missing,
        ));
    }

    Ok(verified)
}

fn requirement_failure(
    flow: &StorageFlow,
    requirement: AsyncStorageRequirement,
    cause: LoweringPlanFailureCause,
) -> LoweringPlanFailure {
    requirement_failure_for_identity(flow, requirement.identity(), cause)
}

fn requirement_failure_for_identity(
    flow: &StorageFlow,
    identity: StorageIdentityId,
    cause: LoweringPlanFailureCause,
) -> LoweringPlanFailure {
    flow.exits()
        .iter()
        .find(|exit| exit.live().contains(&identity))
        .map_or_else(
            || LoweringPlanFailure::storage_requirement(cause, identity),
            |exit| LoweringPlanFailure::for_storage(cause, exit.scope(), exit.exit(), identity),
        )
}

fn verify_scope_exit_storage(
    unit: &BoundUnit,
    storage: &StoragePlan,
    dependencies: &bray_bound_tree::CheckedDependencyContracts,
    flow: &StorageExitDecision,
    plan: &AsyncScopeExitPlan,
    requirements: &BTreeMap<StorageIdentityId, AsyncStorageRequirement>,
    lifecycle_storage: &mut BTreeSet<StorageIdentityId>,
) -> Result<(), LoweringPlanFailure> {
    if let Some(identity) = flow
        .live()
        .iter()
        .find(|identity| storage.identity(**identity).is_none())
        .copied()
    {
        return Err(storage_failure(
            plan,
            identity,
            LoweringPlanFailureCause::Unexpected,
        ));
    }

    if let Some(identity) = flow
        .initialized()
        .iter()
        .find(|identity| !flow.live().contains(identity))
        .copied()
    {
        return Err(storage_failure(
            plan,
            identity,
            LoweringPlanFailureCause::Unexpected,
        ));
    }

    if let Some(identity) = flow
        .fully_moved()
        .iter()
        .find(|identity| !flow.live().contains(identity))
        .copied()
    {
        return Err(storage_failure(
            plan,
            identity,
            LoweringPlanFailureCause::Unexpected,
        ));
    }

    if let Some(access) = flow.moved().iter().find(|access| {
        storage
            .root_identity(**access)
            .is_none_or(|identity| !flow.live().contains(&identity))
    }) {
        return Err(LoweringPlanFailure::for_access(
            LoweringPlanKind::StorageDisposition,
            LoweringPlanFailureCause::Unexpected,
            plan.scope(),
            plan.exit(),
            *access,
        ));
    }

    let mut remaining = flow.live().iter().copied().collect::<BTreeSet<_>>();
    let mut order = Vec::new();
    let mut cancellation = Vec::new();
    let mut lifecycle = Vec::new();

    for decision in plan.storage() {
        let identity = decision.identity();

        if storage.identity(identity).is_none() || !flow.live().contains(&identity) {
            return Err(LoweringPlanFailure::for_storage(
                LoweringPlanFailureCause::Unexpected,
                plan.scope(),
                plan.exit(),
                identity,
            ));
        }

        if !remaining.remove(&identity) {
            return Err(LoweringPlanFailure::for_storage(
                LoweringPlanFailureCause::Duplicate,
                plan.scope(),
                plan.exit(),
                identity,
            ));
        }

        order.push(identity);

        let Some(requirement) = requirements.get(&identity).copied() else {
            return Err(storage_failure(
                plan,
                identity,
                LoweringPlanFailureCause::Missing,
            ));
        };

        let expected = requirement.exit_disposition(storage, flow);

        if let AsyncStorageExitDisposition::Recovered(cause) = decision.disposition() {
            return Err(storage_failure(
                plan,
                identity,
                LoweringPlanFailureCause::StorageRecovery(cause),
            ));
        }

        if let AsyncStorageExitDisposition::Recovered(cause) = expected {
            return Err(storage_failure(
                plan,
                identity,
                LoweringPlanFailureCause::StorageRecovery(cause),
            ));
        }

        if decision.disposition() != expected {
            return Err(storage_failure(
                plan,
                identity,
                LoweringPlanFailureCause::Contradictory,
            ));
        }

        if let AsyncStorageExitDisposition::Cleanup { access, phases } = decision.disposition() {
            if phases.includes_cancellation() {
                cancellation.push(access);
            }

            if phases.includes_lifecycle() {
                lifecycle.push(access);
                lifecycle_storage.insert(identity);
            }
        }
    }

    if let Some(identity) = remaining.first().copied() {
        return Err(LoweringPlanFailure::for_storage(
            LoweringPlanFailureCause::Missing,
            plan.scope(),
            plan.exit(),
            identity,
        ));
    }

    verify_storage_order(flow, plan, &order)?;

    verify_phase(
        plan,
        LoweringPlanKind::CancellationPhase,
        &cancellation,
        plan.cancellation_broadcast(),
    )?;

    let lifecycle = dependencies
        .lifecycle_order(unit, storage, &lifecycle)
        .map_err(|access| {
            LoweringPlanFailure::for_access(
                LoweringPlanKind::LifecyclePhase,
                LoweringPlanFailureCause::OutOfOrder,
                plan.scope(),
                plan.exit(),
                access,
            )
        })?;

    verify_phase(
        plan,
        LoweringPlanKind::LifecyclePhase,
        &lifecycle,
        plan.lifecycle_resolution(),
    )?;

    if let Some(access) = first_access_mismatch(flow.moved(), plan.moved()).or_else(|| {
        plan.moved()
            .iter()
            .find(|access| storage.access(**access).is_none())
            .copied()
    }) {
        return Err(LoweringPlanFailure::for_access(
            LoweringPlanKind::StorageDisposition,
            LoweringPlanFailureCause::Contradictory,
            plan.scope(),
            plan.exit(),
            access,
        ));
    }

    Ok(())
}

fn verify_storage_order(
    flow: &StorageExitDecision,
    plan: &AsyncScopeExitPlan,
    order: &[StorageIdentityId],
) -> Result<(), LoweringPlanFailure> {
    if order.iter().copied().eq(flow.live().iter().rev().copied()) {
        return Ok(());
    }

    if let Some(identity) = order
        .iter()
        .zip(flow.live().iter().rev())
        .find_map(|(actual, expected)| (actual != expected).then_some(*actual))
        .or_else(|| order.first().copied())
    {
        return Err(storage_failure(
            plan,
            identity,
            LoweringPlanFailureCause::OutOfOrder,
        ));
    }

    Err(LoweringPlanFailure::scope_exit(
        LoweringPlanKind::StorageDisposition,
        LoweringPlanFailureCause::OutOfOrder,
        plan.scope(),
        plan.exit(),
    ))
}

fn verify_phase(
    plan: &AsyncScopeExitPlan,
    kind: LoweringPlanKind,
    expected: &[StorageAccessId],
    actual: &[StorageAccessId],
) -> Result<(), LoweringPlanFailure> {
    if let Some(access) = first_access_mismatch(expected, actual) {
        return Err(LoweringPlanFailure::for_access(
            kind,
            LoweringPlanFailureCause::Contradictory,
            plan.scope(),
            plan.exit(),
            access,
        ));
    }

    Ok(())
}

fn first_access_mismatch(
    expected: &[StorageAccessId],
    actual: &[StorageAccessId],
) -> Option<StorageAccessId> {
    actual
        .iter()
        .zip(expected)
        .find_map(|(actual, expected)| (actual != expected).then_some(*actual))
        .or_else(|| actual.get(expected.len()).copied())
        .or_else(|| expected.get(actual.len()).copied())
}

fn storage_failure(
    plan: &AsyncScopeExitPlan,
    identity: StorageIdentityId,
    cause: LoweringPlanFailureCause,
) -> LoweringPlanFailure {
    LoweringPlanFailure::for_storage(cause, plan.scope(), plan.exit(), identity)
}
