use crate::analysis::guarantee::CleanupMutation;
use crate::analysis::storage_flow::memory::Effect;
use bray_bound_tree::{
    StorageAccessId, StorageAccessPlan, StorageAccessPurpose, StorageCleanupAction,
    StorageCleanupActionKind, StorageCleanupRemainder, StorageMutation, StorageReplacementState,
};
use bray_symbols::DependencyFailureKind;
use std::sync::Arc;

use super::core::StorageFlowCollector;
use super::effect_rows::EffectRows;
use super::snapshot::ReplacementState;
use crate::CheckerRequestContext;
use crate::analysis::guarantee::{CleanupObservation, CleanupSelection, CleanupTransition, ExecutionState};
use crate::analysis::model::{AnalysisOperation, AnalysisOperationKind, AnalysisScopeExitPhase};
use crate::analysis::outcomes::CleanupProgress;
use crate::analysis::storage_flow::{
    memory::{EffectOutcomes, State, Target},
    model::StorageFlowState,
};

pub(super) struct CleanupOperands<'a> {
    pub(super) receiver: Option<Target>,
    pub(super) task_path: Option<std::borrow::Cow<'a, bray_bound_tree::StoragePath>>,
    pub(super) frame: Option<Arc<crate::ExecutionDeferredValue>>,
    pub(super) inactive: bool,
    pub(super) inactive_captures: Option<Vec<bray_symbols::TypeId>>,
    pub(super) lifecycle_effects: Option<(EffectOutcomes, EffectRows)>,
}

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn apply_cleanup(
        &mut self,
        state: &mut StorageFlowState,
        execution: &mut ExecutionState,
        point: &AnalysisOperation,
    ) -> std::collections::BTreeSet<bray_bound_tree::BorrowCapabilityId> {
        let operation = point.kind();
        let domain = self.execution;

        let scope_exit = match operation {
            AnalysisOperationKind::ScopeExit { scope, exit, phase: AnalysisScopeExitPhase::LifecycleResolution, .. } => {
                Some(bray_bound_tree::StorageExitPoint::new(scope, exit))
            }
            _ => None,
        };

        let replacement = match operation {
            AnalysisOperationKind::Bound(bray_bound_tree::AnyBoundNodeId::Expression(expression))
            | AnalysisOperationKind::AssignmentInstall(expression) => self.storage.node_plans(expression.into())
                .find(|plan| plan.purpose() == StorageAccessPurpose::Assignment)
                .map(|plan| (expression, plan)),
            _ => None,
        };

        let destination = replacement.and_then(|(expression, plan)| {
            self.assignment_destination(&state.memory, expression, plan.access())?.address.single_location().cloned()
        });

        let pending = CleanupProgress::resume(&mut state.cleanup, scope_exit, point.id());

        let selected = if let Some(remainder) = pending {
            Some(CleanupSelection { accesses: Some(remainder.accesses), after: Some(remainder.after), recoveries: Vec::new(), replacement: None })
        } else if let AnalysisOperationKind::ScopeExit { scope, phase, .. } = operation {
            Some(domain.select_scope_cleanup(state, self.dependencies, scope, phase))
        } else {
            replacement.and_then(|(_, plan)| self.select_replacement(state, plan, destination.as_ref()))
        };

        let Some(mut selected) = selected else {
            return Default::default();
        };

        let displaced = destination.as_ref().into_iter().flat_map(|owner| state.memory.value_loans(owner)).collect();

        let reached = destination.as_ref().filter(|target| target.is_exact())
            .and_then(Target::value_observation_path);

        let roots = selected.accesses.iter().flat_map(|order| order.iter())
            .filter_map(|&access| self.storage.root_identity(access))
            .chain(reached.map(|path| path.root))
            .collect();

        let absent = state.cleanup_entry_absence(&roots);
        let mut trusted = std::mem::take(&mut self.trusted);

        let publish = self.publish;
        let lifecycle = scope_exit.filter(|_| operation.is_normal_cleanup() || state.cleanup.is_some());
        let replacement = replacement.map(|(expression, plan)| (expression, plan.access()));

        selected.replacement = destination;

        if publish {
            self.exits.record_entry(point.id(), &selected, state);
        }

        let mut observe = |observation: CleanupObservation<'_>| {
            if let CleanupObservation::Invocation { lifecycle, proof, .. } = observation {
                trusted.cleanup(operation.node(), lifecycle, proof);
            }
        };

        let mut skipped = None;

        let mut mutate =
            |execution: &mut ExecutionState, transition: CleanupTransition<'_>| {
                let (identity, mutation) = match transition {
                    CleanupTransition::BeginConditional(payloads) => {
                        let mut inactive = state.clone();

                        for payload in payloads {
                            inactive.mark_path_absent(payload, bray_bound_tree::StorageAbsence::NoPayload);
                        }

                        assert!(skipped.replace(inactive).is_none(), "cleanup parts cannot nest a selection");

                        return true;
                    }
                    CleanupTransition::EndConditional { completes } => {
                        let inactive = skipped.take().expect("a conditional cleanup part retains its inactive state");

                        if completes {
                            state.merge(&inactive, crate::analysis::guarantee::ExecutionJoin::Boundary);
                        } else {
                            *state = inactive;
                        }

                        return true;
                    }
                    CleanupTransition::Action(identity, mutation) => (identity, mutation),
                };

                if let CleanupMutation::Repeat(remaining) = mutation {
                    let mut continued = state.clone();

                    continued.execution = Some(execution.clone());
                    continued.cleanup = Some(CleanupProgress::repeat(state.cleanup.as_ref(), remaining));
                    self.cleanup_continuations.push(continued);

                    return true;
                }

                let mut operands = self.cleanup_operands(&state.memory, execution, &mutation, replacement);

                if let CleanupMutation::Lifecycle(call) = &mutation {
                    operands.lifecycle_effects = Some(self.lifecycle_effects(&state.memory, operands.receiver.as_ref(), call));
                }

                if (lifecycle.is_some() || replacement.is_some())
                    && let (StorageMutation::Cleanup(action), Some(order)) = (identity, &selected.accesses)
                    && action.kind != StorageCleanupActionKind::Broadcast
                {
                    self.capture_cleanup_faults(state, execution, operation, lifecycle, order, action, &mutation, &operands);

                    if let CleanupMutation::Lifecycle(call) = &mutation
                        && (!call.contract.normal_completion()
                            || (!call.contract.captured_values().is_empty() && call.outcomes.is_empty()))
                    {
                        return false;
                    }
                }

                self.apply_cleanup_outcomes(state, execution, operation, identity, &mutation, &operands)
            };

        let completes = domain.transition_storage_cleanup(
            execution,
            point,
            &selected,
            &absent,
            publish.then_some(&mut observe as &mut crate::analysis::guarantee::Observer<'_>),
            &mut mutate,
        );

        state.reachable &= completes;

        if publish && matches!(operation, AnalysisOperationKind::ScopeExit { .. }) {
            self.exits.record_phase(point, state, selected);
        }

        self.trusted = trusted;

        displaced
    }

    fn select_replacement(
        &mut self,
        state: &StorageFlowState,
        plan: StorageAccessPlan,
        destination: Option<&Target>,
    ) -> Option<CleanupSelection> {
        let required = self.execution.replacement_cleanup_required(plan.access());

        if !required && !self.publish {
            return None;
        }

        let reached = ReplacementState::new(state, plan, self.storage, destination);
        let selected = required && !reached.recovered && reached.state() != StorageReplacementState::Absent;

        if self.publish {
            self.record_replacement(plan, reached);
        }

        selected.then(|| CleanupSelection { accesses: Some([plan.access()].into()), recoveries: Vec::new(), after: None, replacement: None })
    }

    fn cleanup_operands<'a>(
        &self,
        memory: &State,
        execution: &ExecutionState,
        mutation: &CleanupMutation<'a>,
        replacement: Option<(bray_bound_tree::BoundExpressionId, StorageAccessId)>,
    ) -> CleanupOperands<'a> {
        match mutation {
            CleanupMutation::Opaque { ty, access, path, route, .. } => {
                let receiver = self.cleanup_receiver(memory, *access, *route, replacement);

                let task_path = if replacement.is_some_and(|(_, destination)| destination == *access) {
                    receiver.as_ref().filter(|target| target.is_exact())
                        .and_then(Target::value_observation_path).map(std::borrow::Cow::Owned)
                } else {
                    path.map(std::borrow::Cow::Borrowed)
                };

                let frame = task_path.as_deref().and_then(|path| self.execution.deferred_cleanup_frame(execution, *ty, path));

                let inactive = crate::representation::type_representation(self.request, *ty)
                    == Some(bray_compiler_known::RepresentationRole::Future);

                let inactive_captures = frame.as_ref().filter(|_| inactive)
                    .and_then(|frame| self.execution.inactive_frame_capture_types(execution, frame));

                CleanupOperands { receiver, task_path, frame, inactive, inactive_captures, lifecycle_effects: None }
            }
            CleanupMutation::Lifecycle(call) => CleanupOperands {
                receiver: self.cleanup_receiver(memory, call.access, call.route, replacement),
                task_path: None,
                frame: None,
                inactive: false,
                inactive_captures: None,
                lifecycle_effects: None,
            },
            CleanupMutation::Unchanged | CleanupMutation::Recovery => CleanupOperands {
                receiver: None,
                task_path: None,
                frame: None,
                inactive: false,
                inactive_captures: None,
                lifecycle_effects: None,
            },
            CleanupMutation::Repeat(_) => unreachable!("a repeated region has no separate physical effect"),
        }
    }

    fn apply_cleanup_outcomes(
        &mut self,
        state: &mut StorageFlowState,
        execution: &mut ExecutionState,
        operation: AnalysisOperationKind,
        identity: StorageMutation,
        mutation: &CleanupMutation<'_>,
        operands: &CleanupOperands<'_>,
    ) -> bool {
        let CleanupMutation::Lifecycle(call) = mutation else {
            self.apply_cleanup_action(state, execution, operation, identity, mutation, operands);

            return true;
        };

        let StorageMutation::Cleanup(action) = identity else {
            unreachable!("normal lifecycle selection begins at the reached cleanup action");
        };

        if call.contract.captured_values().is_empty() {
            self.apply_cleanup_action(state, execution, operation, identity, mutation, operands);

            return true;
        }

        let domain = self.execution;
        let mut rows = call.outcomes.iter().map(|(row, _, guard)| (*row, guard.clone()));

        let Some(first) = rows.next() else { return false; };

        let apply = |collector: &mut Self, state: &mut StorageFlowState, execution: &mut ExecutionState,
            (row, guard): (usize, crate::ExecutionCondition)| {
            domain.assume_cleanup_guard(execution, guard);

            collector.apply_cleanup_action(state, execution, operation,
                    StorageMutation::CleanupOutcome { action, row }, mutation, operands);
        };

        let Some(last) = rows.next_back() else {
            apply(self, state, execution, first);

            return true;
        };

        let mut merged = state.clone();
        let mut first_execution = execution.clone();

        apply(self, &mut merged, &mut first_execution, first);
        merged.execution = Some(first_execution);

        for row in rows {
            let mut incoming = state.clone();
            let mut incoming_execution = execution.clone();

            apply(self, &mut incoming, &mut incoming_execution, row);
            incoming.execution = Some(incoming_execution);

            merged.merge(&incoming, crate::analysis::guarantee::ExecutionJoin::Boundary);
        }

        apply(self, state, execution, last);
        state.execution = Some(std::mem::take(execution));

        merged.merge(state, crate::analysis::guarantee::ExecutionJoin::Boundary);

        *execution = merged.execution.take().expect("joined lifecycle outcomes retain their execution state");
        *state = merged;

        true
    }

    #[expect(clippy::too_many_arguments, reason = "a cleanup fault retains the reached action, schedule, and admitted state")]
    fn capture_cleanup_faults(
        &mut self,
        state: &StorageFlowState,
        execution: &ExecutionState,
        operation: AnalysisOperationKind,
        exit: Option<bray_bound_tree::StorageExitPoint>,
        order: &[StorageAccessId],
        action: StorageCleanupAction,
        mutation: &CleanupMutation<'_>,
        operands: &CleanupOperands<'_>,
    ) {
        let (call, failures) = match mutation {
            CleanupMutation::Lifecycle(call) => (Some(call), DependencyFailureKind::ALL.as_slice()),
            CleanupMutation::Opaque { .. } if operands.inactive_captures.as_ref().is_some_and(Vec::is_empty) => return,
            CleanupMutation::Opaque { failures, .. } => (None, *failures),
            CleanupMutation::Unchanged | CleanupMutation::Recovery | CleanupMutation::Repeat(_) => return,
        };

        let mut remainder = None;

        for &kind in failures {
            let bound = call.and_then(|call| call.contract.failure_outcomes());

            let rows = bound.into_iter().flatten().enumerate().filter_map(|(index, row)| {
                (row.covers(kind) && call.is_some_and(|call| call.entry.admits_failure(index))).then_some(Some(index))
            }).chain((bound.is_none() && !call.is_some_and(|call| {
                kind == DependencyFailureKind::Panic && self.request.target_cannot_unwind(
                    bray_bound_tree::BoundCallableTarget::Declaration(call.lifecycle.call.callable),
                )
            })).then_some(None));

            for row in rows {
                let mut execution = execution.clone();

                if call.is_some_and(|call| !self.execution.assume_cleanup_failure(&mut execution, call.entry, row)) {
                    continue;
                }

                let remainder = remainder.get_or_insert_with(|| StorageCleanupRemainder::at(exit, order, action));
                let mut failure = state.clone();
                let identity = StorageMutation::CleanupFailure { action, kind, row };

                self.apply_cleanup_action(&mut failure, &mut execution, operation, identity, mutation, operands);

                if self.publish {
                    self.cleanup_remainders.entry(identity).or_default().insert(remainder.clone());
                }

                failure.cleanup = Some(CleanupProgress::during(state.cleanup.as_ref(), kind, remainder.clone()));
                failure.execution = Some(execution);

                self.cleanup_continuations.push(failure);
            }
        }

    }

    pub(super) fn collect_callable_cleanup(
        &mut self,
        expression: bray_bound_tree::BoundExpressionId,
    ) -> crate::CheckerQueryResult<(), C::UpstreamError> {
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
                return Ok(());
            };

        let template = crate::dependency::resolved_result_template(request, instance, template?, None)?;

        self.synchronous_cleanup.extend(template.synchronous_cleanup().iter().cloned());

        Ok(())
    }

    pub(super) fn record_cleanup_obligations(
        &mut self,
        operation: AnalysisOperationKind,
        mutation: &CleanupMutation<'_>,
        effects: Option<&[Effect]>,
    ) -> bool {

        let (ty, access) = match mutation {
            CleanupMutation::Opaque { ty, access, .. } => (*ty, *access),
            CleanupMutation::Lifecycle(call) => (call.ty, call.access),
            CleanupMutation::Unchanged | CleanupMutation::Recovery | CleanupMutation::Repeat(_) => return false,
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
                    phase: AnalysisScopeExitPhase::TaskCancellationBroadcast, ..
                }) {
                    self.receiver_cleanup.insert(ty);
                }
            } else if synchronous {
                self.synchronous_cleanup
                    .extend(effects.into_iter().flatten().filter_map(|effect| {
                        let ty = match effect {
                            Effect::Mutation { kind, .. } | Effect::UnboundedMutation { kind, .. } => kind.cleanup_type(),
                            Effect::CapturedCleanup { ty, .. } => Some(ty),
                            _ => None,
                        };

                        ty.and_then(|&ty| deferred(ty)).map(bray_symbols::DependencyRequirement::SynchronousCleanup)
                    }));
            }
        }

        inherited_tail
    }
}
