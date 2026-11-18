use std::borrow::Cow;

use bray_bound_tree::{StorageMutation, StorageTaskOutcome, StorageValueBoundary, StorageValuePhase};

use super::super::super::fixed_point::{FixedPointDomain, FlowDirection};
use super::super::super::id::{AnalysisBlockId, AnalysisEdgeId};
use super::super::super::model::{AnalysisBlock, AnalysisCallPhase, AnalysisEdge, AnalysisOperationKind, ControlFlowGraph};
use super::super::super::outcomes::{FlowOutcomes, OutcomeStep, apply_call_rows, edge_path, extend_path};
use super::super::state::ExecutionState;
use super::merge::{ExecutionJoin, join_states};
use super::model::{ExecutionFlow, ExecutionFlowDomain};
use crate::CheckerRequestContext;

impl<C: CheckerRequestContext + ?Sized> ExecutionFlow<'_, '_, C> {
    pub(in crate::analysis::guarantee) fn is_block_reachable(&self, block: AnalysisBlockId) -> bool {
        self.states.state(block).is_some_and(|outcomes| outcomes.states().next().is_some())
    }

    pub(in crate::analysis::guarantee) fn entry(&self, block: AnalysisBlockId) -> Option<Cow<'_, ExecutionState>> {
        self.states.state(block)?.joined(|state, next| { join_states(state, next, ExecutionJoin::Block(block)); })
    }

    pub(in crate::analysis::guarantee) fn output_states(&self, block: AnalysisBlockId) -> impl Iterator<Item = &ExecutionState> {
        self.states.output(block).into_iter().flat_map(FlowOutcomes::states)
    }

    pub(in crate::analysis::guarantee) fn output(&self, block: AnalysisBlockId) -> Option<Cow<'_, ExecutionState>> {
        self.states.output(block)?.joined(|state, next| { join_states(state, next, ExecutionJoin::Block(block)); })
    }

    pub(in crate::analysis::guarantee) fn edge_output(&self, edge: AnalysisEdgeId) -> Option<ExecutionState> {
        let edge = self.domain.graph.edge(edge)?;
        let mut states = self.output_states(edge.source()).filter_map(|state| self.domain.edge_state(state, edge));
        let mut merged = states.next()?;

        for state in states {
            join_states(&mut merged, &state, ExecutionJoin::Block(edge.target()));
        }

        Some(merged)
    }
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn replay_task_observation(&self, state: &mut ExecutionState, operation: AnalysisOperationKind) -> bool {
        let Some(outcome) = self.task_observation_outcome(state, operation) else { return true; };

        let occurrence = operation.occurrence();
        let mutation = StorageMutation::TaskObservation { occurrence, outcome };
        let flow = self.checked_body.expect("task observation retains its checked transition").storage_flow();

        let compatible = |reached| reached == outcome || reached == StorageTaskOutcome::Widened
            || outcome == StorageTaskOutcome::Widened
            || matches!((reached, outcome), (
                StorageTaskOutcome::Completed { .. }, StorageTaskOutcome::Completed { row: None }
            ) | (
                StorageTaskOutcome::Completed { row: None }, StorageTaskOutcome::Completed { .. }
            ));

        let mut candidates = flow.task_outcome_mutations(occurrence).filter(|(reached, _)| compatible(*reached));

        let Some((reached, invalidation)) = candidates.next() else {
            // An opaque physical transition can already have applied its full
            // envelope at suspension. Otherwise the incompatible result is unreachable.
            return flow.task_outcome_mutations(occurrence).next().is_none();
        };

        let mut observed = Cow::Borrowed(invalidation);
        let mut selected = vec![reached];

        for (reached, invalidation) in candidates {
            observed.to_mut().merge(invalidation);
            selected.push(reached);
        }

        self.invalidate_observations(state, mutation, &observed);

        for outcome in selected {
            state.retire_captured_storage(flow.capture_retirements(
                StorageMutation::TaskObservation { occurrence, outcome },
                StorageValuePhase::Completion, StorageValueBoundary::Execution,
            ));
        }

        true
    }

    pub(super) fn retain_admitted_failure(&self, state: &mut ExecutionState, operation: AnalysisOperationKind) {
        let invocation = operation.occurrence();

        if !self.failure_sites.contains(&invocation) { return; }

        // The snapshot itself has no pending outcomes. A looping suspension can
        // replace it at the same stable operation site.
        state.admitted_failure = None;

        let mut failure = state.clone();

        if let Some(expression) = operation.awaited_expression() {
            self.suspend_awaited_execution(&mut failure, expression);
        }

        state.admitted_failure = Some((invocation, std::sync::Arc::new(failure)));
    }

    pub(super) fn prepare_replay_outputs(&self, state: &mut ExecutionState, operation: AnalysisOperationKind) {
        let AnalysisOperationKind::Call { invocation, phase: AnalysisCallPhase::Attempt } = operation else { return; };

        self.prepare_failure_rows(state, invocation);

        if state.admitted_call(invocation).is_none_or(|entry| !entry.outputs.0.is_empty()) { return; }

        if let Some(completion) = self.checked_body.and_then(|body|
            body.storage_flow().completion(bray_bound_tree::StorageMutation::Operation(invocation)))
        {
            self.prepare_captured_values(state, invocation, completion.values(), false);
        }
    }

    fn replay_outcomes(&self, outcomes: &mut FlowOutcomes<ExecutionState>, block: AnalysisBlockId, operation: AnalysisOperationKind) {
        let (mut budget, incoming) = outcomes.take();

        let invocation = operation.occurrence();

        for (path, mut state) in incoming {
            self.prepare_await_execution(&mut state, operation);

            if let Some(alternatives) = self.split_task_completion(&state, operation, budget + 1) {
                budget -= alternatives.len().saturating_sub(1);

                for (outcome, mut branch) in alternatives {
                    if self.replay_operation(&mut branch, operation, None) {
                        let path = Some(extend_path(path.as_ref(), OutcomeStep::TaskCompletion(invocation, outcome)));

                        outcomes.merge(path, Cow::Owned(branch), |state, next| join_states(state, next, ExecutionJoin::Block(block)));
                    }
                }

                continue;
            }

            let frame = self.direct_await_outputs(&state, operation);

            let rows = if matches!(operation, AnalysisOperationKind::Call { phase: AnalysisCallPhase::Attempt, .. }) {
                self.prepare_replay_outputs(&mut state, operation);

                self.feasible_call_outputs(&state, invocation)
            } else {
                frame.as_ref().map_or_else(Vec::new, |(_, _, rows)| rows.clone())
            };

            apply_call_rows(state, path, invocation, &rows, &mut budget, |mut state, path, selected, _| {
                if let Some(row) = selected {
                    if let Some((frame, producer, _)) = &frame {
                        state.select_frame_output(frame, producer, row);
                    } else {
                        state.select_call_output(invocation, row);
                    }
                }

                if self.replay_operation(&mut state, operation, None) {
                    outcomes.merge(path, Cow::Owned(state), |state, next| join_states(state, next, ExecutionJoin::Block(block)));
                }
            });
        }
    }
}

