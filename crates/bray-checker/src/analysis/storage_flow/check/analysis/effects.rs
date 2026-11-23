use crate::analysis::storage_flow::memory::{Effect, EffectOutcomes};
use bray_bound_tree::{StorageDischarge, StorageEffectRow, StorageIdentity, StorageInvalidation, StorageObservedTarget, StoragePath, StorageProjection, StorageReferencePath};
use bray_symbols::{
    DependencyFailureKind, DependencyMutationKind, DependencyStorageEffect, DependencySubject,
    TypeData,
};
use std::collections::BTreeMap;

use super::core::StorageFlowCollector;
use super::effect_rows::{EffectRows, SelectedEffectRows};
use crate::CheckerRequestContext;
use crate::analysis::model::{AnalysisCallPhase, AnalysisOperation, AnalysisOperationKind};
use crate::analysis::storage_flow::memory::{Origin, Target};
use crate::analysis::storage_flow::model::{EffectPrefix, StorageFlowState};

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn begin_transition(
        &mut self,
        state: &mut StorageFlowState,
        point: &AnalysisOperation,
        execution: &crate::analysis::guarantee::ExecutionState,
        roots: Option<&[Target]>,
        started: Option<&crate::ExecutionCondition>,
        (failure_prefixes, returned_referents): (&mut EffectRows, &mut BTreeMap<StoragePath, StorageObservedTarget>),
    ) -> Option<EffectOutcomes> {
        let operation = point.kind();

        if let bray_bound_tree::BoundExecutionSite::Node(
            bray_bound_tree::AnyBoundNodeId::Expression(expression),
        ) = operation.occurrence()
            && self.memory.operation(expression).is_some()
        {
            if matches!(
                operation,
                AnalysisOperationKind::Call {
                    phase: AnalysisCallPhase::Input | AnalysisCallPhase::Completion,
                    ..
                }
            ) {
                return Some(EffectOutcomes::new([]));
            }

            return self.memory_effects(state, expression, execution).map(EffectOutcomes::new);
        }

        let roots = roots?;
        let invocation = operation.occurrence();

        let entry = execution.admitted_call(invocation);

        if self.stage_inline_input(state, invocation, entry) {
            return Some(EffectOutcomes::new([]));
        }

        if operation.awaited_expression().is_some() {
            return self.begin_await_transition(state, execution, operation, failure_prefixes);
        }

        if !self.dependencies.calls().contains_key(&invocation)
            && invocation.expression().is_some_and(|expression| {
                matches!(
                    self.execution.semantics.selections().expression(expression),
                    Some(bray_bound_tree::SemanticSelection::Operation(_))
                ) || self.request.view().expression(expression).is_some_and(|bound| {
                    !matches!(bound, bray_bound_tree::BoundExpression::Call(_) | bray_bound_tree::BoundExpression::Await(_))
                })
            })
        {
            // Intrinsic operations and ownership admission checks have invocation
            // edges without calling source code. Their checked moves and writes
            // remain separate storage transitions.
            return Some(EffectOutcomes::new([]));
        }

        let contracts = self.dependencies.calls().get(&invocation)?;
        let proof = execution.ordinary_proof();

        let executes_body = contracts.deferred_effect_outcomes().is_none();

        let admitted = contracts.captured_values().iter().enumerate().filter(|(index, _)| {
            executes_body && entry.is_none_or(|entry| entry.outputs.is_admitted(*index))
        }).collect::<Vec<_>>();

        let admitted_values = admitted.iter().map(|(_, row)| *row);

        if executes_body {
            let mut references = contracts.storage().reference_outputs(admitted_values.clone());

            self.stage_invocation_outputs(
                state, invocation, super::references::InvocationOutputs::Call { contracts, entry, roots },
                &mut references, admitted_values.clone(), Some(returned_referents),
            );

            self.stage_call_report_payloads(state, invocation, contracts, &admitted);
        } else {
            state.memory.begin_transition(invocation);
        }

        if self.publish {
            self.synchronous_cleanup
                .extend(contracts.synchronous_cleanup().iter().cloned());
        }

        if let Some(started) = started {
            let frame = started.deferred_value();
            let body = frame.and_then(|frame| frame.effects(&proof));
            let scheduled = body.as_ref().map_or(&[Effect::Unknown][..], EffectOutcomes::possible);

            let abandoned = frame.and_then(|frame| frame.cleanup_effects(false, &proof))
                .unwrap_or_else(|| vec![Effect::Unknown]);

            let failed = bray_base::sorted_unique_shared_slice(abandoned.into_iter().chain([Effect::Ambient]));

            // The scheduler must allow interference after publication. Before
            // publication fails, only the inactive capture environment runs cleanup.
            return Some(EffectOutcomes::new(scheduled.iter().cloned().chain([Effect::Ambient]))
                .with_failure_prefixes([failed.clone(), failed]));
        }

        let pure = contracts.has_pure_normal_effect_bound()
            && entry.is_some_and(|entry| self.execution.admits_normal_effect_bound(invocation, entry));

        if pure && self.publish {
            self.pure_effect_premises.insert(point.id());
        }

        Some(self.call_effect_outcomes(state, execution, invocation, contracts, (&admitted, pure), failure_prefixes))
    }

    fn begin_await_transition(
        &mut self,
        state: &mut StorageFlowState,
        execution: &crate::analysis::guarantee::ExecutionState,
        operation: AnalysisOperationKind,
        failure_prefixes: &mut EffectRows,
    ) -> Option<EffectOutcomes> {
        let invocation = operation.occurrence();
        let expression = invocation.expression().expect("await transition retains its expression");
        let value = execution.admitted_receiver(invocation);
        let proof = execution.ordinary_proof();

        let Some(frame) = value.deferred_value().filter(|frame| !frame.unknown_possible(&proof)) else {
            state.memory.begin_transition(invocation);

            return None;
        };

        if let Some(task) = self.execution.observed_task_frame(execution, &value) {
            state.memory.begin_transition(invocation);

            // The suspended observer has yet to acquire a child result. If its
            // own run ends here, the child could already have executed any prefix.
            let envelope = task.cleanup_effects(false, &proof)
                .and_then(|cleanup| task_effects(task.effects(&proof), cleanup))
                .unwrap_or_else(|| EffectOutcomes::new([Effect::Unknown]));

            let performed = bray_base::sorted_unique_shared_slice(envelope.possible().iter().cloned());

            return Some(envelope.with_failure_prefixes([performed.clone(), performed]).with_normal([]));
        }

        let panic = DependencyFailureKind::Panic;

        for row in self.execution.invocation_failure_rows(execution, invocation, panic).flatten() {
            if let Some(effects) = frame.failure_effects(&proof, panic, Some(row)) {
                failure_prefixes.insert(StorageEffectRow::Abnormal(panic, row), effects.into());
            }

            if let Some(cells) = frame.cell_changes(&proof, Some((panic, Some(row)))) {
                failure_prefixes.insert(StorageEffectRow::AbnormalCells(panic, row), cells);
            }
        }

        self.prepare_deferred_result(state, expression, frame, &proof, 0);

        let effects = frame.effects(&proof)?;
        let cancellation = DependencyFailureKind::Cancellation;
        let capture_cleanup = frame.cleanup_effects(false, &proof).unwrap_or_else(|| vec![Effect::Unknown]);

        let cancelled = bray_base::sorted_unique_shared_slice(
            effects.failure(cancellation).iter().cloned().chain(capture_cleanup),
        );

        let panicked = bray_base::sorted_unique_shared_slice(effects.failure(panic).iter().cloned());

        Some(effects.with_failure_prefixes([panicked, cancelled]))
    }

    fn prepare_deferred_result(
        &self,
        state: &mut StorageFlowState,
        expression: bray_bound_tree::BoundExpressionId,
        frame: &crate::ExecutionDeferredValue,
        proof: &crate::execution_guarantees::ExecutionProof<'_>,
        depth: usize,
    ) {
        let mut result = self.expression_result_storage(expression)
            .expect("await retains its checked result storage");

        if depth > 0 {
            result.projections = result.projections.iter().copied()
                .chain(std::iter::repeat_n(self.execution.task_completed_payload(), depth))
                .collect();
        }

        self.stage_frame_storage_outputs(state, expression.into(), &result, frame, proof);

        if let Some(cells) = frame.cell_changes(proof, None) {
            state.memory.stage_cell_changes(expression.into(), &cells);
        }

        self.stage_deferred_report_payloads(state, expression.into(), result, frame, proof);
    }

    pub(super) fn apply_task_observation(
        &mut self,
        state: &mut StorageFlowState,
        point: &crate::analysis::model::AnalysisOperation,
        selection: &crate::analysis::guarantee::TaskSelection,
    ) -> Option<std::sync::Arc<bray_bound_tree::ReportDependencies>> {
        use bray_bound_tree::{StorageMutation, StorageTaskOutcome};

        let execution = state.execution.as_ref().expect("reached completion retains execution facts");

        let outcome = selection.outcome;
        let operation = point.kind();
        let occurrence = operation.occurrence();
        let expression = occurrence.expression().expect("task observation retains its await expression");
        let proof = execution.ordinary_proof();

        let mut panic_report = match outcome {
            StorageTaskOutcome::Abnormal { kind: DependencyFailureKind::Panic, .. } => {
                self.execution.observed_task_reports(execution, expression, outcome)
            }
            _ => None,
        };

        let widened_panics = if outcome == StorageTaskOutcome::Widened {
            self.execution.widened_task_panics(execution, expression)
        } else {
            Vec::new()
        };

        let body = if matches!(outcome, StorageTaskOutcome::Abnormal { .. }) {
            None
        } else {
            self.execution.deferred_body(execution, execution.admitted_receiver(occurrence))
        };

        let resolved = match outcome {
            StorageTaskOutcome::Completed { .. } => body.as_ref().and_then(|(_, _, frame)| {
                frame.effects(&proof).map(|effects| effects.normal().to_vec())
            }),
            StorageTaskOutcome::Abnormal { depth, kind, row } => self.execution.observed_task_at(execution, expression, depth)
                .and_then(|frame| frame.failure_effects(&proof, kind, row)),
            StorageTaskOutcome::Widened => self.execution.awaited_task_frame(execution, expression)
                .and_then(|frame| frame.effects(&proof)).map(|effects| effects.possible().to_vec()),
        };

        let mut resolved = resolved.unwrap_or_else(|| vec![Effect::Unknown]);

        resolved.push(Effect::Ambient);

        let roots = Self::invocation_roots(&state.memory, execution.admitted_call(occurrence));

        let cells = if let StorageTaskOutcome::Abnormal { depth, kind, row } = outcome {
            self.execution.observed_task_at(execution, expression, depth)
                .and_then(|frame| frame.cell_changes(&proof, Some((kind, row))))
        } else {
            None
        };

        let effects = self.resolve_effect_prefix(state, operation, execution, Some(resolved.into()), cells, &roots)
            .expect("observing a running task has at least its scheduling effects");

        if let Some((_, depth, body)) = &body {
            // Stage the actual selected body's value on its guarded payload path.
            let execution = state.execution.take().expect("task completion retains execution facts");

            self.prepare_deferred_result(state, expression, body, &execution.ordinary_proof(), *depth);
            state.execution = Some(execution);
        }

        let mutation = StorageMutation::TaskObservation { occurrence, operation: point.id(), outcome };

        let (_, retirements) = self.apply_effect_prefix(state, mutation, &effects);

        if self.publish {
            let guard = selection.guard();

            self.publish_operation_retirements((point.id(), Some(&guard)), bray_bound_tree::StorageValueBoundary::Execution, retirements);
            self.record_operation_invalidation(point.id(), guard, effects.invalidation);
        }

        let completed_depth = match outcome {
            StorageTaskOutcome::Abnormal { depth, .. } => Some(depth),
            StorageTaskOutcome::Completed { .. } => body.as_ref().map(|(_, depth, _)| *depth),
            StorageTaskOutcome::Widened => None,
        };

        if completed_depth.is_some() || !widened_panics.is_empty() {
            let root = Target::storage(self.expression_result_storage(expression)
                .expect("task observation retains its checked output"));

            let representation = self.request.available_compiler_known_symbols().run_result_representation()
                .expect("task observation retains the checked RunResult representation");

            let panic_payload = StorageProjection::ActiveUnionPayloadField {
                variant: representation.panicked_variant(),
                field: representation.panicked_field(),
            };

            let completed = self.execution.task_completed_payload();

            if let Some(depth) = completed_depth {
                let mut selected = root.clone();

                for _ in 0..depth {
                    state.memory.stage_no_payload_alternative(occurrence, selected.project(&[panic_payload]));
                    selected = selected.project(&[completed]);
                }

                match outcome {
                    StorageTaskOutcome::Abnormal { kind: DependencyFailureKind::Cancellation, .. } => {
                        state.memory.stage_no_payload_alternative(occurrence, selected.project(&[panic_payload]));
                    }
                    StorageTaskOutcome::Abnormal { kind: DependencyFailureKind::Panic, .. } => {
                        if let Some(report) = &panic_report {
                            state.memory.stage_report(occurrence, root, selected.project(&[panic_payload]), std::sync::Arc::clone(report), true);
                        }
                    }
                    _ => {}
                }
            } else {
                for (depth, report) in widened_panics {
                    let route = std::iter::repeat_n(completed, depth).chain([panic_payload]).collect::<Vec<_>>();

                    state.memory.stage_report(occurrence, root.clone(), root.project(&route), std::sync::Arc::clone(&report), true);

                    if let Some(previous) = &mut panic_report {
                        previous.merge(&report);
                    } else {
                        panic_report = Some(report);
                    }
                }
            }
        }

        panic_report
    }

    pub(super) fn call_attempt(
        &self,
        operation: AnalysisOperationKind,
    ) -> Option<bray_bound_tree::BoundExecutionSite> {
        match operation {
            AnalysisOperationKind::Suspension {
                occurrence,
                kind:
                    crate::analysis::model::AnalysisSuspensionKind::Await
                    | crate::analysis::model::AnalysisSuspensionKind::Yield,
            } => Some(occurrence),
            AnalysisOperationKind::Call {
                invocation,
                phase: AnalysisCallPhase::Attempt,
            } => Some(invocation),
            AnalysisOperationKind::Bound(bray_bound_tree::AnyBoundNodeId::Expression(
                expression,
            )) => {
                // Calls without an abnormal edge complete in one graph operation.
                let occurrence = expression.into();

                self.dependencies
                    .calls()
                    .contains_key(&occurrence)
                    .then_some(occurrence)
            }
            _ => None,
        }
    }

    pub(super) fn assignment_mutation(
        &self,
        operation: AnalysisOperationKind,
        plan: bray_bound_tree::StorageAccessPlan,
        execution: &crate::analysis::guarantee::ExecutionState,
    ) -> DependencyMutationKind {
        let ty = self
            .storage
            .access(plan.access())
            .expect("assignment retains its checked access")
            .reached_type();

        if matches!(operation, AnalysisOperationKind::AssignmentInstall(_))
            && matches!(self.request.semantic_values().type_data(ty).as_ref(), TypeData::Nullable(_))
            && self.execution.invocation_value(execution, operation.occurrence()).known_presence() == Some(false)
        {
            return DependencyMutationKind::Clear;
        }

        if let bray_bound_tree::AnyBoundNodeId::Expression(expression) = operation.node()
            && let Some(bray_bound_tree::BoundExpression::Assignment(assignment)) =
                self.request.view().expression(expression)
            && assignment.operator() == bray_bound_tree::BoundAssignmentOperator::Assign
            && let [_, source] = assignment.operands()
            && self.nullable_is_absent(execution, *source, ty)
        {
            return DependencyMutationKind::Clear;
        }

        DependencyMutationKind::Replace(ty)
    }

    pub(super) fn record_operation_effects(
        &mut self,
        state: &mut StorageFlowState,
        kind: AnalysisOperationKind,
        call_effects: Option<&[Effect]>,
        local: &[Effect],
    ) {
        self.record_resolved_effects(&state.memory, local, Some(&mut state.effect_prefix));

        if let Some(effects) = call_effects {
            self.record_resolved_effects(&state.memory, effects, Some(&mut state.effect_prefix));
        } else if self.call_attempt(kind).is_some() || kind.explicit_attempt_expression()
            .is_some_and(|expression| self.memory.operation(expression).is_some())
        {
            self.record_effect(DependencyStorageEffect::Unknown, Some(&mut state.effect_prefix));
        }
    }

    #[expect(clippy::too_many_arguments, reason = "call observations use the reached referents and effects")]
    pub(super) fn operation_observations(
        &self,
        state: &StorageFlowState,
        kind: AnalysisOperationKind,
        returned_referents: BTreeMap<StoragePath, StorageObservedTarget>,
        local: &[Effect],
        effects: Option<&[Effect]>,
        roots: Option<&[Target]>,
        escaped: &std::collections::BTreeSet<Target>,
    ) -> Option<StorageInvalidation> {

        let mut observed = effects.map(|effects| {
            let mut invalidation = self.resolved_observations(&state.memory, effects, roots.into_iter().flatten().cloned(), escaped);

            if invalidation.observation_transition() != bray_bound_tree::StorageObservationTransition::Unchanged
                && let StorageInvalidation::Observed { snapshots, .. } = &mut invalidation {
                // An unchanged descriptor exposes the new contents of exactly its reached referent.
                snapshots.extend(returned_referents.into_iter().filter(|(_, observed)| {
                    effects.iter().filter_map(Effect::mutation).any(|(changed, _)| observed.target.contains(changed))
                }));
            }

            invalidation
        }).or_else(|| {
            roots.map(|roots| self.opaque_observations_with_escapes(&state.memory, (roots.iter().cloned(), false), escaped, &[], false))
        }).or_else(|| {
            kind.explicit_attempt_expression().is_some_and(|expression| self.memory.operation(expression).is_some())
                .then(StorageInvalidation::unknown)
        });

        if local.is_empty() {
            return observed;
        }

        let local_observations = if local.iter().any(Effect::may_reach_all_inputs) {
            StorageInvalidation::unknown()
        } else {
            let transition = if local.iter().all(|effect| {
                matches!(effect.mutation(), Some((_, DependencyMutationKind::Consume)))
            }) {
                bray_bound_tree::StorageObservationTransition::Transfer
            } else {
                bray_bound_tree::StorageObservationTransition::Unknown
            };

            let targets = local.iter().filter_map(Effect::mutation).map(|(target, _)| target);

            self.target_observations(&state.memory, targets).with_observation_transition(transition)
        };

        if let Some(observed) = &mut observed {
            observed.merge(&local_observations);
        } else {
            observed = Some(local_observations);
        }

        observed
    }

    pub(super) fn apply_call_effects(
        &mut self,
        state: &mut StorageFlowState,
        kind: AnalysisOperationKind,
        call_effects: Option<&[Effect]>,
        invalidation: Option<&StorageInvalidation>,
    ) {
        if self.call_attempt(kind).is_some()
            && kind.occurrence().expression().is_some_and(|expression| {
                !matches!(
                    kind.occurrence(),
                    bray_bound_tree::BoundExecutionSite::Node(_)
                ) || self.memory.operation(expression).is_none_or(|operation| {
                    crate::memory::execution::commits_buffer_cleanup(operation.kind())
                })
            })
        {
            let occurrence = kind.occurrence();

            if let Some(effects) = call_effects {
                self.check_raw_retirements(state, occurrence, effects);
            }

            if let Some(invalidation) = invalidation {
                state.memory.apply_effects(call_effects, invalidation, occurrence);
            }
        }
    }

    pub(super) fn call_effects(
        &self,
        state: &crate::analysis::storage_flow::model::StorageFlowState,
        execution: &crate::analysis::guarantee::ExecutionState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        effects: &[DependencyStorageEffect],
    ) -> Vec<Effect> {
        let contract = &self.dependencies.calls()[&occurrence];
        let storage = contract.storage();
        let entry = execution.admitted_call(occurrence);
        let result = self.invocation_result(occurrence).map(Target::storage);

        self.resolve_effects(&state.memory, storage, (effects, entry, result.as_ref().map(|result| (occurrence, result))),
            || entry?.argument_regions(contract.input_count()), |subject, path, indirect| {
            let values = self.request.semantic_values();
            let root = subject.subject_root();

            if root.is_static() {
                let start = path.map_or_else(|| self.static_location(subject), |_| {
                    self.static_location(&DependencySubject::root(root))
                })?;

                return state.memory.reference_effect_targets(values, start, path, indirect);
            }

            if root == bray_symbols::DependencySubjectRoot::Result {
                return None;
            }

            let path = self.invocation_reference_path(occurrence, root, path?)?;

            if indirect || !path.steps().is_empty() {
                let captured = self.execution.invocation_capture(entry?, occurrence, root)?;

                return state.memory.captured_effect_targets(values, captured, &path, indirect);
            }

            let slot = self.invocation_place(occurrence, root)?;
            let target = Target::storage(slot).project(path.projections());

            Some(vec![bray_bound_tree::StorageMutationTarget::location(target)])
        }, |subject, path, ty, include_inline| {
            let root = subject.subject_root();
            let path = self.invocation_reference_path(occurrence, root, path)?;
            let captured = self.execution.invocation_capture(entry?, occurrence, root)?;

            state.memory.captured_cleanup_effects(self.request.semantic_values(), captured, &path, ty, include_inline)
        })
    }

    fn call_effect_outcomes<'outcome>(
        &self,
        state: &mut StorageFlowState,
        execution: &crate::analysis::guarantee::ExecutionState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        contracts: &'outcome bray_bound_tree::BoundCallDependencyContracts,
        (admitted, pure): (&[(usize, &'outcome bray_symbols::CapturedValueOutcome)], bool),
        failure_prefixes: &mut EffectRows,
    ) -> EffectOutcomes {
        let mut selection = SelectedEffectRows::new(contracts.deferred_effect_outcomes().is_none()
            && admitted.len() != contracts.captured_values().len());

        selection.apply_pure_normal_bound(pure);

        for &(index, row) in admitted {
            selection.select_normal(index, row, false);
        }

        let entry = execution.admitted_call(occurrence);
        let failures = contracts.reports().failures();

        if failures.is_some() {
            let retained = DependencyFailureKind::ALL.into_iter().flat_map(|kind| {
                self.execution.invocation_failure_rows(execution, occurrence, kind)
                    .flatten()
                    .map(move |row| StorageEffectRow::Abnormal(kind, row))
            }).collect::<std::collections::BTreeSet<_>>();

            selection.select_failures(
                failures,
                |row| entry.is_none_or(|entry| entry.admits_failure(row)),
                |row| retained.contains(&row),
            );
        }

        let (effects, mut prefixes) = selection.bind(contracts.effect_outcomes(), |effect| {
            self.call_effects(state, execution, occurrence, std::slice::from_ref(effect))
        });

        let cells = bray_symbols::common_cell_changes(admitted.iter().map(|(index, _)| {
            prefixes.get(&StorageEffectRow::NormalCells(*index))
        }));

        if let Some(cells) = cells {
            state.memory.stage_cell_changes(occurrence, &cells);
        }

        prefixes.retain(|row, _| !matches!(row, StorageEffectRow::NormalCells(_)));
        failure_prefixes.extend(prefixes);

        effects
    }

    pub(super) fn resolve_effects(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        storage: &bray_bound_tree::BoundCallStorage,
        selection: (&[DependencyStorageEffect], Option<&crate::ExecutionCallEvidence>, Option<(bray_bound_tree::BoundExecutionSite, &Target)>),
        mut input_regions: impl FnMut() -> Option<std::sync::Arc<[Target]>>,
        mut resolve: impl FnMut(&DependencySubject, Option<&StorageReferencePath>, bool) -> Option<Vec<crate::analysis::storage_flow::memory::MutationTarget>>,
        mut carried: impl FnMut(&DependencySubject, &StorageReferencePath, bray_symbols::TypeId, bool) -> Option<Vec<Effect>>,
    ) -> Vec<Effect> {
        self.resolve_effect_routes(memory, storage, selection, &mut input_regions, &mut resolve, &mut carried)
    }

    fn resolve_effect_routes(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
        storage: &bray_bound_tree::BoundCallStorage,
        (effects, entry, staged): (&[DependencyStorageEffect], Option<&crate::ExecutionCallEvidence>, Option<(bray_bound_tree::BoundExecutionSite, &Target)>),
        input_regions: &mut impl FnMut() -> Option<std::sync::Arc<[Target]>>,
        resolve: &mut impl FnMut(&DependencySubject, Option<&StorageReferencePath>, bool) -> Option<Vec<crate::analysis::storage_flow::memory::MutationTarget>>,
        carried: &mut impl FnMut(&DependencySubject, &StorageReferencePath, bray_symbols::TypeId, bool) -> Option<Vec<Effect>>,
    ) -> Vec<Effect> {
        let mut resolved = Vec::new();

        for effect in effects {
            let (effect, discharged) = effect.retirement_parts();

            let (subject, indirect, continues, kind, access_type, addressed) = match effect {
                DependencyStorageEffect::OrderedRetirement { .. } => unreachable!("retirement has one stated discharge prefix"),
                DependencyStorageEffect::Escape | DependencyStorageEffect::UnboundedMutation { kind: DependencyMutationKind::Escape, .. } => {
                    let regions = input_regions();

                    if regions.as_ref().is_none_or(|regions| !regions.is_empty()) {
                        resolved.push(Effect::Escape(regions));
                    }

                    continue;
                }
                DependencyStorageEffect::UnboundedMutation { kind, access_type, ambient } => {
                    resolved.push(Effect::unbounded(*kind, *access_type, *ambient));
                    continue;
                }
                DependencyStorageEffect::Ambient => {
                    resolved.push(Effect::Ambient);
                    continue;
                }
                DependencyStorageEffect::Unknown => {
                    resolved.push(Effect::Unknown);
                    continue;
                }
                DependencyStorageEffect::Atomic => {
                    resolved.push(Effect::Atomic);
                    continue;
                }
                DependencyStorageEffect::Direct { addressed, subject, kind, access_type } => {
                    (subject, false, false, *kind, *access_type, *addressed)
                }
                DependencyStorageEffect::Indirect { addressed, reads,
                    reference,
                    projections,
                    kind,
                    access_type,
                } => (reference, true, !projections.is_empty() || !reads.is_empty(), *kind, *access_type, *addressed),
            };

            let path = storage.subject(subject).map(|(_, path)| path);
            let static_subject = subject.subject_root().is_static();

            if let DependencyMutationKind::DisposeCaptured { ty, include_inline } = kind && !static_subject && !indirect && entry.is_some() {
                if let Some(effects) = path.and_then(|path| carried(subject, path, ty, include_inline)) {
                    resolved.extend(effects);
                } else {
                    resolved.push(Effect::unbounded(kind, access_type, false));
                }

                continue;
            }

            let static_backing = static_subject && !addressed && !indirect
                && subject.projections().contains(&bray_symbols::DependencyProjection::OwnedTarget)
                && !subject.projections().contains(&bray_symbols::DependencyProjection::AddressTarget);

            let values = self.request.semantic_values();

            let finish_route = |targets, continuation: Option<&StorageReferencePath>| {
                if let Some(continuation) = continuation {
                    memory.follow_effect_path(values, targets, continuation)
                } else { Some(targets) }
            };

            let mut resolve_subject = |path: Option<&StorageReferencePath>, indirect, continuation: Option<&StorageReferencePath>| {
                if subject.subject_root() == bray_symbols::DependencySubjectRoot::Result
                    && let Some((site, result)) = staged
                {
                    return memory.staged_effect_targets(site, values, result, (path?, continuation), indirect);
                }

                let targets = resolve(subject, path, indirect).or_else(|| {
                    static_backing.then(|| self.static_effect_targets(subject, true)).flatten()
                })?;

                finish_route(targets, continuation)
            };

            let targets = (|| {
                let continuation = if continues {
                    Some(storage.effect_path(effect)?.bind_selectors(|projection| {
                        let StorageProjection::ElementFromInput(ordinal) = projection else { return projection; };

                        let value = entry.and_then(|entry| entry.arguments.get(&crate::ExecutionPlace::argument(ordinal)));

                        value.and_then(|value| self.evaluated_element(value)).unwrap_or(StorageProjection::AnyElement)
                    }))
                } else { None };

                let continuation = continuation.as_deref();

                if static_subject && !matches!(subject.subject_root(), bray_symbols::DependencySubjectRoot::StaticInstance(..)) {
                    let targets = self.static_effect_targets(subject, static_backing)?;

                    let targets = if indirect {
                        targets.into_iter().try_fold(Vec::new(), |mut reached, target| {
                            reached.extend(crate::analysis::storage_flow::memory::Source::Value(target.address?).mutation_targets(memory)?);

                            Some(reached)
                        })?
                    } else { targets };

                    finish_route(targets, continuation)
                } else {
                    resolve_subject(path, indirect, continuation)
                }
            })();

            let Some(targets) = targets else {
                if kind == DependencyMutationKind::Escape
                    && let Some(path) = path
                    && let Some((step, index, _, _)) = path.reads().find(|(_, _, _, kind)| {
                        *kind == bray_symbols::DependencyReferenceKind::Address
                    })
                    && let Some(reached) = resolve_subject(Some(&path.prefix(index + 1, step + 1)), false, None)
                {
                    // Unknown contents below an admitted raw address can publish
                    // addresses carried by that region. The outer loan is no new root.
                    resolved.extend(reached.into_iter().map(|target|
                        Effect::bounded(target.footprint, kind, access_type)));

                    continue;
                }

                if kind == DependencyMutationKind::Escape {
                    // An unresolved carried address can reach outside its known
                    // subject; absence of a route does not establish no escape.
                    resolved.push(Effect::Escape(None));
                    continue;
                }

                if kind == DependencyMutationKind::Atomic {
                    resolved.push(Effect::Atomic);
                    continue;
                }

                resolved.push(Effect::unbounded(kind, access_type, false));

                continue;
            };

            if let DependencyMutationKind::DisposeCaptured { ty, include_inline } = kind {
                resolved.extend(targets.into_iter().filter_map(|target| match target.address {
                    Some(address) => memory.capture_cleanup(&address, ty, include_inline),
                    None => Some(memory.bounded_cleanup(&[target.footprint], ty, include_inline)),
                }));

                continue;
            }

            let duties = discharged.iter().filter_map(|duty| {
                let reached = self.resolve_effect_routes(memory, storage, (std::slice::from_ref(duty), entry, staged), input_regions, resolve, carried);

                let [effect] = reached.as_slice() else { return None; };

                let (cell, DependencyMutationKind::Discharge(ty)) = effect.cell_change()? else { return None; };

                // A discharged cell is a proof fact. Alternative targets contribute only bounds.
                Some(StorageDischarge { cell: cell.clone(), ty })
            }).collect::<Vec<_>>();

            for target in targets {
                let target = if addressed { target } else { bray_bound_tree::StorageMutationTarget::bounded(target.footprint) };

                let effects = Effect::through(target, kind, access_type);

                let effects = if kind == DependencyMutationKind::Retire
                    && let Effect::Mutation { address: Some(address), .. } = &effects
                {
                    // A nested retirement runs after the caller's already completed
                    // exact moves. Carry that reached prestate with the retirement;
                    // the unordered effect envelope cannot establish this order.
                    let prefix = duties.iter().cloned().chain(memory.retirement_discharges(address)).collect::<Vec<_>>();

                    effects.with_retirement_discharges(prefix)
                } else { effects };

                resolved.push(effects);
            }
        }

        resolved.sort_unstable();
        resolved.dedup();

        resolved
    }

    pub(super) fn record_resolved_effects(
        &mut self,
        memory: &crate::analysis::storage_flow::memory::State,
        effects: &[Effect],
        mut prefix: Option<&mut EffectPrefix>,
    ) {
        if !self.publish && prefix.is_none() {
            return;
        }

        for effect in effects {
            match effect {
                Effect::Escape(_) | Effect::Mutation { kind: DependencyMutationKind::Escape, .. } => {}
                Effect::Mutation { address, footprint, discharged, kind, access_type } => {
                    let by_operand = address.as_ref() == Some(footprint);
                    let discharged = discharged.as_deref().unwrap_or_default();
                    let footprint_discharges = if by_operand { discharged } else { &[] };

                    self.record_target_effect(memory, footprint, by_operand, *kind, *access_type, footprint_discharges, prefix.as_deref_mut());

                    if let Some(operand) = address && operand != footprint {
                        self.record_target_effect(memory, operand, true, *kind, *access_type, discharged, prefix.as_deref_mut());
                    }
                }
                Effect::UnboundedMutation { kind, access_type, ambient } => {

                    self.record_effect(DependencyStorageEffect::UnboundedMutation {
                        kind: *kind, access_type: *access_type, ambient: *ambient,
                    }, prefix.as_deref_mut());
                }
                Effect::CapturedCleanup { ty, sources, inline, .. } => {
                    let mut complete = !sources.is_empty() && inline.iter().all(|target| sources.contains(target));

                    for source in sources.iter() {
                        if let Some(effect) = self.portable_cleanup_source(source, *ty, inline.contains(source)) {
                            self.record_effect(effect, prefix.as_deref_mut());
                        } else {
                            complete = false;
                        }
                    }

                    if !complete {
                        self.record_effect(DependencyStorageEffect::UnboundedMutation {
                            kind: DependencyMutationKind::Cleanup(*ty), access_type: Some(*ty), ambient: false,
                        }, prefix.as_deref_mut());
                    }
                }
                Effect::Ambient => {
                    self.record_effect(DependencyStorageEffect::Ambient, prefix.as_deref_mut())
                }
                Effect::Unknown => {
                    self.record_effect(DependencyStorageEffect::Unknown, prefix.as_deref_mut())
                }
                Effect::Atomic => {
                    self.record_effect(DependencyStorageEffect::Atomic, prefix.as_deref_mut())
                }
            }
        }
    }

    pub(super) fn portable_cleanup_source(&self, source: &Target, ty: bray_symbols::TypeId, include_inline: bool) -> Option<DependencyStorageEffect> {
        if source.previous { return None; }

        let kind = DependencyMutationKind::DisposeCaptured { ty, include_inline };
        let effect = self.portable_target_effect(source, false, kind, Some(ty))?;

        matches!(effect, DependencyStorageEffect::Direct { kind: reached, .. } | DependencyStorageEffect::Indirect { kind: reached, .. } if reached == kind)
            .then_some(effect)
    }

    #[expect(clippy::too_many_arguments, reason = "effect publication keeps the actual operand and its performed discharge prefix together")]
    pub(super) fn record_target_effect(
        &mut self,
        memory: &crate::analysis::storage_flow::memory::State,
        target: &Target,
        addressed: bool,
        kind: DependencyMutationKind,
        access_type: Option<bray_symbols::TypeId>,
        discharged: &[StorageDischarge],
        mut prefix: Option<&mut EffectPrefix>,
    ) {
        if !self.publish && prefix.is_none() {
            return;
        }

        if matches!(target.origin, Origin::Returned { .. })
            && kind != DependencyMutationKind::Atomic
        {
            // Alias bounds retain the possible region, not the disposed value's layout.
            let kind = kind.region_kind().unlocated_kind();

            let (origins, unbounded) = memory.effect_origins(target);

            if unbounded && kind != DependencyMutationKind::Escape {

                // An independent alternative may reach external storage, without granting freshness.
                self.record_effect(DependencyStorageEffect::UnboundedMutation {
                    kind, access_type, ambient: true,
                }, prefix.as_deref_mut());
            }

            // Preserve every possible input region and the actual ownership transition.
            for origin in origins {
                self.record_target_effect(memory, origin, false, kind, access_type, &[], prefix.as_deref_mut());
            }

            return;
        }

        if let Some(effect) = self.portable_target_effect(target, addressed, kind, access_type) {
            let effect = if effect.is_addressed_retirement() && !discharged.is_empty() {
                let duties = discharged.iter().filter_map(|duty| self.portable_target_effect(
                    &duty.cell, true, DependencyMutationKind::Discharge(duty.ty), None,
                )).filter(DependencyStorageEffect::is_addressed_discharge);

                effect.with_retirement_discharges(bray_base::sorted_unique_shared_slice(duties))
            } else {
                effect
            };

            self.record_effect(effect, prefix);
        }
    }

    pub(super) fn outcome_cell_changes(
        &self,
        state: &StorageFlowState,
    ) -> Option<std::sync::Arc<[DependencyStorageEffect]>> {
        let changes = state.memory.entry_cell_changes().filter_map(|(target, kind)| {
            self.portable_target_effect(target, true, kind, None)
                .filter(DependencyStorageEffect::is_cell_change)
        }).collect::<Vec<_>>();

        (!changes.is_empty()).then(|| bray_base::sorted_unique_shared_slice(changes))
    }

    fn portable_target_effect(
        &self,
        target: &Target,
        addressed: bool,
        kind: DependencyMutationKind,
        access_type: Option<bray_symbols::TypeId>,
    ) -> Option<DependencyStorageEffect> {
        let effect = match &target.origin {
            Origin::Entry {
                reference,
                projections,
                layer,
                ..
            } => {
                let root = self.formal_root(reference.root)?;

                let source = crate::storage::dependency_projections(self.request, &reference.projections);

                // An unportable selector widens the reached region, never the identity
                // of the reference through which this operation obtained access.
                let suffix = projections
                    .iter()
                    .copied()
                    .map_while(|projection| {
                        if let StorageProjection::ElementFromInput(ordinal) = projection {
                            let term = self.request.semantic_values()
                                .intern_constant_term(bray_symbols::ConstantTermData::CallableArgument(ordinal))
                                .expect("an evaluated entry selector belongs to the semantic store");

                            return Some(bray_symbols::DependencyProjection::Element(term));
                        }

                        crate::storage::dependency_projection(self.request, projection)
                    })
                    .collect::<Vec<_>>();

                let kind = if suffix.len() == projections.len() { kind } else { kind.region_kind().unlocated_kind() };

                match source {
                    Some(source) => Some(DependencyStorageEffect::Indirect { addressed: addressed && suffix.len() == projections.len(),
                        reads: vec![0; layer.checked_sub(1).expect("an entry referent has a positive layer")].into(),
                        reference: DependencySubject::new(root, source),
                        projections: suffix.into(),
                        kind,
                        access_type: None,
                    }),
                    _ => Some(DependencyStorageEffect::Unknown),
                }
            }
            Origin::Returned { .. } | Origin::BackingRegion => {
                Some(DependencyStorageEffect::Unknown)
            }
            Origin::Storage(path) => self.formal_effect(path, kind),
            Origin::Allocation { .. } => None,
            Origin::Static { root, projections, .. } => {
                // An unresolved selector widens within the same static identity.
                let suffix = projections.iter().copied().map_while(|projection| crate::storage::dependency_projection(self.request, projection)).collect::<Vec<_>>();

                let precise = suffix.len() == projections.len();

                Some(DependencyStorageEffect::Direct { addressed: addressed && precise,
                    subject: DependencySubject::new(*root, suffix),
                    kind: if precise { kind } else { kind.region_kind().unlocated_kind() },
                    access_type: None,
                })
            }
        };

        let effect = effect.or_else(|| kind.cleanup_type().map(|_| DependencyStorageEffect::UnboundedMutation {
            kind: kind.unlocated_kind(),
            access_type, ambient: !matches!(kind, DependencyMutationKind::DisposeCaptured { .. }),
        }));

        let mut effect = effect?;

        if let DependencyStorageEffect::Direct { access_type: original, .. }
            | DependencyStorageEffect::Indirect { access_type: original, .. } = &mut effect {
            *original = access_type;
        }

        let effect = if effect == DependencyStorageEffect::Unknown {
            DependencyStorageEffect::unresolved(kind, access_type)
        } else {
            effect
        };

        let effect = if !addressed || !target.is_exact() || matches!(effect,
            DependencyStorageEffect::Direct { addressed: false, .. } | DependencyStorageEffect::Indirect { addressed: false, .. }) {
            effect.into_region()
        } else { effect };

        Some(effect)
    }

    fn record_effect(&mut self, effect: DependencyStorageEffect, prefix: Option<&mut EffectPrefix>) {
        if let Some(prefix) = prefix && !prefix.contains(&effect) {
            // The current prefix and published envelope share immutable effect paths.
            std::sync::Arc::make_mut(prefix).insert(effect.clone());
        }

        if self.publish {
            self.effects.insert(effect);
        }
    }

    fn formal_effect(
        &self,
        path: &StoragePath,
        kind: DependencyMutationKind,
    ) -> Option<DependencyStorageEffect> {
        let identity = self
            .storage
            .identity(path.root)
            .expect("flow mutation retains its planned root");

        let borrowed = self.storage.identity_type(path.root).is_some_and(|ty| {
            matches!(
                self.request.semantic_values().type_data(ty).as_ref(),
                TypeData::Borrow { .. }
            )
        });

        if matches!(
            identity,
            StorageIdentity::Parameter(_)
                | StorageIdentity::Receiver(_)
                | StorageIdentity::AnonymousParameter(_)
                | StorageIdentity::ContractParameter(_)
        ) && identity.is_invocation_owned(self.storage.kind())
            && (!borrowed || path.projections.is_empty() && kind == DependencyMutationKind::Consume)
            && !matches!(kind, DependencyMutationKind::Dispose(_) | DependencyMutationKind::DisposeCaptured { .. })
            && (kind != DependencyMutationKind::Escape
                || self.input.reference_free(self.storage, path).is_some())
            && !path.projections.contains(&StorageProjection::OwnedTarget)
        {
            // Writes to inline owned input storage stay in this invocation.
            // Escape summaries separately carry its reachable value contents.
            return None;
        }

        let root = match identity {
            StorageIdentity::Parameter(_)
            | StorageIdentity::Receiver(_)
            | StorageIdentity::AnonymousParameter(_)
            | StorageIdentity::ContractParameter(_) => self
                .formal_root(path.root)
                .expect("checked callable input retains its formal dependency subject"),
            StorageIdentity::Static(_) => self.formal_root(path.root)
                .expect("planned static storage retains its selected dependency root"),
            StorageIdentity::Error(_)
            | StorageIdentity::CustomIndexBorrow(_)
            | StorageIdentity::Alternative { .. } => return Some(DependencyStorageEffect::Unknown),
            // Resolved referent writes use physical targets. Initializing or retiring a local
            // reference slot does not mutate the storage reached by its contents.
            _ => return None,
        };

        let projections = crate::storage::dependency_projections(self.request, &path.projections);

        let Some(projections) = projections else {
            return Some(DependencyStorageEffect::Unknown);
        };

        Some(if borrowed {
            DependencyStorageEffect::Indirect { addressed: true, reads: [].into(),
                reference: DependencySubject::root(root),
                projections: projections.into(),
                kind,
                access_type: None,
            }
        } else {
            DependencyStorageEffect::Direct { addressed: true,
                subject: DependencySubject::new(root, projections),
                kind,
                access_type: None,
            }
        })
    }
}

pub(super) fn task_effects(
    effects: Option<EffectOutcomes>,
    cleanup: impl IntoIterator<Item = Effect>,
) -> Option<EffectOutcomes> {
    let effects = effects?;
    let cleanup = cleanup.into_iter().chain([Effect::Ambient]).collect::<Vec<_>>();

    let failed = bray_symbols::DependencyFailureKind::ALL.map(|kind| {
        bray_base::sorted_unique_shared_slice(effects.failure(kind).iter().chain(&cleanup).cloned())
    });

    // Task scheduling and cleanup use the same captured descriptors on each outcome.
    Some(EffectOutcomes::new(effects.possible().iter().chain(&cleanup).cloned())
        .with_normal(effects.normal().iter().chain(&cleanup).cloned())
        .with_failure_prefixes(failed))
}
