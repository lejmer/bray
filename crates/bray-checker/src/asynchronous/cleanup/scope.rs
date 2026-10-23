use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AsyncScopeExitPlan, AsyncStorageCleanupRequirement, AsyncStorageExitDecision,
    AsyncStorageExitDisposition, AsyncStorageExitRecoveryCause, AsyncStorageRequirement,
    StorageCleanupType, StorageFlow, StoragePlan, storage_identity_transfers_at_unit_exit,
};
use bray_diagnostics::DiagnosticBag;

use super::super::parts::CleanupExpansion;
use super::shape::{CleanupShape, CleanupShapeResolver, cleanup_requirement};
use crate::storage::storage_scope_owners;
use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

pub(crate) fn cleanup_scopes<C>(
    cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> Result<
    (
        BTreeSet<bray_bound_tree::BoundBlockId>,
        BTreeSet<bray_bound_tree::BoundExpressionId>,
    ),
    CheckerQueryError<C::UpstreamError>,
>
where
    C: CheckerRequestContext + ?Sized,
{
    let owners = storage_scope_owners(request).map_err(CheckerQueryError::with_upstream)?;
    let resolver = cleanup_shapes;
    let mut scopes = BTreeSet::new();
    let mut cleanup_free_operations = BTreeSet::new();

    for (id, identity) in storage.identity_entries() {
        let (Some(scope), Some(ty)) = (owners.scope(Some(identity)), storage.storage_type(id))
        else {
            continue;
        };

        let shape = resolver.resolve(ty)?;

        if shape.cancellation || shape.lifecycle {
            scopes.insert(scope);
        } else if !shape.recovered
            && let bray_bound_tree::StorageIdentity::Temporary(expression) = identity
            && !matches!(
                request.unit().tree().expression(expression),
                Some(bray_bound_tree::BoundExpression::Assignment(_))
            )
        {
            cleanup_free_operations.insert(expression);
        }
    }

    let mut replacements = BTreeMap::new();

    for plan in storage
        .access_plans()
        .iter()
        .filter(|plan| plan.purpose() == bray_bound_tree::StorageAccessPurpose::Assignment)
    {
        let ty = storage
            .access(plan.access())
            .expect("assignment retains its checked destination")
            .reached_type();

        let shape = resolver.resolve(ty)?;

        let free = !shape.cancellation && !shape.lifecycle && !shape.recovered;

        replacements
            .entry(plan.expression())
            .and_modify(|previous| *previous &= free)
            .or_insert(free);
    }

    cleanup_free_operations.extend(
        replacements
            .into_iter()
            .filter_map(|(expression, free)| free.then_some(expression)),
    );

    Ok((scopes, cleanup_free_operations))
}

pub(crate) fn cleanup_free_storage<C>(
    cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> Result<
    (BTreeSet<bray_bound_tree::StorageIdentityId>, DiagnosticBag),
    CheckerQueryError<C::UpstreamError>,
>
where
    C: CheckerRequestContext + ?Sized,
{
    let resolver = cleanup_shapes;
    let mut identities = BTreeSet::new();

    for (identity, provenance) in storage.identity_entries() {
        if request.is_cancelled() {
            return Err(CheckerQueryError::Cancelled);
        }

        if !matches!(
            provenance,
            bray_bound_tree::StorageIdentity::Temporary(_)
                | bray_bound_tree::StorageIdentity::LocalOwned(_)
                | bray_bound_tree::StorageIdentity::Parameter(_)
                | bray_bound_tree::StorageIdentity::Receiver(_)
                | bray_bound_tree::StorageIdentity::AnonymousParameter(_)
                | bray_bound_tree::StorageIdentity::PredicateParameter(_)
        ) {
            continue;
        }

        let Some(ty) = storage.storage_type(identity) else {
            continue;
        };

        let shape = resolver.resolve(ty)?;

        if !shape.cancellation && !shape.lifecycle && !shape.recovered {
            identities.insert(identity);
        }
    }

    Ok((identities, std::mem::take(&mut resolver.diagnostics)))
}

pub(in crate::asynchronous) fn scope_exit_plans<C>(
    cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    flow: &StorageFlow,
    dependencies: &bray_bound_tree::CheckedDependencyContracts,
    selections: &bray_bound_tree::CheckedSemanticSelections,
    types: &bray_bound_tree::CheckedExpressionTypes,
) -> Result<
    (
        Vec<AsyncStorageRequirement>,
        Vec<StorageCleanupType>,
        Vec<AsyncScopeExitPlan>,
        Vec<bray_bound_tree::StorageReplacementPlan>,
        DiagnosticBag,
    ),
    CheckerQueryError<C::UpstreamError>,
>
where
    C: CheckerRequestContext + ?Sized,
{
    let requirements = cleanup_shapes.storage_requirements(storage)?;
    let replacements = super::super::replacement::replacement_plans(storage, flow, cleanup_shapes)?;

    cleanup_shapes.require_inputs(selections, types)?;

    let requirements_by_identity = requirements
        .iter()
        .map(|requirement| (requirement.identity(), requirement))
        .collect::<BTreeMap<_, _>>();

    let mut plans = Vec::new();

    let scoped_uses = selections
        .entries()
        .iter()
        .filter_map(|entry| {
            let bray_bound_tree::SemanticSelection::ScopedUse(scoped) = entry.selection() else {
                return None;
            };

            let Some(bray_bound_tree::BoundExpression::Structured(expression)) =
                request.view().expression(scoped.expression())
            else {
                panic!("selected scoped use retains its structured expression");
            };

            let [body] = expression.blocks() else {
                panic!("selected scoped use retains its sole body");
            };

            Some((*body, scoped.expression()))
        })
        .collect::<BTreeMap<_, _>>();

    for exit in flow.exits() {
        let mut cancellation = Vec::new();
        let mut lifecycle = Vec::new();
        let mut dispositions = Vec::new();
        let mut is_recovered = exit.is_recovered();

        for identity in exit.live().iter().rev().copied() {
            let Some(requirement) = requirements_by_identity.get(&identity).copied() else {
                is_recovered = true;

                dispositions.push(AsyncStorageExitDecision::new(
                    identity,
                    AsyncStorageExitDisposition::Recovered(
                        AsyncStorageExitRecoveryCause::UnavailableCleanupShape,
                    ),
                ));

                continue;
            };

            let disposition = requirement.exit_disposition(
                storage,
                exit.scope(),
                exit.initialized().contains(&identity),
                exit.fully_moved().contains(&identity),
                exit.moved().iter().cloned(),
                exit.definitely_absent().iter().cloned(),
            );

            is_recovered |= matches!(disposition, AsyncStorageExitDisposition::Recovered(_));
            dispositions.push(AsyncStorageExitDecision::new(identity, disposition));

            if let AsyncStorageExitDisposition::Cleanup { access, phases, .. } = disposition {
                if phases.includes_cancellation() {
                    cancellation.push(access);
                }

                if phases.includes_lifecycle() {
                    lifecycle.push(access);
                }
            }
        }

        match dependencies.lifecycle_order(storage, &lifecycle) {
            Ok(ordered) => lifecycle = ordered,
            Err(access) => {
                is_recovered = true;

                for decision in &mut dispositions {
                    if Some(decision.identity()) == storage.root_identity(access) {
                        *decision = AsyncStorageExitDecision::new(
                            decision.identity(),
                            AsyncStorageExitDisposition::Recovered(
                                AsyncStorageExitRecoveryCause::UnavailableCleanupOrder,
                            ),
                        );
                    }
                }
            }
        }

        let mut plan = AsyncScopeExitPlan::new(
            exit.scope(),
            exit.exit(),
            dispositions,
            cancellation,
            lifecycle,
            exit.moved().iter().cloned(),
            is_recovered,
        );

        if let Some(expression) = scoped_uses.get(&exit.scope()) {
            plan = plan.with_scoped_exit(*expression);
        }

        plans.push(plan);
    }

    if matches!(
        request.unit().root(),
        bray_bound_tree::BoundUnitRoot::CallableBody {
            execution: bray_symbols::CallableExecution::Asynchronous,
            ..
        } | bray_bound_tree::BoundUnitRoot::AnonymousCallable {
            execution: bray_symbols::CallableExecution::Asynchronous,
            ..
        }
    ) && let Some(result) = types.callable_result_type()
    {
        cleanup_shapes.requirement(result)?;
    }

    Ok((
        requirements.to_vec(),
        // Publication owns a snapshot while preparation remains available to later consumers.
        cleanup_shapes.cleanup_types.values().cloned().collect(),
        plans,
        replacements,
        std::mem::take(&mut cleanup_shapes.diagnostics),
    ))
}

fn storage_requirements<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    owners: &bray_bound_tree::StorageScopeOwners,
    cleanup_shapes: &mut CleanupShapeResolver<'_, C>,
) -> Result<Vec<AsyncStorageRequirement>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut moved_by_identity = BTreeMap::<_, BTreeSet<_>>::new();

    for plan in storage.access_plans() {
        if storage.effective_purpose(*plan) == bray_bound_tree::StorageAccessPurpose::Move
            && !storage.is_root_access(plan.access())
            && let Some(identity) = storage.root_identity(plan.access())
        {
            moved_by_identity
                .entry(identity)
                .or_default()
                .insert(plan.access());
        }
    }

    let mut requirements = Vec::with_capacity(storage.identities().len());

    for (identity, _) in storage.identity_entries() {
        let mut cleanup = match (
            storage.root_access(identity),
            storage.storage_type(identity),
        ) {
            (None, _) => AsyncStorageCleanupRequirement::Recovered(
                AsyncStorageExitRecoveryCause::UnavailableRootAccess,
            ),
            (_, None) => AsyncStorageCleanupRequirement::Recovered(
                AsyncStorageExitRecoveryCause::UnavailableCleanupShape,
            ),
            (Some(_), Some(ty)) => cleanup_shapes.requirement(ty)?,
        };

        let owner = owners.scope(storage.identity(identity));
        let transfers = storage_identity_transfers_at_unit_exit(storage, identity);

        let moved = moved_by_identity
            .get(&identity)
            .into_iter()
            .flatten()
            .filter_map(|access| storage.resolved_projections(*access))
            .collect::<Vec<_>>();

        let destructor = bray_bound_tree::storage_identity_is_destructor_receiver(
            request.unit(),
            storage,
            identity,
        );

        let parts = match (
            storage.storage_type(identity),
            storage
                .root_access(identity)
                .and_then(|access| storage.access(access)),
        ) {
            (Some(ty), Some(root)) if destructor || !moved.is_empty() => cleanup_shapes
                .represented_parts(
                    ty,
                    &moved,
                    if destructor {
                        CleanupExpansion::DestructorReceiver
                    } else {
                        CleanupExpansion::MovedPaths
                    },
                    root.source(),
                )?,
            _ => None,
        };

        if destructor {
            cleanup = match &parts {
                Some(parts) => cleanup_requirement(CleanupShape {
                    cancellation: parts
                        .iter()
                        .any(|part| part.phases().includes_cancellation()),
                    lifecycle: parts.iter().any(|part| part.phases().includes_lifecycle()),
                    ..CleanupShape::default()
                }),
                None => AsyncStorageCleanupRequirement::Recovered(
                    AsyncStorageExitRecoveryCause::UnavailablePartialCleanup,
                ),
            };
        }

        let mut requirement = AsyncStorageRequirement::new(identity, owner, transfers, cleanup);

        if let Some(parts) = parts {
            requirement = requirement.with_parts(parts);
        }

        requirements.push(requirement);
    }

    Ok(requirements)
}

impl<C: CheckerRequestContext + ?Sized> CleanupShapeResolver<'_, C> {
    pub(crate) fn storage_requirements(
        &mut self,
        storage: &StoragePlan,
    ) -> Result<std::sync::Arc<[AsyncStorageRequirement]>, CheckerQueryError<C::UpstreamError>>
    {
        if let Some(requirements) = &self.requirements {
            return Ok(std::sync::Arc::clone(requirements));
        }

        let owners =
            storage_scope_owners(self.request).map_err(CheckerQueryError::with_upstream)?;

        let requirements: std::sync::Arc<[_]> =
            storage_requirements(self.request, storage, &owners, self)?.into();

        self.requirements = Some(std::sync::Arc::clone(&requirements));

        Ok(requirements)
    }
}
