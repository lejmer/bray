use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use bray_bound_tree::{
    StorageAccessId, StorageCleanupAction, StorageCleanupActionKind, StorageEffectRow,
    StorageInvalidation, StorageMutation, StoragePath,
};
use bray_symbols::DependencySubjectRoot;

use super::cleanup::CleanupOperands;
use super::core::StorageFlowCollector;
use super::effect_rows::{EffectRows, SelectedEffectRows};
use crate::CheckerRequestContext;
use crate::analysis::guarantee::{CleanupMutation, ExecutionState};
use crate::analysis::model::AnalysisOperationKind;
use crate::analysis::storage_flow::model::StorageFlowState;
use crate::analysis::storage_flow::memory::{Effect, EffectOutcomes, State, Target};

struct ReceiverWrites {
    input: Arc<bray_bound_tree::StorageCapturedValue>,
    transition: Arc<bray_bound_tree::StorageValueTransition>,
    values: BTreeMap<bray_bound_tree::StorageValueDestination, crate::ExecutionCondition>,
}

fn apply_lifecycle_cell_changes(memory: &mut State, mutation: StorageMutation, rows: &EffectRows, completes: bool) {
    let occurrence = mutation.occurrence();

    match mutation {
        StorageMutation::CleanupFailure { kind, row: Some(row), .. } => {
            if let Some(cells) = rows.get(&StorageEffectRow::AbnormalCells(kind, row)) {
                let cells = memory.checked_cell_changes(cells);

                memory.apply_cell_changes(cells, occurrence);
            }
        }
        StorageMutation::CleanupOutcome { row, .. } if completes => {
            if let Some(cells) = rows.get(&StorageEffectRow::NormalCells(row)) {
                memory.stage_cell_changes(occurrence, cells);
            }

            memory.complete_transition(occurrence);
        }
        StorageMutation::Cleanup(_) if completes => memory.complete_transition(occurrence),
        _ => {}
    }
}

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    fn opaque_cleanup_effects(
        &self,
        memory: &State,
        receiver: Option<&Target>,
        ty: bray_symbols::TypeId,
        task: Option<&crate::ExecutionDeferredValue>,
        inactive: bool,
        proof: &crate::execution_guarantees::ExecutionProof<'_>,
    ) -> Vec<Effect> {
        task.and_then(|frame| {
            if inactive {
                return self.inactive_frame_cleanup(memory, frame, proof);
            }

            super::effects::task_effects(frame.effects(proof), frame.cleanup_effects(true, proof)?)
                .map(|effects| effects.possible().to_vec())
        })
        .or_else(|| self.buffer_cleanup_effects(memory, receiver?, ty))
        .unwrap_or_else(|| memory.cleanup_effects(self.request.semantic_values(), receiver, ty))
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn buffer_cleanup_effects(
        &self,
        memory: &State,
        receiver: &Target,
        ty: bray_symbols::TypeId,
    ) -> Option<Vec<Effect>> {
        let effects = self.input.buffer_cleanup_effects.get(&ty)?;
        let storage = &self.input.buffer_cleanup_storage[&ty];

        Some(self.resolve_cleanup_effects(memory, Some(receiver), false, effects, storage))
    }

    fn resolve_cleanup_effects(
        &self,
        memory: &State,
        receiver: Option<&Target>,
        receiver_borrow_reached: bool,
        effects: &[bray_symbols::DependencyStorageEffect],
        storage: &bray_bound_tree::BoundCallStorage,
    ) -> Vec<Effect> {
        let values = self.request.semantic_values();

        self.resolve_effects(memory, storage, (effects, None, None),
            || receiver.map(|target| [target.clone()].into()), |subject, path, indirect| {
            let start = self.cleanup_subject_root(receiver, subject)?;

            if path.is_none() && !subject.projections().is_empty() { return None; }

            let reached_receiver = receiver_borrow_reached
                && subject.subject_root() == DependencySubjectRoot::Receiver
                && subject.projections().is_empty();

            memory.reference_effect_targets(values, start, path, indirect && !reached_receiver)
        }, |_, _, _, _| None)
    }

    pub(super) fn cleanup_receiver(
        &self,
        memory: &State,
        access: StorageAccessId,
        route: Option<&bray_bound_tree::StorageReferencePath>,
        replacement: Option<(bray_bound_tree::BoundExpressionId, StorageAccessId)>,
    ) -> Option<Target> {
        let route = route?;

        let reached = if let Some((assignment, _)) = replacement.filter(|&(_, destination)| destination == access) {
            self.assignment_destination(memory, assignment, access)?.address.single_location()?.clone()
        } else {
            self.input.reference_target(self.request.semantic_values(), memory, self.storage, access)?
        };

        let locations = memory.reference_locations(self.request.semantic_values(), reached, route, None)?;

        let [receiver] = locations.as_slice() else { return None; };

        Some(receiver.clone())
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn cleanup_subject_locations(
        &self,
        memory: &State,
        receiver: Option<&Target>,
        subject: &bray_symbols::DependencySubject,
        path: Option<&bray_bound_tree::StorageReferencePath>,
    ) -> Option<Vec<Target>> {
        let start = self.cleanup_subject_root(receiver, subject)?;

        if let Some(path) = path {
            memory.reference_locations(self.request.semantic_values(), start, path, None)
        } else {
            subject.projections().is_empty().then_some(vec![start])
        }
    }

    fn cleanup_subject_root(&self, receiver: Option<&Target>, subject: &bray_symbols::DependencySubject) -> Option<Target> {
        if subject.subject_root() == DependencySubjectRoot::Receiver {
            receiver.cloned()
        } else {
            self.static_location(&bray_symbols::DependencySubject::root(subject.subject_root()))
        }
    }

    pub(super) fn lifecycle_effects(
        &self,
        memory: &State,
        receiver: Option<&Target>,
        call: &crate::analysis::guarantee::LifecycleInvocation<'_>,
    ) -> (EffectOutcomes, EffectRows) {
        let storage = &self.input.lifecycle_storage[&call.lifecycle.call.callable];

        let receiver_borrow_reached = crate::representation::borrow_target_type(
            self.request.semantic_values(), call.lifecycle.call.receiver,
        ).is_some();

        let mut selection = SelectedEffectRows::new(!call.contract.captured_values().is_empty());

        selection.select_failures(call.contract.failure_outcomes(), |row| call.entry.admits_failure(row), |_| true);

        for (index, row, _) in &call.outcomes {
            selection.select_normal(*index, row, true);
        }

        selection.bind(call.contract.effect_outcomes(), |effect| {
            self.resolve_cleanup_effects(memory, receiver, receiver_borrow_reached, std::slice::from_ref(effect), storage)
        })
    }

    pub(super) fn apply_cleanup_action(
        &mut self,
        state: &mut StorageFlowState,
        execution: &mut ExecutionState,
        operation: AnalysisOperationKind,
        identity: StorageMutation,
        mutation: &CleanupMutation<'_>,
        operands: &CleanupOperands<'_>,
    ) {
        let domain = self.execution;
        let publish = self.publish;

        let failure = match identity {
            StorageMutation::CleanupFailure { kind, row, .. } => Some((kind, row)),
            _ => None,
        };

        let normal = match identity {
            StorageMutation::CleanupOutcome { row, .. } => Some(row),
            _ => None,
        };

        let receiver = operands.receiver.as_ref();

        self.record_capture_retirements(execution, identity, bray_bound_tree::StorageValueBoundary::Execution, state.memory.take_retirements());

        if let CleanupMutation::Lifecycle(call) = mutation {
            self.collect_lifecycle_reports(state, operation, call, normal, failure);
        }

        let invocation = match mutation {
            CleanupMutation::Lifecycle(call) if failure.is_none() => Some(call),
            _ => None,
        };

        let mut publication = match mutation {
            CleanupMutation::Lifecycle(call) => receiver.and_then(|receiver| {
                self.bind_receiver_writes(&state.memory, execution, identity, call, receiver)
            }),
            _ => None,
        };

        let mut references = invocation.map(|call| {
            self.input.lifecycle_storage[&call.lifecycle.call.callable]
                .reference_outputs(call.normal_outcomes(normal))
        }).unwrap_or_default();

        let task_path = operands.task_path.as_deref();
        let task_source = operands.frame.as_deref();
        let inactive_captures = operands.inactive_captures.as_deref();

        let broadcasts = matches!(identity, StorageMutation::Cleanup(StorageCleanupAction { kind: StorageCleanupActionKind::Broadcast, .. }));

        let inactive_broadcast = inactive_captures.is_some()
            && broadcasts
            && task_path.is_some_and(|path| {
                state.memory
                    .unexposed_storage(std::slice::from_ref(path), [])
                    .next()
                    .is_some()
            });

        let broadcast_frames = if broadcasts {
            task_path
                .into_iter()
                .flat_map(|path| self.unexposed_deferred_paths(&state.memory, execution, path))
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        let mut effects = self.selected_cleanup_effects(&state.memory, execution, identity, mutation, operands);

        let inherited_tail =
            self.record_cleanup_obligations(operation, mutation, effects.as_deref());

        if let CleanupMutation::Opaque { ty, access, .. } = mutation
            && let Some((requirements, pending)) = self.inactive_cleanup_report(
                operation, *ty, inactive_captures, inherited_tail,
            )
            && let Some(report) = self.cleanup_report_snapshot(state, *access, task_path, &requirements)
        {
            state.retain_reports(self.report_boundary, report, pending);
        }

        if let Some(frame) = task_source && !operands.inactive && !broadcasts {
            let (output, row) = failure.map_or(
                (bray_bound_tree::CallReportOutput::DeferredCompleted, None),
                |(kind, row)| (bray_bound_tree::CallReportOutput::DeferredAbnormal(Some(kind)), row),
            );

            if let Some(report) = domain.captured_frame_reports(execution, frame, output, row) {
                state.retain_reports(self.report_boundary, report, failure.is_none() && operation.is_normal_cleanup());
            }
        }

        if inherited_tail && matches!(operation, AnalysisOperationKind::ScopeExit {
            phase: crate::analysis::model::AnalysisScopeExitPhase::TaskCancellationBroadcast, ..
        }) {
            // Broadcast work can affect external state without invoking the receiver's destructor.
            effects = Some(Cow::Owned(vec![Effect::Unknown, Effect::Escape(None)]));
        }

        let escaped = self.escape_effects(
            &state.memory,
            effects.as_deref(),
            operands.receiver.as_slice(),
            false,
            (operation.occurrence(), Some(&mut state.effect_prefix)),
        );

        // Remaining receiver parts use the selected tail disposition. Their
        // ordinary finalizers execute after the body, while abandonment omits them.
        if !inherited_tail {
            self.check_terminal_effects(
                state,
                operation,
                effects.as_deref(),
                receiver.into_iter().cloned(),
                &escaped,
            );
        }

        state.memory.escape_regions(escaped, Some(operation.occurrence()));

        let mut invalidation =
            self.cleanup_invalidation(&state.memory, effects.as_deref(), receiver);

        if (task_source.is_some() || inactive_broadcast)
            && let Some(path) = task_path
            && let Some(effects) = &effects
        {
            preserve_run_descriptor(
                &state.memory,
                path,
                effects,
                inactive_broadcast,
                &mut invalidation,
            );
        }

        // Broadcast exposes captures, not the previously unexposed run handles.
        for frame in &broadcast_frames {
            preserve_run_descriptor(&state.memory, frame, &[], true, &mut invalidation);
        }

        self.preserve_storage_values(state, (operation.occurrence(), None), effects.as_deref(), &mut invalidation);

        if let Some(call) = &invocation
            && call.completes
        {
            let invocation = super::references::InvocationOutputs::Lifecycle {
                storage: &self.input.lifecycle_storage[&call.lifecycle.call.callable],
                receiver,
                path: call.path,
            };

            self.stage_invocation_outputs(
                state, operation.occurrence(), invocation, &mut references, call.normal_outcomes(normal), None,
            );
        }

        let performed = effects.as_deref().unwrap_or(&[Effect::Unknown]);

        self.check_raw_retirements(state, operation.occurrence(), performed);
        self.record_resolved_effects(&state.memory, performed, Some(&mut state.effect_prefix));
        state.memory.apply_effects(Some(performed), &invalidation, operation.occurrence());

        if let Some((_, rows)) = &operands.lifecycle_effects {
            apply_lifecycle_cell_changes(&mut state.memory, identity, rows, invocation.is_some_and(|call| call.completes));
        }

        let retirements = state.memory.take_retirements();

        if !retirements.is_empty() && let Some(writes) = &mut publication {
            let mut retire = crate::ExecutionCondition::retiring_storage(&retirements);

            for value in writes.values.values_mut() {
                *value = retire(value);
            }
        }

        self.record_capture_retirements(execution, identity, bray_bound_tree::StorageValueBoundary::Execution, retirements);

        let mut completion = if let (Some(call), Some(effects)) = (&invocation, &effects) {
            self.cleanup_observations(&state.memory, receiver, call, &references, effects, &mut invalidation)
        } else {
            bray_bound_tree::StorageCompletion::default()
        };

        state.invalidate_observed_absence(&invalidation, &self.input.footprint.discriminants);
        domain.invalidate_observations(execution, identity, &invalidation);
        self.record_outcome_mutation(identity, Cow::Owned(invalidation));
        domain.refresh_storage_observations(execution, completion.references());

        if let Some(writes) = publication {
            self.publish_receiver_writes(state, execution, identity, writes, &mut completion);
        }

        self.record_capture_retirements(execution, identity,
            bray_bound_tree::StorageValueBoundary::Retirement, state.memory.take_retirements());

        if publish && (invocation.is_some() || completion.lifecycle_receiver().is_some()) {
            self.completions.record(self.block(), identity, completion);
        }

    }

    fn selected_cleanup_effects<'effects>(
        &self,
        memory: &State,
        execution: &ExecutionState,
        identity: StorageMutation,
        mutation: &CleanupMutation<'_>,
        operands: &'effects CleanupOperands<'_>,
    ) -> Option<Cow<'effects, [Effect]>> {
        match mutation {
            CleanupMutation::Unchanged => Some(Cow::Borrowed(&[])),
            CleanupMutation::Recovery => None,
            CleanupMutation::Repeat(_) => unreachable!("a repeated region has no separate physical effect"),
            CleanupMutation::Opaque { ty, .. } => Some(Cow::Owned(self.opaque_cleanup_effects(
                memory, operands.receiver.as_ref(), *ty, operands.frame.as_deref(),
                operands.inactive, &execution.ordinary_proof(),
            ))),
            CleanupMutation::Lifecycle(_) => {
                let (outcomes, prefixes) = operands.lifecycle_effects.as_ref()
                    .expect("lifecycle outcomes share one admitted receiver");

                let effects = match identity {
                    StorageMutation::CleanupFailure { kind, row, .. } => row.and_then(|row| prefixes.get(&StorageEffectRow::Abnormal(kind, row)))
                        .map_or_else(|| outcomes.failure(kind), Arc::as_ref),
                    StorageMutation::CleanupOutcome { row, .. } => prefixes[&StorageEffectRow::Normal(row)].as_ref(),
                    _ => outcomes.normal(),
                };

                Some(Cow::Borrowed(effects))
            }
        }
    }

    fn bind_receiver_writes(
        &self,
        memory: &State,
        execution: &ExecutionState,
        mutation: StorageMutation,
        call: &crate::analysis::guarantee::LifecycleInvocation<'_>,
        receiver: &Target,
    ) -> Option<ReceiverWrites> {
        if !receiver.is_exact() { return None; }

        let storage = &self.input.lifecycle_storage[&call.lifecycle.call.callable];
        let recipes = self.execution.cleanup_receiver_writes(call.contract, call.entry, mutation);

        let writes = recipes.into_iter().filter_map(|(subject, value)| {
            let (ty, target) = if subject.projections().is_empty() {
                (call.ty, receiver.clone())
            } else {
                let (ty, path) = storage.subject(subject)?;

                let locations = memory.reference_locations(self.request.semantic_values(), receiver.clone(), path, None)?;

                let [target] = locations.as_slice() else { return None; };

                if !target.is_exact() { return None; }

                (ty, target.clone())
            };

            let destination = bray_bound_tree::StorageValueDestination::Current { target, ty, installed: false };

            Some((destination, self.execution.storage_value_expression(value)))
        });

        let transition = bray_bound_tree::StorageValueTransition::new(None, writes);

        if transition.outcome.bindings.is_empty() { return None; }

        let input = memory.capture_reached_value(receiver)
            .with_observations(self.input.footprint.typed_observations(call.ty));

        let values = self.execution.bind_cleanup_receiver_outputs(execution, call.entry, mutation, &input, &transition);

        Some(ReceiverWrites { input: Arc::new(input), transition: Arc::new(transition), values })
    }

    fn publish_receiver_writes(
        &self,
        state: &mut StorageFlowState,
        execution: &mut ExecutionState,
        mutation: StorageMutation,
        writes: ReceiverWrites,
        completion: &mut bray_bound_tree::StorageCompletion,
    ) {
        completion.record_lifecycle_receiver(writes.input, writes.transition);

        for (destination, value) in writes.values {
            let bray_bound_tree::StorageValueDestination::Current { target, ty, .. } = &destination else {
                unreachable!("cleanup publishes a reached receiver slot");
            };

            let value = self.execution.complete_captured_references(execution, mutation, value);

            if let Some(path) = target.storage_path() {
                state.initialize_path(path);
            }

            self.record_current_write(state, target);
            self.publish_value_graph(state, target.clone(), value.clone(), super::references::ValuePublication::ReceiverWrite);

            if let Some(path) = target.storage_path() {
                self.observe_transferred_value(state, Some(&value), path, *ty);
            }

            let snapshot = Arc::new(state.memory.capture_reached_value(target)
                .with_observations(self.input.footprint.typed_observations(*ty)));

            self.execution.publish_current_value(execution, target, *ty, value.captured_value(snapshot.clone()));
            completion.publish(destination, snapshot);
        }
    }

    fn unexposed_deferred_paths(&self, memory: &State, execution: &crate::analysis::guarantee::ExecutionState,
        root: &bray_bound_tree::StoragePath,
    ) -> Vec<bray_bound_tree::StoragePath> {
        let mut pending = vec![(root.clone(), self.execution.storage_path_value(execution, root))];
        let mut frames = Vec::new();

        while let Some((path, value)) = pending.pop() {
            if value.is_reference() { continue; }

            if value.deferred_value().is_some()
                && memory.unexposed_storage(std::slice::from_ref(&path), []).next().is_some()
            { frames.push(path.clone()); }

            for projection in value.component_projections() {
                if !bray_bound_tree::StorageProjection::remains_in_allocation(&[projection]) { continue; }

                let mut child = path.clone();

                child.projections = path.projections.iter().copied().chain([projection]).collect();
                pending.push((child, value.clone().project(projection)));
            }
        }

        frames
    }

    fn cleanup_observations(
        &self,
        memory: &State,
        receiver: Option<&Target>,
        call: &crate::analysis::guarantee::LifecycleInvocation<'_>,
        references: &[bray_bound_tree::BoundCallReference],
        effects: &[Effect],
        invalidation: &mut StorageInvalidation,
    ) -> bray_bound_tree::StorageCompletion {
        let mut observations = std::collections::BTreeMap::new();

        let (Some(path), Some(receiver)) = (call.path, receiver) else {
            return bray_bound_tree::StorageCompletion::default();
        };

        let storage = &self.input.lifecycle_storage[&call.lifecycle.call.callable];

        for reference in references {
            let relation = reference.relation();

            if !relation.is_exact()
                || relation.kind() != bray_symbols::DependencyReferenceKind::Borrow
                || relation.dereferences() != 0
                || !relation.target_projections().is_empty()
                || relation.exit().subject_root() != DependencySubjectRoot::Receiver
                || (!call.completes && relation.entry() != Some(relation.exit()))
            {
                continue;
            }

            let Some((ty, prepared)) = storage.subject(relation.exit())
            else {
                continue;
            };

            let pointee = crate::representation::borrow_target_type(self.request.semantic_values(), ty)
                .expect("a checked borrow relation binds a borrow type");

            let Some(locations) = memory.reference_locations(self.request.semantic_values(), receiver.clone(), prepared, None) else {
                continue;
            };

            let observed = StoragePath {
                root: path.root,
                projections: path
                    .projections
                    .iter()
                    .chain(prepared.projections())
                    .copied()
                    .collect(),
            };

            for location in locations {
                self.reference_observation(
                    memory,
                    effects,
                    invalidation,
                    call.access,
                    (observed.clone(), pointee),
                    &crate::analysis::storage_flow::memory::Source::Value(location),
                    call.completes && relation.entry() != Some(relation.exit()),
                );
            }
        }

        // A normal destructor can establish facts about a borrowed field even
        // when it consumes that field instead of forwarding it into another value.
        if call.completes {
            for (subject, ty, prepared) in storage.subjects() {
                if subject.subject_root() != DependencySubjectRoot::Receiver {
                    continue;
                }

                let Some(pointee) = crate::representation::borrow_target_type(self.request.semantic_values(), ty) else { continue; };

                let Some(locations) = memory.reference_locations(self.request.semantic_values(), receiver.clone(), prepared, None) else {
                    continue;
                };

                let [location] = locations.as_slice() else {
                    continue;
                };

                let Some(targets) = memory.value_targets(location) else {
                    continue;
                };

                let [target] = targets.as_slice() else {
                    continue;
                };

                observations.insert(
                    StoragePath {
                        root: path.root,
                        projections: path
                            .projections
                            .iter()
                            .chain(prepared.projections())
                            .copied()
                            .collect(),
                    },
                    bray_bound_tree::StorageObservedTarget::borrowed(target.clone(), pointee),
                );
            }
        }

        bray_bound_tree::StorageCompletion::new(observations)
    }

}

