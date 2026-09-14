use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{AnyBoundNodeId, BoundReferenceTarget, CheckedAsync, StorageAccessId};
use bray_symbols::{
    CallableExecution, CallableInstanceData, TypeAssociatedLifecycleSlot, TypeData, TypeId,
};

use super::super::model::{AnalysisOperationKind, AnalysisScopeExitPhase};
use super::flow::{ExecutionFlow, ExecutionState};
use crate::{CheckerQueryError, CheckerRequestContext, ExecutionCallEvidence, ExecutionPlace};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlow<'_, '_, C> {
    pub(super) fn candidate(
        &self,
        cleanup: &CheckedAsync,
        collect_cleanup: bool,
    ) -> Result<crate::ExecutionCandidate, CheckerQueryError<C::UpstreamError>> {
        let mut candidate = crate::ExecutionCandidate::default();
        let mut dependencies = BTreeSet::new();
        let mut results = BTreeSet::new();

        for block in self.domain.graph.blocks() {
            let Some(Some(state)) = self.states.state(block.id()) else {
                continue;
            };

            // Each replay retains independently mutable value observations at the cleanup boundary.
            let mut state = state.clone();

            for operation in block
                .operations()
                .iter()
                .filter_map(|id| self.domain.graph.operation(*id))
            {
                let targets = match operation.kind() {
                    AnalysisOperationKind::ScopeExit {
                        block,
                        exit,
                        phase: AnalysisScopeExitPhase::LifecycleResolution,
                    } if collect_cleanup => cleanup
                        .scope_exits()
                        .iter()
                        .filter(|plan| plan.scope() == block && plan.exit() == exit)
                        .flat_map(|plan| {
                            plan.lifecycle_resolution()
                                .iter()
                                .map(move |access| (exit, *access))
                        })
                        .collect::<Vec<_>>(),
                    AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression))
                        if collect_cleanup =>
                    {
                        cleanup
                            .replacements()
                            .iter()
                            .filter(|plan| {
                                plan.expression() == expression
                                    && plan.parts().is_none()
                                    && matches!(
                                        plan.cleanup(),
                                        bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(_)
                                    )
                            })
                            .map(|plan| (expression.into(), plan.access()))
                            .collect()
                    }
                    _ => Vec::new(),
                };

                for (node, access) in targets {
                    if let Some((callable, result, evidence)) =
                        self.cleanup_entry(&state, access)?
                    {
                        candidate
                            .cleanup
                            .entry((node, access))
                            .and_modify(|(_, _, previous)| previous.intersect(&evidence))
                            .or_insert((callable, result, evidence));
                    }

                    state.invalidate_cleanup();
                }

                self.domain.operation(&mut state, operation.kind());
            }

            for operation in block
                .operations()
                .iter()
                .filter_map(|id| self.domain.graph.operation(*id))
            {
                if let AnyBoundNodeId::Expression(expression) = operation.kind().node()
                    && let Some(evidence) = state.entries.remove(&expression)
                {
                    candidate.calls.entry(expression.into()).or_insert(evidence);
                }
            }

            dependencies.extend(state.completion_dependencies);

            if self.domain.graph.exits().iter().any(|exit| {
                exit.block() == block.id()
                    && matches!(
                        exit.kind(),
                        super::super::model::AnalysisExitKind::Return
                            | super::super::model::AnalysisExitKind::NormalFallthrough
                            | super::super::model::AnalysisExitKind::ResultErrorPropagation
                    )
            }) {
                results.insert(match state.result {
                    crate::ExecutionCondition::Expression(expression) => {
                        match self.domain.semantics.selections().expression(expression) {
                            Some(bray_bound_tree::SemanticSelection::Operation(
                                bray_bound_tree::SelectedOperation::Construction(construction),
                            )) => match construction.target() {
                                bray_bound_tree::ConstructionTarget::UnionVariant(variant) => {
                                    Some(variant)
                                }
                                _ => None,
                            },
                            _ => None,
                        }
                    }
                    _ => None,
                });
            }
        }

        candidate.completion_dependencies = dependencies.into_iter().collect();

        candidate.result_variant = if results.len() == 1 {
            results.pop_first().flatten()
        } else {
            None
        };

        Ok(candidate)
    }

    fn cleanup_entry(
        &self,
        state: &ExecutionState,
        access: StorageAccessId,
    ) -> Result<
        Option<(CallableInstanceData, TypeId, ExecutionCallEvidence)>,
        CheckerQueryError<C::UpstreamError>,
    > {
        let request = self.domain.request;

        let Some(place) = ExecutionPlace::storage(self.domain.storage, access) else {
            return Ok(None);
        };

        let Some(access) = self.domain.storage.access(access) else {
            return Ok(None);
        };

        let selected = request.context().lifecycle_callable(
            access.reached_type(),
            TypeAssociatedLifecycleSlot::Finalizer,
        )?;

        let Some((callable, signature)) = selected.value() else {
            return Ok(None);
        };

        let Some(receiver) = signature.receiver() else {
            return Ok(None);
        };

        if selected.diagnostics().has_errors()
            || !matches!(request.semantic_values().type_data(signature.callable_type())
                .map_err(crate::CheckerInfrastructureError::SemanticValueStore).map_err(CheckerQueryError::Infrastructure)?
                .as_ref(), TypeData::Callable(callable) if callable.execution() == CallableExecution::Synchronous)
        {
            return Ok(None);
        }

        let mut arguments = BTreeMap::new();
        let receiver = BoundReferenceTarget::Surface(receiver.parameter().into());

        for (observed, value) in &state.current {
            if place.contains(observed) {
                let mut input = ExecutionPlace::from(receiver);

                for field in observed.fields.iter().skip(place.fields.len()) {
                    input = input.field(*field);
                }

                // The finalizer candidate retains the current immutable value snapshot.
                arguments.insert(input, value.clone());
            }
        }

        if let Some(value) = place.value_in(&state.current) {
            arguments.entry(receiver.into()).or_insert(value);
        }

        // A candidate owns its entry assumptions independently of subsequent cleanup.
        Ok(Some((
            *callable,
            signature.result(),
            ExecutionCallEvidence {
                arguments,
                assumptions: state.assumptions.clone(),
            },
        )))
    }
}
