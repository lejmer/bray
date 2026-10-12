use std::collections::BTreeMap;

use bray_bound_tree::{StorageInvalidation, StoragePath, StorageRelationship};

use super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::Target;
use crate::analysis::storage_flow::model::StorageFlowState;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn planned_observations(
        &self,
        state: &StorageFlowState,
        invalidation: &StorageInvalidation,
    ) -> StorageInvalidation {
        let paths = match invalidation {
            StorageInvalidation::All => return StorageInvalidation::All,
            StorageInvalidation::Paths(paths) => paths,
            StorageInvalidation::Observed(_) => {
                panic!("structural preparation cannot publish flow observations")
            }
        };

        let targets = paths
            .iter()
            .map(|path| {
                match path.access_id(self.storage) {
                    Some(access) => {
                        self.input
                            .reference_target(&state.memory, self.storage, access)
                    }
                    None if path.projections.is_empty()
                        && self.storage.identity(path.root).is_some_and(|identity| {
                            identity.is_invocation_owned(self.storage.kind())
                        }) =>
                    {
                        // Synthesized result slots can be consumed without a source access.
                        Some(Target::storage(path.clone()))
                    }
                    None => None,
                }
            })
            .collect::<Option<Vec<_>>>();

        match targets {
            Some(targets) => self.target_observations(state, targets.iter()),
            None => StorageInvalidation::All,
        }
    }

    pub(super) fn target_observations<'target>(
        &self,
        state: &StorageFlowState,
        mutations: impl Iterator<Item = &'target Target> + Clone,
    ) -> StorageInvalidation {
        let mut observations = BTreeMap::new();

        if mutations.clone().next().is_none() {
            return StorageInvalidation::Observed(observations);
        }

        // A symbolic target can have a precise physical effect without an ordinary
        // observation of its bytes. Such a target must not produce an empty invalidation.
        let mut unrepresented = mutations
            .clone()
            .filter(|target| target.storage_path().is_none())
            .collect::<Vec<_>>();

        for (access, _) in self.storage.access_entries() {
            let Some(logical) = self.storage.logical_path(access) else {
                continue;
            };

            let observed = self
                .input
                .reference_target(&state.memory, self.storage, access);

            if let Some(observed) = &observed {
                unrepresented.retain(|mutation| !observed.contains(mutation));
            }

            let mut changed = Vec::new();

            for mutation in mutations.clone() {
                if observed.as_ref().is_some_and(|observed| {
                    state.memory.target_relationship(observed, mutation)
                        == StorageRelationship::Disjoint
                }) {
                    continue;
                }

                // A known enclosing target preserves the unaffected components. Other
                // possible aliases lose the entire observation, never acquire equality.
                let suffix = observed
                    .as_ref()
                    .filter(|observed| observed.contains(mutation))
                    .map(|observed| &mutation.projections()[observed.projections().len()..])
                    .unwrap_or_default();

                changed.push(StoragePath {
                    root: logical.root,
                    projections: logical.projections.iter().chain(suffix).copied().collect(),
                });
            }

            if !changed.is_empty() {
                changed.sort_unstable();
                changed.dedup();
                observations.insert(access, changed);
            }
        }

        if unrepresented.is_empty() {
            StorageInvalidation::Observed(observations)
        } else {
            StorageInvalidation::All
        }
    }
}
