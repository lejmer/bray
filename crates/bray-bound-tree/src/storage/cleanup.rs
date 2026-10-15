use std::collections::{BTreeMap, BTreeSet};

use crate::{
    AsyncCleanupGuard, AsyncStorageCleanupRequirement, AsyncStorageExitDisposition,
    AsyncStorageExitRecoveryCause, BoundDependencySubject, BoundUnit, CheckedDependencyContracts,
    StorageAccessId, StorageIdentityId, StoragePlan,
};

impl CheckedDependencyContracts {
    /// Orders lifecycle consumers before the storage they depend on.
    ///
    /// Input order breaks ties between independent values. Guarded requirements are retained
    /// conservatively until cleanup has a representation for their runtime conditions.
    /// Returns an access with no storage root or participating in a cycle when no complete order exists.
    pub fn lifecycle_order(
        &self,
        storage: &StoragePlan,
        accesses: &[StorageAccessId],
    ) -> Result<Vec<StorageAccessId>, StorageAccessId> {
        for access in accesses {
            if storage.root_identity(*access).is_none() {
                return Err(*access);
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
                    self.lifecycle_consumers
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

    pub(crate) fn prepare_lifecycle_dependencies(
        &self,
        unit: &BoundUnit,
        storage: &StoragePlan,
    ) -> BTreeMap<StorageIdentityId, BTreeSet<StorageIdentityId>> {
        let mut contracts = BTreeMap::<StorageIdentityId, BTreeSet<_>>::new();

        for (access, _) in storage.access_entries() {
            if let Some(identity) = storage.root_identity(access) {
                contracts
                    .entry(identity)
                    .or_default()
                    .extend(self.access(access));
            }
        }

        for plan in storage.access_plans() {
            if let Some(identity) = storage.root_identity(plan.access()) {
                let entries = contracts.entry(identity).or_default();

                entries.extend(self.expression(plan.expression()));
                entries.extend(self.deferred_expression_through_bindings(unit, plan.expression()));
            }
        }

        for (identity, definition) in storage.identity_entries() {
            if let Some(crate::AnyBoundNodeId::Expression(expression)) =
                definition.definition_node()
            {
                let entries = contracts.entry(identity).or_default();

                entries.extend(self.expression(expression));
                entries.extend(self.deferred_expression_through_bindings(unit, expression));
            }
        }

        let dependencies = contracts
            .into_iter()
            .map(|(identity, contracts)| {
                let required = contracts
                    .into_iter()
                    .filter_map(|contract| self.contract(contract))
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

        consumers
    }
}

impl crate::AsyncStorageRequirement {
    /// Derives the exit disposition from independently checked ownership, shape, and flow.
    pub fn exit_disposition(
        &self,
        storage: &StoragePlan,
        scope: crate::BoundBlockId,
        initialized: bool,
        fully_moved: bool,
        moved: impl Iterator<Item = crate::StoragePath>,
        definitely_absent: impl Iterator<Item = crate::StoragePath> + Clone,
    ) -> AsyncStorageExitDisposition {
        if self.owner() != Some(scope) {
            return AsyncStorageExitDisposition::Retained;
        }

        if self.transfers() {
            return AsyncStorageExitDisposition::Transferred;
        }

        if fully_moved {
            return AsyncStorageExitDisposition::Moved;
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

        let is_partial = moved
            .into_iter()
            .any(|access| access.root == self.identity() && !access.projections.is_empty());

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
                        guard: if self.parts().is_none() && initialized {
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
