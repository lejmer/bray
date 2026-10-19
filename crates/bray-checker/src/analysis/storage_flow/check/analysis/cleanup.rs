use crate::analysis::storage_flow::memory::Effect;
use bray_bound_tree::{StorageAccessId, StorageInvalidation, StoragePath};
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
                           established: bool| {
            trusted.cleanup(
                operation.node(),
                established,
                !lifecycle.contract.requirements.is_empty(),
            );
        };

        let dependencies = self.dependencies;

        let mut mutate =
            |execution: &mut ExecutionState,
             identity: bray_bound_tree::StorageMutation,
             mutation: crate::analysis::guarantee::CleanupMutation<'_>| {
                use crate::analysis::guarantee::CleanupMutation;

                if let CleanupMutation::Lifecycle(call) = &mutation {
                    let contract = &dependencies.lifecycle_dependencies()[&call.lifecycle.call.callable];
                    if let Some(requirements) = contract.completion_reports() {
                        report_outputs.push((call.access, requirements.to_vec()));
                    }
                    if !self.on_normal_path {
                        if let Some(requirements) = contract.abnormal_reports() {
                            report_outputs.push((call.access, requirements.to_vec()));
                        }
                        if call.lifecycle.call.callable.definition().callable_symbol().kind() == bray_symbols::SymbolKind::Finalizer
                            && crate::representation::type_representation(self.request, call.lifecycle.call.result.completion_type())
                                == Some(bray_compiler_known::RepresentationRole::Result) {
                            report_outputs.push((call.access, contract.requirements().to_vec()));
                        }
                    }
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

                let effects = match mutation {
                    CleanupMutation::Unchanged => Some(Vec::new()),
                    CleanupMutation::Recovery => None,
                    CleanupMutation::Opaque { ty, .. } => Some(vec![Effect::Cleanup(ty)]),
                    CleanupMutation::Lifecycle(ref call) => {
                        self.lifecycle_effects(&memory, call.access, call.path, call.lifecycle)
                    }
                };

                let inherited_tail = match &mutation {
                    CleanupMutation::Opaque { access, .. } => Some(*access),
                    CleanupMutation::Lifecycle(call) => Some(call.access),
                    CleanupMutation::Unchanged | CleanupMutation::Recovery => None,
                }
                .is_some_and(|access| {
                    matches!(operation, AnalysisOperationKind::ScopeExit { .. })
                        && self
                            .input
                            .terminal_receiver
                            .as_ref()
                            .is_some_and(|receiver| {
                                self.storage.root_identity(access) == Some(receiver.root)
                            })
                });

                if publish && inherited_tail {
                    let ty = match &mutation {
                        CleanupMutation::Opaque { ty, .. } => *ty,
                        CleanupMutation::Lifecycle(call) => call.ty,
                        CleanupMutation::Unchanged | CleanupMutation::Recovery => {
                            unreachable!("receiver tail has a selected cleanup operation")
                        }
                    };

                    self.receiver_cleanup.insert(ty);
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

                self.record_escape_effects(
                    &mut memory,
                    effects.as_deref(),
                    receiver.as_ref().map(std::slice::from_ref).unwrap_or(&[]),
                    false,
                );

                let mut invalidation = match effects.as_deref() {
                    Some(effects)
                        if !effects.iter().any(|effect| {
                            matches!(effect, Effect::Unknown | Effect::Cleanup(_))
                        }) =>
                    {
                        self.resolved_observations(&memory, effects, receiver.iter().cloned())
                    }
                    _ => receiver.map_or_else(StorageInvalidation::unknown, |receiver| {
                        self.opaque_observations(&memory, [receiver])
                    }),
                };

                self.preserve_borrowed_carriers(state, &memory, &mut invalidation);

                if let Some(call) = &invocation
                    && call.completes
                {
                    self.prepare_cleanup_references(&mut memory, operation.occurrence(), call);
                }

                if let Some(effects) = &effects {
                    self.record_resolved_effects(effects);
                } else {
                    self.record_effects(
                        &StorageInvalidation::unknown(),
                        DependencyMutationKind::Retire,
                    );
                }

                self.apply_resolved_effects(&mut memory, effects.as_deref(), &invalidation);

                if invocation.as_ref().is_some_and(|call| call.completes) {
                    memory.complete_transition(operation.occurrence());
                }

                if let (Some(call), Some(effects)) = (&invocation, &effects) {
                    self.cleanup_observations(&memory, call, effects, &mut invalidation);
                }

                if publish {
                    self.cleanup_mutations
                        .entry(identity)
                        .and_modify(|previous| previous.merge(&invalidation))
                        // Publication owns observations while the current proof transition consumes them.
                        .or_insert_with(|| invalidation.clone());
                }

                domain.invalidate_observations(execution, identity, &invalidation, false, false);
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
        for (access, requirements) in report_outputs {
            self.retain_cleanup_reports(state, access, &requirements);
        }
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
                let path = self
                    .input
                    .lifecycle_paths
                    .get(&(call.lifecycle.call.callable, subject.clone()))?;

                memory.reference_location(receiver.clone(), path, None)
            };

            let (Some(source), Some(destination)) =
                (location(relation.entry()), location(relation.exit()))
            else {
                continue;
            };

            self.stage_reference(
                memory,
                occurrence,
                crate::analysis::storage_flow::memory::Source::Value(source),
                destination,
                reference,
            );
        }
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
                .reference_target(memory, self.storage, access)?
                .project(&path.projections[base.projections.len()..]),
        )
    }

    fn cleanup_observations(
        &self,
        memory: &State,
        call: &crate::analysis::guarantee::LifecycleInvocation<'_>,
        effects: &[Effect],
        invalidation: &mut StorageInvalidation,
    ) {
        let Some(path) = call.path else {
            return;
        };

        let contracts = &self.dependencies.lifecycle_dependencies()[&call.lifecycle.call.callable];

        let Some(receiver) = self.cleanup_receiver(memory, call.access, call.path) else {
            return;
        };

        for relation in contracts.borrows() {
            if relation.kind() != bray_symbols::DependencyReferenceKind::Borrow
                || relation.dereferences() != 0
                || !relation.target_projections().is_empty()
                || relation.exit().subject_root() != DependencySubjectRoot::Receiver
                || (!call.completes && relation.entry() != relation.exit())
            {
                continue;
            }

            let Some(prepared) = self
                .input
                .lifecycle_paths
                .get(&(call.lifecycle.call.callable, relation.exit().clone()))
            else {
                continue;
            };

            let Some(location) = memory.reference_location(receiver.clone(), prepared, None) else {
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
                &location,
                call.completes && relation.entry() != relation.exit(),
            );
        }
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
                if subject.subject_root() != DependencySubjectRoot::Receiver {
                    return None;
                }

                let receiver = self.cleanup_receiver(memory, access, path)?;

                if subject.projections().is_empty() {
                    return Some(vec![receiver]);
                }

                let prepared = self
                    .input
                    .lifecycle_paths
                    .get(&(lifecycle.call.callable, subject.clone()))?;

                let location = memory.reference_location(receiver, prepared, None)?;

                if indirect {
                    memory.referenced_regions(&location)
                } else {
                    Some(vec![location])
                }
            }),
        )
    }
}
