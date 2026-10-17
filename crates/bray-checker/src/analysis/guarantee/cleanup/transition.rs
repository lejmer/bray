use std::collections::BTreeMap;

use bray_bound_tree::{AnyBoundNodeId, StorageAccessId, StorageMutation};
use bray_symbols::TypeAssociatedLifecycleSlot;

use super::super::super::model::{AnalysisOperationKind, AnalysisScopeExitPhase};
use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use super::plan::{Lifecycle, Step};
use crate::{CheckerRequestContext, ExecutionCallEvidence, ExecutionPlace};

pub(in crate::analysis) type Observer<'a> = dyn FnMut(
        StorageAccessId,
        usize,
        TypeAssociatedLifecycleSlot,
        &Lifecycle,
        bool,
        &ExecutionCallEvidence,
        bool,
    ) + 'a;

pub(in crate::analysis) struct LifecycleInvocation<'a> {
    pub(in crate::analysis) access: StorageAccessId,
    pub(in crate::analysis) lifecycle: &'a Lifecycle,
    pub(in crate::analysis) completes: bool,
    pub(in crate::analysis) path: Option<&'a bray_bound_tree::StoragePath>,
}

pub(in crate::analysis) enum CleanupMutation<'a> {
    /// Source cleanup ordering was diagnosed and has no valid selected order.
    Recovery,
    Opaque {
        ty: bray_symbols::TypeId,
        access: StorageAccessId,
        path: Option<&'a bray_bound_tree::StoragePath>,
    },
    Lifecycle(LifecycleInvocation<'a>),
}

