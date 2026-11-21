use std::collections::{BTreeMap, BTreeSet};

use crate::{
    AsyncStorageCleanupRequirement, AsyncStorageExitDisposition,
    AsyncStorageExitRecoveryCause, BoundDependencyContract, BoundDependencySubject, BoundExecutionSite,
    CheckedDependencyContracts,
    StorageAccessId, StorageCleanupSchedule, StorageIdentityId, StoragePlan,
};

impl CheckedDependencyContracts {
    /// Prepares runtime order selection for reached paths with conflicting orders.
    pub fn conditional_cleanup_schedule(
        &self,
        storage: &StoragePlan,
        mut accesses: Vec<StorageAccessId>,
    ) -> StorageCleanupSchedule {
        let root = |access| storage.root_identity(access).expect("selected cleanup access retains its owner");

        accesses.sort_unstable_by_key(|access| std::cmp::Reverse(root(*access)));

        let consumers = accesses.iter().map(|access| {
            self.lifecycle_consumers.get(&root(*access)).into_iter()
                .flat_map(|owners| accesses.iter().copied().filter(|candidate| owners.contains(&root(*candidate))))
                .collect::<std::sync::Arc<[_]>>()
        }).collect();

        StorageCleanupSchedule::Conditional { priority: accesses.into(), consumers }
    }

    /// Returns the established transitive storage dependencies of pending lifecycle consumers.
    /// Guarded dependencies remain retained until their consuming cleanup has completed.
    pub fn retained_lifecycle_storage<'a>(
        &'a self,
        consumers: &'a BTreeSet<StorageIdentityId>,
    ) -> impl Iterator<Item = StorageIdentityId> + 'a {
        self.lifecycle_consumers
            .iter()
            .filter_map(move |(dependency, owners)| {
                (!owners.is_disjoint(consumers)).then_some(*dependency)
            })
    }

    /// Orders lifecycle consumers before the storage they depend on.
    ///
    /// Input order breaks ties between independent values. Guarded requirements are retained
    /// conservatively until cleanup has a representation for their runtime conditions.
    /// Returns an access with no storage root or participating in a cycle when no complete order exists.
    pub fn lifecycle_order(
        &self,
        storage: &StoragePlan,
        accesses: Vec<StorageAccessId>,
    ) -> Result<Vec<StorageAccessId>, StorageAccessId> {
        if accesses.len() < 2 || self.lifecycle_consumers.is_empty() {
            if let Some(&access) = accesses.iter().find(|&&access| storage.root_identity(access).is_none()) {
                return Err(access);
            }

            return Ok(accesses);
        }

        let mut remaining = Vec::with_capacity(accesses.len());
        let mut pending = BTreeMap::<_, usize>::new();

        for access in accesses {
            let identity = storage.root_identity(access).ok_or(access)?;

            remaining.push((access, identity));
            *pending.entry(identity).or_default() += 1;
        }

        let mut ordered = Vec::with_capacity(remaining.len());

        while !remaining.is_empty() {
            let next = remaining.iter().position(|(_, identity)| {
                self.lifecycle_consumers.get(identity)
                    .is_none_or(|required| !required.iter().any(|consumer| pending.contains_key(consumer)))
            });

            let Some(next) = next else {
                return Err(remaining[0].0);
            };

            let (access, identity) = remaining.remove(next);

            let remaining_accesses = pending.get_mut(&identity).expect("selected access has a pending root");

            *remaining_accesses -= 1;
            if *remaining_accesses == 0 { pending.remove(&identity); }

            ordered.push(access);
        }

        Ok(ordered)
    }

    /// Records structural dependencies of all values that can reach a planned owner.
    pub fn with_lifecycle_dependencies_from_storage(
        mut self,
        storage: &StoragePlan,
        bindings: impl IntoIterator<Item = (BoundExecutionSite, Vec<StorageIdentityId>)>,
    ) -> Self {
        let mut contracts = BTreeMap::<StorageIdentityId, BTreeSet<&BoundDependencyContract>>::new();

        let bound = |id: Option<crate::BoundDependencyContractId>| id.and_then(|id| self.contract(id));

        for (access, _) in storage.access_entries() {
            if let Some(identity) = storage.root_identity(access) {
                contracts
                    .entry(identity)
                    .or_default()
                    .extend(bound(self.access(access)));
            }
        }

        for plan in storage.access_plans() {
            if let Some(identity) = storage.root_identity(plan.access()) {
                let entries = contracts.entry(identity).or_default();

                entries.extend(bound(self.expression(plan.expression())));
                entries.extend(bound(self.deferred_expression(plan.occurrence())));
            }
        }

        let sources = bindings.into_iter().flat_map(|(site, owners)| owners.into_iter().map(move |owner| (site, owner)))
            .chain(storage.identity_entries().filter_map(|(owner, identity)| Some((identity.definition_occurrence()?, owner))));

        for (site, owner) in sources {
            let entries = contracts.entry(owner).or_default();

            entries.extend(bound(self.expression(site)));
            entries.extend(bound(self.deferred_expression(site)));
            entries.extend(self.calls().get(&site).map(|call| call.result()));
        }

        let dependencies = contracts
            .into_iter()
            .map(|(identity, contracts)| {
                let required = contracts
                    .into_iter()
                    .flat_map(|contract| contract.required_subjects())
                    .filter_map(|subject| match subject {
                        BoundDependencySubject::Storage(identity) => Some(identity),
                        BoundDependencySubject::StorageAccess(access) => {
                            storage.root_identity(access)
                        }
                        BoundDependencySubject::BorrowCapability(borrow) => storage
                            .borrow_capability(borrow)
                            .and_then(|borrow| storage.root_identity(borrow.access())),
                        _ => None,
                    })
                    .collect::<BTreeSet<_>>();

                (identity, required)
            })
            .collect::<BTreeMap<_, _>>();

        let mut consumers = BTreeMap::<StorageIdentityId, BTreeSet<StorageIdentityId>>::new();

        for identity in dependencies.keys().copied() {
            let mut pending = vec![identity];
            let mut visited = BTreeSet::new();

            while let Some(dependency) = pending.pop() {
                if visited.insert(dependency) {
                    if dependency != identity {
                        consumers.entry(dependency).or_default().insert(identity);
                    }

                    if let Some(required) = dependencies.get(&dependency) {
                        pending.extend(required);
                    }
                }
            }
        }

        self.lifecycle_consumers = std::sync::Arc::new(consumers);

        self
    }
}

