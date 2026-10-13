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
            StorageInvalidation::Observed { .. } => {
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
            Some(targets) => self.target_observations(&state.memory, targets.iter()),
            None => StorageInvalidation::All,
        }
    }

    pub(super) fn reference_observation(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        effects: &[(Target, bray_symbols::DependencyMutationKind)],
        invalidation: &mut StorageInvalidation,
        access: bray_bound_tree::StorageAccessId,
        observed: StoragePath,
        location: &Target,
        replaced: bool,
    ) {
        let StorageInvalidation::Observed {
            accesses,
            snapshots,
            ..
        } = invalidation
        else {
            return;
        };

        let Some(targets) = memory.value_targets(location) else {
            return;
        };

        let [target] = targets.as_slice() else {
            return;
        };

        if !replaced && !effects.iter().any(|(changed, _)| target.contains(changed)) {
            return;
        }

        let Some(target) = target.observation_path() else {
            return;
        };

        let paths = accesses.entry(access).or_default();

        if !paths.contains(&observed) {
            paths.push(observed.clone());
        }

        snapshots.insert(observed, target);
    }

    pub(super) fn call_observations(
        &self,
        state: &StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        effects: &[(Target, bray_symbols::DependencyMutationKind)],
    ) -> StorageInvalidation {
        let mut invalidation =
            self.target_observations(&state.memory, effects.iter().map(|(target, _)| target));

        if effects.is_empty() {
            return invalidation;
        }

        let Some(contracts) = self.dependencies.calls().get(&occurrence) else {
            return invalidation;
        };

        let Some(bray_bound_tree::SemanticSelection::Call(call)) = occurrence
            .expression()
            .and_then(|expression| self.execution.semantics.selections().expression(expression))
        else {
            return invalidation;
        };

        for reference in contracts.references() {
            let relation = reference.relation();

            if relation.kind() != bray_symbols::DependencyReferenceKind::Borrow
                || relation.entry() != relation.exit()
                || !relation.target_projections().is_empty()
            {
                continue;
            }

            let Some(argument) =
                crate::dependency::result_argument(call, relation.entry().subject_root())
            else {
                continue;
            };

            let Some(prepared) = contracts.reference_path(relation.entry()) else {
                continue;
            };

            let Some(locations) = self.call_reference_locations(state, argument, prepared) else {
                continue;
            };

            let [location] = locations.as_slice() else {
                continue;
            };

            let Some(base) = self.input.expression_places(argument).first() else {
                continue;
            };

            let Some(access) = base.access_id(self.storage) else {
                continue;
            };

            let observed = StoragePath {
                root: base.root,
                projections: base
                    .projections
                    .iter()
                    .chain(prepared.projections())
                    .copied()
                    .collect(),
            };

            self.reference_observation(
                &state.memory,
                effects,
                &mut invalidation,
                access,
                observed,
                location,
                false,
            );
        }

        invalidation
    }

    pub(super) fn completed_references(
        &mut self,
        state: &StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) -> BTreeMap<StoragePath, StoragePath> {
        let mut observations = BTreeMap::new();

        let Some(contracts) = self.dependencies.calls().get(&occurrence) else {
            return observations;
        };

        let Some(bray_bound_tree::SemanticSelection::Call(call)) = occurrence
            .expression()
            .and_then(|expression| self.execution.semantics.selections().expression(expression))
        else {
            return observations;
        };

        for reference in contracts.references() {
            let relation = reference.relation();

            if relation.kind() != bray_symbols::DependencyReferenceKind::Borrow {
                continue;
            }

            let Some(argument) =
                crate::dependency::result_argument(call, relation.exit().subject_root())
            else {
                continue;
            };

            let Some(prepared) = contracts.reference_path(relation.exit()) else {
                continue;
            };

            let Some(base) = self.input.expression_places(argument).first() else {
                continue;
            };

            let Some(locations) = self.call_reference_locations(state, argument, prepared) else {
                continue;
            };

            let [location] = locations.as_slice() else {
                continue;
            };

            let Some(targets) = state.memory.value_targets(location) else {
                continue;
            };

            let [target] = targets.as_slice() else {
                continue;
            };

            let Some(target) = target.observation_path() else {
                continue;
            };

            observations.insert(
                StoragePath {
                    root: base.root,
                    projections: base
                        .projections
                        .iter()
                        .chain(prepared.projections())
                        .copied()
                        .collect(),
                },
                target,
            );
        }

        if self.publish {
            self.completion_observations
                .entry(bray_bound_tree::StorageMutation::Operation(occurrence))
                .and_modify(|previous| {
                    previous.retain(|path, target| observations.get(path) == Some(target))
                })
                // Publication and the current transition retain independent maps of shared paths.
                .or_insert_with(|| observations.clone());
        }

        observations
    }

    pub(super) fn target_observations<'target>(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        mutations: impl Iterator<Item = &'target Target> + Clone,
    ) -> StorageInvalidation {
        let mut observations = BTreeMap::new();
        let mut snapshots = BTreeMap::new();
        let mut referents = std::collections::BTreeSet::new();

        if mutations.clone().next().is_none() {
            return StorageInvalidation::Observed {
                accesses: observations,
                snapshots,
                referents,
            };
        }

        // A symbolic target can have a precise physical effect without an ordinary
        // observation of its bytes. Such a target must not produce an empty invalidation.
        let mut unrepresented = mutations
            .clone()
            .filter(|target| target.storage_path().is_none())
            .collect::<Vec<_>>();

        for (slot, targets, unknown) in memory.observed_references() {
            if crate::ExecutionPlace::storage_path(self.storage, &slot).is_none() {
                continue;
            }

            unrepresented
                .retain(|mutation| !targets.iter().any(|target| target.contains(mutation)));

            if unknown {
                referents.insert(slot.clone());
            }

            for target in targets {
                for mutation in mutations.clone().filter(|mutation| {
                    memory.target_relationship(target, mutation) != StorageRelationship::Disjoint
                }) {
                    let suffix = if target.contains(mutation) {
                        &mutation.projections()[target.projections().len()..]
                    } else {
                        &[]
                    };

                    referents.insert(observable_path(
                        slot.root,
                        slot.projections.iter().chain(suffix).copied(),
                    ));
                }
            }
        }

        for (access, _) in self.storage.access_entries() {
            let Some(logical) = self.storage.logical_path(access) else {
                continue;
            };

            let observed = self.input.reference_target(memory, self.storage, access);

            if let Some(observed) = &observed {
                unrepresented.retain(|mutation| !observed.contains(mutation));
            }

            let mut changed = Vec::new();

            for mutation in mutations.clone() {
                if observed.as_ref().is_some_and(|observed| {
                    memory.target_relationship(observed, mutation) == StorageRelationship::Disjoint
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

                let path = observable_path(
                    logical.root,
                    logical.projections.iter().chain(suffix).copied(),
                );

                if observed
                    .as_ref()
                    .is_some_and(|observed| observed.contains(mutation))
                    && let Some(target) = mutation.observation_path()
                {
                    let target = observable_path(target.root, target.projections.iter().copied());

                    snapshots.insert(path.clone(), target);
                }

                changed.push(path);
            }

            if !changed.is_empty() {
                changed.sort_unstable();
                changed.dedup();
                observations.insert(access, changed);
            }
        }

        if unrepresented.is_empty() {
            StorageInvalidation::Observed {
                accesses: observations,
                snapshots,
                referents,
            }
        } else {
            StorageInvalidation::All
        }
    }
}

fn observable_path(
    root: bray_bound_tree::StorageIdentityId,
    projections: impl Iterator<Item = bray_bound_tree::StorageProjection>,
) -> StoragePath {
    // Protected bytes have no source field. Their Uninit owner is the narrowest observation.
    StoragePath {
        root,
        projections: projections
            .take_while(|projection| {
                *projection != bray_bound_tree::StorageProjection::UninitPayload
            })
            .collect(),
    }
}
