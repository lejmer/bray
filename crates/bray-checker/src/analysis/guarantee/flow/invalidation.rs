use super::model::ExecutionFlowDomain;
use super::pointee::record_pointee_changes;
use std::collections::{BTreeMap, BTreeSet};

use crate::CheckerRequestContext;
use crate::execution_guarantees::ExecutionCondition;

use super::super::state::ExecutionState;
use bray_bound_tree::{StorageInvalidation, StorageObservationKind, StoragePath, StorageProjection};

struct StoragePreservation<'path> {
    place: Option<crate::ExecutionPlace>,
    path: &'path StoragePath,
    kind: StorageObservationKind,
}

impl StoragePreservation<'_> {
    fn mask(&self) -> Option<(&crate::ExecutionPlace, &[StorageProjection], StorageObservationKind)> {
        Some((self.place.as_ref()?, &self.path.projections, self.kind))
    }
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    fn disjoint_initialized_values(
        &self,
        state: &ExecutionState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        pointee: bray_symbols::TypeId,
    ) -> BTreeSet<ExecutionCondition> {
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
        let equalities = state.proofs.equalities(&state.assumptions, None);

        let normalize = |condition: &ExecutionCondition| condition.with_equalities(&equalities)
            .pointer_observations(&self.pointer_predicates);

        let pointer = normalize(&pointer).descriptor_identity().clone();

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

        let count = |value: &ExecutionCondition| match value.kind() {
            crate::ExecutionConditionKind::Literal(value) => match value.kind() {
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

            let normalized = normalize(original);

            let crate::ExecutionConditionKind::Trusted(condition) = normalized.kind() else {
                continue;
            };

            let crate::ExecutionConditionKind::Predicate(identity, substitution, arguments) =
                (condition.as_ref()).kind()
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

                    let normalized = normalize(original);

                    let crate::ExecutionConditionKind::Trusted(condition) = normalized.kind() else {
                        return None;
                    };

                    let crate::ExecutionConditionKind::Predicate(identity, substitution, arguments) =
                        (condition.as_ref()).kind()
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

    pub(super) fn invalidate(
        &self,
        state: &mut ExecutionState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        invalidation: Option<&StorageInvalidation>,
    ) {
        let Some(invalidation) = invalidation else {
            return;
        };

        self.invalidate_observations(
            state,
            bray_bound_tree::StorageMutation::Operation(occurrence),
            invalidation,
        );
    }

    fn storage_preservations<'path>(
        &self,
        paths: &'path BTreeMap<StoragePath, StorageObservationKind>,
    ) -> Vec<StoragePreservation<'path>> {
        paths.iter().map(|(path, kind)| StoragePreservation {
            place: crate::ExecutionPlace::storage_path(self.storage, path),
            path,
            kind: *kind,
        }).collect()
    }

    fn preserved_storage_values(
        &self,
        state: &ExecutionState,
        values: &[StoragePreservation<'_>],
    ) -> crate::execution_guarantees::ObservationSet {
        let mut observations = crate::execution_guarantees::ObservationSet::default();

        for StoragePreservation { place, path, kind } in values.iter().filter(|entry| {
            entry.kind.is_immutable_inline() || entry.kind == StorageObservationKind::Discriminant
        }) {
            let value = match place {
                Some(place) => state.current.live_value(place),
                None => Some(self.storage_path_value(state, path)),
            };

            let Some(snapshot) = value else { continue; };

            if kind.is_immutable_inline() {
                for value in state.owned_values(&snapshot).filter(|value| *value != ExecutionCondition::unknown()) {
                    // Existing loans can still observe the unchanged value.
                    observations.insert(value.clone());

                    if !value.is_reference() && value.has_discriminant() {
                        // A complete inline value also retains its tag after an
                        // earlier result wrapper has left the live value tree.
                        observations.insert_classified(value.clone(), StorageObservationKind::Discriminant);
                    }

                    let borrowed = if value.is_reference() {
                        value.predicate_observation()
                    } else {
                        value.borrowed()
                    };

                    observations.insert(borrowed);
                }
            } else {
                observations.insert_classified(snapshot.clone(), *kind);
                observations.insert_classified(snapshot.borrowed(), *kind);
            }
        }

        observations
    }

    fn preserve_normal_snapshots(
        &self,
        state: &ExecutionState,
        mutation: bray_bound_tree::StorageMutation,
        snapshots: &BTreeMap<(bray_bound_tree::StorageSnapshotRoot, std::sync::Arc<[bray_bound_tree::StorageProjection]>), bray_bound_tree::StorageObservationKind>,
        observations: &mut crate::execution_guarantees::ObservationSet,
    ) {
        let bray_bound_tree::StorageMutation::Operation(site) = mutation else { return; };

        let Some(expression) = site.expression() else { return; };

        let entry = state.entries.get(&site);

        for ((root, projections), kind) in snapshots {
            let value = match root {
                bray_bound_tree::StorageSnapshotRoot::Produced(destination) => entry.and_then(|entry| entry.outputs.common_value(destination)).cloned(),
                bray_bound_tree::StorageSnapshotRoot::PendingResult =>
                    state.pending_results.get(&expression).or_else(|| state.expressions.get(&expression)).cloned(),
                bray_bound_tree::StorageSnapshotRoot::Argument(ordinal) =>
                    entry.and_then(|entry| entry.arguments.get(&crate::ExecutionPlace::argument(*ordinal))).cloned(),
                bray_bound_tree::StorageSnapshotRoot::Extracted { producer, term } =>
                    state.admitted_extraction(site, producer.as_ref(), *term).cloned(),
            };

            let Some(mut value) = value else { continue; };

            // The snapshot owns inline contents extracted from the old loan. It
            // does not preserve the source slot or the loan's mutable referents.
            value = value.into_referent();

            for projection in projections.iter().copied() {
                value = value.project(projection);
            }

            if *kind == bray_bound_tree::StorageObservationKind::Initialization {
                self.preserve_initialized_snapshot(
                    entry.map_or(&state.trusted_assumptions, |entry| &entry.trusted_assumptions),
                    value, observations,
                );
            } else if matches!(root, bray_bound_tree::StorageSnapshotRoot::Extracted { .. })
                && projections.is_empty() && *kind == bray_bound_tree::StorageObservationKind::Value
            {
                for retained in state.owned_values(&value).filter(|value| !value.is_unknown()) {
                    observations.insert(retained.clone());
                    observations.insert(retained.borrowed());
                }
            } else {
                observations.insert_classified(value.clone(), *kind);
                observations.insert_classified(value.borrowed(), *kind);
            }
        }
    }

    fn preserve_initialized_snapshot(
        &self,
        facts: &crate::execution_guarantees::ExecutionAssumptions,
        value: ExecutionCondition,
        observations: &mut crate::execution_guarantees::ObservationSet,
    ) {
        let predicate = self.request.available_compiler_known_symbols().provider()
            .declaration_symbol::<bray_symbols::PredicateSymbolId>(
                &bray_compiler_known::CompilerKnownDeclarationKey::try_new("UninitInitialized")
                    .expect("closed protected initialization predicate key is valid"),
            ).map(bray_symbols::PredicateDefinitionSymbolId::from);

        let snapshots = crate::execution_guarantees::RetainedSnapshots::from_iter([value]);

        for (condition, holds) in facts {
            let crate::ExecutionConditionKind::Trusted(inner) = condition.kind() else {
                continue;
            };

            let crate::ExecutionConditionKind::Predicate(identity, _, arguments) = inner.kind() else {
                continue;
            };

            // Carry the existing initialization clause, never an opaque predicate
            // on the wrapper or a permission on its consumed source storage.
            if *holds && Some(*identity) == predicate
                && let [owner] = arguments.as_ref()
                && snapshots.retains_inline_contents(&owner.clone().into_referent()) {
                observations.insert(condition.clone());
            }
        }
    }

    fn invalidate_all_observations(
        &self,
        state: &mut ExecutionState,
        mutation: bray_bound_tree::StorageMutation,
        invalidation: &StorageInvalidation,
        moved: &crate::execution_guarantees::RetainedSnapshots,
    ) {
        let StorageInvalidation::All { stable_descriptors: descriptors, stable_values, normal_snapshots, .. } = invalidation else {
            unreachable!("global observation invalidation has an opaque frame");
        };

        let descriptor_paths = descriptors
            .iter()
            .flat_map(|path| crate::ExecutionPlace::storage_path(self.storage, &path).into_iter().map(move |place| (place, path.clone())))
            .collect::<BTreeMap<_, _>>();

        let values = self.storage_preservations(stable_values);
        let mut snapshots = self.preserved_storage_values(state, &values);

        self.preserve_normal_snapshots(state, mutation, normal_snapshots, &mut snapshots);

        let pointees = self.pointee_preservations(state, invalidation, Some(&mut snapshots));

        let mut masks = self.preservation_masks(
            descriptor_paths.iter().map(|(place, path)| (place, path.projections.as_ref())),
            values.iter().filter_map(StoragePreservation::mask)
                .chain(pointees.iter().map(|(place, path, kind)| (place, path.as_ref(), *kind))));

        let current = state.current.root_slots().filter(|(_, _, availability)| {
            super::merge::resolve_availability(state, availability) != super::super::current::ValueAvailability::Absent
        }).map(|(place, value, _)| (place.clone(), value.clone())).collect::<Vec<_>>();

        if std::env::var_os("BRAY_BRA694_TRACE").is_some()
            && format!("{mutation:?}").contains("BoundUnitId(1111194), slot: 52")
        {
            eprintln!("BRA694 FRAME INPUT mutation={mutation:?} stable_paths={} mapped_paths={} roots={}", stable_values.len(), values.len(), current.len());

            for value in &values {
                eprintln!("BRA694 FRAME PATH place={:?} path={:?} kind={:?}", value.place, value.path, value.kind);
            }

            for (place, source) in &current {
                if format!("{:?}", source.kind()).contains("BoundUnitId(1111194), slot: 1") {
                    eprintln!("BRA694 FRAME ROOT place={place:?} source={:?} mask={:?}",
                        source.kind(), masks.get(place).map(|_| "present"));
                }
            }
        }

        for (place, source) in current {
            let mut mask = masks.remove(&place).unwrap_or_default();

            mask.prepare_frame(&source);

            let next = mask.frame_prepared(state, &source, &[], Some(&place), mutation, moved);

            if !std::ptr::eq(source.kind(), next.kind()) {
                // Whole-root framing changes contents while the current slot
                // retains its existing availability and observed components.
                *state.current.get_mut(&place).expect("framing a current root retains its slot") = next;
            }
        }

        // Authority follows the actual surviving value tree. A descriptor keeps
        // its own identity, but cannot retain overwritten children by prefix.
        // Evaluated snapshots are not a second source of live ownership.
        let owners = state.current.live_values().collect::<crate::execution_guarantees::RetainedSnapshots>();

        snapshots.preserve_copied_data(state.current.live_scalar_data().cloned());

        state.invalidate_cleanup(&snapshots, &owners, mutation.invocation());

    }

    fn preservation_masks<'a>(
        &self,
        descriptors: impl Iterator<Item = (&'a crate::ExecutionPlace, &'a [bray_bound_tree::StorageProjection])>,
        values: impl Iterator<Item = (&'a crate::ExecutionPlace, &'a [bray_bound_tree::StorageProjection], bray_bound_tree::StorageObservationKind)>,
    ) -> BTreeMap<crate::ExecutionPlace, super::preservation::ValuePreservation> {
        let mut masks = BTreeMap::<crate::ExecutionPlace, super::preservation::ValuePreservation>::new();

        for (place, path, kind) in descriptors.map(|(place, path)| (place, path, bray_bound_tree::StorageObservationKind::Identity)).chain(values) {
            let root = super::super::current::root_place(place);
            let mask = masks.entry(root).or_default().at_mut(path);

            mask.record(kind);
        }

        masks
    }

    pub(in crate::analysis) fn invalidate_observations(
        &self,
        state: &mut ExecutionState,
        mutation: bray_bound_tree::StorageMutation,
        invalidation: &StorageInvalidation,
    ) {
        use bray_bound_tree::StorageObservationTransition;

        let transition = invalidation.observation_transition();
        let unchanged = transition == StorageObservationTransition::Unchanged;

        if matches!(mutation, bray_bound_tree::StorageMutation::Operation(_) | bray_bound_tree::StorageMutation::TaskObservation { .. })
            && let Some(site) = mutation.invocation()
            && state.admitted_call(site).is_some_and(|entry| entry.storage_unchanged != unchanged)
        {
            state.admitted_call_mut(site).expect("the reached transition retains its call entry").storage_unchanged = unchanged;
        }

        if unchanged {
            return;
        }

        let transfers = transition == StorageObservationTransition::Transfer;
        let content_only = transition.changes_contents_only();

        if self.repeats_mutation(mutation) {
            state.retire_evaluation(mutation);
        }

        let admitted = mutation.invocation();
        let moved = self.extracted_input_snapshots(state, mutation);
        let extraction = admitted.map(|site| (site, &moved));

        if matches!(invalidation, StorageInvalidation::All { .. }) {
            self.invalidate_all_observations(state, mutation, invalidation, &moved);

            return;
        }

        let values = match invalidation {
            StorageInvalidation::Observed { stable_values, .. } => self.storage_preservations(stable_values),
            _ => Vec::new(),
        };

        let collect_snapshots = |state: &ExecutionState| {
            let mut observations = self.preserved_storage_values(state, &values);

            if let Some(snapshots) = invalidation.normal_snapshots() {
                self.preserve_normal_snapshots(state, mutation, snapshots, &mut observations);
            }

            let pointees = self.pointee_preservations(state, invalidation, Some(&mut observations));

            observations.preserve_copied_data(state.current.live_scalar_data().cloned());

            (observations, pointees)
        };

        // The proof only needs observation snapshots when something reached
        // an existing observation. Framing the current slots uses the checked mask.
        let mut snapshots = None;

        if !transfers && let StorageInvalidation::Observed { referents, .. } = invalidation {
            let mut observed = Vec::new();
            let mut address_slots = BTreeSet::new();

            for path in referents {
                let Some(value) = crate::ExecutionPlace::storage_path(self.storage, path)
                    .and_then(|place| place.value_in(&state.current))
                else {
                    continue;
                };

                let ty = self.storage_path_type(path);

                if ty.is_some_and(|ty| {
                    crate::representation::reference_kind(self.request, ty)
                        == Some(bray_symbols::DependencyReferenceKind::Address)
                }) {
                    address_slots.insert(value.owner_identity().clone());

                    let pointer = value.descriptor_identity().clone();

                    address_slots.insert(pointer.clone());
                    observed.push(pointer);
                }

                observed.push(value);
            }

            if !observed.is_empty() {
                let unstable = BTreeSet::new();

                let preserved = match (transition, admitted) {
                    (StorageObservationTransition::CellContents(ty), Some(invocation)) =>
                        self.disjoint_initialized_values(state, invocation, ty),
                    _ => BTreeSet::new(),
                };

                let (stable_values, _) = snapshots.get_or_insert_with(|| collect_snapshots(state));

                state.invalidate_referents(
                    &observed,
                    &address_slots,
                    if content_only { &self.content_predicates } else { &unstable },
                    &preserved,
                    stable_values,
                    extraction,
                );
            }
        }

        let StorageInvalidation::Observed { accesses, referents, stable_descriptors, affected_pointees, .. } = invalidation else {
            unreachable!("finite proof invalidation retains its checked observation frame");
        };

        // Storage flow distinguishes the descriptor slot from its reached elements.
        // Keep only descriptor identity. A fresh payload snapshot prevents opaque calls
        // and predicates from equating the old contents with the new contents.
        let descriptor_paths = stable_descriptors.iter().filter_map(|path| {
            let place = crate::ExecutionPlace::storage_path(self.storage, path)?;

            Some((place, path.projections.clone()))
        }).collect::<BTreeMap<_, _>>();

        let static_referents = referents.iter().filter(|path|
            matches!(self.storage.identity(path.root), Some(bray_bound_tree::StorageIdentity::Static(_))));

        let logical_paths = accesses.values().flatten().chain(static_referents).collect::<BTreeSet<_>>();
        let mut changed_places = BTreeMap::new();
        let mut invalidated_values = Vec::new();

        for path in logical_paths {
            let Some(root) = crate::ExecutionPlace::storage_identity(self.storage, path.root) else {
                continue;
            };

            let (place, observed) = root.project_observation(&path.projections);

            // Borrow invalidation uses the reached path. Value framing uses
            // precisely the observable prefix that selected the current place.
            if !transfers && !descriptor_paths.contains_key(&place)
                && let Some(value) = crate::execution_guarantees::storage_borrow_identity(
                    self.request.semantic_values(), self.request.available_compiler_known_symbols(),
                    self.storage, &state.current, path,
                )
            {
                invalidated_values.push(value.borrowed());
            }

            changed_places.insert(place, observed);
        }

        record_pointee_changes(state, affected_pointees.as_ref(), &mut changed_places);

        let mut scalar_places = BTreeSet::new();

        invalidated_values.extend(
            changed_places
                .keys()
                .filter(|place| !transfers || place.is_pointee())
                .filter(|place| !descriptor_paths.contains_key(*place))
                .filter_map(|place| {
                    let value = value_for_invalidation(state, place)?;

                    if self.observed_types.get(place).copied().flatten()
                        .map(|ty| self.request.semantic_values().unborrowed_type(ty))
                        .and_then(|ty| crate::representation::type_representation(self.request, ty))
                        .is_some_and(crate::representation::is_scalar_value)
                    {
                        // A write retires this slot's borrowed observation, not
                        // the immutable value also held by another scalar copy.
                        scalar_places.insert(place.clone());

                        return None;
                    }

                    Some(value)
                }),
        );

        if !scalar_places.is_empty() || !invalidated_values.is_empty() {
            let no_predicates = BTreeSet::new();

            let (stable_values, _) = snapshots.get_or_insert_with(|| collect_snapshots(state));

            state.invalidate_contents(
                &scalar_places,
                invalidated_values,
                if content_only { &self.content_predicates } else { &no_predicates },
                stable_values,
                extraction,
            );
        }

        let pointees = snapshots.map_or_else(|| self.pointee_preservations(state, invalidation, None), |(_, paths)| paths);
        let changed_roots = changed_places.keys().map(|place| &place.root).collect::<BTreeSet<_>>();

        let masks = self.preservation_masks(
            descriptor_paths.iter().filter(|(place, _)| changed_roots.contains(&place.root))
                .map(|(place, path)| (place, path.as_ref())),
            values.iter().filter_map(StoragePreservation::mask)
                .chain(pointees.iter().map(|(place, path, kind)| (place, path.as_ref(), *kind)))
                .filter(|(place, _, _)| changed_roots.contains(&place.root)));

        Self::frame_changed_observations(state, (mutation, &moved), transfers, changed_places, masks);

        if !transfers && let StorageInvalidation::Observed { snapshots, .. } = invalidation {
            self.refresh_storage_observations(state, snapshots);
        }

    }

    fn extracted_input_snapshots(
        &self, state: &ExecutionState, mutation: bray_bound_tree::StorageMutation,
    ) -> crate::execution_guarantees::RetainedSnapshots {
        // A move retires its source even when none of the old observations survive.
        let frame = match mutation {
            bray_bound_tree::StorageMutation::Operation(site) => self.direct_await_body(state, site)
                .map(|(_, _, producer)| producer.entry.as_ref()),
            _ => None,
        };

        let mut moved = state.extracted_input_snapshots(mutation, frame);

        if let bray_bound_tree::StorageMutation::Operation(site) = mutation
            && let Some(entry) = state.admitted_call(site)
            && let Some(payload) = self.protected_extraction_payload(site, entry)
        {
            moved.insert(payload);
        }

        moved
    }

    fn frame_changed_observations(
        state: &mut ExecutionState,
        (mutation, moved): (bray_bound_tree::StorageMutation, &crate::execution_guarantees::RetainedSnapshots),
        transfers: bool,
        changed_places: BTreeMap<crate::ExecutionPlace, &[StorageProjection]>,
        mut masks: BTreeMap<crate::ExecutionPlace, super::preservation::ValuePreservation>,
    ) {
        let mut updates = BTreeMap::new();

        for (place, path) in changed_places {
            if !can_invalidate(state, &place) {
                continue;
            }

            let root = super::super::current::root_place(&place);

            let (source, updated, mask, covered_path) = match updates.entry(root) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    let root = entry.key();

                    let Some(source) = state.current.get(root).cloned() else { continue; };

                    let mut mask = masks.remove(root).unwrap_or_default();

                    mask.prepare_frame(&source);

                    entry.insert((source.clone(), source, mask, None))
                }
            };

            // The map visits enclosing paths before their descendants. The
            // enclosing update already frames that storage with its checked mask.
            if covered_path.is_some_and(|covered| path.starts_with(covered)) {
                continue;
            }

            let next = if transfers && !place.is_pointee() { ExecutionCondition::unknown() } else {
                mask.frame_prepared(state, source, path, Some(&place), mutation, moved)
            };

            *updated = updated.with_storage_component(path, next);
            *covered_path = Some(path);
        }

        state.current.install_rewritten_values(updates.into_iter().filter_map(|(place, (source, value, ..))| {
            (!std::ptr::eq(source.kind(), value.kind())).then_some((place, value))
        }));
    }

    fn repeats_mutation(&self, mutation: bray_bound_tree::StorageMutation) -> bool {
        use bray_bound_tree::StorageMutation;

        match mutation {
            StorageMutation::Operation(_) => false,
            StorageMutation::Cleanup(action)
            | StorageMutation::CleanupOutcome { action, .. }
            | StorageMutation::CleanupFailure { action, .. } => {
                self.values.can_repeat(action.occurrence) || self.disposal.repeats_action(action)
            }
            _ => self.values.can_repeat(mutation.occurrence()),
        }
    }

    pub(in crate::analysis) fn refresh_storage_observations(
        &self,
        state: &mut ExecutionState,
        observations: &BTreeMap<bray_bound_tree::StoragePath, bray_bound_tree::StorageObservedTarget>,
    ) {
        // Read all targets before assigning aliases, so map order cannot change the result.
        let aliases = observations
            .iter()
            .filter_map(|(path, observation)| {
                let target = &observation.target;

                if target.value_observation_path().as_ref() == Some(path) {
                    return None;
                }

                let place = crate::ExecutionPlace::storage_path(self.storage, path);
                let value = self.current_target_value(state, target, observation.pointee)?;

                let value = if observation.pointee.is_some() {
                    let previous = self.storage_path_value(state, path);

                    if previous.is_reference() { previous.with_referent(value) } else { value.borrowed() }
                } else {
                    value
                };

                Some((path, place, value))
            })
            .collect::<Vec<_>>();

        for (path, place, value) in aliases {
            if let Some(place) = place {
                state.observe(place, value, Some(&path.projections));
            } else if let Some(bray_bound_tree::StorageIdentity::Temporary(expression)) =
                self.storage.identity(path.root)
            {
                // Refresh the retained result in its current publication phase.
                // A dead temporary is not recreated from its source identity.
                let Some(previous) = state.expressions.get_mut(&expression)
                    .or_else(|| state.pending_results.get_mut(&expression)) else {
                    continue;
                };

                let value = if path.projections.is_empty() {
                    value
                } else {
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

                *previous = value;
            }
        }
    }
}

