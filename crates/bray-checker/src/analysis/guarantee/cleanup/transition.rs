use std::collections::BTreeMap;

use bray_bound_tree::{AnyBoundNodeId, StorageAccessId};
use bray_symbols::TypeAssociatedLifecycleSlot;

use super::super::super::model::{AnalysisOperationKind, AnalysisScopeExitPhase};
use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use super::plan::{Lifecycle, Step};
use crate::{CheckerRequestContext, ExecutionCallEvidence, ExecutionPlace};

pub(in crate::analysis) type Observer<'a> = dyn FnMut(
        StorageAccessId,
        TypeAssociatedLifecycleSlot,
        &Lifecycle,
        bool,
        &ExecutionCallEvidence,
        bool,
    ) + 'a;

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
                            if !plan.cancellation_broadcast().is_empty() {
                                state.invalidate_cleanup(None);
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
                    );
                }
            }
            _ => {}
        }
    }

    pub(in crate::analysis) fn transition_storage_cleanup(
        &self,
        state: &mut ExecutionState,
        storage_state: &crate::analysis::storage_flow::StorageFlowState,
        dependencies: &bray_bound_tree::CheckedDependencyContracts,
        operation: AnalysisOperationKind,
        mut observe: Option<&mut Observer<'_>>,
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
                                state.invalidate_cleanup(None);
                                break;
                            }
                            AnalysisScopeExitPhase::LifecycleResolution
                                if phases.includes_lifecycle() =>
                            {
                                lifecycle.push(access)
                            }
                            _ => {}
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
                                );
                            }
                        }
                        // The owning cleanup analysis publishes the unavailable-order recovery.
                        Err(_) => state.invalidate_cleanup(None),
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
                        );
                    }
                }
            }
            _ => {}
        }
    }

    fn dispose(
        &self,
        state: &mut ExecutionState,
        access: StorageAccessId,
        initialized: bool,
        moved: impl Iterator<Item = bray_bound_tree::StoragePath>,
        definitely_absent: impl Iterator<Item = bray_bound_tree::StoragePath>,
        observe: &mut Option<&mut Observer<'_>>,
    ) {
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
            .map(|access| ExecutionPlace::storage_path(self.storage, &access))
            .collect::<Option<Vec<_>>>();

        let absent = definitely_absent
            .filter_map(|access| ExecutionPlace::storage_path(self.storage, &access))
            .collect::<Vec<_>>();

        for step in steps {
            let Step::Lifecycle {
                ty,
                place,
                whole,
                conditional,
            } = step
            else {
                if !preserves {
                    state.invalidate_cleanup(None);
                }

                continue;
            };

            if place
                .as_ref()
                .is_some_and(|place| absent.iter().any(|absent| absent.contains(place)))
            {
                continue;
            }

            let always = initialized
                && !conditional
                && place.as_ref().is_some_and(|place| {
                    // Unrepresentable moved paths cannot establish an unaffected receiver.
                    moved
                        .as_ref()
                        .is_some_and(|moved| !moved.iter().any(|moved| moved.overlaps(place)))
                });

            for slot in [
                TypeAssociatedLifecycleSlot::Finalizer,
                TypeAssociatedLifecycleSlot::Destructor,
            ] {
                let Some(lifecycle) = self.disposal.plan.lifecycles.get(&(*ty, slot)) else {
                    continue;
                };

                let completes = lifecycle.contract.completes
                    || (lifecycle.synchronous
                        && self.request.target_cannot_unwind(
                            bray_bound_tree::BoundCallableTarget::Declaration(lifecycle.callable),
                        ));

                if observe.is_none() && !completes {
                    // Propagation needs no entry snapshot when this call contributes no facts.
                    if !preserves {
                        state.invalidate_cleanup(None);
                    }

                    continue;
                }

                let evidence = lifecycle.entry(state, place.as_ref());

                let established = self
                    .trusted_requirements_proven(&evidence, &lifecycle.contract.requirements)
                    && evidence.proves(&lifecycle.contract.preconditions);

                if let Some(observe) = observe {
                    observe(access, slot, lifecycle, *whole, &evidence, established);
                }

                if !preserves {
                    state.invalidate_cleanup(None);
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

        if let Some(value) = place.and_then(|place| place.value_in(&state.current)) {
            arguments.entry(self.receiver.into()).or_insert(value);
        }

        // Entry assumptions must survive the mutations made by this lifecycle invocation.
        ExecutionCallEvidence {
            trusted_boundary: !state.trust_boundaries.is_empty(),
            pending_execution: false,
            arguments,
            assumptions: state.assumptions.clone(),
            trusted_assumptions: state.trusted_assumptions.clone(),
        }
    }
}
