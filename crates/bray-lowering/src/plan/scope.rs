use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, AsyncScopeExitPlan, AsyncStorageExitDisposition, BoundBlockId, BoundUnit,
    CheckedAsync, StorageAccessId, StorageExitDecision, StorageFlow, StorageIdentityId,
    StoragePlan,
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
    analysis: &CheckedAsync,
) -> Result<ScopeExitVerification, LoweringPlanFailure> {
    let mut expected = BTreeMap::new();

    for exit in flow.exits() {
        let key = (exit.scope(), exit.exit());

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

        if expected.insert(key, exit).is_some() {
            return Err(LoweringPlanFailure::scope_exit(
                LoweringPlanKind::ScopeExit,
                LoweringPlanFailureCause::Duplicate,
                exit.scope(),
                exit.exit(),
            ));
        }
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

        verify_scope_exit_storage(storage, flow_exit, plan, &mut lifecycle_storage)?;

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

fn verify_scope_exit_storage(
    storage: &StoragePlan,
    flow: &StorageExitDecision,
    plan: &AsyncScopeExitPlan,
    lifecycle_storage: &mut BTreeSet<StorageIdentityId>,
) -> Result<(), LoweringPlanFailure> {
    if let Some(identity) = flow
        .initialized()
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
        .fully_moved()
        .iter()
        .find(|identity| !flow.initialized().contains(identity))
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
            .is_none_or(|identity| !flow.initialized().contains(&identity))
    }) {
        return Err(LoweringPlanFailure::for_access(
            LoweringPlanKind::StorageDisposition,
            LoweringPlanFailureCause::Unexpected,
            plan.scope(),
            plan.exit(),
            *access,
        ));
    }

    let mut remaining = flow.initialized().iter().copied().collect::<BTreeSet<_>>();
    let mut order = Vec::new();
    let mut cancellation = Vec::new();
    let mut lifecycle = Vec::new();

    for decision in plan.storage() {
        let identity = decision.identity();

        if storage.identity(identity).is_none() || !flow.initialized().contains(&identity) {
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
        verify_storage_disposition(storage, flow, plan, decision.disposition(), identity)?;

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

fn verify_storage_disposition(
    storage: &StoragePlan,
    flow: &StorageExitDecision,
    plan: &AsyncScopeExitPlan,
    disposition: AsyncStorageExitDisposition,
    identity: StorageIdentityId,
) -> Result<(), LoweringPlanFailure> {
    if matches!(disposition, AsyncStorageExitDisposition::Recovered(_)) {
        return Err(storage_failure(
            plan,
            identity,
            LoweringPlanFailureCause::Recovered,
        ));
    }

    let root = storage.root_access(identity);

    let is_fully_moved = flow.fully_moved().contains(&identity);

    let is_consistent = match disposition {
        AsyncStorageExitDisposition::Cleanup { access, .. } => {
            root == Some(access) && !is_fully_moved
        }
        AsyncStorageExitDisposition::Moved => is_fully_moved,
        AsyncStorageExitDisposition::NoCleanup => !is_fully_moved,
        AsyncStorageExitDisposition::Retained | AsyncStorageExitDisposition::Transferred => true,
        AsyncStorageExitDisposition::Recovered(_) => false,
    };

    if !is_consistent {
        return Err(storage_failure(
            plan,
            identity,
            LoweringPlanFailureCause::Contradictory,
        ));
    }

    Ok(())
}

fn verify_storage_order(
    flow: &StorageExitDecision,
    plan: &AsyncScopeExitPlan,
    order: &[StorageIdentityId],
) -> Result<(), LoweringPlanFailure> {
    if order
        .iter()
        .copied()
        .eq(flow.initialized().iter().rev().copied())
    {
        return Ok(());
    }

    if let Some(identity) = order
        .iter()
        .zip(flow.initialized().iter().rev())
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