impl<C: CheckerRequestContext + ?Sized> FixedPointDomain for ExecutionFlowDomain<'_, '_, C> {
    type State = FlowOutcomes<ExecutionState>;

    fn direction(&self) -> FlowDirection { FlowDirection::Forward }

    fn bottom(&self) -> Self::State { FlowOutcomes::default() }

    fn boundary(&self) -> Self::State { self.entry_state().map_or_else(FlowOutcomes::default, FlowOutcomes::entry) }

    fn merge(&self, block: Option<AnalysisBlockId>, target: &mut Self::State, boundary: &Self::State) -> bool {
        let mut changed = false;

        for (path, state) in boundary.iter() {
            changed |= target.merge(path.cloned(), Cow::Borrowed(state), |state, next| join_states(state, next, block.into()));
        }

        changed
    }

    fn transfer(&self, block: &AnalysisBlock, source: &Self::State) -> Self::State {
        // The solver retains the immutable entry while operations update its selected outputs.
        let mut outcomes = source.clone();

        for operation in block.operations().iter().filter_map(|id| self.graph.operation(*id)) {
            self.replay_outcomes(&mut outcomes, block.id(), operation.kind());
        }

        outcomes
    }

    fn propagate(&self, source: &Self::State, edge: &AnalysisEdge, target: &mut Self::State) -> bool {
        let mut changed = false;

        for (path, state) in source.iter() {
            let Some(incoming) = self.edge_state(state, edge) else { continue; };

            let path = edge_path(self.graph, edge, path, self.result_edges.contains(&edge.id()));

            changed |= target.merge(path, Cow::Owned(incoming), |state, next| join_states(state, next, ExecutionJoin::Block(edge.target())));
        }

        changed
    }

    fn convergence_bound(&self, graph: &ControlFlowGraph) -> usize {
        graph.blocks().len()
            .saturating_mul(graph.operations().len().saturating_add(graph.edges().len()).saturating_add(1))
            .saturating_mul(16)
    }
}
