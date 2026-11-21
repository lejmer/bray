use super::model::ExecutionFlowDomain;
use std::collections::{BTreeMap, BTreeSet};

use crate::CheckerRequestContext;
use crate::execution_guarantees::ExecutionCondition;

use super::super::state::ExecutionState;
use bray_bound_tree::StorageInvalidation;

fn cleanup_observation_profile(unit: bray_bound_tree::BoundUnitId, mutation: bray_bound_tree::StorageMutation) -> bool {
    use bray_bound_tree::StorageMutation;
    static UNIT: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();

    matches!(mutation, StorageMutation::Cleanup(_) | StorageMutation::CleanupOutcome { .. }
        | StorageMutation::CleanupFailure { .. })
        && UNIT.get_or_init(|| std::env::var("BRA_694_PROFILE_CLEANUP").ok()?.parse().ok()) == &Some(unit.raw())
}

fn record_cleanup_observation_profile(unit: bray_bound_tree::BoundUnitId, times: [u64; 6]) {
    use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
    static TOTALS: [AtomicU64; 6] = [const { AtomicU64::new(0) }; 6];
    static COUNT: AtomicU64 = AtomicU64::new(0);

    for (total, elapsed) in TOTALS.iter().zip(times) {
        total.fetch_add(elapsed, Relaxed);
    }

    let count = COUNT.fetch_add(1, Relaxed) + 1;

    if count % 500 == 0 {
        let totals = TOTALS.each_ref().map(|total| total.load(Relaxed));

        eprintln!("cleanup proof {unit:?} count={count} snapshots/referents/places/retirement/rewrite/references us={totals:?}");
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

    fn storage_preservations(
        &self,
        paths: &BTreeMap<bray_bound_tree::StoragePath, bray_bound_tree::StorageObservationKind>,
    ) -> Vec<(crate::ExecutionPlace, bray_bound_tree::StoragePath, bray_bound_tree::StorageObservationKind)> {
        paths.iter().flat_map(|(path, kind)| crate::ExecutionPlace::storage_path(self.storage, &path).into_iter()
            .map(move |place| (place, path.clone(), *kind))).collect()
    }

    fn preserved_storage_values(
        &self,
        state: &ExecutionState,
        paths: &BTreeMap<bray_bound_tree::StoragePath, bray_bound_tree::StorageObservationKind>,
    ) -> crate::execution_guarantees::ObservationSet {
        let mut observations = crate::execution_guarantees::ObservationSet::default();

        for (path, kind) in paths.iter().filter(|(_, kind)| {
            kind.is_immutable_inline() || **kind == bray_bound_tree::StorageObservationKind::Discriminant
        }) {
            let value = match crate::ExecutionPlace::storage_path(self.storage, path) {
                Some(place) => state.current.live_value(&place),
                None => Some(self.storage_path_value(state, path)),
            };

            let Some(snapshot) = value else { continue; };

            if kind.is_immutable_inline() {
                for value in state.owned_values(&snapshot).filter(|value| *value != ExecutionCondition::unknown()) {
                    // Existing loans can still observe the unchanged value.
                    observations.insert(value.clone());
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

        let trace = self.trace_authority_site(mutation);

        static PROFILE_UNIT: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();

        let profile = PROFILE_UNIT.get_or_init(|| std::env::var("BRA_694_PROFILE_CLEANUP").ok()?.parse().ok());
        let mut profile = (*profile == Some(self.storage.unit().raw())).then(std::time::Instant::now);
        let mut times = [0; 4];

        let mut checkpoint = |index| {
            if let Some(start) = &mut profile {
                let now = std::time::Instant::now();

                times[index] = (now - *start).as_micros();
                *start = now;
            }
        };

        let descriptor_paths = descriptors
            .iter()
            .flat_map(|path| crate::ExecutionPlace::storage_path(self.storage, &path).into_iter().map(move |place| (place, path.clone())))
            .collect::<BTreeMap<_, _>>();

        let mut snapshots = self.preserved_storage_values(state, stable_values);

        self.preserve_normal_snapshots(state, mutation, normal_snapshots, &mut snapshots);

        let values = self.storage_preservations(stable_values);
        let pointees = self.pointee_preservations(state, invalidation, Some(&mut snapshots));

        if trace {
            eprintln!(
                "opaque proof observations {mutation:?}: values={stable_values:?} pointees={pointees:?}"
            );
        }

        let mut masks = self.preservation_masks(
            descriptor_paths.iter().map(|(place, path)| (place, path.projections.as_ref())),
            values.iter().map(|(place, path, kind)| (place, path.projections.as_ref(), *kind))
                .chain(pointees.iter().map(|(place, path, kind)| (place, path.as_ref(), *kind))));

        checkpoint(0);

        let current = state.current.root_slots().filter(|(_, _, availability)| {
            super::merge::resolve_availability(state, availability) != super::super::current::ValueAvailability::Absent
        }).map(|(place, value, _)| (place.clone(), value.clone())).collect::<Vec<_>>();

        for (place, source) in current {
            let mut mask = masks.remove(&place).unwrap_or_default();

            mask.prepare_frame(&source);

            let next = mask.frame_prepared(state, &source, &[], Some(&place), mutation, moved);

            if trace {
                for field in source.component_projections() {
                    if !["SymbolId(2401)", "SymbolId(2072)", "SymbolId(2074)"].iter()
                        .any(|index| format!("{field:?}").contains(index)) { continue; }

                    let before = source.clone().project(field);
                    let after = next.clone().project(field);
                    let known = descriptor_paths.iter().filter(|(path, _)| place.contains(path)).collect::<Vec<_>>();

                    eprintln!("selected descriptor {mutation:?} {place:?} preserved={known:?} before={} after={}",
                        format!("{:?}", before.kind()).chars().take(320).collect::<String>(),
                        format!("{:?}", after.kind()).chars().take(320).collect::<String>());
                }
            }

            if !std::ptr::eq(source.kind(), next.kind()) {
                // Whole-root framing changes contents while the current slot
                // retains its existing availability and observed components.
                *state.current.get_mut(&place).expect("framing a current root retains its slot") = next;
            }
        }

        checkpoint(1);

        // Authority follows the actual surviving value tree. A descriptor keeps
        // its own identity, but cannot retain overwritten children by prefix.
        // Evaluated snapshots are not a second source of live ownership.
        let owners = state.current.live_values().collect::<crate::execution_guarantees::RetainedSnapshots>();

        if trace && matches!(mutation, bray_bound_tree::StorageMutation::Operation(_)) {
            for (carrier, witness) in &state.witnesses {
                for condition in witness.conditions.keys() {
                    let condition_text = format!("{:?}", condition.kind());

                    if condition_text.contains("SymbolId(6425)") {
                        let observations = condition.observations().map(|value| {
                            (snapshots.preserves(value), format!("{:?}", value.kind()).chars().take(650).collect::<String>())
                        }).collect::<Vec<_>>();

                        eprintln!(
                            "opaque witness {mutation:?} available={} owner_live={} carrier={} condition={} observations={observations:?}",
                            witness.available, owners.retains_owner(carrier),
                            format!("{:?}", carrier.kind()).chars().take(650).collect::<String>(),
                            condition_text.chars().take(850).collect::<String>()
                        );
                    }
                }
            }
        }

        checkpoint(2);

        state.invalidate_cleanup(&snapshots, &owners, mutation.invocation());
        checkpoint(3);

        if times.iter().sum::<u128>() >= 1_000 {
            eprintln!("opaque observation profile {mutation:?} sections_us={times:?}");
        }
    }

    fn preservation_masks<'a>(
        &self,
        descriptors: impl Iterator<Item = (&'a crate::ExecutionPlace, &'a [bray_bound_tree::StorageProjection])>,
        values: impl Iterator<Item = (&'a crate::ExecutionPlace, &'a [bray_bound_tree::StorageProjection], bray_bound_tree::StorageObservationKind)>,
    ) -> BTreeMap<crate::ExecutionPlace, super::preservation::ValuePreservation> {
        let mut masks = BTreeMap::<crate::ExecutionPlace, super::preservation::ValuePreservation>::new();

        for (place, path, kind) in descriptors.map(|(place, path)| (place, path, bray_bound_tree::StorageObservationKind::Identity)).chain(values) {
            let mut root = place.clone();

            root.projections = [].into();

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

        let trace = self.trace_authority_site(mutation).then(|| {
            eprintln!("authority invalidation {mutation:?} scope={:?} kind={transition:?}", std::mem::discriminant(invalidation));

            state.trusted_assumptions.clone()
        });

        let transfers = transition == StorageObservationTransition::Transfer;
        let content_only = transition.changes_contents_only();

        if self.repeats_mutation(mutation) {
            state.retire_evaluation(mutation);
        }

        let admitted = mutation.invocation();
        let moved = self.extracted_input_snapshots(state, mutation);

        if matches!(invalidation, StorageInvalidation::All { .. }) {
            self.invalidate_all_observations(state, mutation, invalidation, &moved);
            trace_authority_change(mutation, trace, state);

            return;
        }

        let mut profile = cleanup_observation_profile(self.storage.unit(), mutation).then(std::time::Instant::now);
        let mut times = [0; 6];

        let mut checkpoint = |index| {
            if let Some(start) = &mut profile {
                let now = std::time::Instant::now();

                times[index] = (now - *start).as_micros() as u64;
                *start = now;
            }
        };

        let collect_snapshots = |state: &ExecutionState| {
            let mut observations = match invalidation {
                StorageInvalidation::Observed { stable_values, .. } => self.preserved_storage_values(state, stable_values),
                _ => crate::execution_guarantees::ObservationSet::default(),
            };

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

        checkpoint(0);

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
                );
            }
        }

        checkpoint(1);

        let (mut changed_places, changed_paths) = self.invalidated_places(invalidation);

        let StorageInvalidation::Observed { affected_pointees, .. } = invalidation else {
            unreachable!("finite proof invalidation retains its checked observation frame");
        };

        changed_places.extend(state.current.iter().filter_map(|(place, _)| {
            let crate::execution_guarantees::ExecutionInput::Pointee { target, .. } = &place.root else { return None; };

            affected_pointees.as_ref().is_none_or(|affected| affected.contains(target)).then(|| place.clone())
        }));

        let trace_dispatch = trace.is_some() && mutation.occurrence().expression().is_some_and(|expression|
            expression.unit().raw() == 749970 && expression.ordinal() == 117);

        // Storage flow distinguishes the descriptor slot from its reached elements.
        // Keep only descriptor identity. A fresh payload snapshot prevents opaque calls
        // and predicates from equating the old contents with the new contents.
        let descriptor_paths = match invalidation {
            StorageInvalidation::Observed {
                stable_descriptors, ..
            } => stable_descriptors
                .iter()
                .filter_map(|path| {
                    let place = crate::ExecutionPlace::storage_path(self.storage, path)?;

                    Some((place, path.projections.clone()))
                })
                .collect::<BTreeMap<_, _>>(),
            _ => BTreeMap::new(),
        };

        let mut scalar_places = BTreeSet::new();

        let mut invalidated_values: Vec<_> = {
            changed_places
                .iter()
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
                })
                .collect()
        };

        if !transfers {
            invalidated_values.extend(changed_paths.iter()
                .filter(|(place, _)| !descriptor_paths.contains_key(*place))
                .filter_map(|(_, path)| crate::execution_guarantees::storage_borrow_identity(
                    self.request.semantic_values(), self.request.available_compiler_known_symbols(),
                    self.storage, &state.current, path))
                .map(ExecutionCondition::borrowed));
        }

        checkpoint(2);

        if !scalar_places.is_empty() || !invalidated_values.is_empty() {
            let no_predicates = BTreeSet::new();
            let (stable_values, _) = snapshots.get_or_insert_with(|| collect_snapshots(state));

            state.invalidate_contents(
                &scalar_places,
                invalidated_values,
                if content_only { &self.content_predicates } else { &no_predicates },
                stable_values,
            );
        }

        checkpoint(3);

        let values = match invalidation {
            StorageInvalidation::Observed { stable_values, .. } => self.storage_preservations(stable_values),
            _ => Vec::new(),
        };

        let pointees = snapshots.map_or_else(|| self.pointee_preservations(state, invalidation, None), |(_, paths)| paths);

        let masks = self.preservation_masks(
            descriptor_paths.iter().map(|(place, path)| (place, path.as_ref())),
            values.iter().map(|(place, path, kind)| (place, path.projections.as_ref(), *kind))
                .chain(pointees.iter().map(|(place, path, kind)| (place, path.as_ref(), *kind))));

        Self::frame_changed_observations(state, (mutation, &moved), transfers, changed_places, &changed_paths, masks);

        checkpoint(4);

        if !transfers && let StorageInvalidation::Observed { snapshots, stable_values, .. } = invalidation {
            self.refresh_invalidated_references(state, snapshots, trace_dispatch.then_some(stable_values));
        }

        checkpoint(5);

        if profile.is_some() {
            record_cleanup_observation_profile(self.storage.unit(), times);
        }

        trace_authority_change(mutation, trace, state);
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

        state.extracted_input_snapshots(mutation, frame)
    }

    fn frame_changed_observations(
        state: &mut ExecutionState,
        (mutation, moved): (bray_bound_tree::StorageMutation, &crate::execution_guarantees::RetainedSnapshots),
        transfers: bool,
        changed_places: BTreeSet<crate::ExecutionPlace>,
        changed_paths: &BTreeMap<crate::ExecutionPlace, bray_bound_tree::StoragePath>,
        mut masks: BTreeMap<crate::ExecutionPlace, super::preservation::ValuePreservation>,
    ) {
        let mut updates = BTreeMap::new();

        for place in changed_places {
            if value_for_invalidation(state, &place).is_none() {
                continue;
            }

            let mut root = place.clone();

            root.projections = [].into();

            let (source, updated, mask) = match updates.entry(root) {
                std::collections::btree_map::Entry::Occupied(entry) => entry.into_mut(),
                std::collections::btree_map::Entry::Vacant(entry) => {
                    let root = entry.key();

                    let Some(source) = root.value_in(&state.current) else { continue; };

                    let mut mask = masks.remove(root).unwrap_or_default();

                    mask.prepare_frame(&source);

                    entry.insert((source.clone(), source, mask))
                }
            };

            let path = changed_paths.get(&place).map_or(&[][..], |path| path.projections.as_ref());

            let next = if transfers && !place.is_pointee() { ExecutionCondition::unknown() } else {
                mask.frame_prepared(state, source, path, Some(&place), mutation, moved)
            };

            *updated = updated.with_storage_component(path, next);
        }

        state.current.install_rewritten_values(updates.into_iter().filter_map(|(place, (source, value, _))| {
            (!std::ptr::eq(source.kind(), value.kind())).then_some((place, value))
        }));
    }

    fn refresh_invalidated_references(
        &self,
        state: &mut ExecutionState,
        snapshots: &BTreeMap<bray_bound_tree::StoragePath, bray_bound_tree::StorageObservedTarget>,
        trace_values: Option<&BTreeMap<bray_bound_tree::StoragePath, bray_bound_tree::StorageObservationKind>>,
    ) {
        let acquisition = trace_values.and_then(|values| values.keys().find(|path| path.root.ordinal() == 1
            && format!("{:?}", path.projections).contains("SymbolId(2072)")));

        let view = acquisition.and_then(|path| crate::ExecutionPlace::storage_path(self.storage, path));
        let before = view.as_ref().and_then(|place| place.value_in(&state.current));

        self.refresh_storage_observations(state, snapshots);

        if let Some(view) = view {
            let after = view.value_in(&state.current);
            let targets = snapshots.iter().filter(|(path, _)| path.root.ordinal() == 1).collect::<Vec<_>>();

            eprintln!("dispatch acquisition refresh {view:?}: targets={targets:?} before={} after={}",
                format!("{before:?}").chars().take(540).collect::<String>(),
                format!("{after:?}").chars().take(540).collect::<String>());
        }
    }

    fn repeats_mutation(&self, mutation: bray_bound_tree::StorageMutation) -> bool {
        use bray_bound_tree::StorageMutation;

        match mutation {
            StorageMutation::Operation(_) => false,
            StorageMutation::Cleanup(action)
            | StorageMutation::CleanupOutcome { action, .. }
            | StorageMutation::CleanupFailure { action, .. } => {
                self.values.can_repeat(action.occurrence) || self.disposal.plan.repeats_action(action)
            }
            _ => self.values.can_repeat(mutation.occurrence()),
        }
    }

    pub(in crate::analysis) fn trace_authority_site(&self, mutation: bray_bound_tree::StorageMutation) -> bool {
        static RANGES: std::sync::OnceLock<Vec<(u32, u32, u32)>> = std::sync::OnceLock::new();

        let ranges = RANGES.get_or_init(|| {
            let setting = std::env::var("BRA_694_TRACE_AUTHORITY").unwrap_or_default();

            setting.split(',').filter_map(|range| {
                let mut parts = range.split(':').map(str::parse::<u32>);

                Some((parts.next()?.ok()?, parts.next()?.ok()?, parts.next()?.ok()?))
            }).collect()
        });

        if ranges.is_empty() { return false; }

        let Some(node) = crate::diagnostic::bound_node_origin(self.request, mutation.occurrence().node()) else { return false; };

        let span = self.request.source(node.source_anchor()).span();

        ranges.iter().any(|&(source, start, end)| span.source_id().raw() == source && (start..=end).contains(&span.start().bytes()))
    }

    fn invalidated_places(
        &self,
        invalidation: &StorageInvalidation,
    ) -> (
        BTreeSet<crate::ExecutionPlace>,
        BTreeMap<crate::ExecutionPlace, bray_bound_tree::StoragePath>,
    ) {
        let mut changed_places = BTreeSet::new();

        // Execution places omit union discriminants. Keep the checked physical
        // path while rebuilding affected parents, including unselected variants.
        let mut changed_paths = BTreeMap::new();

        match invalidation {
            StorageInvalidation::Observed { accesses, referents, .. } => {
                let static_referents = referents.iter().filter(|path|
                    matches!(self.storage.identity(path.root), Some(bray_bound_tree::StorageIdentity::Static(_))));

                for path in accesses.values().flatten().chain(static_referents) {
                    if let Some(place) =
                        crate::ExecutionPlace::storage_observation(self.storage, path)
                    {
                        // An unresolved selector invalidates its observable enclosing value.
                        changed_paths.insert(place.clone(), path.clone());
                        changed_places.insert(place);
                    }
                }
            }
            StorageInvalidation::Unchanged => {}
            StorageInvalidation::All { .. } => unreachable!("global invalidation handled above"),
        }

        (changed_places, changed_paths)
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

fn trace_authority_change(mutation: bray_bound_tree::StorageMutation,
    before: Option<crate::execution_guarantees::ExecutionAssumptions>, state: &ExecutionState,
) {
    let Some(before) = before else { return; };

    let removed = before.difference(&state.trusted_assumptions);

    for (condition, holds) in removed {
        let text = format!("{:?}", condition.kind());

        eprintln!("authority retired {mutation:?} holds={holds}: {}", text.chars().take(520).collect::<String>());
    }
}

/// Definite absence retains terminal data, not another object that an effect can update.
fn value_for_invalidation(state: &ExecutionState, place: &crate::ExecutionPlace) -> Option<ExecutionCondition> {
    if super::merge::resolved_availability(state, place) == super::super::current::ValueAvailability::Absent {
        None
    } else {
        place.value_in(&state.current)
    }
}

#[cfg(test)]
mod tests {
    use super::{ExecutionState, value_for_invalidation};
    use super::super::preservation::ValuePreservation;
    use crate::analysis::guarantee::current::{CurrentValue, ValueAvailability};
    use crate::{ExecutionCondition, ExecutionPlace};
    use bray_bound_tree::{BoundUnitId, StorageMutation, StorageProjection};
    use bray_symbols::{DependencyOwnedResult as Layout, DependencyOwnedTransfer as Transfer, DependencyValuePreservation as Preservation, SymbolOrdinal};

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