/// Definite absence retains terminal data, not another object that an effect can update.
fn value_for_invalidation(state: &ExecutionState, place: &crate::ExecutionPlace) -> Option<ExecutionCondition> {
    if can_invalidate(state, place) {
        place.value_in(&state.current)
    } else {
        None
    }
}

fn can_invalidate(state: &ExecutionState, place: &crate::ExecutionPlace) -> bool {
    super::merge::resolved_availability(state, place) != super::super::current::ValueAvailability::Absent
}

#[cfg(test)]
mod tests {
    use super::{ExecutionFlowDomain, ExecutionState, record_pointee_changes, value_for_invalidation};
    use super::super::preservation::ValuePreservation;
    use crate::analysis::guarantee::current::{CurrentValue, ValueAvailability};
    use crate::{ExecutionCondition, ExecutionPlace};
    use bray_bound_tree::{BoundUnitId, BoundUnitKind, StorageIdentity, StorageMutation, StorageOrigin, StoragePath, StoragePlanBuilder, StoragePointeeChange, StorageProjection, StorageTarget};
    use bray_symbols::{DependencyOwnedResult as Layout, DependencyOwnedTransfer as Transfer, DependencyValuePreservation as Preservation, SymbolOrdinal};

    #[test]
    fn a_reached_component_change_keeps_its_sibling_and_frames_other_typed_views() {
        let unit = BoundUnitId::new(0);

        let (_, expressions) = crate::test_support::integer_expression_unit(unit, 3);

        let mut storage = StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);
        let reference = storage.push_identity(StorageIdentity::Temporary(expressions[2])).expect("test producer belongs to its storage unit");
        let storage = storage.finish();

