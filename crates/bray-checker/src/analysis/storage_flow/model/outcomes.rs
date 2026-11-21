use std::borrow::Cow;
use std::sync::Arc;

use crate::analysis::guarantee::ExecutionJoin;
use crate::analysis::id::AnalysisBlockId;
use crate::analysis::model::{AnalysisCallPhase, AnalysisOperation, AnalysisOperationKind};
use crate::analysis::storage_flow::check::StorageFlowCollector;
use crate::analysis::outcomes::{FlowOutcomes, OutcomePath, OutcomeStep, apply_call_rows, continue_cleanup, extend_path};
use crate::{CheckerRequestContext, ExecutionDeferredValue};

use super::StorageFlowState;

type FrameAlternatives = (Arc<ExecutionDeferredValue>, Vec<ExecutionDeferredValue>);

/// Storage states kept distinct by the edges that delivered actual result values.
#[derive(Clone, Default, Eq, PartialEq)]
pub(in crate::analysis::storage_flow) struct StorageFlowOutcomes(FlowOutcomes<StorageFlowState>);

impl StorageFlowOutcomes {
    pub(super) fn entry(state: StorageFlowState) -> Self {
        Self(FlowOutcomes::entry(state))
    }

    pub(in crate::analysis::storage_flow) fn states(&self) -> impl Iterator<Item = &StorageFlowState> {
        self.0.states()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (Option<&OutcomePath>, &StorageFlowState)> {
        self.0.iter()
    }

    pub(super) fn into_states(self) -> impl Iterator<Item = (Option<OutcomePath>, StorageFlowState)> {
        self.0.into_states()
    }

    pub(in crate::analysis::storage_flow) fn apply_operation<C: CheckerRequestContext + ?Sized>(
        &mut self,
        collector: &mut StorageFlowCollector<'_, C>,
        block: AnalysisBlockId,
        operation: &AnalysisOperation,
    ) {
        for (mut budget, incoming) in self.0.take() {
            self.apply_paths(collector, block, operation, &mut budget, incoming);
        }
    }

    fn apply_paths<C: CheckerRequestContext + ?Sized>(
        &mut self,
        collector: &mut StorageFlowCollector<'_, C>,
        block: AnalysisBlockId,
        operation: &AnalysisOperation,
        budget: &mut usize,
        incoming: impl IntoIterator<Item = (Option<OutcomePath>, StorageFlowState)>,
    ) {

        for (path, mut state) in incoming {
            let rows = collector.prepare_operation_outcomes(&mut state, operation);
            let invocation = operation.kind().occurrence();

            apply_call_rows(state, path, invocation, &rows, budget, |mut branch, path, row, budget| {
                if let Some(row) = row {
                    branch.execution.as_mut().expect("a bound output has an execution state")
                        .select_call_output(invocation, row);
                }

                self.apply_selected(collector, block, operation, path, branch, budget);
            });
        }
    }

    fn apply_selected<C: CheckerRequestContext + ?Sized>(
        &mut self,
        collector: &mut StorageFlowCollector<'_, C>,
        block: AnalysisBlockId,
        operation: &AnalysisOperation,
        path: Option<OutcomePath>,
        mut state: StorageFlowState,
        budget: &mut usize,
    ) {
        let kind = operation.kind();
        let site = kind.occurrence();
        let execution = state.execution.as_ref().expect("reached operation retains execution facts");

        let frames = match collector.execution.deferred_alternatives(execution, kind) {
            Some((frame, alternatives)) if alternatives.len() - 1 <= *budget => {
                *budget -= alternatives.len() - 1;

                collector.execution.visit_deferred_alternatives(state, kind, (frame, alternatives), operation_execution, |producer, branch| {
                    let path = Some(extend_path(path.as_ref(), OutcomeStep::Frame(site, producer)));

                    self.apply_selected(collector, block, operation, path, branch, budget);
                });

                return;
            }
            frames => frames,
        };

        if let Some((frame, producer, rows)) = collector.execution.direct_await_outputs(execution, kind) {
            apply_call_rows(state, path, site, &rows, budget, |mut branch, path, row, _| {
                if let Some(row) = row {
                    branch.execution.as_mut().expect("selected await retains execution facts")
                        .select_frame_output(&frame, &producer, row);
                }

                collector.apply_operation(&mut branch, operation, None);
                self.merge(Some(block), path, Cow::Owned(branch));
            });

            return;
        }

        if let Some(mut alternatives) = collector.execution.split_task_completion(execution, kind, *budget + 1) {
            let Some(last) = alternatives.pop() else { return; };

            *budget -= alternatives.len();
            state.execution = None;

            let mut apply = |mut branch: StorageFlowState, (selected, execution): (crate::analysis::guarantee::TaskSelection, _)| {
                branch.execution = Some(execution);

                let path = Some(extend_path(path.as_ref(), OutcomeStep::TaskCompletion(site, selected.outcome)));
                let branch = Self::apply_refined(collector, operation, branch, Some(selected));

                self.merge(Some(block), path, Cow::Owned(branch));
            };

            for alternative in alternatives {
                // Task result siblings each own their physical transition.
                apply(state.clone(), alternative);
            }

            apply(state, last);

            return;
        }

        let branch = Self::apply_widened(collector, operation, state, frames, None);
        let continuations = std::mem::take(&mut collector.cleanup_continuations);

        continue_cleanup(operation.id(), continuations, |continued, next| {
            collector.resume_cleanup(continued, operation);
            next.append(&mut collector.cleanup_continuations);

            true
        }, |current, incoming| current.merge(incoming, None), |continued| {
            self.merge(Some(block), path.clone(), Cow::Owned(continued));
        });

        self.merge(Some(block), path, Cow::Owned(branch));
    }

    fn apply_refined<C: CheckerRequestContext + ?Sized>(
        collector: &mut StorageFlowCollector<'_, C>,
        operation: &AnalysisOperation,
        state: StorageFlowState,
        task: Option<crate::analysis::guarantee::TaskSelection>,
    ) -> StorageFlowState {
        let execution = state.execution.as_ref().expect("refined operation retains execution facts");
        let frames = collector.execution.deferred_alternatives(execution, operation.kind());

        Self::apply_widened(collector, operation, state, frames, task)
    }

    fn apply_widened<C: CheckerRequestContext + ?Sized>(
        collector: &mut StorageFlowCollector<'_, C>,
        operation: &AnalysisOperation,
        mut state: StorageFlowState,
        frames: Option<FrameAlternatives>,
        task: Option<crate::analysis::guarantee::TaskSelection>,
    ) -> StorageFlowState {
        let Some(frames) = frames else {
            // The provisional enclosing Completed branch can now identify its body.
            let task = task.filter(|selection| selection.outcome != bray_bound_tree::StorageTaskOutcome::Widened);

            collector.apply_operation(&mut state, operation, task);

            return state;
        };

        let kind = operation.kind();
        let mut merged = StorageFlowState::default();

        // Keep the nested merge at this producer before returning to the enclosing frame.
        collector.execution.visit_deferred_alternatives(state, kind, frames, operation_execution, |_, branch| {
            let mut branch = Self::apply_refined(collector, operation, branch, task.clone());

            if !merged.reachable {
                merged = branch;
            } else if matches!(kind, AnalysisOperationKind::Call { phase: AnalysisCallPhase::Completion, .. }) {
                let expression = kind.occurrence().expression().expect("deferred completion retains its await expression");

                collector.merge_completed(&mut merged, &mut branch, expression);
            } else {
                let join = if matches!(kind,
                    AnalysisOperationKind::Call { phase: AnalysisCallPhase::Attempt, .. } | AnalysisOperationKind::Bound(_))
                {
                    ExecutionJoin::Production(kind.occurrence().expression().expect("frame attempt retains its result"))
                } else {
                    ExecutionJoin::Boundary
                };

                merged.merge(&branch, join);
            }
        });

        merged
    }

    pub(super) fn merge<'join>(
        &mut self,
        join: impl Into<ExecutionJoin<'join>>,
        path: Option<OutcomePath>,
        incoming: Cow<'_, StorageFlowState>,
    ) -> bool {
        if !incoming.reachable {
            return false;
        }

        let join = join.into();

        self.0.merge(path, incoming, |state, incoming| state.merge(incoming, join))
    }
}

fn operation_execution(branch: &mut StorageFlowState) -> &mut crate::analysis::guarantee::ExecutionState {
    branch.execution.as_mut().expect("selected frame retains execution facts")
}
