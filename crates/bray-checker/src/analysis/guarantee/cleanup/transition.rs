use std::collections::BTreeMap;

use bray_bound_tree::{AnyBoundNodeId, StorageAccessId};
use bray_symbols::TypeAssociatedLifecycleSlot;

use super::super::super::model::{AnalysisOperationKind, AnalysisScopeExitPhase};
use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use super::plan::{Lifecycle, Step};
use crate::{CheckerRequestContext, ExecutionCallEvidence, ExecutionPlace};

pub(in crate::analysis::guarantee) type Observer<'a> = dyn FnMut(
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
                                    exit.moved(),
                                    exit.definitely_absent(),
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
                        decision.moved(),
                        decision.definitely_absent(),
                        &mut observe,
                    );
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
        moved: &[StorageAccessId],
        definitely_absent: &[StorageAccessId],
        observe: &mut Option<&mut Observer<'_>>,
    ) {
        let steps = self
            .disposal
            .steps
            .get(&access)
            .expect("selected cleanup has prepared transitions");

        let preserves = self.cleanup_preserves_inputs(state, access);

        let root = self.storage.root_identity(access);

        let moved = moved
            .iter()
            .filter(|moved| {
                self.storage.root_identity(**moved) == root
                    || self.storage.access_contains(access, **moved)
                    || self.storage.access_contains(**moved, access)
            })
            .map(|access| ExecutionPlace::storage(self.storage, *access))
            .collect::<Option<Vec<_>>>();

        let absent = definitely_absent
            .iter()
            .filter_map(|access| ExecutionPlace::storage(self.storage, *access))
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
                let Some(lifecycle) = self.disposal.lifecycles.get(&(*ty, slot)) else {
                    continue;
                };

                if observe.is_none() && !lifecycle.contract.completes {
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

                if always && established && lifecycle.contract.completes {
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
