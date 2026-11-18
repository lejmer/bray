use crate::analysis::storage_flow::memory::{Effect, EffectOutcomes};
use bray_bound_tree::{StorageIdentity, StorageInvalidation, StoragePath, StorageProjection};
use bray_symbols::{
    DependencyMutationKind, DependencyStorageEffect, DependencySubject,
    TypeData,
};

use super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::model::{AnalysisCallPhase, AnalysisOperationKind};
use crate::analysis::storage_flow::memory::{Origin, Target};
use crate::analysis::storage_flow::model::{EffectPrefix, StorageFlowState};
use crate::storage::storage_projections;
use bray_bound_tree::StorageAccessPurpose;

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn begin_transition(
        &mut self,
        state: &mut StorageFlowState,
        operation: AnalysisOperationKind,
        assignment: Option<&[Target]>,
        execution: &crate::analysis::guarantee::ExecutionState,
        started: Option<&crate::ExecutionCondition>,
    ) -> Option<EffectOutcomes> {
        self.prepare_assignment_reference(state, operation, assignment);

        if let Some(invocation) = self.call_attempt(operation)
            && matches!(invocation, bray_bound_tree::BoundExecutionSite::CallInput { .. })
        {
            self.observe_call_reference_inputs(state, invocation);
        }

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

        let invocation = self.call_attempt(operation)?;

        if operation.awaited_expression().is_some() {
            return self.begin_await_transition(state, execution, operation);
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
        let entry = execution.admitted_call(invocation);

        let admitted = contracts.captured_values().iter().enumerate().filter(|(index, row)| {
            entry.is_none_or(|entry| entry.admits_outcome(self.request.semantic_values(), &proof, *index, row.guard))
        }).map(|(_, row)| row).collect::<Vec<_>>();

        self.prepare_call_references(state, invocation, contracts, &admitted);

        if self.publish {
            self.synchronous_cleanup
                .extend(contracts.synchronous_cleanup().iter().cloned());
        }

        if contracts.deferred_effect_outcomes().is_some() {
            self.capture_deferred_cleanup(state, execution, invocation);
        }

        let effects = if let Some(started) = started {
            let frame = started.deferred_value();
            let body = frame.and_then(|frame| frame.effects(&proof));
            let scheduled = body.as_ref().map_or(&[Effect::Unknown][..], EffectOutcomes::possible);

            let abandoned = frame.and_then(|frame| frame.cleanup_effects(false, &proof))
                .unwrap_or_else(|| vec![Effect::Unknown]);

            let failed = bray_base::sorted_unique_shared_slice(abandoned.into_iter().chain([Effect::Ambient]));

            // The scheduler must allow interference after publication. Before
            // publication fails, only the inactive capture environment runs cleanup.
            EffectOutcomes::new(scheduled.iter().cloned().chain([Effect::Ambient]))
                .with_failure_prefixes([failed.clone(), failed])
        } else {
            self.call_effect_outcomes(state, execution, invocation, contracts, &admitted)
        };

        Some(effects)
    }

    fn begin_await_transition(
        &mut self,
        state: &mut StorageFlowState,
        execution: &crate::analysis::guarantee::ExecutionState,
        operation: AnalysisOperationKind,
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

        self.prepare_deferred_result(state, expression, frame, &proof, 0);

        frame.effects(&proof)
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
            let (variant, field) = crate::representation::union_payload(
                self.request,
                "RunResultVariant0Completed",
                "RunResultVariant0CompletedValue",
            );

            result.projections = result.projections.iter().copied()
                .chain(std::iter::repeat_n(
                    StorageProjection::ActiveUnionPayloadField { variant, field },
                    depth,
                )).collect();
        }

        if let Some(transitions) = frame.transitions(proof) {
            state.memory.prepare_deferred_completion(expression.into(), result, transitions);
        } else {
            state.memory.begin_transition(expression.into());
        }
    }

    pub(super) fn apply_task_observation(&mut self, state: &mut StorageFlowState, operation: AnalysisOperationKind) {
        use bray_bound_tree::{StorageMutation, StorageTaskOutcome, StorageValuePhase};

        let execution = state.execution.as_ref().expect("reached completion retains execution facts");

        let Some(outcome) = self.execution.task_observation_outcome(execution, operation) else { return; };

        let occurrence = operation.occurrence();
        let expression = occurrence.expression().expect("task observation retains its await expression");
        let proof = execution.ordinary_proof();
        let body = self.execution.deferred_body(execution, execution.admitted_receiver(occurrence));

        let resolved = match outcome {
            StorageTaskOutcome::Completed { .. } => body.as_ref().and_then(|(_, _, frame)| {
                frame.effects(&proof).map(|effects| effects.normal().to_vec())
            }),
            StorageTaskOutcome::Abnormal { depth, kind } => self.execution.observed_task_at(execution, expression, depth)
                .and_then(|frame| frame.effects(&proof)).map(|effects| effects.failure(kind).to_vec()),
            StorageTaskOutcome::Widened => self.execution.awaited_task_frame(execution, expression)
                .and_then(|frame| frame.effects(&proof)).map(|effects| effects.possible().to_vec()),
        };

        let mut resolved = resolved.unwrap_or_else(|| vec![Effect::Unknown]);

        resolved.push(Effect::Ambient);

        let roots = self.invocation_roots(state, occurrence);

        let effects = self.resolve_effect_prefix(state, operation, execution, Some(&resolved), &roots)
            .expect("observing a running task has at least its scheduling effects");

        if !matches!(outcome, StorageTaskOutcome::Abnormal { .. })
            && let Some((_, depth, body)) = body
        {
            // Stage the actual selected body's value on its guarded payload path.
            let execution = state.execution.take().expect("task completion retains execution facts");

            self.prepare_deferred_result(state, expression, &body, &execution.ordinary_proof(), depth);
            state.execution = Some(execution);
        }

        self.apply_effect_prefix(
            state, StorageMutation::TaskObservation { occurrence, outcome },
            StorageValuePhase::Completion, &effects,
        );
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
        execution: &crate::analysis::guarantee::ExecutionState,
    ) -> Option<DependencyMutationKind> {
        let plan = self
            .storage
            .node_plans(operation.node())
            .find(|plan| plan.purpose() == StorageAccessPurpose::Assignment)?;

        let ty = self
            .storage
            .access(plan.access())
            .expect("assignment retains its checked access")
            .reached_type();

        if let bray_bound_tree::AnyBoundNodeId::Expression(expression) = operation.node()
            && let Some(bray_bound_tree::BoundExpression::Assignment(assignment)) =
                self.request.view().expression(expression)
            && assignment.operator() == bray_bound_tree::BoundAssignmentOperator::Assign
            && let [_, source] = assignment.operands()
            && self.nullable_is_absent(execution, *source, ty)
        {
            return Some(DependencyMutationKind::Clear);
        }

        Some(DependencyMutationKind::Replace(ty))
    }

    pub(super) fn record_operation_effects(
        &mut self,
        state: &mut StorageFlowState,
        kind: AnalysisOperationKind,
        call_effects: Option<&[Effect]>,
        assignment: Option<&[Effect]>,
    ) {
        if let Some(effects) = assignment {
            self.record_resolved_effects(&state.memory, effects, Some(&mut state.effect_prefix));
        }

        if let Some(effects) = call_effects {
            self.record_resolved_effects(&state.memory, effects, Some(&mut state.effect_prefix));
        } else if self.call_attempt(kind).is_some() {
            // An unresolved call can introduce or retire storage on every outcome.
            self.record_effect(DependencyStorageEffect::Unknown, Some(&mut state.effect_prefix));
        } else if assignment.is_none()
            && let Some(effects) = self.refinements.invalidation_at(kind.occurrence())
        {
            let effect_kind = if self.storage.node_plans(kind.node()).any(|plan| {
                matches!(
                    self.storage.effective_purpose(plan),
                    StorageAccessPurpose::Move | StorageAccessPurpose::ValueTransfer
                )
            }) {
                DependencyMutationKind::Consume
            } else {
                DependencyMutationKind::Write
            };

            self.record_effects(effects, effect_kind, &mut state.effect_prefix);
        }
    }

    pub(super) fn operation_observations(
        &self,
        state: &StorageFlowState,
        kind: AnalysisOperationKind,
        assignment: Option<&[Target]>,
        effects: Option<&[Effect]>,
        roots: Option<&[Target]>,
        escaped: &std::collections::BTreeSet<Target>,
    ) -> Option<StorageInvalidation> {
        let mut observed = effects
            .map(|effects| {
                self.call_observations(state, kind.occurrence(), effects, roots.unwrap_or(&[]), escaped)
            })
            .or_else(|| {
                roots.map(|roots| self.opaque_observations_with_escapes(&state.memory, roots.iter().cloned(), escaped))
            })
            .or_else(|| {
                assignment
                    .is_none()
                    .then(|| {
                        self.refinements
                            .invalidation_at(kind.occurrence())
                            .map(|planned| {
                                self.planned_observations(state, planned, kind.occurrence())
                            })
                    })
                    .flatten()
            });

        if let Some(targets) = assignment {
            let destination = self.target_observations(&state.memory, targets.iter());

            if let Some(observed) = &mut observed {
                observed.merge(&destination);
            } else {
                observed = Some(destination);
            }
        }

        observed.map(|observed| {
            if observed.observation_transition() == bray_bound_tree::StorageObservationTransition::Unchanged {
                observed
            } else {
                observed.with_observation_transition(self.observation_transition(kind))
            }
        })
    }

    pub(super) fn apply_call_effects(
        &self,
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
                ) || self.memory.operation(expression).is_none()
            })
            && let Some(invalidation) =
                invalidation.or_else(|| self.refinements.invalidation_at(kind.occurrence()))
        {
            state.memory.apply_effects(call_effects, invalidation, self.storage);
        }
    }

    pub(super) fn capture_failure_effects(
        &mut self,
        state: &mut StorageFlowState,
        kind: AnalysisOperationKind,
        execution: &crate::analysis::guarantee::ExecutionState,
        outcomes: Option<&EffectOutcomes>,
        roots: &[Target],
    ) {
        if let Some(outcomes) = outcomes {
            self.record_resolved_effects(&state.memory, outcomes.possible(), None);
        }

        let [panic, cancellation] = bray_symbols::DependencyFailureKind::ALL;

        let panicked = outcomes.map(|outcomes| outcomes.failure(panic));
        let cancelled = outcomes.map(|outcomes| outcomes.failure(cancellation));
        let capture = self.resolve_effect_prefix(state, kind, execution, panicked, roots).map(std::sync::Arc::new);

        let cancelled = if cancelled == panicked { capture.clone() }
            else { self.resolve_effect_prefix(state, kind, execution, cancelled, roots).map(std::sync::Arc::new) };

        for (reason, effects) in [(panic, capture), (cancellation, cancelled)] {
            if let Some(effects) = effects {
                let occurrence = kind.occurrence();

                state.memory.capture_failure_effects(occurrence, reason, effects);
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn failure_edge_state(
        &mut self,
        source: &StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        kind: bray_symbols::DependencyFailureKind,
    ) -> Option<StorageFlowState> {
        let admitted = source.failure_entry.as_ref().filter(|(site, _)| *site == occurrence)
            .map_or(source, |(_, admitted)| admitted.as_ref());

        if !admitted.reachable {
            return None;
        }

        let execution = admitted.execution.as_ref().expect("reachable failure retains its admitted execution");

        if !self.execution.invocation_can_fail(execution, occurrence, kind) {
            return None;
        }

        let mut failure = admitted.clone();

        failure.failure_entry = None;

        if let Some(effects) = failure.memory.take_failure_effects(occurrence, kind) {
            self.apply_effect_prefix(
                &mut failure, bray_bound_tree::StorageMutation::InvocationFailure { occurrence, kind },
                bray_bound_tree::StorageValuePhase::Attempt, &effects,
            );
        }

        Some(failure)
    }

    fn resolve_effect_prefix(
        &mut self, state: &StorageFlowState, kind: AnalysisOperationKind,
        execution: &crate::analysis::guarantee::ExecutionState, effects: Option<&[Effect]>, roots: &[Target],
    ) -> Option<crate::analysis::storage_flow::memory::CapturedEffects> {
        let occurrence = kind.occurrence();
        let mut escape_prefix = EffectPrefix::default();
        let escaped = self.address_escapes(state, kind, effects, roots, &mut escape_prefix);

        if effects.is_some_and(<[_]>::is_empty) && escaped.is_empty() && escape_prefix.is_empty() {
            return None;
        }

        let mut invalidation = self.operation_observations(state, kind, None, effects, Some(roots), &escaped);

        // Observing a running task does not establish a new body-entry memory snapshot.
        let observes_running_task = occurrence.expression().is_some_and(|expression| {
            self.execution.awaited_task_frame(execution, expression).is_some()
        });

        if !observes_running_task && let Some(invalidation) = &mut invalidation {
            let memory = occurrence.expression().and_then(|expression| self.memory.operation(expression));

            self.preserve_storage_values(state, &state.memory, memory, effects, invalidation);
        }

        Some(crate::analysis::storage_flow::memory::CapturedEffects {
            mutations: effects.map_or_else(|| vec![Effect::Unknown], <[_]>::to_vec),
            escaped: escaped.into_iter().collect(),
            escape_prefix: escape_prefix.iter().cloned().collect(),
            invalidation: invalidation.expect("resolved call effects have storage observations"),
        })
    }

    fn apply_effect_prefix(
        &mut self,
        state: &mut StorageFlowState,
        mutation: bray_bound_tree::StorageMutation,
        phase: bray_bound_tree::StorageValuePhase,
        effects: &crate::analysis::storage_flow::memory::CapturedEffects,
    ) {
        // The addresses belong to the admitted transition.
        state.memory.escape_regions(effects.escaped.iter().cloned());
        std::sync::Arc::make_mut(&mut state.effect_prefix).extend(effects.escape_prefix.iter().cloned());

        self.record_resolved_effects(&state.memory, &effects.mutations, Some(&mut state.effect_prefix));

        state.memory.apply_effects(
            Some(&effects.mutations),
            &effects.invalidation,
            self.storage,
        );

        state.invalidate_observed_absence(&effects.invalidation, self.storage, &self.input.footprint.discriminants);

        self.refinements.invalidate_for_operation(&mut state.refinements, mutation.occurrence(), self.storage,
            Some(&effects.invalidation), &self.input.footprint.discriminants);

        let execution = state.execution.as_mut().expect("reached outcome retains execution facts");

        self.record_outcome_mutation(mutation, &effects.invalidation);
        self.execution.invalidate_observations(execution, mutation, &effects.invalidation);

        self.record_capture_retirements(execution, mutation, phase,
            bray_bound_tree::StorageValueBoundary::Execution, state.memory.take_retirements());
    }

    pub(super) fn call_effects(
        &self,
        state: &crate::analysis::storage_flow::model::StorageFlowState,
        execution: &crate::analysis::guarantee::ExecutionState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        effects: &[DependencyStorageEffect],
    ) -> Vec<Effect> {
        let read = |slot| crate::analysis::storage_flow::memory::Source::Value(slot).mutation_targets(&state.memory);

        self.resolve_effects(effects, read, |subject, indirect| {
            if indirect && !subject.subject_root().is_static() {
                let ordinal = occurrence.expression().and_then(|expression| {
                    self.execution.semantics.selections().expression(expression)?
                        .invocation_input_ordinal(occurrence, subject.subject_root())
                });

                let entry = execution.admitted_call(occurrence);

                if let Some(captured) = entry.zip(ordinal).and_then(|(entry, ordinal)| entry.argument_storage(ordinal)) {
                    let path = self.dependencies.calls().get(&occurrence)?.storage().subject(subject)?.1;

                    return state.memory.captured_effect_targets(self.request.semantic_values(), captured, path, true);
                }

                if subject.projections().is_empty() {
                    return self.invocation_reference(state, occurrence, subject.subject_root())?
                        .mutation_targets(&state.memory);
                }
            }

            bray_bound_tree::StorageMutationTarget::resolve_locations(
                self.invocation_locations(state, occurrence, subject)?, indirect, read)
        })
    }

    fn call_effect_outcomes(
        &self,
        state: &StorageFlowState,
        execution: &crate::analysis::guarantee::ExecutionState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        contracts: &bray_bound_tree::BoundCallDependencyContracts,
        admitted: &[&bray_symbols::CapturedValueOutcome],
    ) -> EffectOutcomes {
        let admitted = admitted.iter().flat_map(|row| row.effects.iter()).collect::<std::collections::BTreeSet<_>>();

        let entry = execution.admitted_call(occurrence);

        contracts.effect_outcomes().clone().with_selected_failures(contracts.reports().failures(), |index| {
            entry.is_none_or(|entry| entry.admits_failure(index))
        }).try_flat_map_admitted(
            |effect| contracts.captured_values().is_empty() || admitted.contains(effect),
            |effect| Ok::<_, std::convert::Infallible>(
                self.call_effects(state, execution, occurrence, std::slice::from_ref(effect))),
        ).expect("every formal effect resolves to a captured transition or an explicit conservative bound")
    }

    pub(super) fn resolve_effects(
        &self,
        effects: &[DependencyStorageEffect],
        mut read: impl FnMut(Target) -> Option<Vec<bray_bound_tree::StorageMutationTarget>>,
        mut resolve: impl FnMut(&DependencySubject, bool) -> Option<Vec<crate::analysis::storage_flow::memory::MutationTarget>>,
    ) -> Vec<Effect> {
        let mut resolved = Vec::new();

        for effect in effects {
            let (subject, indirect, projections, reads, kind, access_type, addressed) = match effect {
                DependencyStorageEffect::UnboundedMutation { kind, access_type, ambient } => {
                    resolved.push(Effect::unbounded(*kind, *access_type, *ambient));
                    continue;
                }
                DependencyStorageEffect::Escape => {
                    resolved.push(Effect::Escape);
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
                    (subject, false, &[][..], &[][..], *kind, *access_type, *addressed)
                }
                DependencyStorageEffect::Indirect { addressed, reads,
                    reference,
                    projections,
                    kind,
                    access_type,
                } => (reference, true, projections.as_ref(), reads.as_ref(), *kind, *access_type, *addressed),
            };

            let static_subject = subject.subject_root().is_static();

            let suffix = storage_projections(
                self.request.semantic_values(),
                self.dependencies,
                projections,
            );

            let addressed = addressed && suffix.is_some();

            let suffix = if let Some(suffix) = suffix {
                suffix
            } else if static_subject && !indirect && reads.is_empty() {
                Vec::new()
            } else {
                resolved.push(Effect::unbounded(kind, access_type, false));

                continue;
            };

            let targets = if static_subject && !matches!(subject.subject_root(), bray_symbols::DependencySubjectRoot::StaticInstance(..)) {
                self.static_effect_targets(subject).and_then(|targets| {
                    if indirect {
                        targets.into_iter().try_fold(Vec::new(), |mut reached, target| {
                            reached.extend(read(target.address?)?);

                            Some(reached)
                        })
                    } else { Some(targets) }
                })
            } else {
                resolve(subject, indirect)
            }.and_then(|targets|
                bray_bound_tree::StorageMutationTarget::resolve_route(targets, &suffix, reads, &mut read));

            let Some(targets) = targets else {
                if kind == DependencyMutationKind::Atomic {
                    resolved.push(Effect::Atomic);
                    continue;
                }

                resolved.push(Effect::unbounded(kind, access_type, false));

                continue;
            };

            for target in targets {
                let target = if addressed { target } else { bray_bound_tree::StorageMutationTarget::bounded(target.footprint) };

                resolved.push(Effect::through(target, kind, access_type));
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
                Effect::Escape | Effect::Mutation { kind: DependencyMutationKind::Escape, .. } => {}
                Effect::Mutation { address, footprint, kind, access_type } => {
                    self.record_target_effect(memory, footprint, address.as_ref() == Some(footprint), *kind, *access_type, prefix.as_deref_mut());

                    if let Some(operand) = address && operand != footprint {
                        self.record_target_effect(memory, operand, true, *kind, *access_type, prefix.as_deref_mut());
                    }
                }
                Effect::UnboundedMutation { kind, access_type, ambient } => {
                    self.record_effect(DependencyStorageEffect::UnboundedMutation {
                        kind: *kind, access_type: *access_type, ambient: *ambient,
                    }, prefix.as_deref_mut());
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

    pub(super) fn record_target_effect(
        &mut self,
        memory: &crate::analysis::storage_flow::memory::State,
        target: &Target,
        addressed: bool,
        kind: DependencyMutationKind,
        access_type: Option<bray_symbols::TypeId>,
        mut prefix: Option<&mut EffectPrefix>,
    ) {
        if !self.publish && prefix.is_none() {
            return;
        }

        if matches!(target.origin, Origin::Returned { .. })
            && kind != DependencyMutationKind::Atomic
        {
            // Alias bounds retain the possible region, not the disposed value's layout.
            let kind = kind.region_kind();

            let (origins, unbounded) = memory.effect_origins(target);

            if unbounded && kind != DependencyMutationKind::Escape {
                // An independent alternative may reach external storage, without granting freshness.
                self.record_effect(DependencyStorageEffect::UnboundedMutation {
                    kind, access_type, ambient: true,
                }, prefix.as_deref_mut());
            }

            // Preserve every possible input region and the actual ownership transition.
            for origin in origins {
                self.record_target_effect(memory, origin, false, kind, access_type, prefix.as_deref_mut());
            }

            return;
        }

        let effect = match &target.origin {
            Origin::Entry {
                reference,
                projections,
                layer,
                ..
            } => {
                let Some(root) = self.formal_root(reference.root) else {
                    return;
                };

                let source = crate::storage::dependency_projections(&reference.projections);

                // An unportable selector widens the reached region, never the identity
                // of the reference through which this operation obtained access.
                let suffix = projections
                    .iter()
                    .copied()
                    .map_while(crate::storage::dependency_projection)
                    .collect::<Vec<_>>();

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
            Origin::Returned { .. } => {
                Some(DependencyStorageEffect::Unknown)
            }
            Origin::Storage(path) => self.formal_effect(path, kind),
            Origin::Allocation { .. } => None,
            Origin::Static { root, projections, .. } => {
                // An unresolved selector widens within the same static identity.
                let suffix = projections.iter().copied().map_while(crate::storage::dependency_projection).collect::<Vec<_>>();

                Some(DependencyStorageEffect::Direct { addressed: addressed && suffix.len() == projections.len(),
                    subject: DependencySubject::new(*root, suffix),
                    kind,
                    access_type: None,
                })
            }
        };

        let effect = effect.or_else(|| matches!(kind, DependencyMutationKind::Dispose(_) | DependencyMutationKind::Cleanup(_)).then_some(
            DependencyStorageEffect::UnboundedMutation { kind, access_type, ambient: true }
        ));

        if let Some(mut effect) = effect {
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

            self.record_effect(effect, prefix);
        }
    }

    pub(super) fn record_effects(
        &mut self,
        invalidation: &StorageInvalidation,
        kind: DependencyMutationKind,
        prefix: &mut EffectPrefix,
    ) {
        let StorageInvalidation::Paths(paths) = invalidation else {
            self.record_effect(DependencyStorageEffect::unresolved(kind, None), Some(prefix));
            return;
        };

        for path in paths {
            match self.formal_effect(path, kind) {
                Some(DependencyStorageEffect::Unknown) => {
                    self.record_effect(DependencyStorageEffect::unresolved(kind, None), Some(prefix));
                    return;
                }
                Some(effect) => {
                    self.record_effect(effect.into_region(), Some(prefix));
                }
                None => {}
            }
        }
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
            && !borrowed
            && !matches!(kind, DependencyMutationKind::Dispose(_))
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

        let projections = crate::storage::dependency_projections(&path.projections);

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
