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

type LaneInput = (Option<OutcomePath>, StorageFlowState);

/// One outcome lane's collector and reached states for the operation in progress.
struct LaneRun<'analysis, C: CheckerRequestContext + ?Sized> {
    spent: usize,
    collector: StorageFlowCollector<'analysis, C>,
    outputs: Vec<LaneInput>,
}

enum LaneSlot<'analysis, C: CheckerRequestContext + ?Sized> {
    Pending(StorageFlowCollector<'analysis, C>, Option<OutcomePath>, StorageFlowState, Option<LaneInput>),
    Running,
    Finished(LaneRun<'analysis, C>, Option<LaneInput>),
}

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
        for (budget, incoming) in self.0.take() {
            for (path, state) in Self::apply_lanes(collector, block, operation, budget, incoming) {
                self.merge(Some(block), path, Cow::Owned(state));
            }
        }
    }

    /// Applies one operation to each lane of an outcome group and returns the
    /// reached states in sequential order.
    ///
    /// Lanes share only the budget for new alternatives. Each lane runs with its
    /// own collector and the whole remaining budget. A lane that selects no new
    /// alternative makes the same decisions under any smaller budget, so only a
    /// lane that spends budget after an earlier lane did runs again with the
    /// remainder. Lane records then merge in lane order.
    fn apply_lanes<'analysis, C: CheckerRequestContext + ?Sized>(
        collector: &mut StorageFlowCollector<'analysis, C>,
        block: AnalysisBlockId,
        operation: &AnalysisOperation,
        budget: usize,
        incoming: Vec<(Option<OutcomePath>, StorageFlowState)>,
    ) -> Vec<(Option<OutcomePath>, StorageFlowState)> {
        let mut outputs = Vec::new();

        if incoming.len() < 2 {
            let mut budget = budget;

            for (path, state) in incoming {
                Self::apply_path(collector, block, operation, &mut budget, path, state, &mut outputs);
            }

            return outputs;
        }

        let run = |mut lane: StorageFlowCollector<'analysis, C>, path, state, budget| -> LaneRun<'analysis, C> {
            let mut remaining = budget;
            let mut outputs = Vec::new();

            Self::apply_path(&mut lane, block, operation, &mut remaining, path, state, &mut outputs);

            LaneRun { spent: budget - remaining, collector: lane, outputs }
        };

        // A retained input lets a lane run again when an earlier lane spends budget first.
        let lanes = incoming
            .into_iter()
            .map(|(path, state)| {
                let retained = (budget > 0).then(|| (path.clone(), state.clone()));

                std::sync::Mutex::new(LaneSlot::Pending(collector.lane(), path, state, retained))
            })
            .collect::<Vec<_>>();

        let start = |index: usize| {
            let mut slot = lanes[index].lock().expect("outcome lane lock is not poisoned");

            if let LaneSlot::Pending(lane, path, state, retained) = std::mem::replace(&mut *slot, LaneSlot::Running) {
                drop(slot);

                let result = run(lane, path, state, budget);

                *lanes[index].lock().expect("outcome lane lock is not poisoned") = LaneSlot::Finished(result, retained);
            }
        };

        collector.for_each_lane(lanes.len(), &start);

        let mut remaining = budget;

        for slot in lanes {
            let slot = slot.into_inner().expect("outcome lane lock is not poisoned");

            let result = match slot {
                LaneSlot::Finished(result, _) if result.spent == 0 || remaining == budget => result,
                LaneSlot::Finished(_, retained) => {
                    let (path, state) = retained.expect("a lane that spends budget retains its input");

                    run(collector.lane(), path, state, remaining)
                }
                LaneSlot::Pending(lane, path, state, _) => run(lane, path, state, remaining),
                LaneSlot::Running => unreachable!("every started outcome lane publishes its result"),
            };

            remaining -= result.spent;
            collector.merge(result.collector);
            outputs.extend(result.outputs);
        }

        outputs
    }

    #[expect(clippy::too_many_arguments, reason = "one lane applies an operation with its own collector, budget and output")]
    fn apply_path<C: CheckerRequestContext + ?Sized>(
        collector: &mut StorageFlowCollector<'_, C>,
        block: AnalysisBlockId,
        operation: &AnalysisOperation,
        budget: &mut usize,
        path: Option<OutcomePath>,
        mut state: StorageFlowState,
        outputs: &mut Vec<(Option<OutcomePath>, StorageFlowState)>,
    ) {
        let rows = collector.prepare_operation_outcomes(&mut state, operation);
        let invocation = operation.kind().occurrence();

        apply_call_rows(state, path, invocation, &rows, budget, |mut branch, path, row, budget| {
            if let Some(row) = row {
                branch.execution.as_mut().expect("a bound output has an execution state")
                    .select_call_output(invocation, row);
            }

            Self::apply_selected(collector, block, operation, path, branch, budget, outputs);
        });
    }

    #[expect(clippy::too_many_arguments, reason = "one lane applies an operation with its own collector, budget and output")]
    fn apply_selected<C: CheckerRequestContext + ?Sized>(
        collector: &mut StorageFlowCollector<'_, C>,
        block: AnalysisBlockId,
        operation: &AnalysisOperation,
        path: Option<OutcomePath>,
        mut state: StorageFlowState,
        budget: &mut usize,
        outputs: &mut Vec<(Option<OutcomePath>, StorageFlowState)>,
    ) {
        let kind = operation.kind();
        let site = kind.occurrence();
        let execution = state.execution.as_ref().expect("reached operation retains execution facts");

        let frames = match collector.execution.deferred_alternatives(execution, kind) {
            Some((frame, alternatives)) if alternatives.len() - 1 <= *budget => {
                *budget -= alternatives.len() - 1;

                collector.execution.visit_deferred_alternatives(state, kind, (frame, alternatives), operation_execution, |producer, branch| {
                    let path = Some(extend_path(path.as_ref(), OutcomeStep::Frame(site, producer)));

                    Self::apply_selected(collector, block, operation, path, branch, budget, outputs);
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
                outputs.push((path, branch));
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

                outputs.push((path, branch));
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
            outputs.push((path.clone(), continued));
        });

        outputs.push((path, branch));
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
