use std::borrow::Cow;
use std::sync::Arc;

use bray_bound_tree::BoundExecutionSite;

use super::id::AnalysisEdgeId;
use super::model::{AnalysisEdge, AnalysisEdgeKind, AnalysisOperationKind, AnalysisRefinement, AnalysisResultPhase, ControlFlowGraph};
use crate::execution_guarantees::DeferredProducer;

#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum OutcomeStep {
    Production(AnalysisEdgeId),
    NormalCall(BoundExecutionSite, usize),
    Frame(BoundExecutionSite, Option<DeferredProducer>),
    TaskCompletion(BoundExecutionSite, bray_bound_tree::StorageTaskOutcome),
}

pub(super) type OutcomePath = Arc<[OutcomeStep]>;

pub(super) fn extend_path(path: Option<&OutcomePath>, step: OutcomeStep) -> OutcomePath {
    let mut steps = path.map(|path| path.as_ref()).unwrap_or_default().to_vec();

    steps.push(step);

    steps.into()
}

pub(super) fn edge_path(
    graph: &ControlFlowGraph,
    edge: &AnalysisEdge,
    path: Option<&OutcomePath>,
    produces_result: bool,
) -> Option<OutcomePath> {
    // A loop joins its stable header state instead of recording iteration history.
    if graph.block(edge.target()).expect("flow edge has a target block").predecessors()
        .iter().filter_map(|id| graph.edge(*id))
        .any(|edge| matches!(edge.kind(), AnalysisEdgeKind::LoopBack | AnalysisEdgeKind::LoopContinue))
    {
        return None;
    }

    if produces_result {
        return Some(extend_path(path, OutcomeStep::Production(edge.id())));
    }

    if let Some(AnalysisRefinement::InvocationFailure(invocation, _)) = edge.refinement()
        && let Some(steps) = path
        && matches!(steps.last(), Some(OutcomeStep::NormalCall(site, _)) if *site == invocation)
    {
        return Some(Arc::from(&steps[..steps.len() - 1]));
    }

    path.cloned()
}

pub(super) fn result_production_edges(graph: &ControlFlowGraph, storage: &bray_bound_tree::StoragePlan) -> std::collections::BTreeSet<AnalysisEdgeId> {
    graph.blocks().iter().filter(|block| {
        block.predecessors().len() > 1
            && block.operations().iter().filter_map(|id| graph.operation(*id)).any(|operation| {
                matches!(operation.kind(), AnalysisOperationKind::Result { expression, phase: AnalysisResultPhase::Completion }
                    if storage.value_slot(expression).is_some())
            })
    }).flat_map(|block| block.predecessors().iter().copied()).filter(|id| {
        graph.edge(*id).is_some_and(|edge| matches!(edge.kind(), AnalysisEdgeKind::Yield | AnalysisEdgeKind::LoopBreak))
    }).collect()
}

pub(super) fn apply_call_rows<State: Clone>(
    state: State,
    path: Option<OutcomePath>,
    invocation: BoundExecutionSite,
    rows: &[usize],
    budget: &mut usize,
    mut apply: impl FnMut(State, Option<OutcomePath>, Option<usize>, &mut usize),
) {
    match rows {
        [row] => apply(state, path, Some(*row), budget),
        [earlier @ .., last] if !earlier.is_empty() && earlier.len() <= *budget => {
            *budget -= earlier.len();

            let selected_path = |row| Some(extend_path(path.as_ref(), OutcomeStep::NormalCall(invocation, row)));

            for row in earlier {
                // Mutable checking state belongs to each selected completion.
                apply(state.clone(), selected_path(*row), Some(*row), budget);
            }

            apply(state, selected_path(*last), Some(*last), budget);
        }
        _ => apply(state, path, None, budget),
    }
}

/// Reached states distinguished by the stable operations that produced actual values.
#[derive(Clone, Eq, PartialEq)]
pub(crate) struct FlowOutcomes<State> {
    // An empty path is ordinary flow. None is the conservative union.
    entries: Vec<(Option<OutcomePath>, State)>,
}

impl<State> Default for FlowOutcomes<State> {
    fn default() -> Self { Self { entries: Vec::new() } }
}

impl<State: Clone> FlowOutcomes<State> {
    const MAX_ALTERNATIVES: usize = 8;

    pub(super) fn entry(state: State) -> Self {
        Self { entries: vec![(Some(Arc::default()), state)] }
    }

    pub(super) fn states(&self) -> impl Iterator<Item = &State> {
        self.entries.iter().map(|(_, state)| state)
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (Option<&OutcomePath>, &State)> {
        self.entries.iter().map(|(path, state)| (path.as_ref(), state))
    }

    pub(super) fn joined(&self, mut join: impl FnMut(&mut State, &State)) -> Option<Cow<'_, State>> {
        let mut states = self.states();
        let first = states.next()?;

        let Some(second) = states.next() else { return Some(Cow::Borrowed(first)); };

        // The joined view cannot mutate the selected states retained by the solver.
        let mut joined = first.clone();

        join(&mut joined, second);

        for state in states { join(&mut joined, state); }

        Some(Cow::Owned(joined))
    }

    pub(super) fn take(&mut self) -> (usize, Vec<(Option<OutcomePath>, State)>) {
        (Self::MAX_ALTERNATIVES - self.entries.len(), std::mem::take(&mut self.entries))
    }

    pub(super) fn merge(
        &mut self,
        path: Option<OutcomePath>,
        incoming: Cow<'_, State>,
        mut join: impl FnMut(&mut State, &State) -> bool,
    ) -> bool {
        if self.entries.is_empty() {
            self.entries.push((path, incoming.into_owned()));

            return true;
        }

        if let [(None, combined)] = self.entries.as_mut_slice() {
            return join(combined, &incoming);
        }

        if let Some((_, state)) = self.entries.iter_mut().find(|(current, _)| current == &path) {
            return join(state, &incoming);
        }

        if path.is_none() || self.entries.len() == Self::MAX_ALTERNATIVES {
            let mut outcomes = std::mem::take(&mut self.entries).into_iter();

            let (_, mut combined) = outcomes.next().expect("widening has a reached outcome");

            for (_, state) in outcomes {
                join(&mut combined, &state);
            }

            join(&mut combined, &incoming);
            self.entries.push((None, combined));
        } else {
            self.entries.push((path, incoming.into_owned()));
            self.entries.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        }

        true
    }
}
