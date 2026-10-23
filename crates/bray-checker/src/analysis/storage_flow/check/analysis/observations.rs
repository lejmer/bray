use crate::analysis::storage_flow::memory::Effect;
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
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) -> StorageInvalidation {
        let paths = match invalidation {
            StorageInvalidation::All { .. } => {
                let invocation = self.dependencies.calls().contains_key(&occurrence)
                    || occurrence.expression().is_some_and(|expression| {
                        matches!(
                            self.request.view().expression(expression),
                            Some(bray_bound_tree::BoundExpression::Await(_))
                        )
                    });

                return if invocation {
                    self.opaque_observations(
                        &state.memory,
                        self.invocation_roots(state, occurrence),
                    )
                } else {
                    StorageInvalidation::unknown()
                };
            }
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
            // Unresolved writes are not calls. Their affected storage cannot be bounded
            // by invocation arguments, which may be empty for an assignment.
            None => StorageInvalidation::unknown(),
        }
    }

    pub(super) fn opaque_observations(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        roots: impl IntoIterator<Item = Target>,
    ) -> StorageInvalidation {
        let mut stable_descriptors = std::collections::BTreeSet::new();
        let mut stable_values = std::collections::BTreeSet::new();

        for path in memory.unexposed_storage(&self.input.opaque_candidates, roots) {
            if path.projections.is_empty() {
                stable_descriptors.insert(path.clone());
            }

            if self.input.opaque_values.contains(path) {
                // Reaching one payload does not expose disjoint scalar or marker fields.
                // Only the prepared immutable observations retain their values.
                stable_values.insert(path.clone());
            }
        }

        // Ordinary statics cannot be replaced while their product or exact-thread
        // owner is retained. Interior mutation cannot reach their empty markers.
        let exposed = memory.raw_exposed_roots();

        for (root, markers) in &self.input.stable_identities {
            if matches!(
                self.storage.identity(*root),
                Some(bray_bound_tree::StorageIdentity::Static(_))
            ) && !exposed.contains(root)
            {
                stable_descriptors.insert(StoragePath::root(*root));
                stable_values.extend(markers.iter().cloned());
            }
        }

        StorageInvalidation::All {
            stable_descriptors,
            stable_values,
        }
    }

    pub(super) fn preserve_borrowed_carriers(
        &self,
        state: &StorageFlowState,
        memory: &crate::analysis::storage_flow::memory::State,
        invalidation: &mut StorageInvalidation,
    ) {
        let StorageInvalidation::All {
            stable_descriptors,
            stable_values,
        } = invalidation
        else {
            return;
        };

        let exposed = memory.raw_exposed_roots();

        for borrow in &state.definitely_active_borrows {
            let capability = self
                .storage
                .borrow_capability(*borrow)
                .expect("active borrow retains its plan");

            if capability.kind() != bray_symbols::BorrowKind::Shared {
                continue;
            }

            let referent = memory.borrow_observation(*borrow);

            for path in self
                .storage
                .logical_path(capability.access())
                .into_iter()
                .chain(referent.as_ref())
            {
                // An address formed from this borrow can be used by native code to replace
                // its allocation. The temporary borrow does not bound that opaque effect.
                if exposed.contains(&path.root) {
                    continue;
                }

                let Some(markers) = self
                    .input
                    .stable_identities
                    .get(&path.root)
                    .or_else(|| self.input.borrowed_identities.get(&path.root))
                else {
                    continue;
                };

                // The logical access can be an invocation input. Its exact physical
                // referent still retains the complete owner for this shared loan.
                stable_descriptors.insert(path.clone());

                stable_values.extend(
                    markers
                        .iter()
                        .filter(|marker| path.contains(marker))
                        .cloned(),
                );
            }
        }
    }

    pub(super) fn reference_observation(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        effects: &[Effect],
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

        if !replaced
            && !effects
                .iter()
                .filter_map(Effect::mutation)
                .any(|(changed, _)| target.contains(changed))
        {
            return;
        }

        let Some(target) = target.observation_path() else {
            return;
        };

        // A preserved reference observes the target's updated components. Replacing
        // its whole snapshot would also revoke evidence about untouched siblings.
        // The target effects above already identify every changed component.
        if replaced {
            let paths = accesses.entry(access).or_default();

            if !paths.contains(&observed) {
                paths.push(observed.clone());
            }
        }

        snapshots.insert(observed, target);
    }

    pub(super) fn resolved_observations(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        effects: &[Effect],
        roots: impl IntoIterator<Item = Target>,
    ) -> StorageInvalidation {
        if effects
            .iter()
            .any(|effect| matches!(effect, Effect::Unknown | Effect::Cleanup(_)))
        {
            return self.opaque_observations(memory, roots);
        }

        if effects.contains(&Effect::Ambient) {
            // Resolve the union once. Merging an opaque observation with precise
            // observations would discard its established preservation frame.
            let inputs = effects
                .iter()
                .any(|effect| matches!(effect, Effect::Escape | Effect::Atomic));

            let escaped = effects.iter().filter_map(|effect| match effect {
                Effect::Mutation(target, bray_symbols::DependencyMutationKind::Escape) => {
                    Some(target.clone())
                }
                _ => None,
            });

            let roots = roots
                .into_iter()
                .filter(|_| inputs)
                .chain(memory.escaped_regions(escaped))
                .chain(
                    effects
                        .iter()
                        .filter_map(Effect::mutation)
                        .map(|(target, _)| target.clone()),
                );

            return self.opaque_observations(memory, roots);
        }

        let atomic = effects.iter().any(|effect| {
            matches!(
                effect,
                Effect::Atomic | Effect::Mutation(_, bray_symbols::DependencyMutationKind::Atomic)
            )
        });

        let ordinary = effects
            .iter()
            .filter_map(Effect::mutation)
            .filter(|(_, kind)| *kind != bray_symbols::DependencyMutationKind::Atomic)
            .map(|(target, _)| target);

        let mut invalidation = if atomic {
            let frame = self
                .input
                .atomic_observations
                .as_ref()
                .expect("atomic effects prepare their observation frame");

            if effects
                .iter()
                .any(|effect| matches!(effect, Effect::Atomic))
            {
                crate::storage::observation::restrict(
                    self.opaque_observations(memory, roots),
                    frame,
                )
            } else {
                let targets = self.target_observations(
                    memory,
                    effects
                        .iter()
                        .filter_map(Effect::mutation)
                        .filter(|(_, kind)| *kind == bray_symbols::DependencyMutationKind::Atomic)
                        .map(|(target, _)| target),
                );

                crate::storage::observation::restrict(targets, frame)
            }
        } else {
            self.target_observations(memory, ordinary.clone())
        };

        if atomic && ordinary.clone().next().is_some() {
            invalidation.merge(&self.target_observations(memory, ordinary));
        }

        invalidation
    }

    pub(super) fn call_observations(
        &self,
        state: &StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        effects: &[Effect],
        roots: &[Target],
    ) -> StorageInvalidation {
        let mut invalidation =
            self.resolved_observations(&state.memory, effects, roots.iter().cloned());

        if effects.is_empty() {
            return invalidation;
        }

        let Some(contracts) = self.dependencies.calls().get(&occurrence) else {
            return invalidation;
        };

        for reference in contracts.references() {
            let relation = reference.relation();

            if !relation.is_exact()
                || relation.kind() != bray_symbols::DependencyReferenceKind::Borrow
                || relation.entry() != Some(relation.exit())
                || relation.dereferences() != 0
                || !relation.target_projections().is_empty()
            {
                continue;
            }

            let entry = relation
                .entry()
                .expect("exact reference has an input origin");

            let Some(prepared) = contracts.reference_path(entry) else {
                continue;
            };

            let Some(locations) = self.invocation_locations(state, occurrence, entry) else {
                continue;
            };

            let [location] = locations.as_slice() else {
                continue;
            };

            let Some(base) = self.invocation_place(occurrence, entry.subject_root()) else {
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
        operation: crate::analysis::model::AnalysisOperationKind,
    ) -> bray_bound_tree::StorageCompletion {
        use crate::analysis::model::{AnalysisCallPhase, AnalysisOperationKind};

        let occurrence = operation.occurrence();
        let mut observations = BTreeMap::new();
        let contracts = self.dependencies.calls().get(&occurrence);

        if !matches!(
            operation,
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Completion,
                ..
            } | AnalysisOperationKind::TaskOperation { .. }
                | AnalysisOperationKind::Result {
                    phase: crate::analysis::model::AnalysisResultPhase::Completion,
                    ..
                }
        ) && !(matches!(operation, AnalysisOperationKind::Bound(_)) && contracts.is_some())
        {
            return bray_bound_tree::StorageCompletion::default();
        }

        for (contracts, reference) in contracts.into_iter().flat_map(|contracts| {
            contracts
                .references()
                .iter()
                .map(move |reference| (contracts, reference))
        }) {
            let relation = reference.relation();

            if relation.kind() != bray_symbols::DependencyReferenceKind::Borrow {
                continue;
            }

            let Some(prepared) = contracts.reference_path(relation.exit()) else {
                continue;
            };

            let Some(base) = self.invocation_place(occurrence, relation.exit().subject_root())
            else {
                continue;
            };

            let Some(locations) = self.invocation_locations(state, occurrence, relation.exit())
            else {
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

        let completion = bray_bound_tree::StorageCompletion::new(
            observations,
            self.invocation_borrow_origins(occurrence),
        );

        if self.publish {
            self.completions
                .entry(bray_bound_tree::StorageMutation::Operation(occurrence))
                .and_modify(|previous| previous.merge(&completion))
                // Publication and the current transition retain independent maps of shared paths.
                .or_insert_with(|| completion.clone());
        }

        completion
    }

    pub(super) fn target_observations<'target>(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        mutations: impl Iterator<Item = &'target Target> + Clone,
    ) -> StorageInvalidation {
        let mut observations = BTreeMap::new();
        let mut snapshots = BTreeMap::new();
        let mut referents = std::collections::BTreeSet::new();
        let mut stable_descriptors = std::collections::BTreeSet::new();

        if mutations.clone().next().is_none() {
            return StorageInvalidation::Observed {
                scope: mutations
                    .clone()
                    .map(|target| {
                        target.mutation_scope(self.storage, self.request.semantic_values())
                    })
                    .max()
                    .unwrap_or(bray_bound_tree::StorageMutationScope::Invocation),
                accesses: observations,
                snapshots,
                referents,
                stable_descriptors,
            };
        }

        // A symbolic target can have a precise physical effect without an ordinary
        // observation of its bytes. Such a target must not produce an empty invalidation.
        let mut unrepresented = mutations
            .clone()
            .filter(|target| target.storage_path().is_none())
            .collect::<Vec<_>>();

        let unknown_referents_affected = mutations
            .clone()
            .any(|target| memory.unknown_reference_may_reach(target));

        for (slot, targets, unknown) in memory.observed_references() {
            if crate::ExecutionPlace::storage_path(self.storage, &slot).is_none() {
                continue;
            }

            unrepresented
                .retain(|mutation| !targets.iter().any(|target| target.contains(mutation)));

            if unknown && unknown_referents_affected {
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

            self.stable_descriptors(
                memory,
                access,
                logical.root,
                mutations.clone(),
                &mut stable_descriptors,
            );

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
                scope: mutations
                    .clone()
                    .map(|target| {
                        target.mutation_scope(self.storage, self.request.semantic_values())
                    })
                    .max()
                    .unwrap_or(bray_bound_tree::StorageMutationScope::Invocation),
                accesses: observations,
                snapshots,
                referents,
                stable_descriptors,
            }
        } else {
            self.opaque_observations(memory, mutations.cloned())
        }
    }
    fn stable_descriptors<'target>(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        access: bray_bound_tree::StorageAccessId,
        root: bray_bound_tree::StorageIdentityId,
        mutations: impl Iterator<Item = &'target Target> + Clone,
        descriptors: &mut std::collections::BTreeSet<StoragePath>,
    ) {
        let Some(path) = self.input.reference_paths.get(&access) else {
            return;
        };

        for (step, (index, ty)) in path.borrows().iter().enumerate() {
            if !matches!(
                self.request.semantic_values().type_data(*ty).as_ref(),
                bray_symbols::TypeData::Slice(_)
            ) {
                continue;
            }

            let descriptor = bray_bound_tree::StorageReferencePath::new(
                path.projections()[..*index].into(),
                path.borrows()[..step].into(),
            );

            let Some(location) =
                crate::analysis::storage_flow::model::StorageFlowInput::reference_location(
                    memory,
                    root,
                    &descriptor,
                    None,
                )
            else {
                continue;
            };

            if mutations.clone().all(|mutation| {
                memory.target_relationship(&location, mutation) == StorageRelationship::Disjoint
            }) {
                descriptors.insert(StoragePath {
                    root,
                    projections: path.projections()[..*index].into(),
                });
            }
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