        let target = StorageTarget {
            previous: false,
            origin: StorageOrigin::Returned { reference: StoragePath::root(reference), projections: [].into() },
        };

        let element = crate::test_support::semantic_values().intern_type(bray_symbols::TypeData::ExactInteger {
            bits: std::num::NonZeroU16::new(32).expect("nonzero integer width"),
            signed: false,
        }).expect("integer type");

        let pair = crate::test_support::tuple_type([crate::test_support::array_type(element, 2), element]);

        let place = |ty| ExecutionPlace::pointee(&storage, &target, ty);

        let component = |ordinal| StorageProjection::TupleElement(SymbolOrdinal::new(ordinal));
        let view = place(pair);
        let left = view.clone().project(&[component(0)]).expect("left field");
        let right = view.clone().project(&[component(1)]).expect("right field");
        let origin = ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expressions[0]));
        let index = StorageProjection::ElementFromStart(SymbolOrdinal::new(0));
        let last = StorageProjection::ElementFromEnd(SymbolOrdinal::new(0));
        let member = left.clone().project(&[index]).expect("array member");
        let owner = ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expressions[2]));
        let owned = ExecutionCondition::owned_value(owner.clone(), ExecutionCondition::unknown());
        let before = origin.with_storage_component(&[component(0), index], owned);
        let effect = StorageMutation::Operation(expressions[1].into());

        assert_eq!(before.clone().project(component(0)).project(index).owner_identity(), &owner);

        for paths in [
            vec![vec![component(0)]],
            vec![vec![component(0)], vec![component(0), index]],
            vec![vec![component(0), last]],
        ] {
            let mut state = ExecutionState::default();

            state.current.insert(view.clone(), before.clone());
            state.current.insert(place(element), before.clone());

            let affected = [(target.clone(), StoragePointeeChange::Components {
                ty: pair,
                paths: paths.into_iter().map(std::sync::Arc::from).collect(),
            })].into();

            let mut changed = std::collections::BTreeMap::new();

            record_pointee_changes(&state, Some(&affected), &mut changed);

            ExecutionFlowDomain::<crate::test_support::TestCheckerContext>::frame_changed_observations(
                &mut state, (effect, &Default::default()), false, changed, Default::default(),
            );

            assert_ne!(left.value_in(&state.current), Some(before.clone().project(component(0))));
            assert_eq!(right.value_in(&state.current), Some(before.clone().project(component(1))));
            assert_ne!(place(element).value_in(&state.current), Some(before.clone()));
            assert_ne!(member.value_in(&state.current).map(|value| value.owner_identity().clone()), Some(owner.clone()));
        }
    }

    #[test]
    fn absent_sources_cannot_overwrite_live_return_ownership_during_invalidation() {
        let (_, expressions) = crate::test_support::integer_expression_unit(BoundUnitId::new(0), 2);

        let place = |ordinal| ExecutionPlace::argument(SymbolOrdinal::new(ordinal));
        let left = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place(3)));
        let right = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place(4)));
        let owner = ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expressions[0]));
        let current = ExecutionCondition::owned_value(owner.clone(), ExecutionCondition::unknown());
        let unknown = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place(5)));
        let guard = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place(6)));
        let transfer = |input| Layout::Transfer(Transfer::new(input, []).with_preservation(Preservation::all()));
        let mutation = StorageMutation::Operation(expressions[1].into());
        let field = StorageProjection::TupleElement(SymbolOrdinal::new(0));

        for absent in [ValueAvailability::Absent, ValueAvailability::Guarded(guard.clone())] {
            let mut state = ExecutionState::default();

            guard.clone().assume(false, &mut state.assumptions);
            state.assign(place(0), current.clone(), None);
            state.current.set_slot(place(1), CurrentValue { value: owner.clone(), availability: absent });
            state.current.set_slot(place(2), CurrentValue { value: unknown.clone(), availability: ValueAvailability::Unknown });

            let child = place(2).project(&[field]).expect("tuple child");

            state.current.retire(&child);
            state.owned_results.insert(current.clone(), transfer(right.clone()));
            state.owned_results.insert(owner.clone(), transfer(left.clone()));
            state.owned_results.insert(unknown.clone(), transfer(unknown.clone()));

            assert!(value_for_invalidation(&state, &place(2)).is_some(), "unknown storage remains a possible mutation target");
            assert!(value_for_invalidation(&state, &child).is_none(), "an absent child does not suppress its remaining root");

            let values = state.current.iter().filter_map(|(place,_)|
                value_for_invalidation(&state, place).map(|value| (place.clone(),value))).collect::<Vec<_>>();

            for (place,value) in values {
                let moved = state.extracted_input_snapshots(mutation, None);
                let next = ValuePreservation::default().frame(&mut state, &value, &[], Some(&place), mutation, &moved);

                state.observe(place, next, None);
            }

            let result = place(0).value_in(&state.current).expect("live return");
            let layout = state.owned_result(&result);
            let actual = layout.guaranteed_transfers().into_iter().collect::<Vec<_>>();

            assert_eq!(actual.len(), 1);
            assert_eq!(actual[0].input(), right, "the terminal source cannot replace the live return's input");
            assert_eq!(place(1).value_in(&state.current), Some(owner.clone()), "terminal data remains readable and unchanged");
            assert_ne!(place(2).value_in(&state.current), Some(unknown.clone()), "unknown availability cannot freeze contents");
        }
    }
}