pub(in crate::analysis) type Mutation<'a> =
    dyn FnMut(&mut ExecutionState, StorageMutation, CleanupMutation<'_>) + 'a;

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis::guarantee) fn transition_cleanup(
        &self,
        state: &mut ExecutionState,
        operation: AnalysisOperationKind,
        mut observe: Option<&mut Observer<'_>>,
    ) {
        match operation {
            AnalysisOperationKind::ScopeExit {
                block, exit, phase, ..
            } => {
                if let Some((plan, flow)) = self.disposal.selected.and_then(|(cleanup, flow)| {
                    cleanup
                        .scope_exit_plan(block, exit)
                        .map(|plan| (plan, flow))
                }) {
                    match phase {
                        AnalysisScopeExitPhase::TaskCancellationBroadcast => {
                            for access in plan.cancellation_broadcast() {
                                self.apply_published_cleanup(
                                    state,
                                    StorageMutation::Cancellation {
                                        occurrence: operation.occurrence(),
                                        access: *access,
                                    },
                                );
                            }
                        }
                        AnalysisScopeExitPhase::LifecycleResolution => {
                            let exit = flow
                                .exits()
                                .iter()
                                .find(|decision| {
                                    decision.scope() == block && decision.exit() == exit
                                })
                                .expect("selected scope cleanup retains its storage decision");

                            for access in plan.lifecycle_resolution() {
                                let initialized = self
                                    .storage
                                    .root_identity(*access)
                                    .is_some_and(|identity| exit.initialized().contains(&identity));

                                self.dispose(
                                    state,
                                    *access,
                                    initialized,
                                    exit.moved().iter().cloned(),
                                    exit.definitely_absent().iter().cloned(),
                                    &mut observe,
                                    operation,
                                    &mut None,
                                );
                            }
                        }
                    }
                }

                if phase == AnalysisScopeExitPhase::LifecycleResolution {
                    self.expire_trusted_scope(state, block);
                }
            }
            AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression)) => {
                if let Some((plan, flow)) = self.disposal.selected.and_then(|(cleanup, flow)| {
                    cleanup.replacement(expression).map(|plan| (plan, flow))
                }) && matches!(
                    plan.cleanup(),
                    bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(_)
                ) {
                    let decision = flow
                        .replacements()
                        .iter()
                        .find(|decision| decision.expression() == expression)
                        .expect("selected replacement retains its storage decision");

                    self.dispose(
                        state,
                        plan.access(),
                        decision.root_initialized(),
                        decision.moved().iter().cloned(),
                        decision.definitely_absent().iter().cloned(),
                        &mut observe,
                        operation,
                        &mut None,
                    );
                }
            }
            // These operations do not dispose storage. Their explicit invocation
            // effects are handled by the ordinary operation transition.
            AnalysisOperationKind::Bound(_)
            | AnalysisOperationKind::PatternObservation(_)
            | AnalysisOperationKind::Call { .. }
            | AnalysisOperationKind::Suspension { .. }
            | AnalysisOperationKind::TaskOperation { .. }
            | AnalysisOperationKind::Recovery(_) => {}
        }
    }

    pub(in crate::analysis) fn transition_storage_cleanup(
        &self,
        state: &mut ExecutionState,
        storage_state: &crate::analysis::storage_flow::StorageFlowState,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        operation: AnalysisOperationKind,
        mut observe: Option<&mut Observer<'_>>,
        mutate: &mut Mutation<'_>,
    ) {
        match operation {
            AnalysisOperationKind::ScopeExit { block, phase, .. } => {
                let mut lifecycle = Vec::new();

                for requirement in self.disposal.plan.requirements.iter().rev() {
                    let identity = requirement.identity();

                    if !storage_state.live.contains(&identity) {
                        continue;
                    }

                    let disposition = requirement.exit_disposition(
                        self.storage,
                        block,
                        storage_state.initialized.contains(&identity),
                        storage_state.fully_moved.contains(&identity),
                        storage_state.moved.keys().cloned(),
                        storage_state.definitely_absent.keys().cloned(),
                    );

                    if let bray_bound_tree::AsyncStorageExitDisposition::Cleanup {
                        access,
                        phases,
                        ..
                    } = disposition
                    {
                        match phase {
                            AnalysisScopeExitPhase::TaskCancellationBroadcast
                                if phases.includes_cancellation() =>
                            {
                                let ty = self
                                    .storage
                                    .storage_type(identity)
                                    .expect("cleanup obligation retains its type");

                                mutate(
                                    state,
                                    StorageMutation::Cancellation {
                                        occurrence: operation.occurrence(),
                                        access,
                                    },
                                    CleanupMutation::Opaque {
                                        ty,
                                        access,
                                        path: self.storage.logical_path(access),
                                    },
                                );
                            }
                            AnalysisScopeExitPhase::LifecycleResolution
                                if phases.includes_lifecycle() =>
                            {
                                lifecycle.push(access)
                            }
                            AnalysisScopeExitPhase::TaskCancellationBroadcast
                            | AnalysisScopeExitPhase::LifecycleResolution => {}
                        }
                    }
                }

                if phase == AnalysisScopeExitPhase::LifecycleResolution {
                    match dependencies.lifecycle_order(self.storage, &lifecycle) {
                        Ok(ordered) => {
                            for access in ordered {
                                self.dispose(
                                    state,
                                    access,
                                    self.storage.root_identity(access).is_some_and(|identity| {
                                        storage_state.initialized.contains(&identity)
                                    }),
                                    storage_state.moved.keys().cloned(),
                                    storage_state.definitely_absent.keys().cloned(),
                                    &mut observe,
                                    operation,
                                    &mut Some(&mut *mutate),
                                );
                            }
                        }
                        // The owning cleanup analysis publishes the unavailable-order recovery.
                        Err(_) => {
                            mutate(
                                state,
                                StorageMutation::Operation(operation.occurrence()),
                                CleanupMutation::Recovery,
                            );
                        }
                    }

                    self.expire_trusted_scope(state, block);
                }
            }
            AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression)) => {
                for plan in self.storage.node_plans(expression.into()).filter(|plan| {
                    plan.purpose() == bray_bound_tree::StorageAccessPurpose::Assignment
                }) {
                    if !matches!(
                        self.disposal.plan.replacements.get(&plan.access()),
                        Some(bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(_))
                    ) {
                        continue;
                    }

                    let replacement = crate::analysis::storage_flow::ReplacementState::new(
                        storage_state,
                        plan,
                        self.storage,
                    );

                    if !replacement.recovered
                        && replacement.state(self.storage.root_identity(plan.access()).is_some())
                            != bray_bound_tree::StorageReplacementState::Absent
                    {
                        self.dispose(
                            state,
                            plan.access(),
                            replacement.initialized,
                            replacement.moved.iter().cloned(),
                            replacement.definitely_absent.iter().cloned(),
                            &mut observe,
                            operation,
                            &mut Some(&mut *mutate),
                        );
                    }
                }
            }
            // These operations do not dispose storage. Their explicit invocation
            // effects are handled by the ordinary operation transition.
            AnalysisOperationKind::Bound(_)
            | AnalysisOperationKind::PatternObservation(_)
            | AnalysisOperationKind::Call { .. }
            | AnalysisOperationKind::Suspension { .. }
            | AnalysisOperationKind::TaskOperation { .. }
            | AnalysisOperationKind::Recovery(_) => {}
        }
    }

    fn apply_published_cleanup(&self, state: &mut ExecutionState, mutation: StorageMutation) {
        let flow = self
            .checked_body
            .expect("execution cleanup consumes checked body semantics")
            .storage_flow();

        self.invalidate_observations(
            state,
            mutation,
            flow.cleanup_mutation(mutation),
            false,
            false,
        );
    }

    fn dispose(
        &self,
        state: &mut ExecutionState,
        access: StorageAccessId,
        initialized: bool,
        moved: impl Iterator<Item = bray_bound_tree::StoragePath>,
        definitely_absent: impl Iterator<Item = bray_bound_tree::StoragePath>,
        observe: &mut Option<&mut Observer<'_>>,
        operation: AnalysisOperationKind,
        mutate: &mut Option<&mut Mutation<'_>>,
    ) {
        let occurrence = operation.occurrence();
        let normal = operation.is_normal_cleanup();

        let steps = self
            .disposal
            .plan
            .steps
            .get(&access)
            .expect("selected cleanup has prepared transitions");

        let preserves = self.cleanup_preserves_inputs(state, access);
        let root = self.storage.root_identity(access);

        let moved = moved
            .filter(|moved| {
                Some(moved.root) == root
                    || self.storage.relationship_to_path(access, moved)
                        != bray_bound_tree::StorageRelationship::Disjoint
            })
            .collect::<Vec<_>>();

        let absent = definitely_absent.collect::<Vec<_>>();

        for (part, step) in steps.iter().enumerate() {
            let path = match step {
                Step::Opaque { path, .. } | Step::Lifecycle { path, .. } => path.as_ref(),
            };

            if path.is_some_and(|path| absent.iter().any(|absent| absent.contains(path))) {
                continue;
            }

            let Step::Lifecycle {
                ty,
                place,
                path,
                whole,
                conditional,
                guards,
            } = step
            else {
                if !preserves {
                    let mutation = StorageMutation::Cleanup {
                        occurrence,
                        access,
                        part,
                    };

                    if let Some(mutate) = mutate {
                        let Step::Opaque { ty, path } = step else {
                            unreachable!("lifecycle handled below")
                        };

                        mutate(
                            state,
                            mutation,
                            CleanupMutation::Opaque {
                                ty: *ty,
                                access,
                                path: path.as_ref(),
                            },
                        );
                    } else {
                        self.apply_published_cleanup(state, mutation);
                    }
                }

                continue;
            };

            let always = initialized
                && !conditional
                && path.as_ref().is_some_and(|path| {
                    !moved.iter().any(|moved| {
                        moved.relationship(path) != bray_bound_tree::StorageRelationship::Disjoint
                    })
                });

            for slot in [
                TypeAssociatedLifecycleSlot::Finalizer,
                TypeAssociatedLifecycleSlot::Destructor,
            ] {
                let Some(lifecycle) = self.disposal.plan.lifecycles.get(&(*ty, slot)) else {
                    continue;
                };

                let completes = normal
                    || lifecycle.contract.completes
                    || (lifecycle.synchronous
                        && self.request.target_cannot_unwind(
                            bray_bound_tree::BoundCallableTarget::Declaration(lifecycle.callable),
                        ));

                let value = path
                    .as_ref()
                    .map(|path| self.storage_path_value(state, path));

                let mut evidence = lifecycle.entry(state, place.as_ref(), value);

                // These premises hold only on the runtime branch that invokes this payload's cleanup.
                for (path, operator, expected) in guards {
                    crate::ExecutionCondition::pattern(
                        self.storage_path_value(state, path),
                        *operator,
                        expected.clone(),
                    )
                    .assume(true, &mut evidence.assumptions);
                }

                let established = self
                    .trusted_requirements_proven(&evidence, &lifecycle.contract.requirements)
                    && evidence.proves(&lifecycle.contract.preconditions);

                if let Some(observe) = observe {
                    observe(
                        access,
                        part,
                        slot,
                        lifecycle,
                        *whole,
                        &evidence,
                        established,
                    );
                }

                let mutation = StorageMutation::Lifecycle {
                    occurrence,
                    access,
                    part,
                    slot,
                };

                if let Some(mutate) = mutate {
                    mutate(
                        state,
                        mutation,
                        CleanupMutation::Lifecycle(LifecycleInvocation {
                            access,
                            lifecycle,
                            completes: always && established && completes,
                            path: path.as_ref(),
                        }),
                    );
                } else {
                    self.apply_published_cleanup(state, mutation);
                }

                if always && established && completes {
                    self.complete_trusted_cleanup(state, place.clone(), lifecycle, &evidence);
                }
            }
        }
    }
}

impl Lifecycle {
    fn entry(
        &self,
        state: &ExecutionState,
        place: Option<&ExecutionPlace>,
        value: Option<crate::ExecutionCondition>,
    ) -> ExecutionCallEvidence {
        let mut arguments = BTreeMap::new();

        for (observed, value) in &state.current {
            if let Some(place) = place
                && place.contains(observed)
            {
                let mut input = ExecutionPlace::from(self.receiver);

                for field in observed.projections.iter().skip(place.projections.len()) {
                    input = input.component(*field);
                }

                // Cleanup can overwrite the receiver, so entry evidence owns its observations.
                arguments.insert(input, value.clone());
            }
        }

        if let Some(value) = value {
            arguments.entry(self.receiver.into()).or_insert(value);
        }

        // Entry assumptions must survive the mutations made by this lifecycle invocation.
        ExecutionCallEvidence {
            returned: Default::default(),
            anonymous_target: None,
            trusted_boundary: !state.trust_boundaries.is_empty(),
            pending_execution: false,
            arguments,
            assumptions: state.assumptions.clone(),
            trusted_assumptions: state.trusted_assumptions.clone(),
        }
    }
}
