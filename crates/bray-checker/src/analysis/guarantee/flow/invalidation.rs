use super::model::ExecutionFlowDomain;
use std::collections::{BTreeMap, BTreeSet};

use crate::CheckerRequestContext;
use crate::execution_guarantees::ExecutionCondition;

use super::super::state::ExecutionState;
use bray_bound_tree::StorageInvalidation;

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    fn disjoint_initialized_values(
        &self,
        state: &ExecutionState,
        mutation: bray_bound_tree::StorageMutation,
    ) -> BTreeSet<ExecutionCondition> {
        use bray_bound_tree::{CheckedMemoryOperationKind as Operation, MemoryReadKind};

        let bray_bound_tree::StorageMutation::Operation(occurrence) = mutation else {
            return BTreeSet::new();
        };

        let Some(operation) = occurrence
            .expression()
            .and_then(|expression| self.memory.operation(expression))
        else {
            return BTreeSet::new();
        };

        let pointee = match operation.kind() {
            Operation::Read {
                pointee,
                kind: MemoryReadKind::Move,
            }
            | Operation::Write { pointee } => *pointee,
            _ => return BTreeSet::new(),
        };

        let Some(pointer) = state.entries.get(&occurrence).and_then(|entry| {
            crate::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0))
                .value_in(&entry.arguments)
        }) else {
            return BTreeSet::new();
        };

        let predicate = |name| {
            let key = bray_compiler_known::CompilerKnownDeclarationKey::try_new(name)
                .expect("closed memory predicate key must be valid");

            self.request
                .context()
                .available_compiler_known_symbols()
                .provider()
                .declaration_symbol::<bray_symbols::PredicateSymbolId>(&key)
                .map(Into::into)
        };

        let non_overlapping = predicate("NonOverlapping");
        let initialized = predicate("InitializedAs");
        let equalities = ExecutionCondition::equalities(&state.assumptions, None);

        let normalize = |condition: &ExecutionCondition| condition.with_equalities(&equalities);

        let pointer = normalize(&pointer);

        if pointer.observation_path().is_none() {
            return BTreeSet::new();
        }

        let element_matches = |substitution| {
            let substitution = self
                .request
                .semantic_values()
                .generic_substitution_data(substitution);

            matches!(substitution.bindings(), [binding] if binding.argument() == bray_symbols::GenericArgument::Type(pointee))
        };

        let count = |value: &ExecutionCondition| match value {
            ExecutionCondition::Literal(value) => match value.kind() {
                bray_symbols::ConstantValueKind::Integer(value) => value.to_u64(),
                _ => None,
            },
            _ => None,
        };

        // These facts bound exactly one typed slot. Do not preserve opaque predicates:
        // they can observe reachable storage outside a disjoint header's own bytes.
        let mut disjoint = BTreeSet::new();
        let mut preserved = BTreeSet::new();

        for (original, holds) in &state.trusted_assumptions {
            if !holds {
                continue;
            }

            let ExecutionCondition::Trusted(condition) = normalize(original) else {
                continue;
            };

            let ExecutionCondition::Predicate(identity, substitution, arguments) =
                condition.as_ref()
            else {
                continue;
            };

            if Some(*identity) != non_overlapping || !element_matches(*substitution) {
                continue;
            }

            let [left, left_count, right, right_count] = arguments.as_ref() else {
                continue;
            };

            // Contents transfers do not change this captured spatial relation. Its
            // original carrier and owner dependencies still expire normally.
            preserved.insert(original.clone());

            if left == right
                || left.observation_path().is_none()
                || right.observation_path().is_none()
            {
                continue;
            }

            if !count(left_count).is_some_and(|count| count >= 1)
                || !count(right_count).is_some_and(|count| count >= 1)
            {
                continue;
            }

            if left == &pointer {
                disjoint.insert(right.clone());
            }

            if right == &pointer {
                disjoint.insert(left.clone());
            }
        }

        preserved.extend(
            state
                .trusted_assumptions
                .iter()
                .filter_map(|(original, holds)| {
                    if !holds {
                        return None;
                    }

                    let ExecutionCondition::Trusted(condition) = normalize(original) else {
                        return None;
                    };

                    let ExecutionCondition::Predicate(identity, substitution, arguments) =
                        condition.as_ref()
                    else {
                        return None;
                    };

                    if !element_matches(*substitution) {
                        return None;
                    }

                    let pointer = match arguments.as_ref() {
                        [pointer] if Some(*identity) == initialized => pointer,
                        [pointer, extent]
                            if Some(*identity) == self.initialized_range
                                && count(extent) == Some(1) =>
                        {
                            pointer
                        }
                        _ => return None,
                    };

                    disjoint.contains(pointer).then(|| original.clone())
                }),
        );

        preserved
    }

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

    fn storage_path_values<'state>(
        &'state self,
        state: &'state ExecutionState,
        path: bray_bound_tree::StoragePath,
        mutation: bray_bound_tree::StorageMutation,
    ) -> impl Iterator<Item = ExecutionCondition> + 'state {
        let mut places = self.storage_places(state, path.clone()).peekable();

        let temporary = places
            .peek()
            .is_none()
            .then(|| self.storage_path_value(state, &path));

        places
            .filter_map(|place| place.value_in(&state.current))
            .chain(temporary)
            .chain(self.entry_storage_value(state, &path, mutation))
    }

    fn entry_storage_value(
        &self,
        state: &ExecutionState,
        path: &bray_bound_tree::StoragePath,
        mutation: bray_bound_tree::StorageMutation,
    ) -> Option<ExecutionCondition> {
        let place = crate::ExecutionPlace::storage_path(self.storage, path)?;

        if place.value_in(&state.current).is_some_and(|value| value != ExecutionCondition::Unknown) {
            return None;
        }

        let bray_bound_tree::StorageMutation::Operation(invocation) = mutation else {
            return None;
        };

        let entry = state.entries.get(&invocation)?;
        let selection = self.semantics.selections().expression(invocation.expression()?)?;

        entry.owned_inputs.iter().find_map(|ordinal| {
            let root = selection.invocation_input_root(invocation, *ordinal)?;
            let input = selection.invocation_input_site(invocation, root)?;
            let source = self.input_place(input)?;

            if !source.contains(&place) {
                return None;
            }

            // Argument transfer has retired the old path. Its complete value now
            // belongs to this entry, whose result contract will transfer or retire it.
            let mut value = crate::ExecutionPlace::argument(*ordinal).value_in(&entry.arguments)?;

            if !selection.evaluates_defaults() {
                let input = selection.input(*ordinal)?;

                if input.conversion().is_some_and(|conversion| {
                    matches!(conversion.target(), bray_bound_tree::ConversionTarget::NullablePresent)
                }) {
                    let ty = self.semantics.types().expression(input.expression()?)?.ty();

                    for projection in crate::storage::nullable_wrapping(
                        self.request.semantic_values(), ty, input.ty(self.request.semantic_values()),
                    ).expect("a checked nullable input retains its wrapping") {
                        value = value.project(projection);
                    }
                }
            }

            for projection in &path.projections[source.projections.len()..] {
                value = value.project(*projection);
            }

            Some(value)
        })
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

        if let Some(expression) = occurrence.expression() {
            if self
                .awaited_invocations(state, expression)
                .any(|(source, _)| self.is_task_observation(source))
                && let Some(bray_bound_tree::BoundExpression::Await(awaited)) =
                    self.request.view().expression(expression)
            {
                return self
                    .deferred_preserves_storage(state, self.value(state, awaited.operand()));
            }

            if self.awaited_unknown(state, expression) {
                return false;
            }

            let mut deferred = self.awaited_invocations(state, expression).peekable();

            if deferred.peek().is_some() {
                return deferred.all(|(source, _)| {
                    self.dependencies.calls()[&source].deferred_preserves_storage_on_normal_return()
                });
            }
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

    fn preserved_storage_values(
        &self,
        state: &ExecutionState,
        paths: &BTreeSet<bray_bound_tree::StoragePath>,
        mutation: bray_bound_tree::StorageMutation,
    ) -> (
        BTreeMap<crate::ExecutionPlace, (ExecutionCondition, bray_bound_tree::StoragePath)>,
        crate::execution_guarantees::ObservationSet,
    ) {
        let values = paths
            .iter()
            .flat_map(|path| {
                self.storage_places(state, path.clone())
                    .into_iter()
                    .map(move |place| (place, path.clone()))
            })
            .filter_map(|(place, path)| {
                place
                    .value_in(&state.current)
                    .map(|value| (place, (value, path)))
            })
            .collect();

        let snapshots = paths
            .iter()
            .flat_map(|path| self.storage_path_values(state, path.clone(), mutation))
            .flat_map(|value| state.owned_values(&value).collect::<Vec<_>>())
            .filter(|value| *value != ExecutionCondition::Unknown)
            .flat_map(|value| {
                // An unchanged storage value also retains observations through
                // its existing loans. The loan does not establish ownership.
                let owner = value.owner_identity().clone();

                [owner.clone(), owner.borrowed(), value.clone(), value.borrowed()]
            })
            .collect::<Vec<_>>();

        let mut observations = crate::execution_guarantees::ObservationSet::default();

        for value in snapshots { observations.insert(value); }

        (values, observations)
    }

    fn preserve_normal_snapshots(
        &self,
        state: &ExecutionState,
        mutation: bray_bound_tree::StorageMutation,
        snapshots: &BTreeMap<(bray_symbols::DependencySubjectRoot, std::sync::Arc<[bray_bound_tree::StorageProjection]>), bray_bound_tree::StorageObservationKind>,
        observations: &mut crate::execution_guarantees::ObservationSet,
    ) {
        let bray_bound_tree::StorageMutation::Operation(site) = mutation else { return; };

        let Some(expression) = site.expression() else { return; };

        let selection = self.semantics.selections().expression(expression);
        let entry = state.entries.get(&site);

        for ((root, projections), kind) in snapshots {
            let value = if *root == bray_symbols::DependencySubjectRoot::Result {
                state.pending_results.get(&expression).or_else(|| state.expressions.get(&expression)).cloned()
            } else {
                selection.and_then(|selection| selection.invocation_input_ordinal(site, *root))
                    .and_then(|ordinal| crate::ExecutionPlace::argument(ordinal).value_in(&entry?.arguments))
            };

            let Some(mut value) = value else { continue; };

            // The snapshot owns inline contents extracted from the old loan. It
            // does not preserve the source slot or the loan's mutable referents.
            value = value.into_referent();

            for projection in projections.iter().copied() {
                value = value.project(projection);
            }

            observations.insert_classified(value.clone(), *kind);
            observations.insert_classified(value.borrowed(), *kind);
        }
    }

    fn preserve_contents_owners<'place>(
        &self,
        state: &ExecutionState,
        retained: &crate::execution_guarantees::ObservationSet,
        places: impl IntoIterator<Item = &'place crate::ExecutionPlace>,
        descriptors: &mut BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    ) {
        for place in places {
            if descriptors.contains_key(place) {
                continue;
            }

            if let Some(value) = place.value_in(&state.current)
                && retained.overlaps(&value) {
                // Retained observations name this same value through every alias.
                // A surviving component belongs to this same complete owner.
                // The separate post-state still replaces its contents observation.
                descriptors.insert(place.clone(), value.owner_identity().clone());
            }
        }
    }

    fn invalidate_all_observations(
        &self,
        state: &mut ExecutionState,
        mutation: bray_bound_tree::StorageMutation,
        descriptors: &BTreeSet<bray_bound_tree::StoragePath>,
        stable_values: &BTreeSet<bray_bound_tree::StoragePath>,
        normal_snapshots: &BTreeMap<(bray_symbols::DependencySubjectRoot, std::sync::Arc<[bray_bound_tree::StorageProjection]>), bray_bound_tree::StorageObservationKind>,
    ) {
        let mut descriptors = descriptors
            .iter()
            .flat_map(|path| self.storage_places(state, path.clone()))
            .filter_map(|place| {
                let value = place.value_in(&state.current)?;

                Some((place, value.owner_identity().clone()))
            })
            .collect::<BTreeMap<_, _>>();

        let (mut retained, mut snapshots) = self.preserved_storage_values(state, stable_values, mutation);

        self.preserve_normal_snapshots(state, mutation, normal_snapshots, &mut snapshots);

        let stable_roots = stable_values
            .iter()
            .map(|path| path.root)
            .collect::<BTreeSet<_>>();

        let mut owners: BTreeSet<_> = stable_roots
            .into_iter()
            .flat_map(|root| {
                self.storage_path_values(state, bray_bound_tree::StoragePath::root(root), mutation)
            })
            .flat_map(|value| state.owned_values(&value).collect::<Vec<_>>())
            .filter(|value| *value != ExecutionCondition::Unknown)
            .map(|value| value.owner_identity().clone())
            .collect();

        if let bray_bound_tree::StorageMutation::Operation(site) = mutation
            && let Some(entry) = state.entries.get(&site)
            && let Some(contracts) = self.dependencies.calls().get(&site) {
            owners.extend(self.returned_owned_values(site, &entry.arguments, contracts.owned_result().guaranteed_transfers())
                .iter().flat_map(|(_, value)| state.owned_values(value))
                .filter(|value| !matches!(value.owner_identity(), ExecutionCondition::Borrowed(_) | ExecutionCondition::Unknown))
                .map(|value| value.owner_identity().clone()));
        }

        state.invalidate_cleanup(&snapshots, &owners);

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

        self.preserve_contents_owners(state, &snapshots, &roots, &mut descriptors);

        state.current.clear();

        for place in roots {
            if let Some((value, _)) = retained.remove(&place) {
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

            state.assign(place, value, None);
        }

        for (place, (value, path)) in retained {
            state.assign(place, value, Some(&path.projections));
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
            normal_snapshots,
        } = invalidation
        {
            self.invalidate_all_observations(state, mutation, descriptors, stable_values, normal_snapshots);

            return;
        }

        let (mut stable_places, mut stable_values) = match invalidation {
            StorageInvalidation::Observed { stable_values, .. } => {
                self.preserved_storage_values(state, stable_values, mutation)
            }
            _ => (BTreeMap::new(), crate::execution_guarantees::ObservationSet::default()),
        };

        if let Some(snapshots) = invalidation.normal_snapshots() {
            self.preserve_normal_snapshots(state, mutation, snapshots, &mut stable_values);
        }

        if !transfers && let StorageInvalidation::Observed { referents, .. } = invalidation {
            let mut observed = Vec::new();
            let mut address_slots = BTreeSet::new();

            for path in referents {
                let Some(value) = crate::ExecutionPlace::storage_path(self.storage, path)
                    .and_then(|place| place.value_in(&state.current))
                else {
                    continue;
                };

                let ty = path
                    .access_id(self.storage)
                    .and_then(|access| self.storage.access(access))
                    .map(|access| access.reached_type())
                    .or_else(|| {
                        path.projections
                            .is_empty()
                            .then(|| self.storage.storage_type(path.root))
                            .flatten()
                    });

                if ty.is_some_and(|ty| {
                    crate::representation::reference_kind(self.request, ty)
                        == Some(bray_symbols::DependencyReferenceKind::Address)
                }) {
                    address_slots.insert(value.owner_identity().clone());
                }

                observed.push(value);
            }

            let unstable = BTreeSet::new();
            let preserved = self.disjoint_initialized_values(state, mutation);

            state.invalidate_referents(
                &observed,
                &address_slots,
                if content_only {
                    &self.content_predicates
                } else {
                    &unstable
                },
                &preserved,
                &stable_values,
            );
        }

        let mut changed_places = BTreeSet::new();
        // Execution places omit union discriminants. Keep the checked physical
        // path while rebuilding affected parents, including unselected variants.
        let mut changed_paths = BTreeMap::new();

        match invalidation {
            StorageInvalidation::Observed { accesses, .. } => {
                for path in accesses.values().flatten() {
                    if let Some(place) =
                        crate::ExecutionPlace::storage_observation(self.storage, path)
                    {
                        // An unresolved selector invalidates its observable enclosing value.
                        changed_paths.insert(place.clone(), std::sync::Arc::clone(&path.projections));
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

                        changed_paths.insert(changed.clone(), std::sync::Arc::clone(&invalidated.projections));
                        changed_places.insert(changed);
                    }
                }
            }
            StorageInvalidation::All { .. } => unreachable!("global invalidation handled above"),
        }

        self.extend_static_changes(state, invalidation, &mut changed_places, &mut changed_paths);

        // Disjoint values already retain their complete snapshot. Reconstructing
        // their fields can erase a joined variant or revive old loop inputs.
        stable_places.retain(|place, _| place.overlaps_any(&changed_places));

        // Storage flow distinguishes the descriptor slot from its reached elements.
        // Keep only descriptor identity. A fresh payload snapshot prevents opaque calls
        // and predicates from equating the old contents with the new contents.
        let mut descriptors = match invalidation {
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

        let mut scalar_places = BTreeSet::new();

        let invalidated_values = if transfers {
            Vec::new()
        } else {
            changed_places
                .iter()
                .filter(|place| !descriptors.contains_key(*place))
                .filter_map(|place| {
                    if self.observed_types.get(place).copied().flatten()
                        .and_then(|ty| crate::representation::type_representation(self.request, ty))
                        .is_some_and(crate::representation::is_scalar)
                    {
                        // A write retires this slot's borrowed observation, not
                        // the immutable value also held by another scalar copy.
                        scalar_places.insert(place.clone());

                        return None;
                    }

                    Some(place.value_in(&state.current)
                        .unwrap_or_else(|| ExecutionCondition::Input(place.clone())))
                })
                .collect()
        };

        if !transfers {
            self.preserve_contents_owners(state, &stable_values, &changed_places, &mut descriptors);
        }

        let no_predicates = BTreeSet::new();

        state.invalidate_contents(
            &scalar_places,
            invalidated_values,
            if content_only { &self.content_predicates } else { &no_predicates },
            &stable_values,
        );

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

            let path = changed_paths.get(&place).map(|path| path.as_ref());

            state.assign(place, value, path);
        }

        // Restore unchanged components before refreshing aliases to the new snapshot.
        for (place, (value, path)) in stable_places {
            state.assign(place, value, Some(&path.projections));
        }

        if !transfers && let StorageInvalidation::Observed { snapshots, .. } = invalidation {
            self.observe_references(state, mutation, snapshots);
        }
    }
    fn extend_static_changes(
        &self,
        state: &ExecutionState,
        invalidation: &StorageInvalidation,
        changed_places: &mut BTreeSet<crate::ExecutionPlace>,
        changed_paths: &mut BTreeMap<crate::ExecutionPlace, std::sync::Arc<[bray_bound_tree::StorageProjection]>>,
    ) {
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
                let place = place.clone().project(&path.projections).unwrap_or_else(|| place.clone());

                changed_paths.insert(place.clone(), std::sync::Arc::clone(&path.projections));
                changed_places.insert(place);
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
    }

    pub(in crate::analysis) fn observe_references(
        &self,
        state: &mut ExecutionState,
        mutation: bray_bound_tree::StorageMutation,
        observations: &BTreeMap<bray_bound_tree::StoragePath, bray_bound_tree::StorageTarget>,
    ) {
        // Read all targets before assigning aliases, so map order cannot change the result.
        let aliases = observations
            .iter()
            .filter_map(|(path, target)| {
                let target = crate::storage::storage_observation_path(
                    self.storage,
                    self.request.semantic_values(),
                    target,
                )?;

                if *path == target {
                    return None;
                }

                let place = crate::ExecutionPlace::storage_path(self.storage, path);
                let previous = self.storage_path_value(state, path);
                let value = self.storage_path_value(state, &target);

                // The completed call owns a fresh symbolic result even when its
                // protected backing has no ordinary value projection. Existing
                // aliases still refresh to unknown after their contents change.
                if value == ExecutionCondition::Unknown
                    && matches!((self.storage.identity(path.root), mutation), (
                        Some(bray_bound_tree::StorageIdentity::Temporary(expression)),
                        bray_bound_tree::StorageMutation::Operation(bray_bound_tree::BoundExecutionSite::Node(bray_bound_tree::AnyBoundNodeId::Expression(completed)))
                    ) if expression == completed)
                {
                    return None;
                }

                let value = if matches!(previous.owner_identity(), ExecutionCondition::Borrowed(_)) || path.projections.is_empty()
                    && self.storage.storage_type(path.root).is_some_and(|ty| {
                        matches!(
                            self.request.semantic_values().type_data(ty).as_ref(),
                            bray_symbols::TypeData::Borrow { .. }
                        )
                    }) {
                    // The target supplies current contents, not a replacement loan.
                    // An actual slot replacement has already retired its prior context.
                    previous.with_referent(value)
                } else {
                    value
                };

                Some((path, place, value))
            })
            .collect::<Vec<_>>();

        for (path, place, value) in aliases {
            if let Some(place) = place {
                state.assign(place, value, Some(&path.projections));
            } else if let Some(bray_bound_tree::StorageIdentity::Temporary(expression)) =
                self.storage.identity(path.root)
            {
                // A getter result has expression lifetime even when no binding names it.
                let value = if path.projections.is_empty() {
                    value
                } else {
                    let previous = state
                        .expressions
                        .get(&expression)
                        .cloned()
                        .unwrap_or(ExecutionCondition::Expression(expression));

                    let Some(projections) = path
                        .projections
                        .iter()
                        .copied()
                        .map(crate::ExecutionPlace::projection_kind)
                        .collect::<Option<Vec<_>>>()
                    else {
                        continue;
                    };

                    previous.with_component(&projections, value, Some(&path.projections))
                };

                state.expressions.insert(expression, value);
            }
        }
    }
}
