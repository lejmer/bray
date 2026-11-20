use crate::analysis::guarantee::CleanupMutation;
use crate::analysis::storage_flow::memory::Effect;
use bray_bound_tree::{
    StorageAccessId, StorageAccessPlan, StorageAccessPurpose, StorageCleanupAction,
    StorageCleanupActionKind, StorageCleanupRemainder, StorageMutation, StorageReplacementState,
};
use bray_symbols::{DependencyFailureKind, DependencyMutationKind};
use std::sync::Arc;

use super::core::StorageFlowCollector;
use super::effect_rows::EffectRows;
use super::snapshot::ReplacementState;
use crate::CheckerRequestContext;
use crate::analysis::guarantee::{CleanupObservation, CleanupSelection, ExecutionState};
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

#[derive(Clone, Copy)]
enum CleanupProfilePhase {
    Schedule,
    Operands,
    PhysicalCopy,
    LogicalCopy,
    FailureEffects,
    NormalEffects,
    Join,
    Exit,
}

static CLEANUP_PROFILE: [std::sync::atomic::AtomicU64; 8] =
    [const { std::sync::atomic::AtomicU64::new(0) }; 8];
static CLEANUP_ACTION_PROFILE: [std::sync::atomic::AtomicU64; 6] =
    [const { std::sync::atomic::AtomicU64::new(0) }; 6];

pub(super) fn record_cleanup_action_profile(times: [u128; 5]) {
    for (total, micros) in CLEANUP_ACTION_PROFILE.iter().zip(times.into_iter().chain([1])) {
        total.fetch_add(micros as u64, std::sync::atomic::Ordering::Relaxed);
    }
}

pub(super) fn cleanup_profile(unit: bray_bound_tree::BoundUnitId) -> bool {
    static UNIT: std::sync::OnceLock<Option<u32>> = std::sync::OnceLock::new();

    UNIT.get_or_init(|| std::env::var("BRA_694_PROFILE_CLEANUP").ok()?.parse().ok()) == &Some(unit.raw())
}

fn profile<T>(enabled: bool, phase: CleanupProfilePhase, action: impl FnOnce() -> T) -> T {
    let start = enabled.then(std::time::Instant::now);
    let value = action();

    if let Some(start) = start {
        CLEANUP_PROFILE[phase as usize].fetch_add(start.elapsed().as_micros() as u64, std::sync::atomic::Ordering::Relaxed);
    }

    value
}

pub(super) fn report_cleanup_profile(unit: bray_bound_tree::BoundUnitId) {
    if cleanup_profile(unit) {
        let micros = CLEANUP_PROFILE.iter().map(|time| time.swap(0, std::sync::atomic::Ordering::Relaxed))
            .collect::<Vec<_>>();

        eprintln!("cleanup profile {unit:?} schedule/operands/physical-copy/logical-copy/failure-effects/normal-effects/join/exit us={micros:?}");

        let sections = CLEANUP_ACTION_PROFILE.iter().map(|time| time.swap(0, std::sync::atomic::Ordering::Relaxed))
            .collect::<Vec<_>>();

        eprintln!("cleanup action totals {unit:?} selection/observation/memory/proof/publication us and count={sections:?}");
    }
}

