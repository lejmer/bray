use std::collections::BTreeMap;

use bray_bound_tree::StorageAccessId;

use super::super::super::id::AnalysisOperationId;
use super::super::super::model::{AnalysisOperation, AnalysisOperationKind};
use super::super::flow::ExecutionFlowDomain;
use super::super::state::ExecutionState;
use super::{CleanupObservation, ReachedScopeCleanup};
use crate::{CheckerRequestContext, ExecutionCandidate, ExecutionCondition, ExecutionPlace, ExecutionProperty};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis::guarantee) fn observe_candidate_cleanup(
        &self,
        candidate: &mut ExecutionCandidate,
        reached_cleanup: &mut BTreeMap<AnalysisOperationId, ReachedScopeCleanup>,
        operation: &AnalysisOperation,
        scope_property: Option<ExecutionProperty>,
        collect_cleanup: bool,
        observation: CleanupObservation<'_>,
    ) {
        let certify_scope = scope_property.is_some();

        let (access, part, slot, lifecycle, whole, represented, evidence) = match observation {
            CleanupObservation::Continuation(_) => return,
            CleanupObservation::Cancellation => {
                if certify_scope {
                    reached_cleanup.entry(operation.id()).or_default().cancels_tasks = true;
                }

                return;
            }
            CleanupObservation::Part { access, ty, releases_storage, before } => {
                if certify_scope {
                    let inert = scope_property == Some(ExecutionProperty::Pure)
                        && self.cleanup_preserves_inputs(before, access);

                    reached_cleanup.entry(operation.id()).or_default().parts
                        .entry((access, ty, releases_storage))
                        .and_modify(|previous| *previous &= inert)
                        .or_insert(inert);
                }

                return;
            }
            CleanupObservation::Invocation { access, part, slot, lifecycle, whole, represented, evidence, .. } => {
                (access, part, slot, lifecycle, whole, represented, evidence)
            }
        };

        let node = match operation.kind() {
            AnalysisOperationKind::ScopeExit {
                exit, ..
            } => exit,
            operation => operation.node(),
        };

        if represented && let Some(property) = scope_property {
            let call = &lifecycle.call;

            let dependency = crate::asynchronous::lifecycle_execution_dependency(
                self.request,
                (call.callable, call.result.completion_type(), call.result.execution()),
                slot, crate::asynchronous::ExecutionCleanupMode::Disposal, property, node,
            );

            reached_cleanup.entry(operation.id()).or_default().calls.insert((access, part, slot), dependency);
        }

        let collect_disposal = (collect_cleanup || certify_scope && represented)
            && lifecycle.call.result.execution() == bray_symbols::CallableExecution::Synchronous;

        if !collect_cleanup && !collect_disposal {
            return;
        }

        let evidence = std::sync::Arc::new(evidence.clone());

        if collect_cleanup {
            let normal = operation.kind().is_normal_cleanup();

            let cleanup = candidate
                .cleanup
                .entry((node, access, part, slot, normal))
                .or_insert_with(|| crate::ExecutionCleanup {
                    callable: lifecycle.call.callable,
                    result: lifecycle.call.result.completion_type(),
                    synchronous: lifecycle.call.result.execution()
                        == bray_symbols::CallableExecution::Synchronous,
                    caller_continuation: self.storage.is_root_access(access)
                        && self.storage.root_identity(access).is_some_and(|identity| {
                            bray_bound_tree::storage_identity_is_destructor_receiver(
                                self.request.unit(), self.storage, identity,
                            )
                        })
                        && matches!(operation.kind(), AnalysisOperationKind::ScopeExit { .. }),
                    whole,
                    evidence: Default::default(),
                });

            cleanup.evidence.insert(std::sync::Arc::clone(&evidence));
        }

        // Whole-value completion can omit an async finalizer without
        // constructing its future. Invocation certification still distinguishes
        // synchronous execution from deferred execution.
        if collect_disposal {
            candidate
                .disposal
                .entry((node.into(), lifecycle.call.callable))
                .or_default()
                .insert(evidence);
        }
    }

    pub(in crate::analysis::guarantee) fn cleanup_preserves_inputs(
        &self,
        state: &ExecutionState,
        access: StorageAccessId,
    ) -> bool {
        let ty = self
            .storage
            .access(access)
            .expect("cleanup retains its checked storage access")
            .reached_type();

        let place = ExecutionPlace::storage(self.storage, access);

        if place.as_ref().is_some_and(|place| state.current.is_subtree_absent(place)) {
            return true;
        }

        let value = place.and_then(|place| place.value_in(&state.current));

        match crate::representation::type_representation(self.request, ty) {
            Some(bray_compiler_known::RepresentationRole::Future) => {
                self.storage.logical_path(access).is_some_and(|path| {
                    self.inactive_future_captures(state, ty, path).is_some_and(|captures| captures.is_empty())
                })
            }
            Some(bray_compiler_known::RepresentationRole::String) => {
                // Materialized string constants have static backing and no reference-counted owner.
                matches!(value.as_ref().map(ExecutionCondition::kind), Some(crate::ExecutionConditionKind::Literal(value))
                    if matches!(value.kind(), bray_symbols::ConstantValueKind::String(_)))
            }
            _ => false,
        }
    }
}
