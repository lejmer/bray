use crate::analysis::storage_flow::memory::Effect;
use bray_bound_tree::{StorageAccessId, StorageInvalidation, StorageMutation, StoragePath};
use bray_symbols::{DependencyMutationKind, DependencySubjectRoot};

use super::core::StorageFlowCollector;
use crate::CheckerRequestContext;
use crate::analysis::guarantee::{ExecutionState, Lifecycle};
use crate::analysis::model::AnalysisOperationKind;
use crate::analysis::storage_flow::{
    memory::{State, Target},
    model::StorageFlowState,
};

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn apply_cleanup(
        &mut self,
        state: &mut StorageFlowState,
        execution: &mut ExecutionState,
        operation: AnalysisOperationKind,
    ) {
        let domain = self.execution;
        let mut memory = std::mem::take(&mut state.memory);
        let mut trusted = std::mem::take(&mut self.trusted);

        let publish = self.publish;
        let mut report_outputs = Vec::new();

        let mut observe = |_: _,
                           _: _,
                           _: _,
                           lifecycle: &Lifecycle,
                           _: bool,
                           _: &crate::ExecutionCallEvidence,
                           proof: &Result<(), crate::UnprovenExecutionCondition>| {
            trusted.cleanup(
                operation.node(),
                lifecycle,
                proof,
            );
        };

        let dependencies = self.dependencies;

        let mut mutate =
            |execution: &mut ExecutionState,
             identity: bray_bound_tree::StorageMutation,
             mutation: crate::analysis::guarantee::CleanupMutation<'_>| {
                use crate::analysis::guarantee::CleanupMutation;

                if let CleanupMutation::Lifecycle(call) = &mutation {
                    self.collect_lifecycle_reports(operation, call, &mut report_outputs);
                }

                let invocation = match &mutation {
                    CleanupMutation::Lifecycle(call) => Some(call),
                    _ => None,
                };

                let receiver = match &mutation {
                    CleanupMutation::Opaque { access, path, .. } => {
                        self.cleanup_receiver(&memory, *access, *path)
                    }
                    CleanupMutation::Lifecycle(call) => {
                        self.cleanup_receiver(&memory, call.access, call.path)
                    }
                    CleanupMutation::Unchanged | CleanupMutation::Recovery => None,
                };

                let task_path = match &mutation {
                    CleanupMutation::Opaque { path, .. } => *path,
                    _ => None,
                };

                let task_sources = match &mutation {
                    CleanupMutation::Opaque { ty, .. } => task_path
                        .and_then(|path| domain.deferred_cleanup_invocations(execution, *ty, path)),
                    _ => None,
                };

                let task_source = task_sources
                    .as_ref()
                    .and(receiver.clone())
                    .map(crate::analysis::storage_flow::memory::Source::Value);

                let inactive_captures = match &mutation {
                    CleanupMutation::Opaque { ty, .. } => task_path
                        .and_then(|path| domain.inactive_future_captures(execution, *ty, path)),
                    _ => None,
                };

                let inactive_broadcast = inactive_captures.is_some()
                    && matches!(identity, StorageMutation::Cancellation { .. })
                    && task_path.is_some_and(|path| {
                        memory
                            .unexposed_storage(std::slice::from_ref(path), [])
                            .next()
                            .is_some()
                    });

                let broadcast_frames = if matches!(identity, StorageMutation::Cancellation { .. }) {
                    task_path
                        .into_iter()
                        .flat_map(|path| memory.unexposed_deferred_paths(path))
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };

                let mut effects = match mutation {
                    CleanupMutation::Unchanged => Some(Vec::new()),
                    CleanupMutation::Recovery => None,
                    CleanupMutation::Opaque { ty, .. } => Some(
                        inactive_captures
                            .as_ref()
                            .and_then(|_| memory.deferred_cleanup_effects(task_source.as_ref()?, task_sources.as_ref()?, false))
                            .or_else(|| {
                                super::effects::task_effects(
                                    memory.deferred_effects(
                                        task_source.as_ref()?,
                                        task_sources.as_ref()?,
                                    ),
                                    memory.deferred_cleanup_effects(task_source.as_ref()?, task_sources.as_ref()?, true)?,
                                )
                            })
                            .or_else(|| {
                                self.buffer_cleanup_effects(&memory, receiver.as_ref()?, ty)
                            })
                            .unwrap_or_else(|| memory.cleanup_effects(receiver.as_ref(), ty)),
                    ),
                    CleanupMutation::Lifecycle(ref call) => {
                        self.lifecycle_effects(&memory, call.access, call.path, call.lifecycle)
                    }
                };

                let inherited_tail =
                    self.record_cleanup_obligations(operation, &mutation, effects.as_deref());

                if let Some(report) = self.inactive_cleanup_report(
                    operation,
                    &mutation,
                    inactive_captures.as_deref(),
                    inherited_tail,
                ) {
                    report_outputs.push(report);
                }

                if inherited_tail && matches!(operation, AnalysisOperationKind::ScopeExit {
                    phase: crate::analysis::model::AnalysisScopeExitPhase::TaskCancellationBroadcast, ..
                }) {
                    // Broadcast work can affect external state without invoking the receiver's destructor.
                    effects = Some(vec![Effect::Unknown, Effect::Escape]);
                }

                // Remaining receiver parts use the selected tail disposition. Their
                // ordinary finalizers execute after the body, while abandonment omits them.
                if !inherited_tail {
                    self.check_terminal_effects(
                        state,
                        &memory,
                        operation,
                        effects.as_deref(),
                        receiver.iter().cloned(),
                    );
                }

                let escaped = self.escape_effects(
                    &memory,
                    effects.as_deref(),
                    receiver.as_slice(),
                    false,
                    self.on_normal_path,
                );

                memory.escape_regions(escaped);

                let mut invalidation =
                    self.cleanup_invalidation(&memory, effects.as_deref(), receiver.as_ref());

                if (task_source.is_some() || inactive_broadcast)
                    && let Some(path) = task_path
                    && let Some(effects) = &effects
                {
                    preserve_run_descriptor(
                        &memory,
                        path,
                        effects,
                        inactive_broadcast,
                        &mut invalidation,
                    );
                }

                // Broadcast exposes captures, not the previously unexposed run handles.
                for frame in &broadcast_frames {
                    preserve_run_descriptor(&memory, frame, &[], true, &mut invalidation);
                }

                self.preserve_storage_values(state, &memory, None, effects.as_deref(), &mut invalidation);

                if let Some(call) = &invocation
                    && call.completes
                {
                    self.prepare_cleanup_references(&mut memory, operation.occurrence(), call);
                }

                if let Some(effects) = &effects {
                    self.record_resolved_effects(&memory, effects, self.on_normal_path);
                } else {
                    self.record_effects(
                        &StorageInvalidation::unknown(),
                        DependencyMutationKind::Retire,
                    );
                }

                memory.apply_effects(effects.as_deref(), &invalidation, self.storage);

                if invocation.as_ref().is_some_and(|call| call.completes) {
                    memory.complete_transition(operation.occurrence());
                }

                let completion = if let (Some(call), Some(effects)) = (&invocation, &effects) {
                    self.cleanup_observations(&memory, call, effects, &mut invalidation)
                } else {
                    bray_bound_tree::StorageCompletion::default()
                };

                if publish {
                    if invocation.is_some() {
                        self.completions
                            .entry(identity)
                            .and_modify(|previous| previous.merge(&completion))
                            .or_insert_with(|| completion.clone());
                    }

                    self.cleanup_mutations
                        .entry(identity)
                        .and_modify(|previous| previous.merge(&invalidation))
                        // Publication owns observations while the current proof transition consumes them.
                        .or_insert_with(|| invalidation.clone());
                }

                domain.invalidate_observations(execution, identity, &invalidation, false, false);
                domain.observe_references(execution, completion.references());
            };

        domain.transition_storage_cleanup(
            execution,
            state,
            dependencies,
            operation,
            publish.then_some(&mut observe as &mut crate::analysis::guarantee::Observer<'_>),
            &mut mutate,
        );

        self.trusted = trusted;
        state.memory = memory;

        for (access, requirements, pending) in report_outputs {
            self.retain_cleanup_reports(state, access, &requirements, pending);
        }
    }

    pub(super) fn collect_callable_cleanup(
        &mut self,
        expression: bray_bound_tree::BoundExpressionId,
    ) {
        if self.query_failure.is_some() {
            return;
        }

        let request = self.request;

        let (instance, template) =
            if let Some(bray_bound_tree::BoundExpression::AnonymousCallable(body)) =
                request.view().expression(expression)
            {
                (None, request.anonymous_result_dependencies(body.unit()))
            } else if let Some(bray_bound_tree::SemanticSelection::CallableReference(instance)) =
                self.execution.semantics.selections().expression(expression)
                && !request
                    .semantic_values()
                    .generic_substitution_data(instance.substitution())
                    .bindings()
                    .is_empty()
            {
                (
                    Some(*instance),
                    request.callable_result_dependencies(instance.definition().callable_symbol()),
                )
            } else {
                return;
            };

        match template.and_then(|template| {
            crate::dependency::resolved_result_template(request, instance, template, None)
        }) {
            Ok(template) => self
                .synchronous_cleanup
                .extend(template.synchronous_cleanup().iter().cloned()),
            Err(error) => self.query_failure = Some(error),
        }
    }

    fn record_cleanup_obligations(
        &mut self,
        operation: AnalysisOperationKind,
        mutation: &crate::analysis::guarantee::CleanupMutation<'_>,
        effects: Option<&[Effect]>,
    ) -> bool {
        use crate::analysis::guarantee::CleanupMutation;

        let (ty, access) = match mutation {
            CleanupMutation::Opaque { ty, access, .. } => (*ty, *access),
            CleanupMutation::Lifecycle(call) => (call.ty, call.access),
            CleanupMutation::Unchanged | CleanupMutation::Recovery => return false,
        };

        let inherited_tail = matches!(operation, AnalysisOperationKind::ScopeExit { .. })
            && self
                .input
                .terminal_receiver
                .as_ref()
                .is_some_and(|receiver| self.storage.root_identity(access) == Some(receiver.root));

        let synchronous = !matches!(
            self.request.unit().root(),
            bray_bound_tree::BoundUnitRoot::CallableBody {
                execution: bray_symbols::CallableExecution::Asynchronous,
                ..
            } | bray_bound_tree::BoundUnitRoot::AnonymousCallable {
                execution: bray_symbols::CallableExecution::Asynchronous,
                ..
            }
        );

        let values = self.request.semantic_values();
        let deferred = |ty| (!values.type_is_concrete(ty)).then_some(ty);

        if self.publish {
            if let CleanupMutation::Lifecycle(call) = mutation {
                let contract =
                    &self.dependencies.lifecycle_dependencies()[&call.lifecycle.call.callable];

                if call
                    .lifecycle
                    .call
                    .callable
                    .definition()
                    .callable_symbol()
                    .kind()
                    == bray_symbols::SymbolKind::Finalizer
                {
                    if !contract.synchronous_cleanup().is_empty() {
                        self.synchronous_cleanup.insert(
                            bray_symbols::DependencyRequirement::FinalizerBodyCleanup(call.ty),
                        );
                    }
                } else {
                    self.synchronous_cleanup
                        .extend(contract.synchronous_cleanup().iter().cloned());
                }
            }

            if inherited_tail {
                if !matches!(operation, AnalysisOperationKind::ScopeExit {
                    phase: crate::analysis::model::AnalysisScopeExitPhase::TaskCancellationBroadcast, ..
                }) {
                    self.receiver_cleanup.insert(ty);
                }
            } else if synchronous {
                self.synchronous_cleanup
                    .extend(effects.into_iter().flatten().filter_map(|effect| {
                        match effect {
                            Effect::Mutation(_, DependencyMutationKind::Dispose(ty) | DependencyMutationKind::Cleanup(ty), _)
                            | Effect::UnboundedMutation { kind: DependencyMutationKind::Dispose(ty) | DependencyMutationKind::Cleanup(ty), .. } => deferred(*ty)
                                .map(bray_symbols::DependencyRequirement::SynchronousCleanup),
                            _ => None,
                        }
                    }));
            }
        }

        inherited_tail
    }

    fn prepare_cleanup_references(
        &self,
        memory: &mut State,
        occurrence: bray_bound_tree::BoundExecutionSite,
        call: &crate::analysis::guarantee::LifecycleInvocation<'_>,
    ) {
        memory.begin_transition(occurrence);

        let Some(receiver) = self.cleanup_receiver(memory, call.access, call.path) else {
            return;
        };

        for reference in &self.input.lifecycle_references[&call.lifecycle.call.callable] {
            let relation = reference.relation();

            if relation.exit().subject_root() != DependencySubjectRoot::Receiver
                || relation.exit().projections().is_empty()
            {
                continue;
            }

            let location = |subject: &bray_symbols::DependencySubject| {
                let (_, path) = self
                    .input
                    .lifecycle_paths
                    .get(&(call.lifecycle.call.callable, subject.clone()))?;

                memory.reference_location(self.request.semantic_values(), receiver.clone(), path, None)
            };

            let Some(destination) = location(relation.exit()) else {
                continue;
            };

            let mut logical = call
                .path
                .expect("cleanup receiver has its logical path")
                .clone();

            logical.projections = logical
                .projections
                .iter()
                .copied()
                .chain(
                    crate::storage::storage_projections(
                        self.request.semantic_values(),
                        self.dependencies,
                        relation.exit().projections(),
                    )
                    .expect("prepared lifecycle reference retains its projections"),
                )
                .collect();

            let Some(entry) = relation.entry() else {
                self.stage_independent_reference(memory, occurrence, destination, logical, reference);

                continue;
            };

            let Some(source) = location(entry) else {
                continue;
            };

            self.stage_reference(
                memory,
                occurrence,
                crate::analysis::storage_flow::memory::Source::Value(source),
                destination,
                &logical,
                reference,
            );
        }
    }

    fn buffer_cleanup_effects(
        &self,
        memory: &State,
        receiver: &Target,
        ty: bray_symbols::TypeId,
    ) -> Option<Vec<Effect>> {
        let (fields, elements) = self.input.raw_buffers.get(&ty)?;

        let targets = memory.owned_regions(
            receiver,
            bray_bound_tree::StorageProjection::ProductField(fields.pointer),
        )?;

        let effects = targets
            .into_iter()
            .flat_map(|target| {
                let mut effects = if *elements {
                    memory.cleanup_effects(Some(&target), fields.element)
                } else {
                    Vec::new()
                };
                effects.push(Effect::Mutation(target, DependencyMutationKind::Retire, None));
                effects
            })
            .collect::<Vec<_>>();

        Some(effects)
    }

    fn cleanup_receiver(
        &self,
        memory: &State,
        access: StorageAccessId,
        path: Option<&StoragePath>,
    ) -> Option<Target> {
        let path = path?;
        let base = self.storage.logical_path(access)?;

        Some(
            self.input
                .reference_target(self.request.semantic_values(), memory, self.storage, access)?
                .project(&path.projections[base.projections.len()..]),
        )
    }

    fn cleanup_observations(
        &self,
        memory: &State,
        call: &crate::analysis::guarantee::LifecycleInvocation<'_>,
        effects: &[Effect],
        invalidation: &mut StorageInvalidation,
    ) -> bray_bound_tree::StorageCompletion {
        let mut observations = std::collections::BTreeMap::new();

        let Some(path) = call.path else {
            return bray_bound_tree::StorageCompletion::default();
        };

        let contracts = &self.dependencies.lifecycle_dependencies()[&call.lifecycle.call.callable];

        let Some(receiver) = self.cleanup_receiver(memory, call.access, call.path) else {
            return bray_bound_tree::StorageCompletion::default();
        };

        for relation in contracts.borrows() {
            if !relation.is_exact()
                || relation.kind() != bray_symbols::DependencyReferenceKind::Borrow
                || relation.dereferences() != 0
                || !relation.target_projections().is_empty()
                || relation.exit().subject_root() != DependencySubjectRoot::Receiver
                || (!call.completes && relation.entry() != Some(relation.exit()))
            {
                continue;
            }

            let Some((_, prepared)) = self
                .input
                .lifecycle_paths
                .get(&(call.lifecycle.call.callable, relation.exit().clone()))
            else {
                continue;
            };

            let Some(location) = memory.reference_location(self.request.semantic_values(), receiver.clone(), prepared, None) else {
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

            self.reference_observation(
                memory,
                effects,
                invalidation,
                call.access,
                observed,
                &crate::analysis::storage_flow::memory::Source::Value(location),
                call.completes && relation.entry() != Some(relation.exit()),
            );
        }

        // A normal destructor can establish facts about a borrowed field even
        // when it consumes that field instead of forwarding it into another value.
        if call.completes {
            for subject in contracts.reference_subjects() {
                if subject.subject_root() != DependencySubjectRoot::Receiver {
                    continue;
                }

                let Some((ty, prepared)) = self
                    .input
                    .lifecycle_paths
                    .get(&(call.lifecycle.call.callable, subject.clone()))
                else {
                    continue;
                };

                if crate::representation::reference_kind(self.request, *ty)
                    != Some(bray_symbols::DependencyReferenceKind::Borrow)
                {
                    continue;
                }

                let Some(location) = memory.reference_location(self.request.semantic_values(), receiver.clone(), prepared, None)
                else {
                    continue;
                };

                let Some(targets) = memory.value_targets(&location) else {
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
                    target.clone(),
                );
            }
        }

        bray_bound_tree::StorageCompletion::new(observations, Default::default())
    }

    fn lifecycle_effects(
        &self,
        memory: &State,
        access: StorageAccessId,
        path: Option<&StoragePath>,
        lifecycle: &Lifecycle,
    ) -> Option<Vec<Effect>> {
        let contracts = &self.dependencies.lifecycle_dependencies()[&lifecycle.call.callable];

        Some(
            self.resolve_effects(contracts.effects(), |subject, indirect| {
                if let Some(location) = self.static_location(subject) {
                    return if indirect {
                        memory.referenced_regions(&location)
                    } else {
                        Some(vec![location])
                    };
                }

                if subject.subject_root() != DependencySubjectRoot::Receiver {
                    return None;
                }

                let receiver = self.cleanup_receiver(memory, access, path)?;

                if subject.projections().is_empty() {
                    return Some(vec![receiver]);
                }

                let (_, prepared) = self
                    .input
                    .lifecycle_paths
                    .get(&(lifecycle.call.callable, subject.clone()))?;

                let location = memory.reference_location(self.request.semantic_values(), receiver, prepared, None)?;

                if indirect {
                    memory.referenced_regions(&location)
                } else {
                    Some(vec![location])
                }
            }),
        )
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
        .any(|effect| matches!(effect, Effect::Escape | Effect::UnboundedMutation { kind: DependencyMutationKind::Dispose(_) | DependencyMutationKind::Cleanup(_), .. }))
    {
        return;
    }

    if memory
        .unexposed_storage(
            std::slice::from_ref(path),
            effects
                .iter()
                .filter_map(Effect::mutation)
                .map(|(target, _)| target.clone()),
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
