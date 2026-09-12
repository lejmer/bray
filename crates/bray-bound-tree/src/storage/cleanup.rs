use std::collections::{BTreeMap, BTreeSet};

use crate::{
    AsyncCleanupGuard, AsyncStorageCleanupRequirement, AsyncStorageExitDisposition,
    AsyncStorageExitRecoveryCause, BoundDependencySubject, BoundUnit, CheckedDependencyContracts,
    StorageAccessId, StorageExitDecision, StorageIdentityId, StoragePlan,
};

impl CheckedDependencyContracts {
    /// Orders lifecycle consumers before the storage they depend on.
    ///
    /// Input order breaks ties between independent values. Guarded requirements are retained
    /// conservatively until cleanup has a representation for their runtime conditions.
    /// Returns an access with no storage root or participating in a cycle when no complete order exists.
    pub fn lifecycle_order(
        &self,
        unit: &BoundUnit,
        storage: &StoragePlan,
        accesses: &[StorageAccessId],
    ) -> Result<Vec<StorageAccessId>, StorageAccessId> {
        let mut consumers = BTreeMap::<StorageIdentityId, BTreeSet<StorageIdentityId>>::new();

        for access in accesses {
            let Some(identity) = storage.root_identity(*access) else {
                return Err(*access);
            };

            for dependency in self.transitive_lifecycle_dependencies(unit, storage, identity) {
                if dependency != identity {
                    consumers.entry(dependency).or_default().insert(identity);
                }
            }
        }

        let mut remaining = accesses.to_vec();
        let mut ordered = Vec::with_capacity(remaining.len());

        while !remaining.is_empty() {
            let identities = remaining
                .iter()
                .filter_map(|access| storage.root_identity(*access))
                .collect::<BTreeSet<_>>();

            let next = remaining.iter().position(|access| {
                storage.root_identity(*access).is_some_and(|identity| {
                    consumers
                        .get(&identity)
                        .is_none_or(|required| required.is_disjoint(&identities))
                })
            });

            let Some(next) = next else {
                return Err(remaining[0]);
            };

            ordered.push(remaining.remove(next));
        }

        Ok(ordered)
    }

    fn transitive_lifecycle_dependencies(
        &self,
        unit: &BoundUnit,
        storage: &StoragePlan,
        identity: StorageIdentityId,
    ) -> BTreeSet<StorageIdentityId> {
        let mut pending = vec![identity];
        let mut visited = BTreeSet::new();

        while let Some(identity) = pending.pop() {
            if visited.insert(identity) {
                pending.extend(self.lifecycle_dependencies(unit, storage, identity));
            }
        }

        visited
    }

    fn lifecycle_dependencies(
        &self,
        unit: &BoundUnit,
        storage: &StoragePlan,
        identity: StorageIdentityId,
    ) -> BTreeSet<StorageIdentityId> {
        let mut contracts = BTreeSet::new();

        for (access, _) in storage.access_entries() {
            if storage.root_identity(access) == Some(identity) {
                contracts.extend(self.access(access));
            }
        }

        for plan in storage.access_plans() {
            if storage.root_identity(plan.access()) == Some(identity) {
                contracts.extend(self.expression(plan.expression()));

                contracts
                    .extend(self.deferred_expression_through_bindings(unit, plan.expression()));
            }
        }

        if let Some(crate::AnyBoundNodeId::Expression(expression)) = storage
            .identity(identity)
            .and_then(|identity| identity.definition_node())
        {
            contracts.extend(self.expression(expression));
            contracts.extend(self.deferred_expression_through_bindings(unit, expression));
        }

        contracts
            .into_iter()
            .filter_map(|contract| self.contract(contract))
            .flat_map(|contract| contract.required_subjects())
            .filter_map(|subject| match subject {
                BoundDependencySubject::Storage(identity) => Some(identity),
                BoundDependencySubject::StorageAccess(access) => storage.root_identity(access),
                BoundDependencySubject::BorrowCapability(borrow) => storage
                    .borrow_capability(borrow)
                    .and_then(|borrow| storage.root_identity(borrow.access())),
                BoundDependencySubject::ScopedCapability(_)
                | BoundDependencySubject::ImplementationWitness(_)
                | BoundDependencySubject::ProductStatic(_)
                | BoundDependencySubject::ExactThreadStatic(_)
                | BoundDependencySubject::LifecycleObligation(_) => None,
            })
            .collect()
    }
}

impl crate::AsyncStorageRequirement {
    /// Derives the exit disposition from independently checked ownership, shape, and flow.
    pub fn exit_disposition(
        &self,
        storage: &StoragePlan,
        exit: &StorageExitDecision,
    ) -> AsyncStorageExitDisposition {
        if self.owner() != Some(exit.scope()) {
            return AsyncStorageExitDisposition::Retained;
        }

        if self.transfers() {
            return AsyncStorageExitDisposition::Transferred;
        }

        if exit.fully_moved().contains(&self.identity()) {
            return AsyncStorageExitDisposition::Moved;
        }

        if self.parts().is_some_and(|parts| {
            parts.iter().all(|part| {
                exit.definitely_moved().iter().any(|access| {
                    storage.root_identity(*access) == Some(self.identity())
                        && storage
                            .resolved_projections(*access)
                            .is_some_and(|path| part.is_fully_moved_by(path))
                })
            })
        }) {
            return AsyncStorageExitDisposition::NoCleanup;
        }

        let is_partial = exit.moved().iter().any(|access| {
            storage.root_identity(*access) == Some(self.identity())
                && !storage.is_root_access(*access)
        });

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
                        guard: if self.parts().is_none()
                            && exit.initialized().contains(&self.identity())
                        {
                            AsyncCleanupGuard::Always
                        } else {
                            AsyncCleanupGuard::Initialized
                        },
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
