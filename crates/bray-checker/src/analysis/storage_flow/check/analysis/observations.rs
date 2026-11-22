use crate::analysis::storage_flow::memory::Effect;
use std::collections::BTreeMap;

use bray_bound_tree::{StorageInvalidation, StorageObservationKind, StorageObservedTarget, StoragePath, StorageProjection, StorageRelationship, StorageSnapshotRoot};

use super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::storage_flow::memory::{Source, Target};
use crate::analysis::storage_flow::model::StorageFlowState;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn preserve_returned_observations(
        &self,
        occurrence: bray_bound_tree::BoundExecutionSite,
        execution: &crate::analysis::guarantee::ExecutionState,
        memory: &crate::analysis::storage_flow::memory::State,
        effects: Option<&[Effect]>,
        invalidation: &mut StorageInvalidation,
    ) {
        let (producer, entry, storage) = if let Some((_, source, producer)) = self.execution.direct_await_body(execution, occurrence) {
            let Some((storage, _)) = self.execution.frame_storage_outcomes(source, producer) else { return; };

            (Some(source), producer.entry.as_ref(), storage)
        } else if let Some(entry) = execution.admitted_call(occurrence)
            && let Some(contracts) = self.dependencies.calls().get(&occurrence)
        {
            (None, entry, contracts.storage())
        } else {
            return;
        };

        let snapshots = match invalidation {
            StorageInvalidation::All { normal_snapshots, .. }
            | StorageInvalidation::Observed { normal_snapshots, .. } => normal_snapshots,
            StorageInvalidation::Unchanged => return,
        };

        let layouts = entry.outputs.reachable_rows().map(|row| &row.ownership);
        let transfers = bray_symbols::DependencyOwnedResult::common_transfers(layouts, &[]);

        for transfer in transfers {
            let route = storage.returned_value(&transfer);

            if route.is_borrowed() { continue; }

            let (root, prefix, value) = if let Some(value) = entry.extracted_values.get(&transfer.input()) {
                (StorageSnapshotRoot::Extracted { producer: producer.cloned(), term: transfer.input() }, &[][..], value.clone())
            } else {
                if producer.is_some() { continue; }

                let Some(ordinal) = occurrence.expression()
                    .and_then(|expression| self.execution.semantics.selections().expression(expression))
                    .and_then(|selection| selection.invocation_input_ordinal(occurrence, route.input_root()))
                else { continue; };

                let Some(argument) = entry.arguments.get(&crate::ExecutionPlace::argument(ordinal)) else { continue; };

                let prefix = route.input_path().projections();
                let value = prefix.iter().copied().fold(argument.clone().into_referent(), crate::ExecutionCondition::project);

                (StorageSnapshotRoot::Argument(ordinal), prefix, value)
            };

            if matches!(root, StorageSnapshotRoot::Extracted { .. })
                && transfer.preservation().preserves_value() && transfer.preservation().children().is_empty()
                && value.inline_captured_storage().is_some_and(|capture| retained_capture(memory, capture, effects))
            {
                snapshots.insert((root, [].into()), StorageObservationKind::Value);
                continue;
            }

            for (path, kind) in value.captured_observations() {
                if !kind.is_immutable_inline() && kind != StorageObservationKind::Discriminant { continue; }

                let Some(selectors) = path.iter().copied().map(crate::ExecutionPlace::projection_kind).collect::<Option<Vec<_>>>()
                else { continue; };

                if transfer.preservation().subtree(&selectors).preserves_value() {
                    let selected = prefix.iter().copied().chain(path).collect();

                    snapshots.entry((root.clone(), selected)).or_insert(kind);
                }
            }
        }

    }

    pub(super) fn preserve_consumed_input_observations(
        &self,
        invocation: bray_bound_tree::BoundExecutionSite,
        entry: Option<&crate::ExecutionCallEvidence>,
        invalidation: &mut StorageInvalidation,
    ) {
        use bray_bound_tree::{StorageSnapshotRoot, StorageValueConsumptionPhase};

        let Some(entry) = entry else { return; };

        let Some(expression) = invocation.expression() else { return; };

        let (stable, snapshots) = match invalidation {
            StorageInvalidation::All { stable_values, normal_snapshots, .. }
            | StorageInvalidation::Observed { stable_values, normal_snapshots, .. } => (stable_values, normal_snapshots),
            StorageInvalidation::Unchanged => return,
        };

        for &ordinal in entry.owned_inputs.iter() {
            let staged = self.storage.invocation_input(expression, ordinal)
                .and_then(|access| self.storage.logical_path(access));

            let captured = entry.argument_storage(ordinal)
                .and_then(bray_bound_tree::StorageCapturedValue::storage_coordinate);

            let Some(source) = staged.into_iter().chain(captured).find(|source| {
                self.storage.value_consumptions(invocation, StorageValueConsumptionPhase::Admission)
                    .any(|path| path == *source)
            })
            else { continue; };

            for (path, kind) in stable.range(source.clone()..).take_while(|(path, _)| source.contains(path)) {
                if source.contains_storage(path)
                    && (kind.is_immutable_inline() || *kind == bray_bound_tree::StorageObservationKind::Discriminant) {
                    snapshots.entry((StorageSnapshotRoot::Argument(ordinal), path.projections[source.projections.len()..].into()))
                        .or_insert(*kind);
                }
            }
        }
    }

    pub(super) fn opaque_observations(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        roots: impl IntoIterator<Item = Target>,
    ) -> StorageInvalidation {
        self.opaque_observations_with_escapes(memory, roots, &Default::default())
    }

    pub(super) fn opaque_observations_with_escapes(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        roots: impl IntoIterator<Item = Target>,
        escaped: &std::collections::BTreeSet<Target>,
    ) -> StorageInvalidation {
        let mut stable_descriptors = std::collections::BTreeSet::new();
        let mut stable_values = BTreeMap::new();

        let roots = roots.into_iter().chain(escaped.iter().cloned());

        for path in memory.unexposed_storage(&self.input.opaque_candidates, roots) {
            if self.input.opaque_descriptors.contains(path) {
                stable_descriptors.insert(path.clone());
            }

            if self.input.footprint.discriminants.get(&path.root)
                .is_some_and(|paths| paths.contains(path))
            {
                StorageObservationKind::Discriminant.record_preservation(
                    path.clone(), &mut stable_descriptors, &mut stable_values,
                );
            }

            if let Some(kind) = self.input.opaque_values.get(path) {
                // Reaching one payload does not expose disjoint scalar or marker fields.
                // Only the prepared immutable observations retain their values.
                kind.record_preservation(path.clone(), &mut stable_descriptors, &mut stable_values);
            }
        }

        // Ordinary statics cannot be replaced while their product or exact-thread
        // owner is retained. Interior mutation cannot reach their empty markers.
        let exposed = memory.raw_exposed_regions();

        let protected = |target: &Target| !exposed.iter().chain(escaped).any(|raw| {
            memory.target_relationship(raw, target) != StorageRelationship::Disjoint
        });

        let static_observation = |kind| matches!(kind, StorageObservationKind::Identity | StorageObservationKind::Marker(_));

        for (root, observations) in &self.input.footprint.owned_observations {
            if matches!(self.storage.identity(*root), Some(bray_bound_tree::StorageIdentity::Static(_)))
                && protected(&Target::storage(StoragePath::root(*root)))
            {
                stable_descriptors.insert(StoragePath::root(*root));

                for (path, observation) in observations {
                    if static_observation(*observation) {
                        observation.record_preservation(path.clone(), &mut stable_descriptors, &mut stable_values);
                    }
                }
            }
        }

        let mut stable_pointees = BTreeMap::new();

        // A returned view may be the only access to this exact static instance.
        for (target, ty) in memory.typed_storage().filter(|(target, _)| matches!(target.origin,
            bray_bound_tree::StorageOrigin::Static {
                root: bray_symbols::DependencySubjectRoot::StaticInstance(_, bray_symbols::StaticStorageDuration::Product),
                selection: Some(_), ..
            }))
        {
            for (path, observation) in self.input.footprint.typed_observations(ty) {
                if static_observation(observation) && protected(&target.project(path)) {
                    observation.record_pointee_preservation([target], ty, path, &mut stable_pointees);
                }
            }
        }

        StorageInvalidation::All {
            stable_descriptors,
            stable_values,
            stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees,
        }
    }

    pub(super) fn preserve_active_borrows(
        &self,
        state: &StorageFlowState,
        memory: &crate::analysis::storage_flow::memory::State,
        survives: &impl Fn(&Target, bray_bound_tree::StorageObservationKind, bray_symbols::TypeId, bool) -> bool,
        invalidation: &mut StorageInvalidation,
    ) {
        let mut references = Vec::new();

        let (stable_descriptors, stable_values, stable_pointees) = match invalidation {
            StorageInvalidation::All {
                stable_descriptors,
                stable_values,
                stable_pointees,
                ..
            }
            | StorageInvalidation::Observed {
                stable_descriptors,
                stable_values,
                stable_pointees,
                ..
            } => (stable_descriptors, stable_values, stable_pointees),
            StorageInvalidation::Unchanged => return,
        };

        let exposed = memory.raw_exposed_regions();

        for borrow in &state.definitely_active_borrows {
            let capability = self
                .storage
                .borrow_capability(*borrow)
                .expect("active borrow retains its plan");

            let referent = memory.borrow_observation(*borrow);

            let ty = self.storage.access(capability.access())
                .expect("active borrow retains its checked access").reached_type();

            let ty = self.request.semantic_values().unborrowed_type(ty);

            // Resolve the loan once. Historical or unknown backing cannot frame
            // a current owner, and one possible target cannot protect its siblings.
            let source = Source::Borrow(*borrow);

            let Some(targets) = source.regions(memory)
                .filter(|targets| !targets.is_empty() && targets.iter().all(|target| !target.previous)) else {
                continue;
            };

            let exact_targets = source.targets(memory).filter(|targets| targets.len() == 1).unwrap_or_default();

            let exposed_referent = targets.iter().any(|target| exposed.iter().any(|raw| {
                memory.target_relationship(raw, target) != StorageRelationship::Disjoint
            }));

            for path in self
                .storage
                .logical_path(capability.access())
                .into_iter()
                .chain(referent.as_ref())
            {
                if capability.kind() == bray_symbols::BorrowKind::Mutable {
                    for (value, observation) in self.input.footprint.observations(path) {
                        if observation.is_ordinary_inline()
                            && targets.iter().all(|target| {
                                survives(&target.project(&value.projections[path.projections.len()..]), observation, ty, false)
                            }) {
                            observation.record_preservation(value.clone(), stable_descriptors, stable_values);

                            observation.record_pointee_preservation(&exact_targets, ty, &value.projections[path.projections.len()..], stable_pointees);
                        }
                    }

                    if !exposed_referent {
                        for descriptor in self.input.footprint.discriminants.get(&path.root).into_iter().flatten().filter(|descriptor| {
                            descriptor.contains(path)
                                && matches!(path.projections.get(descriptor.projections.len()), Some(bray_bound_tree::StorageProjection::NullableValue | bray_bound_tree::StorageProjection::ActiveUnionPayloadField { .. }))
                        }) {
                            let target = descriptor.access_id(self.storage).and_then(|access| {
                                self.input.reference_target(self.request.semantic_values(), memory, self.storage, access)
                            });

                            if target.is_some_and(|target| !target.previous && survives(
                                &target, bray_bound_tree::StorageObservationKind::Discriminant, ty, false,
                            )) {
                                bray_bound_tree::StorageObservationKind::Discriminant.record_preservation(descriptor.clone(), stable_descriptors, stable_values);
                            }
                        }
                    }

                    continue;
                }

                let discriminants = self.input.footprint.discriminants.get(&path.root);
                let mut observed = false;

                if !exposed_referent {
                    for (value, observation) in self.input.opaque_values.range(path.clone()..).take_while(|(value, _)| path.contains(value)) {
                        if targets.iter().all(|target| {
                            survives(&target.project(&value.projections[path.projections.len()..]), bray_bound_tree::StorageObservationKind::Mutable, ty, true)
                        }) {
                            observation.record_preservation(value.clone(), stable_descriptors, stable_values);

                            observation.record_pointee_preservation(&exact_targets, ty, &value.projections[path.projections.len()..], stable_pointees);
                            observed = true;
                        }
                    }
                }

                for (value, observation) in self.input.footprint.observations(path) {
                    // Pointer descriptors belong to the shared owner, separately
                    // from the mutable storage they reach.
                    if observation == bray_bound_tree::StorageObservationKind::Address {
                        if targets.iter().all(|target| survives(
                            &target.project(&value.projections[path.projections.len()..]), observation, ty, true,
                        )) {
                            observation.record_preservation(value.clone(), stable_descriptors, stable_values);

                            observation.record_pointee_preservation(&exact_targets, ty, &value.projections[path.projections.len()..], stable_pointees);
                            references.extend(targets.iter().map(|target| (value.clone(), target.project(&value.projections[path.projections.len()..]))));
                            observed = true;
                        }

                        continue;
                    }

                    let survives = targets.iter().all(|target| survives(
                        &target.project(&value.projections[path.projections.len()..]), observation, ty, true,
                    ));

                    if survives && (!exposed_referent || observation == bray_bound_tree::StorageObservationKind::Identity || observation.is_immutable_inline()) {
                        observation.record_preservation(value.clone(), stable_descriptors, stable_values);

                        observation.record_pointee_preservation(&exact_targets, ty, &value.projections[path.projections.len()..], stable_pointees);

                        observed = true;
                    }
                }

                if exposed_referent || !observed && discriminants.is_none() { continue; }

                // The logical access can be an invocation input. Its exact physical
                // referent still retains the complete owner for this shared loan.
                stable_descriptors.insert(path.clone());

                for descriptor in discriminants
                        .into_iter()
                        .flatten()
                        .filter(|descriptor| path.contains(descriptor)) {
                    if targets.iter().all(|target| !target.previous && survives(
                        &target.project(&descriptor.projections[path.projections.len()..]),
                        bray_bound_tree::StorageObservationKind::Discriminant, ty, true,
                    )) {
                        bray_bound_tree::StorageObservationKind::Discriminant.record_preservation(descriptor.clone(), stable_descriptors, stable_values);

                        bray_bound_tree::StorageObservationKind::Discriminant.record_pointee_preservation(&exact_targets, ty, &descriptor.projections[path.projections.len()..], stable_pointees);
                    }
                }

            }
        }

        for (logical, physical) in references {
            invalidation.preserve_reference_slot(logical, physical);
        }
    }

    pub(super) fn preserve_shared_views(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        survives: &impl Fn(&Target, bray_bound_tree::StorageObservationKind, bray_symbols::TypeId) -> bool,
        slot_survives: &impl Fn(&Target) -> bool,
        invalidation: &mut StorageInvalidation,
    ) {
        let mut references = Vec::new();

        let exposed = memory.raw_exposed_regions();

        // Project views after active loans have established preservation of their
        // physical referents. A stored alias forwards those classified facts.
        // Raw exposure still governs payload and pointee observations.
        for (reference, (kind, ty, observations)) in &self.input.footprint.borrowed_observations {
            if *kind != bray_symbols::BorrowKind::Shared {
                continue;
            }

            let access = reference.access_id(self.storage);

            let locations = if let Some((slot, _, _)) = self.input.entry_references.get(&(reference.clone(), 1)) {
                memory.reference_locations(
                    self.request.semantic_values(), Target::storage(StoragePath::root(reference.root)), slot, None,
                )
            } else { match access {
                _ if reference.projections.is_empty() => Some(Target::storage(reference.clone())),
                Some(access) => self.input.reference_target(self.request.semantic_values(), memory, self.storage, access),
                None if self
                        .storage
                        .identity(reference.root)
                        .is_some_and(|identity| {
                            identity.is_invocation_owned(self.storage.kind())
                        }) =>
                {
                    Some(Target::storage(reference.clone()))
                }
                None => None,
            }.map(|location| vec![location]) };

            // A shared loan protects each possible referent. Exact pointer identity
            // is unnecessary, but unknown or retired backing supplies no protection.
            let Some(locations) = locations else {
                continue;
            };

            let [location] = locations.as_slice() else {
                continue;
            };

            // Classify the actual descriptor before following its shared referent.
            // The loan protects reached data, not replacement of its containing slot.
            if location.previous || !crate::analysis::storage_flow::memory::State::stable_reference_slot(&location, invalidation)
                && !slot_survives(&location) {
                continue;
            }

            invalidation.preserve_reference_slot(reference.clone(), location.clone());

            let (stable_descriptors, stable_values, stable_pointees) = match invalidation {
                StorageInvalidation::All { stable_descriptors, stable_values, stable_pointees, .. }
                | StorageInvalidation::Observed { stable_descriptors, stable_values, stable_pointees, .. } => (stable_descriptors, stable_values, stable_pointees),
                StorageInvalidation::Unchanged => return,
            };

            let source = Source::Value(location.clone());

            let Some(targets) = source.regions(memory) else {
                continue;
            };

            let exact = source.is_exact(memory);
            let exact_targets = source.targets(memory).filter(|targets| targets.len() == 1).unwrap_or_default();

            for (path, observation) in observations {
                // A whole-value frame must not hide a retired marker inside it.
                // Disjoint observations already survive the physical invalidation.
                let immutable_value = observation.is_immutable_inline();
                let address = *observation == bray_bound_tree::StorageObservationKind::Address;
                let identity = *observation == bray_bound_tree::StorageObservationKind::Identity;

                if !targets.is_empty()
                    && targets.iter().all(|target| {
                        let target =
                            target.project(&path.projections[reference.projections.len()..]);

                        if target.previous || !survives(&target, *observation, *ty) {
                            return false;
                        }

                        match &target.origin {
                            // These referents have stable identities, but no
                            // freshness claim that could exclude an exposed alias.
                            bray_bound_tree::StorageOrigin::Entry { .. }
                            | bray_bound_tree::StorageOrigin::Returned { .. }
                            | bray_bound_tree::StorageOrigin::Static { .. } => {
                                let framed = if address { stable_descriptors.contains(path) || stable_values.get(path) == Some(&bray_bound_tree::StorageObservationKind::Address) } else { stable_values.get(path).is_some_and(|kind| *kind == *observation || kind.is_immutable_inline()) };

                                framed || immutable_value || address || identity
                                    || !exposed.iter().any(|raw| {
                                        memory.target_relationship(raw, &target)
                                            != StorageRelationship::Disjoint
                                    })
                            }
                            _ => target
                                .storage_path()
                                .is_some_and(|path| stable_values.get(path).is_some_and(|kind| *kind == *observation || kind.is_immutable_inline())
                                    || address && stable_descriptors.contains(path)),
                        }
                    })
                {
                    observation.record_pointee_preservation(&exact_targets, *ty, &path.projections[reference.projections.len()..], stable_pointees);

                    // The copied logical path shares immutable projection metadata.
                    if address {
                        observation.record_preservation(path.clone(), stable_descriptors, stable_values);
                        references.extend(targets.iter().map(|target| (path.clone(), target.project(&path.projections[reference.projections.len()..]))));

                        if exact
                            && let Some(physical) = targets[0]
                                .project(&path.projections[reference.projections.len()..])
                                .observation_path()
                        {
                            bray_bound_tree::StorageObservationKind::Address.record_preservation(physical, stable_descriptors, stable_values);
                        }
                    } else {
                        observation.record_preservation(path.clone(), stable_descriptors, stable_values);
                    }
                }
            }
        }

        for (logical, physical) in references {
            invalidation.preserve_reference_slot(logical, physical);
        }
    }

    pub(super) fn preserve_store_values(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        invalidation: &mut StorageInvalidation,
        tags: bool,
        contents: bool,
    ) {
        let (stable_descriptors, stable_values) = match invalidation {
            StorageInvalidation::All { stable_descriptors, stable_values, .. }
            | StorageInvalidation::Observed { stable_descriptors, stable_values, .. } => (stable_descriptors, stable_values),
            StorageInvalidation::Unchanged => return,
        };

        let exposed = memory.raw_exposed_regions();

        if tags {
            // Tag-free stores run no replacement cleanup. Inexact aliasing
            // cannot turn one into a variant replacement.
            for descriptor in self.input
                    .footprint
                    .discriminants
                    .iter()
                    .filter(|(root, _)| {
                        let target = self
                            .storage
                            .root_access(**root)
                            .and_then(|access| {
                                self.input.reference_target(self.request.semantic_values(), memory, self.storage, access)
                            })
                            .unwrap_or_else(|| Target::storage(StoragePath::root(**root)));

                        !exposed.iter().any(|raw| {
                            memory.target_relationship(raw, &target)
                                != StorageRelationship::Disjoint
                        })
                    })
                    .flat_map(|(_, paths)| paths) {
                bray_bound_tree::StorageObservationKind::Discriminant.record_preservation(descriptor.clone(), stable_descriptors, stable_values);
            }
        }

        if contents {
            // Plain assignments and typed data stores cannot transfer empty
            // ownership markers, even when their destination has inexact aliases.
            stable_values.extend(self.input.footprint.empty_markers.keys().map(|path| (path.clone(), bray_bound_tree::StorageObservationKind::Value)));
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "reference observation keeps memory state, effects, invalidation and replacement identity explicit"
    )]
    pub(super) fn reference_observation(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        effects: &[Effect],
        invalidation: &mut StorageInvalidation,
        access: bray_bound_tree::StorageAccessId,
        (observed, pointee): (StoragePath, bray_symbols::TypeId),
        reference: &Source,
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

        let Some(targets) = reference.targets(memory) else {
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

        // A preserved reference observes the target's updated components. Replacing
        // its whole snapshot would also revoke evidence about untouched siblings.
        // The target effects above already identify every changed component.
        if replaced {
            let paths = accesses.entry(access).or_default();

            if !paths.contains(&observed) {
                paths.push(observed.clone());
            }
        }

        snapshots.insert(observed, StorageObservedTarget::borrowed(target.clone(), pointee));
    }

    pub(super) fn resolved_observations(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        effects: &[Effect],
        roots: impl IntoIterator<Item = Target> + Clone,
        escaped: &std::collections::BTreeSet<Target>,
    ) -> StorageInvalidation {
        use bray_bound_tree::StorageObservationTransition;

        let transition = if effects.iter().all(Effect::preserves_storage_lifetimes) {
            StorageObservationTransition::Contents
        } else {
            StorageObservationTransition::Unknown
        };

        if effects.iter().any(Effect::may_reach_all_inputs) {
            return self.opaque_observations_with_escapes(memory, roots, escaped)
                .with_observation_transition(transition);
        }

        let atomic = if effects.iter().any(Effect::is_atomic) {
            let frame = self.input.footprint.atomic.as_ref()
                .expect("atomic effects prepare their observation frame");

            let targets = if effects.contains(&Effect::Atomic) {
                self.opaque_observations_with_escapes(memory, roots.clone(), escaped)
            } else {
                self.target_observations(memory, effects.iter()
                    .filter_map(Effect::mutation)
                    .filter(|(_, kind)| *kind == bray_symbols::DependencyMutationKind::Atomic)
                    .map(|(target, _)| target))
            };

            Some(crate::storage::observation::restrict(targets, frame))
        } else {
            None
        };

        let ordinary = effects.iter().filter_map(Effect::mutation)
            .filter(|(_, kind)| *kind != bray_symbols::DependencyMutationKind::Atomic)
            .map(|(target, _)| target);

        let ambient = effects.iter().any(Effect::is_ambient);

        if !ambient && ordinary.clone().next().is_none() {
            return match atomic {
                Some(invalidation) => invalidation.with_observation_transition(transition),
                None => self.target_observations(memory, ordinary)
                    .with_observation_transition(StorageObservationTransition::Unchanged),
            };
        }

        let mut invalidation = if ambient {
            // Atomic access does not expose ordinary input storage. Keep its
            // protected write domain separate from ambient and ordinary writes.
            let inputs = effects.contains(&Effect::Escape(None));

            self.opaque_observations_with_escapes(memory, roots.into_iter().filter(|_| inputs)
                .chain(ordinary.cloned()), escaped)
        } else {
            self.target_observations(memory, ordinary)
        };

        if let Some(atomic) = atomic {
            invalidation.merge(&atomic);
        }

        invalidation.with_observation_transition(transition)
    }

    pub(super) fn cleanup_invalidation(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        effects: Option<&[Effect]>,
        receiver: Option<&Target>,
    ) -> StorageInvalidation {
        if receiver.is_none()
            && effects.is_none_or(|effects| {
                effects
                    .iter()
                    .any(Effect::may_reach_all_inputs)
            })
        {
            return StorageInvalidation::unknown();
        }

        match effects {
            Some(effects) => self.resolved_observations(memory, effects, receiver.cloned(), &Default::default()),
            None => self.opaque_observations(memory, receiver.cloned()),
        }
    }

    pub(super) fn completed_references(
        &mut self,
        state: &StorageFlowState,
        operation: crate::analysis::model::AnalysisOperationKind,
    ) -> Option<bray_bound_tree::StorageCompletion> {
        use crate::analysis::model::{AnalysisCallPhase, AnalysisOperationKind};

        let occurrence = operation.occurrence();
        let contracts = self.dependencies.calls().get(&occurrence);

        if !matches!(
            operation,
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Completion,
                ..
            } | AnalysisOperationKind::Result {
                phase: crate::analysis::model::AnalysisResultPhase::Completion,
                ..
            }
        ) && self.execution.single_phase_invocation(operation).is_none()
        {
            return None;
        }

        let mut observations = BTreeMap::new();
        let entry = state.execution.as_ref().and_then(|execution| execution.admitted_call(occurrence));

        // Read the current referent through the descriptor admitted at input evaluation.
        for (subject, ty, _) in contracts.into_iter().flat_map(|call| call.storage().subjects()) {
            let Some(pointee) = crate::representation::borrow_target_type(self.request.semantic_values(), ty) else { continue; };

            let Some(observed) = self.invocation_logical_path(occurrence, subject) else {
                continue;
            };

            let Some(sources) = self.admitted_reference_sources(state, occurrence, entry, subject) else {
                continue;
            };

            let [source] = sources.as_slice() else {
                continue;
            };

            let Some(targets) = source.targets(&state.memory) else {
                continue;
            };

            let [target] = targets.as_slice() else {
                continue;
            };

            if !target.is_exact() {
                continue;
            }

            observations.insert(observed, StorageObservedTarget::borrowed(target.clone(), pointee));
        }

        if let Some(result) = self.invocation_result(occurrence) {
            for (path, ty) in self.input.footprint.references_below(&result) {
                let Some(pointee) = crate::representation::borrow_target_type(self.request.semantic_values(), ty) else { continue; };

                if state.no_payload_paths().any(|absent| absent.contains(path)) { continue; }

                let Some(targets) = state.memory.value_targets(&Target::storage(path.clone())) else { continue; };

                if let [target] = targets.as_slice() && target.is_exact() {
                    observations.insert(path.clone(), StorageObservedTarget::borrowed(target.clone(), pointee));
                }
            }
        }

        Some(bray_bound_tree::StorageCompletion::new(observations))
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
                transition: Default::default(),
                scope: mutations
                    .clone()
                    .map(|target| {
                        target.mutation_scope(self.storage, self.request.semantic_values())
                    })
                    .max()
                    .unwrap_or(bray_bound_tree::StorageMutationScope::Invocation),
                accesses: observations,
                snapshots,
                affected_pointees: Some(BTreeMap::new()),
                referents,
                stable_descriptors,
                stable_values: BTreeMap::new(),
                stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
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

        for (target, targets, unknown) in memory.observed_references() {
            let Some(slot) = (target).value_observation_path() else {
                continue;
            };

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

        let candidates = self.local_observation_accesses(mutations.clone());
        let all_accesses = candidates.is_none().then(|| self.storage.access_entries().map(|(access, _)| access));

        for access in candidates.into_iter().flatten().chain(all_accesses.into_iter().flatten()) {
            let Some(logical) = self.storage.logical_path(access) else {
                continue;
            };

            let access_data = self.storage.access(access).expect("observation indexes retain checked accesses");
            let direct = self.input.direct_reference_target(self.storage, access);
            let indirect;

            let observed = if let Some(direct) = &direct {
                Some(std::slice::from_ref(direct))
            } else {
                indirect = self.input.reference_targets(self.request.semantic_values(), memory, self.storage, access);

                indirect.as_deref()
            };

            // Stored borrows observe their reached contents. Borrow-capability routes
            // already name their target. Raw-address accesses observe descriptor slots.
            let pointee = if matches!(access_data.root(), bray_bound_tree::StorageAccessRoot::Storage(_)) {
                crate::representation::borrow_target_type(self.request.semantic_values(), access_data.reached_type())
            } else { None };

            let referents;

            let observed = if pointee.is_some() {
                let Some(slots) = observed else {
                    continue;
                };

                // The access names a stored loan. A pointee write changes reached
                // data, while a disjoint descriptor retains the evaluated loan.
                if slots.iter().all(|slot| mutations.clone().all(|mutation| {
                    memory.target_relationship(slot, mutation) == StorageRelationship::Disjoint
                })) {
                    stable_descriptors.insert(logical.clone());
                }

                let Some(targets) = slots.iter().map(|slot| memory.value_targets(slot)).collect::<Option<Vec<_>>>() else {
                    continue;
                };

                let mut targets = targets.into_iter().flatten().collect::<Vec<_>>();

                targets.sort();
                targets.dedup();

                referents = targets;

                Some(referents.as_slice())
            } else {
                observed
            };

            self.stable_descriptors(
                memory,
                access,
                logical.root,
                mutations.clone(),
                &mut stable_descriptors,
            );

            if let Some(observed) = &observed {
                unrepresented.retain(|mutation| !observed.iter().any(|target| target.contains(mutation)));
            }

            let mut changed = Vec::new();

            for mutation in mutations.clone() {
                // Possible origin bounds revoke evidence independently. They do
                // not establish a particular new snapshot through another loan.
                if pointee.is_some() && !observed.as_ref().is_some_and(|observed| observed.iter().any(|target| target.contains(mutation))) {
                    continue;
                }

                if observed.as_ref().is_some_and(|observed| {
                    observed.iter().all(|target| memory.target_relationship(target, mutation) == StorageRelationship::Disjoint)
                }) {
                    continue;
                }

                // A known enclosing target preserves the unaffected components. Other
                // possible aliases lose the entire observation, never acquire equality.
                let unique = observed.and_then(|targets| match targets {
                    [target] => Some(target),
                    _ => None,
                });

                let suffix = observed.as_ref().and_then(|targets| {
                    let mut suffix = None;

                    for target in targets.iter().filter(|target| {
                        memory.target_relationship(target, mutation) != StorageRelationship::Disjoint
                    }) {
                        if !target.contains(mutation) {
                            return None;
                        }

                        let relative = &mutation.projections()[target.projections().len()..];

                        if suffix.is_some_and(|previous| previous != relative) {
                            return None;
                        }

                        suffix = Some(relative);
                    }

                    suffix
                }).unwrap_or_default();

                let path = observable_path(
                    logical.root,
                    logical.projections.iter().chain(suffix).copied(),
                );

                if unique.is_some_and(|observed| observed.contains(mutation))
                    && path.projections.len() == logical.projections.len() + suffix.len()
                {
                    // An enclosing invalidation is not a value alias of protected
                    // contents. Keep their precise target with the mutation only.
                    snapshots.insert(path.clone(), StorageObservedTarget {
                        target: mutation.clone(),
                        pointee: if suffix.is_empty() { pointee } else { None },
                    });
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
            let affected_pointees = memory.affected_pointee_paths(mutations.clone());

            StorageInvalidation::Observed {
                transition: Default::default(),
                scope: mutations
                    .clone()
                    .map(|target| {
                        target.mutation_scope(self.storage, self.request.semantic_values())
                    })
                    .max()
                    .unwrap_or(bray_bound_tree::StorageMutationScope::Invocation),
                accesses: observations,
                snapshots,
                affected_pointees: Some(affected_pointees),
                referents,
                stable_descriptors,
                stable_values: BTreeMap::new(),
                stable_reference_slots: Default::default(), normal_snapshots: Default::default(), stable_pointees: Default::default(),
            }
        } else {
            self.opaque_observations(memory, mutations.cloned())
        }
    }

    fn local_observation_accesses<'target>(
        &self,
        mutations: impl Iterator<Item = &'target Target>,
    ) -> Option<Vec<bray_bound_tree::StorageAccessId>> {
        use crate::analysis::storage_flow::memory::Origin;

        let mut roots = std::collections::BTreeSet::new();

        for mutation in mutations {
            match &mutation.origin {
                Origin::Storage(path) if !matches!(
                    self.storage.identity(path.root), Some(bray_bound_tree::StorageIdentity::Static(_))
                ) => { roots.insert(path.root); }
                Origin::Allocation { .. } => {}
                Origin::Storage(_) | Origin::Static { .. } | Origin::Entry { .. } | Origin::Returned { .. } => return None,
            }
        }

        // Only direct slots on the same storage origin can overlap these
        // mutations. Indirect routes can still reach any of those origins.
        let mut accesses = self.input.indirect_observation_accesses.clone();

        for root in roots {
            accesses.extend(self.input.direct_observation_accesses.get(&root).into_iter().flatten());
        }

        accesses.sort_unstable();

        Some(accesses)
    }

    fn stable_descriptors<'target>(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        access: bray_bound_tree::StorageAccessId,
        root: bray_bound_tree::StorageIdentityId,
        mutations: impl Iterator<Item = &'target Target> + Clone,
        descriptors: &mut std::collections::BTreeSet<StoragePath>,
    ) {
        let Some((_, path)) = self.storage.reference_route(access) else {
            return;
        };

        for (step, index, ty, _) in path.reads() {
            if !matches!(
                self.request.semantic_values().type_data(ty).as_ref(),
                bray_symbols::TypeData::Slice(_)
            ) {
                continue;
            }

            let descriptor = path.prefix(index, step);

            let Some(locations) =
                memory.reference_locations(
                    self.request.semantic_values(),
                    Target::storage(StoragePath::root(root)),
                    &descriptor,
                    None,
                )
            else {
                continue;
            };

            if locations.iter().all(|location| mutations.clone().all(|mutation| {
                memory.target_relationship(location, mutation) == StorageRelationship::Disjoint
            })) {
                descriptors.insert(StoragePath {
                    root,
                    projections: path.projections()[..index].into(),
                });
            }
        }
    }
}

fn retained_capture(
    memory: &crate::analysis::storage_flow::memory::State,
    capture: &bray_bound_tree::StorageCapturedValue,
    effects: Option<&[Effect]>,
) -> bool {
    let Some(effects) = effects else { return false; };

    if effects.iter().any(|effect| effect.is_ambient() || effect.is_atomic() || effect.may_reach_all_inputs()) {
        return false;
    }

    let Some(regions) = capture.complete_carried_regions() else { return false; };

    let (reached, unknown) = memory.transitive_regions(regions, |region, slot| region.contains_storage(slot));

    !unknown && effects.iter().filter_map(Effect::mutation).all(|(changed, _)| {
        reached.iter().all(|region| memory.target_relationship(region, changed) == StorageRelationship::Disjoint)
    })
}

fn observable_path(
    root: bray_bound_tree::StorageIdentityId,
    projections: impl Iterator<Item = bray_bound_tree::StorageProjection>,
) -> StoragePath {
    // A region can change the enclosing observation without naming a value
    // component. Protected bytes likewise use their Uninit owner's observation.
    StoragePath {
        root,
        projections: projections
            .take_while(|projection| {
                projection.is_exact() && *projection != StorageProjection::UninitPayload
            })
            .collect(),
    }
}
