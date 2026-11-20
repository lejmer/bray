use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_bound_tree::{
    AsyncStorageCleanupRequirement, AsyncStorageExitRecoveryCause,
    AsyncStorageRequirement,
    StorageAccessId, StorageCleanupType, StorageIdentityId, StoragePlan, StorageScope,
    storage_identity_transfers_at_unit_exit,
};
use bray_diagnostics::DiagnosticBag;

use super::super::parts::CleanupExpansion;
use super::shape::{CleanupShape, CleanupShapeResolver, cleanup_requirement};
use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

#[derive(Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct StorageCleanupRequirements {
    entries: Arc<[AsyncStorageRequirement]>,
    early_retirement: BTreeSet<StorageIdentityId>,
    replacements: BTreeMap<StorageAccessId, AsyncStorageCleanupRequirement>,
    scopes: BTreeMap<StorageScope, ScopeStorage>,
}

#[derive(Debug, Default, Eq, Hash, PartialEq)]
struct ScopeStorage {
    retiring: Vec<StorageIdentityId>,
    cleanup: Vec<StorageIdentityId>,
}

impl StorageCleanupRequirements {
    fn new(
        entries: Vec<AsyncStorageRequirement>,
        early_retirement: BTreeSet<StorageIdentityId>,
        replacements: BTreeMap<StorageAccessId, AsyncStorageCleanupRequirement>,
    ) -> Self {
        let mut scopes = BTreeMap::<_, ScopeStorage>::new();

        for requirement in &entries {
            let Some(scope) = requirement.owner().filter(|_| !requirement.transfers()) else {
                continue;
            };

            let scope = scopes.entry(scope).or_default();

            scope.retiring.push(requirement.identity());

            if requirement.cleanup() != AsyncStorageCleanupRequirement::None {
                scope.cleanup.push(requirement.identity());
            }
        }

        Self { entries: entries.into(), early_retirement, replacements, scopes }
    }

    pub(crate) fn entries(&self) -> &[AsyncStorageRequirement] {
        &self.entries
    }

    pub(crate) fn get(&self, identity: StorageIdentityId) -> Option<&AsyncStorageRequirement> {
        self.entries.get(usize::try_from(identity.ordinal()).ok()?)
            .filter(|requirement| requirement.identity() == identity)
    }

    pub(crate) fn ends_at_last_use(&self, identity: StorageIdentityId) -> bool {
        self.early_retirement.contains(&identity)
    }

    pub(crate) fn early_retirement_slots(&self) -> impl Iterator<Item = StorageIdentityId> + '_ {
        self.early_retirement.iter().copied()
    }

    pub(crate) fn replacement(&self, access: StorageAccessId) -> AsyncStorageCleanupRequirement {
        *self.replacements.get(&access).expect("planned assignment retains its destination cleanup")
    }

    pub(crate) fn replacement_accesses(&self) -> impl Iterator<Item = StorageAccessId> + '_ {
        self.replacements.keys().copied()
    }

    pub(crate) fn scope_candidates(&self, scope: StorageScope) -> impl DoubleEndedIterator<Item = StorageIdentityId> + '_ {
        let abandoned = match scope {
            StorageScope::AbandonedTransfer { value, .. } if self.get(value)
                .is_some_and(|owner| owner.cleanup() != AsyncStorageCleanupRequirement::None) => Some(value),
            _ => None,
        };

        self.scopes.get(&scope).into_iter().flat_map(|scope| scope.cleanup.iter().copied()).chain(abandoned)
    }

    pub(crate) fn retiring_storage(&self, scope: StorageScope) -> &[StorageIdentityId] {
        self.scopes.get(&scope).map_or(&[], |scope| scope.retiring.as_slice())
    }

    pub(crate) fn retiring_scopes(&self) -> impl Iterator<Item = (StorageScope, &[StorageIdentityId])> {
        self.scopes.iter().map(|(&scope, storage)| (scope, storage.retiring.as_slice()))
    }
}

pub(crate) fn graph_cleanup_requirements<C>(
    cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
) -> crate::CheckerQueryResult<
    (
        Arc<StorageCleanupRequirements>,
        BTreeSet<bray_bound_tree::BoundExpressionId>,
    ),
    C::UpstreamError,