fn preserve_run_descriptor(
    memory: &State,
    path: &StoragePath,
    effects: &[Effect],
    inactive: bool,
    invalidation: &mut StorageInvalidation,
) {
    let StorageInvalidation::All {
        stable_descriptors,
        stable_values,
        ..
    } = invalidation
    else {
        return;
    };

    if inactive {
        // The owner token was unexposed before capture effects were applied.
        // Broadcasting passes only captures to user code, never this token.
        stable_descriptors.insert(path.clone());
        stable_values.insert(path.clone(), bray_bound_tree::StorageObservationKind::Value);
        return;
    }

    if effects
        .iter()
        .any(|effect| matches!(effect, Effect::Escape(None))
            || matches!(effect, Effect::UnboundedMutation { kind, .. } if kind.cleanup_type().is_some()))
    {
        return;
    }

    if memory
        .unexposed_storage(
            std::slice::from_ref(path),
            effects
                .iter()
                .filter_map(Effect::mutation)
                .map(|(target, _)| target)
                .chain(effects.iter().flat_map(Effect::escape_regions))
                .cloned(),
        )
        .next()
        .is_some()
    {
        // Cancellation changes the task's frame, not its unexposed handle.
        // The lifecycle phase must retain the same computation identity.
        stable_descriptors.insert(path.clone());
        stable_values.insert(path.clone(), bray_bound_tree::StorageObservationKind::Value);
    }
}
