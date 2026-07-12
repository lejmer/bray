use bray_bound_tree::{ControlCompletion, ControlCompletionKind};

use crate::UnitCheckRequest;

use super::fixed_point::{FixedPointDomain, FixedPointOutcome, FlowDirection, solve_fixed_point};
use super::id::{AnalysisBlockId, AnalysisEdgeId};
use super::model::{
    AnalysisBlock, AnalysisEdge, AnalysisExitKind, AnalysisOperationKind, ControlFlowGraph,
};

pub(super) struct ReachabilityConclusions {
    reachable_blocks: Box<[bool]>,
    reachable_edges: Box<[bool]>,
    completion: ControlCompletion,
    is_recovered: bool,
}

impl ReachabilityConclusions {
    pub(super) fn is_block_reachable(&self, block: AnalysisBlockId) -> bool {
        block
            .to_index()
            .and_then(|index| self.reachable_blocks.get(index))
            .copied()
            .unwrap_or(false)
    }

    pub(super) fn is_edge_reachable(&self, edge: AnalysisEdgeId) -> bool {
        edge.to_index()
            .and_then(|index| self.reachable_edges.get(index))
            .copied()
            .unwrap_or(false)
    }

    pub(super) const fn completion(&self) -> ControlCompletion {
        self.completion
    }

    pub(super) const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

pub(crate) fn analyze_reachability(
    graph: &ControlFlowGraph,
    request: UnitCheckRequest<'_>,
) -> Option<ReachabilityConclusions> {
    let forward = solve_fixed_point(graph, &ReachabilityDomain::forward(), &request);

    let forward = match forward {
        FixedPointOutcome::Complete(result) => result,
        FixedPointOutcome::Cancelled => return None,
        FixedPointOutcome::ConvergenceInvariantViolated => {
            panic!("finite reachability analysis exceeded its convergence bound")
        }
    };

    let reverse = solve_fixed_point(graph, &ReachabilityDomain::backward(), &request);

    let reverse = match reverse {
        FixedPointOutcome::Complete(result) => result,
        FixedPointOutcome::Cancelled => return None,
        FixedPointOutcome::ConvergenceInvariantViolated => {
            panic!("finite reverse reachability analysis exceeded its convergence bound")
        }
    };

    for block in graph.blocks() {
        if forward.state(block.id()).is_none() || reverse.state(block.id()).is_none() {
            panic!("reachability state did not cover every analysis block");
        }
    }

    let reachable_blocks = collect_block_states(graph, &forward);

    let reachable_edges = graph
        .edges()
        .iter()
        .map(|edge| state_is_true(&forward, edge.source()))
        .collect::<Box<[_]>>();

    let completion = ControlCompletion::from_kinds(
        graph
            .exits()
            .iter()
            .filter(|exit| state_is_true(&forward, exit.block()))
            .map(|exit| completion_kind(exit.kind())),
    );

    let is_recovered = reachable_recovery_exists(graph, &reachable_blocks)
        || completion.contains(ControlCompletionKind::Recovered);

    Some(ReachabilityConclusions {
        reachable_blocks,
        reachable_edges,
        completion,
        is_recovered,
    })
}

fn collect_block_states(
    graph: &ControlFlowGraph,
    result: &super::fixed_point::FixedPointResult<bool>,
) -> Box<[bool]> {
    graph
        .blocks()
        .iter()
        .map(|block| state_is_true(result, block.id()))
        .collect()
}

fn state_is_true(
    result: &super::fixed_point::FixedPointResult<bool>,
    block: AnalysisBlockId,
) -> bool {
    result.state(block).copied().unwrap_or(false)
}

const fn completion_kind(kind: AnalysisExitKind) -> ControlCompletionKind {
    match kind {
        AnalysisExitKind::NormalFallthrough => ControlCompletionKind::Normal,
        AnalysisExitKind::Return => ControlCompletionKind::Return,
        AnalysisExitKind::Propagation => ControlCompletionKind::Propagation,
        AnalysisExitKind::Divergence => ControlCompletionKind::Divergence,
        AnalysisExitKind::Panic => ControlCompletionKind::Panic,
        AnalysisExitKind::Cancellation => ControlCompletionKind::Cancellation,
        AnalysisExitKind::Yield => ControlCompletionKind::Yield,
        AnalysisExitKind::Recovery => ControlCompletionKind::Recovered,
    }
}

fn reachable_recovery_exists(graph: &ControlFlowGraph, reachable_blocks: &[bool]) -> bool {
    graph.blocks().iter().any(|block| {
        let is_reachable = block
            .id()
            .to_index()
            .and_then(|index| reachable_blocks.get(index))
            .copied()
            .unwrap_or(false);

        is_reachable
            && block.operations().iter().any(|operation| {
                graph.operation(*operation).is_some_and(|operation| {
                    matches!(operation.kind(), AnalysisOperationKind::Recovery(_))
                })
            })
    })
}

impl crate::CheckerCancellation for UnitCheckRequest<'_> {
    fn is_cancelled(&self) -> bool {
        (*self).is_cancelled()
    }
}

pub(super) struct ReachabilityDomain {
    direction: FlowDirection,
}

impl ReachabilityDomain {
    pub(super) const fn forward() -> Self {
        Self {
            direction: FlowDirection::Forward,
        }
    }

    pub(super) const fn backward() -> Self {
        Self {
            direction: FlowDirection::Backward,
        }
    }
}

impl FixedPointDomain for ReachabilityDomain {
    type State = bool;

    fn direction(&self) -> FlowDirection {
        self.direction
    }

    fn bottom(&self) -> Self::State {
        false
    }

    fn boundary(&self) -> Self::State {
        true
    }

    fn merge_boundary(&self, target: &mut Self::State, incoming: &Self::State) -> bool {
        let merged = *target || *incoming;
        let changed = *target != merged;

        *target = merged;

        changed
    }

    fn propagate(
        &self,
        _: &AnalysisBlock,
        source: &Self::State,
        _: &AnalysisEdge,
        target: &mut Self::State,
    ) -> bool {
        self.merge_boundary(target, source)
    }

    fn propagate_self(&self, _: &AnalysisBlock, _: &mut Self::State, _: &AnalysisEdge) -> bool {
        false
    }

    fn convergence_bound(&self, graph: &ControlFlowGraph) -> usize {
        graph.blocks().len()
    }
}
