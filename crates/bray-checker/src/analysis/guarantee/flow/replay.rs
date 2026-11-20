use std::borrow::Cow;

use bray_bound_tree::{StorageOutcomeRows, StorageValueBoundary};

use super::super::super::fixed_point::{FixedPointDomain, FlowDirection};
use super::super::super::id::{AnalysisBlockId, AnalysisEdgeId};
use super::super::super::model::{AnalysisBlock, AnalysisCallPhase, AnalysisEdge, AnalysisOperation, AnalysisOperationKind, ControlFlowGraph};
use super::super::super::outcomes::{FlowOutcomes, OutcomePath, OutcomeStep, apply_call_rows, edge_path, extend_path, reach_cleanup_iteration};
use super::super::state::ExecutionState;
use super::merge::{ExecutionJoin, join_states};
use super::model::{ExecutionFlow, ExecutionFlowDomain};
use crate::CheckerRequestContext;

impl<C: CheckerRequestContext + ?Sized> ExecutionFlow<'_, '_, C> {
    pub(in crate::analysis::guarantee) fn is_block_reachable(&self, block: AnalysisBlockId) -> bool {
        self.states.state(block).is_some_and(|outcomes| outcomes.states().next().is_some())
    }

    pub(in crate::analysis::guarantee) fn entry(&self, block: AnalysisBlockId) -> Option<Cow<'_, ExecutionState>> {
        let join = ExecutionJoin::incoming_block(self.domain.graph, block);