impl<C: CheckerRequestContext + ?Sized> StorageFlowCollector<'_, C> {
    pub(super) fn apply_cleanup(
        &mut self,
        state: &mut StorageFlowState,
        execution: &mut ExecutionState,
        point: &AnalysisOperation,
    ) {
        let operation = point.kind();
        let domain = self.execution;
        let profiling = cleanup_profile(self.storage.unit());

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

        let selected = profile(profiling, CleanupProfilePhase::Schedule, || {
            if let Some(remainder) = pending {
                Some(CleanupSelection { accesses: Some(remainder.accesses), after: Some(remainder.after), recoveries: Vec::new(), replacement: None })
            } else if let AnalysisOperationKind::ScopeExit { scope, phase, .. } = operation {
                Some(domain.select_scope_cleanup(state, self.dependencies, scope, phase))
            } else {
                replacement.and_then(|(_, plan)| self.select_replacement(state, plan, destination.as_ref()))
            }
        });

        let Some(mut selected) = selected else {
            return;
        };

        let absent = std::sync::Arc::clone(&state.definitely_absent);
        let mut trusted = std::mem::take(&mut self.trusted);

        let publish = self.publish;

        let lifecycle = scope_exit.filter(|_| operation.is_normal_cleanup() || state.cleanup.is_some());

        let replacement = replacement.map(|(expression, plan)| (expression, plan.access()));

        selected.replacement = destination;

        if publish {
            self.exits.record_entry(point.id(), &selected, &absent);
        }

        let mut observe = |observation: CleanupObservation<'_>| {
            if let CleanupObservation::Invocation { lifecycle, proof, .. } = observation {
                trusted.cleanup(operation.node(), lifecycle, proof);
            }
        };

        let mut mutate =
            |execution: &mut ExecutionState,
             identity: bray_bound_tree::StorageMutation,
             mutation: crate::analysis::guarantee::CleanupMutation<'_>,
             conditional: bool| {
                if let CleanupMutation::Repeat(remaining) = mutation {
                    let mut continued = profile(profiling, CleanupProfilePhase::PhysicalCopy, || state.clone());

                    continued.execution = Some(profile(profiling, CleanupProfilePhase::LogicalCopy, || execution.clone()));
                    continued.cleanup = Some(CleanupProgress::repeat(state.cleanup.as_ref(), remaining));
                    self.cleanup_continuations.push(continued);

                    return true;
                }

                let mut operands = profile(profiling, CleanupProfilePhase::Operands, ||
                    self.cleanup_operands(&state.memory, execution, &mutation, replacement));

                let skipped = conditional.then(|| profile(profiling, CleanupProfilePhase::PhysicalCopy, || state.memory.clone()));

                if let CleanupMutation::Lifecycle(call) = &mutation {
                    operands.lifecycle_effects = Some(profile(profiling, CleanupProfilePhase::Operands, ||
                        self.lifecycle_effects(&state.memory, operands.receiver.as_ref(), call)));
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
                        if let Some(skipped) = skipped {
                            state.memory = skipped;
                        }

                        return false;
                    }
                }

                let completes = self.apply_cleanup_outcomes(state, execution, operation, identity, &mutation, &operands);

                if let Some(skipped) = skipped {
                    // The guard's inactive branch retains its incoming physical contents.
                    profile(profiling, CleanupProfilePhase::Join, ||
                        state.memory.merge(&skipped, &Default::default(), &Default::default()));
                }

                completes
            };

        let completes = profile(profiling, CleanupProfilePhase::Exit, || domain.transition_storage_cleanup(
            execution,
            point,
            &selected,
            absent.keys(),
            publish.then_some(&mut observe as &mut crate::analysis::guarantee::Observer<'_>),
            &mut mutate,
        ));

        state.reachable &= completes;

        if publish && matches!(operation, AnalysisOperationKind::ScopeExit { .. }) {
            self.exits.record_phase(point, state, selected);
        }

        self.trusted = trusted;
    }

    fn select_replacement(
        &mut self,
        state: &StorageFlowState,
        plan: StorageAccessPlan,
        destination: Option<&Target>,
    ) -> Option<CleanupSelection> {
        let required = self.execution.replacement_cleanup_required(plan.access());

        if !required && !self.publish { return None; }

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
        let profiling = cleanup_profile(self.storage.unit());

        let CleanupMutation::Lifecycle(call) = mutation else {
            profile(profiling, CleanupProfilePhase::NormalEffects, ||
                self.apply_cleanup_action(state, execution, operation, identity, mutation, operands));

            return true;
        };

        let StorageMutation::Cleanup(action) = identity else {
            unreachable!("normal lifecycle selection begins at the reached cleanup action");
        };

        if call.contract.captured_values().is_empty() {
            profile(profiling, CleanupProfilePhase::NormalEffects, ||
                self.apply_cleanup_action(state, execution, operation, identity, mutation, operands));

            return true;
        }

        let domain = self.execution;
        let mut rows = call.outcomes.iter().map(|(row, _, guard)| (*row, guard.clone()));

        let Some(first) = rows.next() else { return false; };

        let apply = |collector: &mut Self, state: &mut StorageFlowState, execution: &mut ExecutionState,
            (row, guard): (usize, crate::ExecutionCondition)| {
            domain.assume_cleanup_guard(execution, guard);

            profile(profiling, CleanupProfilePhase::NormalEffects, ||
                collector.apply_cleanup_action(state, execution, operation,
                    StorageMutation::CleanupOutcome { action, row }, mutation, operands));
        };

        let Some(last) = rows.next_back() else {
            apply(self, state, execution, first);

            return true;
        };

        let mut merged = profile(profiling, CleanupProfilePhase::PhysicalCopy, || state.clone());
        let mut first_execution = profile(profiling, CleanupProfilePhase::LogicalCopy, || execution.clone());

        apply(self, &mut merged, &mut first_execution, first);
        merged.execution = Some(first_execution);

        for row in rows {
            let mut incoming = profile(profiling, CleanupProfilePhase::PhysicalCopy, || state.clone());
            let mut incoming_execution = profile(profiling, CleanupProfilePhase::LogicalCopy, || execution.clone());

            apply(self, &mut incoming, &mut incoming_execution, row);
            incoming.execution = Some(incoming_execution);

            profile(profiling, CleanupProfilePhase::Join, ||
                merged.merge(&incoming, crate::analysis::guarantee::ExecutionJoin::Boundary));
        }

        apply(self, state, execution, last);
        state.execution = Some(std::mem::take(execution));

        profile(profiling, CleanupProfilePhase::Join, ||
            merged.merge(state, crate::analysis::guarantee::ExecutionJoin::Boundary));

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
        let profiling = cleanup_profile(self.storage.unit());

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
                let mut execution = profile(profiling, CleanupProfilePhase::LogicalCopy, || execution.clone());

                if call.is_some_and(|call| !self.execution.assume_cleanup_failure(&mut execution, call.entry, row)) {
                    continue;
                }

                let remainder = remainder.get_or_insert_with(|| StorageCleanupRemainder::at(exit, order, action));
                let mut failure = profile(profiling, CleanupProfilePhase::PhysicalCopy, || state.clone());
                let identity = StorageMutation::CleanupFailure { action, kind, row };

                profile(profiling, CleanupProfilePhase::FailureEffects, ||
                    self.apply_cleanup_action(&mut failure, &mut execution, operation, identity, mutation, operands));

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
        mutation: &crate::analysis::guarantee::CleanupMutation<'_>,
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
                    phase: crate::analysis::model::AnalysisScopeExitPhase::TaskCancellationBroadcast, ..
                }) {
                    self.receiver_cleanup.insert(ty);
                }
            } else if synchronous {
                self.synchronous_cleanup
                    .extend(effects.into_iter().flatten().filter_map(|effect| {
                        match effect {
                            Effect::Mutation { kind: DependencyMutationKind::Dispose(ty) | DependencyMutationKind::Cleanup(ty), .. }
                            | Effect::UnboundedMutation { kind: DependencyMutationKind::Dispose(ty) | DependencyMutationKind::Cleanup(ty), .. } => deferred(*ty)
                                .map(bray_symbols::DependencyRequirement::SynchronousCleanup),
                            _ => None,
                        }
                    }));
            }
        }

        inherited_tail
    }
}