impl crate::AsyncStorageRequirement {
    /// Derives the exit disposition from independently checked ownership, shape, and flow.
    pub fn exit_disposition<'path>(
        &self,
        storage: &StoragePlan,
        scope: crate::StorageScope,
        fully_moved: bool,
        mut moved: impl Iterator<Item = &'path crate::StoragePath>,
        definitely_absent: impl Iterator<Item = &'path crate::StoragePath> + Clone,
    ) -> AsyncStorageExitDisposition {
        let abandons_transfer = matches!(scope, crate::StorageScope::AbandonedTransfer { value, .. }
            if value == self.identity());

        if fully_moved || !abandons_transfer && (self.owner() != Some(scope) || self.transfers()) {
            return AsyncStorageExitDisposition::NoCleanup;
        }

        if self.parts().is_some_and(|parts| {
            parts.iter().all(|part| {
                definitely_absent.clone().any(|access| {
                    access.root == self.identity() && part.is_fully_moved_by(&access.projections)
                })
            })
        }) {
            return AsyncStorageExitDisposition::NoCleanup;
        }

        let is_partial = moved.any(|access| access.root == self.identity() && !access.projections.is_empty());

        if is_partial && self.parts().is_none() {
            return match self.cleanup() {
                AsyncStorageCleanupRequirement::Recovered(cause) => {
                    AsyncStorageExitDisposition::Recovered(cause)
                }
                AsyncStorageCleanupRequirement::None => AsyncStorageExitDisposition::NoCleanup,
                AsyncStorageCleanupRequirement::Cleanup(_) => {
                    AsyncStorageExitDisposition::Recovered(
                        AsyncStorageExitRecoveryCause::UnavailablePartialCleanup,
                    )
                }
            };
        }

        match self.cleanup() {
            AsyncStorageCleanupRequirement::None => AsyncStorageExitDisposition::NoCleanup,
            AsyncStorageCleanupRequirement::Cleanup(phases) => {
                match storage.root_access(self.identity()) {
                    Some(access) => AsyncStorageExitDisposition::Cleanup {
                        access,
                        phases,
                    },
                    None => AsyncStorageExitDisposition::Recovered(
                        AsyncStorageExitRecoveryCause::UnavailableRootAccess,
                    ),
                }
            }
            AsyncStorageCleanupRequirement::Recovered(cause) => {
                AsyncStorageExitDisposition::Recovered(cause)
            }
        }
    }
}