        self.states.state(block)?.joined(|state, next| { join_states(state, next, join); })
    }

    pub(in crate::analysis::guarantee) fn output_states(&self, block: AnalysisBlockId) -> impl Iterator<Item = &ExecutionState> {
        self.states.output(block).into_iter().flat_map(FlowOutcomes::states)
    }

    pub(in crate::analysis::guarantee) fn input_states(&self, block: AnalysisBlockId) -> impl Iterator<Item = &ExecutionState> {
        self.states.state(block).into_iter().flat_map(FlowOutcomes::states)
    }

    pub(in crate::analysis::guarantee) fn output(&self, block: AnalysisBlockId) -> Option<Cow<'_, ExecutionState>> {
        self.states.output(block)?.joined(|state, next| { join_states(state, next, ExecutionJoin::Block(block)); })
    }

    pub(in crate::analysis::guarantee) fn edge_output(&self, edge: AnalysisEdgeId) -> Option<ExecutionState> {
        let edge = self.domain.graph.edge(edge)?;
        let mut merged = None;

        for state in self.output_states(edge.source()) {
            self.domain.replay_edge(state, edge, |_, reached| {
                if let Some(merged) = &mut merged {
                    join_states(merged, &reached, ExecutionJoin::incoming_block(self.domain.graph, edge.target()));
                } else {
                    merged = Some(reached);
                }
            });
        }

        merged
    }
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn replay_admission(
        &self, state: &mut ExecutionState, point: &AnalysisOperation, selection: Option<&StorageOutcomeRows>,
    ) {
        let operation = point.kind();
        let invocation = operation.occurrence();
        let flow = self.checked_body.expect("execution admission retains its checked storage transition").storage_flow();

        let retirements = flow.operation_retirements(point.id(), StorageValueBoundary::Admission, selection);

        state.retire_captured_storage(&retirements);

        // The snapshot itself has no pending outcomes. A looping suspension can
        // replace it at the same stable operation site.
        state.admitted_failure = None;

        if !bray_symbols::DependencyFailureKind::ALL.into_iter().any(|kind| flow.invocation_can_fail(invocation, kind)) {
            return;
        }

        let mut failure = state.clone();

        if let Some(expression) = operation.awaited_expression() {
            self.suspend_awaited_execution(&mut failure, expression);
        }

        state.admitted_failure = Some((invocation, std::sync::Arc::new(failure)));
    }

    pub(super) fn prepare_replay_outputs(&self, state: &mut ExecutionState, operation: &AnalysisOperation) {
        let AnalysisOperationKind::Call { invocation, phase: AnalysisCallPhase::Attempt } = operation.kind() else { return; };

        self.prepare_failure_rows(state, invocation);

        if state.admitted_call(invocation).is_none_or(|entry| !entry.outputs.0.is_empty()) { return; }

        if let Some(values) = self.checked_body.and_then(|body|
            body.storage_flow().attempt_values(operation.id(), None))
        {
            self.prepare_captured_values(state, invocation, values);
        }
    }

    fn replay_outcomes(&self, outcomes: &mut FlowOutcomes<ExecutionState>, block: AnalysisBlockId, point: &AnalysisOperation) {
        let operation = point.kind();

        for (mut budget, incoming) in outcomes.take() {
            for (path, mut state) in incoming {
                if operation.awaited_expression().is_some() {
                    self.prepare_frame_execution(&mut state, point, None, |_, _, _| {});
                }

                self.replay_outcome(outcomes, block, point, &mut budget, path, state);
            }
        }
    }

    fn replay_outcome(
        &self, outcomes: &mut FlowOutcomes<ExecutionState>, block: AnalysisBlockId, point: &AnalysisOperation,
        budget: &mut usize, path: Option<OutcomePath>, mut state: ExecutionState,
    ) {
        let operation = point.kind();
        let invocation = operation.occurrence();

        if let Some((frame, alternatives)) = self.deferred_alternatives(&state, operation)
            && alternatives.len() - 1 <= *budget
        {
            *budget -= alternatives.len() - 1;

            self.visit_deferred_alternatives(state, operation, (frame, alternatives), |branch| branch, |producer, branch| {
                let path = Some(extend_path(path.as_ref(), OutcomeStep::Frame(invocation, producer)));

                self.replay_outcome(outcomes, block, point, budget, path, branch);
            });

            return;
        }

        if let Some(alternatives) = self.split_task_completion(&state, operation, *budget + 1) {
            *budget -= alternatives.len().saturating_sub(1);

            for (task, mut branch) in alternatives {
                let outcome = task.outcome;

                if self.replay_operation(&mut branch, point, None, Some(task)) {
                    let path = Some(extend_path(path.as_ref(), OutcomeStep::TaskCompletion(invocation, outcome)));

                    outcomes.merge(path, Cow::Owned(branch), |state, next| join_states(state, next, ExecutionJoin::Block(block)));
                }
            }

            return;
        }

        let frame = self.direct_await_outputs(&state, operation);
        let call_rows;

        let rows = if matches!(operation, AnalysisOperationKind::Call { phase: AnalysisCallPhase::Attempt, .. }) {
            self.prepare_replay_outputs(&mut state, point);
            call_rows = self.feasible_call_outputs(&state, invocation);

            &call_rows
        } else {
            frame.as_ref().map_or(&[][..], |(_, _, rows)| rows.as_slice())
        };

        apply_call_rows(state, path, invocation, rows, budget, |mut state, path, selected, _| {
            if let Some(row) = selected {
                if let Some((frame, producer, _)) = &frame {
                    state.select_frame_output(frame, producer, row);
                } else {
                    state.select_call_output(invocation, row);
                }
            }

            let completes = self.replay_cleanup_paths(&mut state, point, None, |_, continued| {
                outcomes.merge(path.clone(), Cow::Owned(continued), |state, next| join_states(state, next, ExecutionJoin::Block(block)));
            });

            if completes {
                outcomes.merge(path, Cow::Owned(state), |state, next| join_states(state, next, ExecutionJoin::Block(block)));
            }
        });
    }

    pub(in crate::analysis::guarantee) fn operation(
        &self,
        state: &mut ExecutionState,
        operation: &AnalysisOperation,
        block: AnalysisBlockId,
        observe: Option<&mut super::super::cleanup::Observer<'_>>,
    ) -> bool {
        if operation.kind().awaited_expression().is_some() {
            self.prepare_frame_execution(state, operation, None, |_, _, _| {});
        }

        let mut continuation = None;
        let join = ExecutionJoin::Block(block);

        let completes = self.replay_cleanup_paths(state, operation, observe, |primary, reached| {
            if reached.cleanup.as_ref() == primary {
                super::merge::merge(&mut continuation, &Some(reached), join);
            }
        });

        if let Some(continued) = continuation {
            if completes {
                join_states(state, &continued, join);
            } else {
                *state = continued;
            }

            return true;
        }

        completes
    }

    fn replay_cleanup_paths(
        &self,
        state: &mut ExecutionState,
        point: &AnalysisOperation,
        mut observe: Option<&mut super::super::cleanup::Observer<'_>>,
        mut reached: impl FnMut(Option<&crate::analysis::outcomes::CleanupProgress>, ExecutionState),
    ) -> bool {
        let mut continuations = Vec::new();
        let mut iterations = std::collections::BTreeMap::new();
        let completes = self.replay_with_cleanup_continuations(state, point, &mut continuations, &mut observe);

        while let Some(mut continued) = continuations.pop() {
            if !reach_cleanup_iteration(&mut iterations, &mut continued,
                |current, incoming| join_states(current, incoming, ExecutionJoin::Boundary)) {
                continue;
            }

            let resume = continued.cleanup.as_ref().is_some_and(|cleanup| cleanup.resumes_at(point.id()));

            if !resume || self.replay_with_cleanup_continuations(&mut continued, point, &mut continuations, &mut observe) {
                reached(state.cleanup.as_ref(), continued);
            }
        }

        completes
    }

    fn replay_with_cleanup_continuations(
        &self,
        state: &mut ExecutionState,
        point: &AnalysisOperation,
        continuations: &mut Vec<ExecutionState>,
        observe: &mut Option<&mut super::super::cleanup::Observer<'_>>,
    ) -> bool {
        let cleanup = matches!(point.kind(), AnalysisOperationKind::ScopeExit { .. })
            || self.checked_body.is_some_and(|body| body.storage_flow().cleanup_entry(point.id(), None).is_some());

        let collect = cleanup || observe.is_some();

        let mut emit = |observation: super::super::cleanup::CleanupObservation<'_>| {
            if let super::super::cleanup::CleanupObservation::Continuation(state) = observation {
                continuations.push(*state);
            } else if let Some(observe) = observe {
                observe(observation);
            }
        };

        self.replay_operation(
            state, point,
            collect.then_some(&mut emit as &mut super::super::cleanup::Observer<'_>),
            None,
        )
    }
}

