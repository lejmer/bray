use super::model::ExecutionFlowDomain;
use std::collections::{BTreeMap, BTreeSet};

use crate::CheckerRequestContext;
use crate::execution_guarantees::ExecutionCondition;

use super::super::state::ExecutionState;
use bray_bound_tree::StorageInvalidation;

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    fn storage_places<'state>(
        &'state self,
        state: &'state ExecutionState,
        path: bray_bound_tree::StoragePath,
    ) -> impl Iterator<Item = crate::ExecutionPlace> + 'state {
        let declaration = match self.storage.identity(path.root) {
            Some(bray_bound_tree::StorageIdentity::Static(declaration)) => Some(declaration),
            _ => None,
        };

        let local = declaration
            .is_none()
            .then(|| crate::ExecutionPlace::storage_path(self.storage, &path))
            .flatten();

        local
            .into_iter()
            .chain(declaration.into_iter().flat_map(move |declaration| {
                // Each selected instance keeps its own immutable projection path and snapshot.
                let projections = path.projections.clone();

                state
                    .current
                    .keys()
                    .filter(move |place| {
                        place.projections.is_empty() && place.selected_static() == Some(declaration)
                    })
                    .filter_map(move |place| place.clone().project(&projections))
            }))
    }

    pub(in crate::analysis) fn call_preserves_normal_storage(
        &self,
        state: &ExecutionState,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) -> bool {
        if matches!(occurrence, bray_bound_tree::BoundExecutionSite::Node(_))
            && occurrence.expression().is_some_and(|expression| {
                self.memory.operation(expression).is_some()
                    || matches!(
                        self.request.view().expression(expression),
                        Some(bray_bound_tree::BoundExpression::Assignment(_))
                    )
            })
        {
            return false;
        }

        if let Some(source) = occurrence
            .expression()
            .and_then(|expression| self.awaited_invocation(state, expression))
        {
            return self.dependencies.calls().get(&source)
                .is_some_and(bray_bound_tree::BoundCallDependencyContracts::deferred_preserves_storage_on_normal_return);
        }

        self.dependencies.calls().get(&occurrence).is_some_and(
            bray_bound_tree::BoundCallDependencyContracts::preserves_storage_on_normal_return,
        )
    }

    pub(super) fn invalidate(
        &self,
        state: &mut ExecutionState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        invalidation: Option<&StorageInvalidation>,
    ) {
        if self.call_preserves_normal_storage(state, occurrence) {
            return;
        }

        self.invalidate_call_effects(state, occurrence, invalidation);
    }

    pub(in crate::analysis) fn invalidate_call_effects(
        &self,
        state: &mut ExecutionState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        invalidation: Option<&StorageInvalidation>,
    ) {
        // Resolving a destination does not turn an ownership transfer into a write.
        let changes_contents = self.dependencies.calls().contains_key(&occurrence);

        let Some(invalidation) = invalidation.or_else(|| self.refinements.get(&occurrence)) else {
            return;
        };

        // A proven zero extent has no writes, epoch changes, or ownership effects.
        if self.empty_memory_copy(state, occurrence) {
            return;
        }

        if matches!(invalidation, StorageInvalidation::All { .. })
            && let Some(contract) = self
                .request
                .trusted_contracts()
                .and_then(|contracts| contracts.calls.get(&occurrence))
                && contract.preserves_inputs
                && state.entries.get(&occurrence).is_some_and(|entry| {
                    entry.proves(
                        contract
                            .preconditions
                            .iter()
                            .map(|(condition, _)| condition),
                    ) && (entry.trusted_boundary
                        || self.trusted_requirements_proven(entry, &contract.requirements))
                })
            {
                return;
            }

        let transfers = !changes_contents
            && !self.storage.occurrence_plans(occurrence).any(|plan| {
                matches!(
                    plan.purpose(),
                    bray_bound_tree::StorageAccessPurpose::Assignment
                )
            });

        let content_only = matches!(occurrence, bray_bound_tree::BoundExecutionSite::Node(_))
            .then(|| {
                occurrence
                    .expression()
                    .and_then(|expression| self.memory.operation(expression))
            })
            .flatten()
            .is_some_and(|operation| {
                crate::memory::execution::storage_effect(operation.kind())
                    == crate::memory::execution::StorageEffect::Contents
            });

        self.invalidate_observations(
            state,
            bray_bound_tree::StorageMutation::Operation(occurrence),
            invalidation,
            transfers,
            content_only,
        );
    }

    fn invalidate_all_observations(
        &self,
        state: &mut ExecutionState,
        mutation: bray_bound_tree::StorageMutation,
        descriptors: &BTreeSet<bray_bound_tree::StoragePath>,
        stable_values: &BTreeSet<bray_bound_tree::StoragePath>,
    ) {
        let descriptors = descriptors
            .iter()
            .flat_map(|path| self.storage_places(state, path.clone()))
            .filter_map(|place| {
                let value = place.value_in(&state.current)?;

                Some((place, value.owner_identity().clone()))
            })
            .collect::<BTreeMap<_, _>>();

        let mut retained = stable_values
            .iter()
            .flat_map(|path| self.storage_places(state, path.clone()))
            .filter_map(|place| {
                let value = place.value_in(&state.current)?;

                Some((place, value))
            })
            .collect::<BTreeMap<_, _>>();

        let owners: BTreeSet<_> = stable_values
            .iter()
            .flat_map(|path| {
                self.storage_places(state, bray_bound_tree::StoragePath::root(path.root))
            })
            .filter_map(|place| place.value_in(&state.current))
            .flat_map(|value| {
                value
                    .owned_values()
                    .map(|value| value.owner_identity().clone())
                    .collect::<Vec<_>>()
            })
            .collect();

        state.invalidate_cleanup(&retained.values().cloned().collect(), &owners);

        // A local observation starts at its checked binding, not at a preceding
        // opaque call. Keep existing local roots and all potentially external roots.
        let roots = self
            .observed_types
            .keys()
            .filter(|place| {
                !matches!(
                    place.root,
                    crate::execution_guarantees::ExecutionInput::Reference(
                        bray_bound_tree::BoundReferenceTarget::Local(
                            bray_symbols::AnyLocalSymbolId::Binding(_)
                        )
                    ) | crate::execution_guarantees::ExecutionInput::Storage(_)
                )
            })
            .chain(state.current.keys())
            .filter(|place| place.projections.is_empty())
            .cloned()
            .collect::<BTreeSet<_>>();

        state.current.clear();

        for place in roots {
            if let Some(value) = retained.remove(&place) {
                // A whole stable root needs no intermediate post-state or reconstruction.
                state.current.insert(place, value);
                continue;
            }

            let contents = ExecutionCondition::PostState(mutation, place.clone());

            let value = descriptors.get(&place).map_or_else(
                || contents.clone(),
                |identity| {
                    ExecutionCondition::Constructed(
                        std::sync::Arc::new(identity.clone()),
                        [(
                            bray_bound_tree::StorageProjection::OwnedTarget,
                            contents.clone(),
                        )]
                        .into(),
                    )
                },
            );

            state.assign(place, value);
        }

        for (place, value) in retained {
            state.assign(place, value);
        }
    }

    pub(in crate::analysis) fn invalidate_observations(
        &self,
        state: &mut ExecutionState,
        mutation: bray_bound_tree::StorageMutation,
        invalidation: &StorageInvalidation,
        transfers: bool,
        content_only: bool,
    ) {
        if !matches!(mutation, bray_bound_tree::StorageMutation::Operation(_)) {
            state.retire_evaluation(mutation);
        }

        if let StorageInvalidation::All {
            stable_descriptors: descriptors,
            stable_values,
        } = invalidation
        {
            self.invalidate_all_observations(state, mutation, descriptors, stable_values);
            return;
        }

        if !transfers && let StorageInvalidation::Observed { referents, .. } = invalidation {
            let observed = referents
                .iter()
                .filter_map(|path| {
                    crate::ExecutionPlace::storage_path(self.storage, path)?
                        .value_in(&state.current)
                })
                .collect::<Vec<_>>();

            let unstable = BTreeSet::new();

            state.invalidate_referents(
                &observed,
                if content_only {
                    &self.spatial_predicates
                } else {
                    &unstable
                },
            );
        }

        let mut changed_places = BTreeSet::new();

        match invalidation {
            StorageInvalidation::Observed { accesses, .. } => {
                for path in accesses.values().flatten() {
                    if let Some(place) =
                        crate::ExecutionPlace::storage_observation(self.storage, path)
                    {
                        // An unresolved selector invalidates its observable enclosing value.
                        changed_places.insert(place);
                    }
                }
            }
            StorageInvalidation::Paths(paths) => {
                for (target, binding) in self.storage.bindings() {
                    let access = match binding {
                        bray_bound_tree::StorageBinding::Access(access) => *access,
                        bray_bound_tree::StorageBinding::Identity(identity) => {
                            let Some(access) = self.storage.root_access(*identity) else {
                                continue;
                            };

                            access
                        }
                    };

                    if !invalidation.invalidates(access, self.storage) {
                        continue;
                    }

                    let Some(place) = crate::ExecutionPlace::binding(self.storage, *target) else {
                        continue;
                    };

                    for invalidated in paths {
                        if self.storage.relationship_to_path(access, invalidated)
                            == bray_bound_tree::StorageRelationship::Disjoint
                        {
                            continue;
                        }

                        // Each affected path owns its projection of this binding.
                        let changed = place
                            .clone()
                            .project(&invalidated.projections)
                            .unwrap_or_else(|| place.clone());

                        changed_places.insert(changed);
                    }
                }
            }
            StorageInvalidation::All { .. } => unreachable!("global invalidation handled above"),
        }

        // Storage aliasing conservatively groups specializations by declaration. Revoke
        // each selected instance affected by that storage path, without merging identities.
        let mut changed_static = |path: &bray_bound_tree::StoragePath| {
            let Some(bray_bound_tree::StorageIdentity::Static(declaration)) =
                self.storage.identity(path.root)
            else {
                return;
            };

            for place in state.current.keys().filter(|place| {
                place.projections.is_empty() && place.selected_static() == Some(declaration)
            }) {
                changed_places.insert(
                    place
                        .clone()
                        .project(&path.projections)
                        .unwrap_or_else(|| place.clone()),
                );
            }
        };

        match invalidation {
            StorageInvalidation::Observed {
                accesses,
                referents,
                ..
            } => {
                accesses
                    .values()
                    .flatten()
                    .chain(referents)
                    .for_each(&mut changed_static);
            }
            StorageInvalidation::Paths(paths) => paths.iter().for_each(&mut changed_static),
            StorageInvalidation::All { .. } => unreachable!("global invalidation handled above"),
        }

        // Storage flow distinguishes the descriptor slot from its reached elements.
        // Keep only descriptor identity. A fresh payload snapshot prevents opaque calls
        // and predicates from equating the old contents with the new contents.
        let descriptors = match invalidation {
            StorageInvalidation::Observed {
                stable_descriptors, ..
            } => stable_descriptors
                .iter()
                .filter_map(|path| {
                    let place = crate::ExecutionPlace::storage_path(self.storage, path)?;
                    let value = place.value_in(&state.current)?;

                    Some((place, value.owner_identity().clone()))
                })
                .collect::<BTreeMap<_, _>>(),
            _ => BTreeMap::new(),
        };

        let invalidated_values = if transfers {
            Vec::new()
        } else {
            changed_places
                .iter()
                .filter(|place| !descriptors.contains_key(*place))
                .flat_map(|place| {
                    [
                        place
                            .value_in(&state.current)
                            .unwrap_or_else(|| ExecutionCondition::Input(place.clone())),
                        ExecutionCondition::Input(place.clone()),
                    ]
                })
                .collect()
        };

        if content_only {
            state.invalidate_contents(
                &changed_places,
                invalidated_values,
                &self.spatial_predicates,
            );
        } else {
            state.invalidate_contents(&changed_places, invalidated_values, &BTreeSet::new());
        }

        // Preserve observations of disjoint components before retiring the containing snapshot.
        let retained = self
            .observed_types
            .keys()
            .filter(|place| !place.overlaps_any(&changed_places))
            .filter_map(|place| {
                place
                    .value_in(&state.current)
                    .map(|value| (place.clone(), value))
            })
            .collect::<Vec<_>>();

        state.current.extend(retained);

        for place in changed_places {
            let value = descriptors
                .get(&place)
                .map(|identity| {
                    ExecutionCondition::Constructed(
                        std::sync::Arc::new(identity.clone()),
                        [(
                            bray_bound_tree::StorageProjection::OwnedTarget,
                            ExecutionCondition::PostState(mutation, place.clone()),
                        )]
                        .into(),
                    )
                })
                .unwrap_or_else(|| {
                    if transfers {
                        ExecutionCondition::Unknown
                    } else {
                        // A possible write retires the old contents, not this live place's
                        // identity. Distinct post-state terms assert no equality between aliases.
                        ExecutionCondition::PostState(mutation, place.clone())
                    }
                });

            state.assign(place, value);
        }

        if !transfers && let StorageInvalidation::Observed { snapshots, .. } = invalidation {
            self.observe_references(state, snapshots);
        }
    }
    pub(in crate::analysis) fn observe_references(
        &self,
        state: &mut ExecutionState,
        observations: &BTreeMap<bray_bound_tree::StoragePath, bray_bound_tree::StoragePath>,
    ) {
        // Read all targets before assigning aliases, so map order cannot change the result.
        let aliases = observations
            .iter()
            .filter(|(path, target)| path != target)
            .filter_map(|(path, target)| {
                let place = crate::ExecutionPlace::storage_path(self.storage, path)?;
                let target = crate::ExecutionPlace::storage_path(self.storage, target)?;

                let value = target
                    .value_in(&state.current)
                    .unwrap_or(ExecutionCondition::Input(target));

                let value = if matches!(
                    place.value_in(&state.current),
                    Some(ExecutionCondition::Borrowed(_))
                ) {
                    // Refresh the referent without turning a borrowed field into an owned field.
                    value.borrowed()
                } else {
                    value
                };

                Some((place, value))
            })
            .collect::<Vec<_>>();

        for (place, value) in aliases {
            state.assign(place, value);
        }
    }
}