>
where
    C: CheckerRequestContext + ?Sized,
{
    let requirements = cleanup_shapes.storage_requirements(storage)?;

    let mut cleanup_free_operations = BTreeSet::new();

    for requirement in requirements.entries() {
        if requirement.owner().is_some()
            && requirement.cleanup() == AsyncStorageCleanupRequirement::None
            && let Some(bray_bound_tree::StorageIdentity::Temporary(expression)) = storage.identity(requirement.identity())
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
        let free = requirements.replacement(plan.access()) == AsyncStorageCleanupRequirement::None;

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

    Ok((requirements, cleanup_free_operations))
}

type PreparedCleanup = (
    Arc<[AsyncStorageRequirement]>,
    Vec<StorageCleanupType>,
    DiagnosticBag,
);

pub(in crate::asynchronous) fn prepare_cleanup<C>(
    cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
) -> PreparedCleanup
where
    C: CheckerRequestContext + ?Sized,
{
    let requirements = cleanup_shapes.requirements.as_ref()
        .expect("async analysis consumes the prepared storage requirements");

    let mut cleanup_types = cleanup_shapes.storage_cleanup_types.iter()
        .filter(|(ty, _)| !cleanup_shapes.cleanup_types.contains_key(ty))
        .map(|(_, cleanup)| cleanup)
        .chain(cleanup_shapes.cleanup_types.values())
        .cloned()
        .collect::<Vec<_>>();

    cleanup_types.sort_unstable_by_key(StorageCleanupType::ty);

    (
        Arc::clone(&requirements.entries),
        cleanup_types,
        std::mem::take(&mut cleanup_shapes.diagnostics),
    )
}

fn storage_requirements<C>(
    request: CheckerUnitView<'_, C>,
    storage: &StoragePlan,
    owners: &bray_bound_tree::StorageScopeOwners,
    cleanup_shapes: &mut CleanupShapeResolver<'_, C>,
) -> Result<StorageCleanupRequirements, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut moved_by_identity = BTreeMap::<_, BTreeSet<_>>::new();
    let mut replacements = BTreeMap::new();

    for plan in storage.access_plans() {
        if plan.purpose() == bray_bound_tree::StorageAccessPurpose::Assignment {
            let ty = storage.access(plan.access())
                .expect("assignment retains its checked destination")
                .reached_type();

            if let std::collections::btree_map::Entry::Vacant(entry) = replacements.entry(plan.access()) {
                entry.insert(cleanup_shapes.requirement(ty)?);
            }
        }

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
    let mut early_retirement = BTreeSet::new();

    for (identity, slot) in storage.identity_entries() {
        if request.is_cancelled() {
            return Err(CheckerQueryError::Cancelled);
        }

        let root = storage.root_access(identity);
        let ty = storage.storage_type(identity);
        let owned = slot.is_invocation_owned(storage.kind());

        let mut cleanup = match (root, ty) {
            (None, _) => AsyncStorageCleanupRequirement::Recovered(
                AsyncStorageExitRecoveryCause::UnavailableRootAccess,
            ),
            (_, None) => AsyncStorageCleanupRequirement::Recovered(
                AsyncStorageExitRecoveryCause::UnavailableCleanupShape,
            ),
            (Some(_), Some(_)) if !owned => {
                AsyncStorageCleanupRequirement::None
            }
            (Some(_), Some(ty)) => cleanup_shapes.requirement(ty)?,
        };

        if matches!(
            slot,
            bray_bound_tree::StorageIdentity::Temporary(_)
                | bray_bound_tree::StorageIdentity::Evaluated(_)
                | bray_bound_tree::StorageIdentity::InvocationInput { .. }
                | bray_bound_tree::StorageIdentity::LocalOwned(_)
                | bray_bound_tree::StorageIdentity::Parameter(_)
                | bray_bound_tree::StorageIdentity::Receiver(_)
                | bray_bound_tree::StorageIdentity::AnonymousParameter(_)
                | bray_bound_tree::StorageIdentity::PredicateParameter(_)
        ) && let Some(ty) = ty
        {
            let type_cleanup = if root.is_some() && owned {
                cleanup
            } else {
                cleanup_requirement(cleanup_shapes.resolve(ty)?)
            };

            if type_cleanup == AsyncStorageCleanupRequirement::None {
                early_retirement.insert(identity);
            }
        }

        let owner = owners.scope(Some(slot));
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

        let parts = match (ty, root.and_then(|access| storage.access(access))) {
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

    Ok(StorageCleanupRequirements::new(requirements, early_retirement, replacements))
}

impl<C: CheckerRequestContext + ?Sized> CleanupShapeResolver<'_, C> {
    pub(crate) fn storage_requirements(
        &mut self,
        storage: &StoragePlan,
    ) -> Result<Arc<StorageCleanupRequirements>, CheckerQueryError<C::UpstreamError>>
    {
        if let Some(requirements) = &self.requirements {
            return Ok(Arc::clone(requirements));
        }

        let owners = bray_bound_tree::StorageScopeOwners::collect(self.request.unit())
            .expect("checked bound unit has balanced lexical scopes");

        let requirements = Arc::new(storage_requirements(self.request, storage, &owners, self)?);

        self.requirements = Some(Arc::clone(&requirements));

        Ok(requirements)
    }
}