impl<C: CheckerRequestContext + ?Sized> FixedPointDomain for ExecutionFlowDomain<'_, '_, C> {
    type State = FlowOutcomes<ExecutionState>;

    fn direction(&self) -> FlowDirection { FlowDirection::Forward }

    fn bottom(&self) -> Self::State { FlowOutcomes::default() }

    fn boundary(&self) -> Self::State { self.entry_state().map_or_else(FlowOutcomes::default, FlowOutcomes::entry) }

    fn merge(&self, block: Option<AnalysisBlockId>, target: &mut Self::State, boundary: &Self::State) -> bool {
        let mut changed = false;
        let join = block.map_or(ExecutionJoin::Boundary, |block| ExecutionJoin::incoming_block(self.graph, block));

        for (path, state) in boundary.iter() {
            changed |= target.merge(path.cloned(), Cow::Borrowed(state), |state, next| join_states(state, next, join));
        }

        changed
    }

    fn transfer(&self, block: &AnalysisBlock, source: &Self::State) -> Self::State {
        // The solver retains the immutable entry while operations update its selected outputs.
        let mut outcomes = source.clone();

        for operation in block.operations().iter().filter_map(|id| self.graph.operation(*id)) {
            self.replay_outcomes(&mut outcomes, block.id(), operation);
        }

        outcomes
    }

    fn propagate(&self, source: &Self::State, edge: &AnalysisEdge, target: &mut Self::State) -> bool {
        let mut changed = false;
        let join = ExecutionJoin::incoming_block(self.graph, edge.target());

        for (path, state) in source.iter() {
            self.replay_edge(state, edge, |row, incoming| {
                let path = edge_path(self.graph, edge, path, self.result_edges.contains(&edge.id()), row);

                changed |= target.merge(path, Cow::Owned(incoming), |state, next| join_states(state, next, join));
            });
        }

        changed
    }

    fn convergence_bound(&self, graph: &ControlFlowGraph) -> usize {
        graph.blocks().len()
            .saturating_mul(graph.operations().len().saturating_add(graph.edges().len()).saturating_add(1))
            .saturating_mul(16)
    }
}
