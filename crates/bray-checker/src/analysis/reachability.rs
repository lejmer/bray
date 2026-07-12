use bray_bound_tree::{BoundUnitId, ControlCompletion, ControlCompletionKind};

use crate::UnitCheckRequest;

use super::fixed_point::{FixedPointDomain, FixedPointOutcome, FlowDirection, solve_fixed_point};
use super::id::{AnalysisBlockId, AnalysisEdgeId};
use super::model::{
    AnalysisBlock, AnalysisEdge, AnalysisEdgeKind, AnalysisExitKind, AnalysisOperationKind,
    ControlFlowGraph,
};

pub(super) struct ReachabilityResult {
    unit: BoundUnitId,
    reachable_blocks: Box<[bool]>,
    reachable_edges: Box<[bool]>,
    completion: ControlCompletion,
}

impl ReachabilityResult {
    pub(super) fn is_block_reachable(&self, block: AnalysisBlockId) -> bool {
        if block.unit() != self.unit {
            return false;
        }

        block
            .to_index()
            .and_then(|index| self.reachable_blocks.get(index))
            .copied()
            .unwrap_or(false)
    }

    pub(super) fn is_edge_reachable(&self, edge: AnalysisEdgeId) -> bool {
        if edge.unit() != self.unit {
            return false;
        }

        edge.to_index()
            .and_then(|index| self.reachable_edges.get(index))
            .copied()
            .unwrap_or(false)
    }

    pub(super) const fn completion(&self) -> ControlCompletion {
        self.completion
    }
}

pub(crate) fn analyze_reachability(
    graph: &ControlFlowGraph,
    request: UnitCheckRequest<'_>,
) -> Option<ReachabilityResult> {
    let forward_domain = ReachabilityDomain::forward(graph);
    let forward = solve_fixed_point(graph, &forward_domain, &request);

    let forward = match forward {
        FixedPointOutcome::Complete(result) => result,
        FixedPointOutcome::Cancelled => return None,
        FixedPointOutcome::ConvergenceInvariantViolated => {
            panic!("finite reachability analysis exceeded its convergence bound")
        }
    };

    let reverse_domain = ReachabilityDomain::backward(graph);
    let reverse = solve_fixed_point(graph, &reverse_domain, &request);

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

    let reachable_blocks = graph
        .blocks()
        .iter()
        .map(|block| state_is_reachable(&forward, block.id()))
        .collect::<Box<[_]>>();

    let reachable_edges = graph
        .edges()
        .iter()
        .map(|edge| state_is_reachable(&forward, edge.source()))
        .collect::<Box<[_]>>();

    let completion = collect_completion(graph, &forward);

    Some(ReachabilityResult {
        unit: graph.unit(),
        reachable_blocks,
        reachable_edges,
        completion,
    })
}

fn state_is_reachable(
    result: &super::fixed_point::FixedPointResult<ReachabilityState>,
    block: AnalysisBlockId,
) -> bool {
    result
        .state(block)
        .is_some_and(ReachabilityState::is_reachable)
}

fn collect_completion(
    graph: &ControlFlowGraph,
    result: &super::fixed_point::FixedPointResult<ReachabilityState>,
) -> ControlCompletion {
    let mut kinds = Vec::new();

    for exit in graph.exits() {
        let Some(state) = result.state(exit.block()) else {
            continue;
        };

        if state.is_clean() && exit.kind() != AnalysisExitKind::Recovery {
            kinds.push(completion_kind(exit.kind()));
        }

        if state.is_recovered() || exit.kind() == AnalysisExitKind::Recovery {
            kinds.push(ControlCompletionKind::Recovered);
        }
    }

    ControlCompletion::from_kinds(kinds)
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

impl crate::CheckerCancellation for UnitCheckRequest<'_> {
    fn is_cancelled(&self) -> bool {
        (*self).is_cancelled()
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ReachabilityState {
    clean: bool,
    recovered: bool,
}

impl ReachabilityState {
    pub(super) const fn is_clean(&self) -> bool {
        self.clean
    }

    pub(super) const fn is_recovered(&self) -> bool {
        self.recovered
    }

    const fn is_reachable(&self) -> bool {
        self.clean || self.recovered
    }

    const fn after_recovery(self) -> Self {
        Self {
            clean: false,
            recovered: self.clean || self.recovered,
        }
    }
}

pub(super) struct ReachabilityDomain<'graph> {
    graph: &'graph ControlFlowGraph,
    direction: FlowDirection,
}

impl<'graph> ReachabilityDomain<'graph> {
    pub(super) const fn forward(graph: &'graph ControlFlowGraph) -> Self {
        Self {
            graph,
            direction: FlowDirection::Forward,
        }
    }

    pub(super) const fn backward(graph: &'graph ControlFlowGraph) -> Self {
        Self {
            graph,
            direction: FlowDirection::Backward,
        }
    }

    fn transfer(
        &self,
        block: &AnalysisBlock,
        state: ReachabilityState,
        edge: &AnalysisEdge,
    ) -> ReachabilityState {
        if self.block_has_recovery(block) || edge.kind() == AnalysisEdgeKind::Recovery {
            return state.after_recovery();
        }

        state
    }

    fn block_has_recovery(&self, block: &AnalysisBlock) -> bool {
        block.operations().iter().any(|operation| {
            self.graph.operation(*operation).is_some_and(|operation| {
                matches!(operation.kind(), AnalysisOperationKind::Recovery(_))
            })
        })
    }
}

impl FixedPointDomain for ReachabilityDomain<'_> {
    type State = ReachabilityState;

    fn direction(&self) -> FlowDirection {
        self.direction
    }

    fn bottom(&self) -> Self::State {
        ReachabilityState::default()
    }

    fn boundary(&self) -> Self::State {
        ReachabilityState {
            clean: true,
            recovered: false,
        }
    }

    fn merge_boundary(&self, target: &mut Self::State, incoming: &Self::State) -> bool {
        merge_state(target, *incoming)
    }

    fn propagate(
        &self,
        block: &AnalysisBlock,
        source: &Self::State,
        edge: &AnalysisEdge,
        target: &mut Self::State,
    ) -> bool {
        merge_state(target, self.transfer(block, *source, edge))
    }

    fn propagate_self(
        &self,
        block: &AnalysisBlock,
        state: &mut Self::State,
        edge: &AnalysisEdge,
    ) -> bool {
        merge_state(state, self.transfer(block, *state, edge))
    }

    fn convergence_bound(&self, graph: &ControlFlowGraph) -> usize {
        graph.blocks().len().saturating_mul(2)
    }
}

fn merge_state(target: &mut ReachabilityState, incoming: ReachabilityState) -> bool {
    let merged = ReachabilityState {
        clean: target.clean || incoming.clean,
        recovered: target.recovered || incoming.recovered,
    };

    let changed = *target != merged;

    *target = merged;

    changed
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundUnitId;

    use super::ReachabilityResult;
    use crate::analysis::id::{AnalysisBlockId, AnalysisEdgeId};

    #[test]
    fn reachability_masks_reject_foreign_unit_ids() {
        let unit = BoundUnitId::new(4);
        let result = ReachabilityResult {
            unit,
            reachable_blocks: [true].into(),
            reachable_edges: [true].into(),
            completion: Default::default(),
        };

        let foreign = BoundUnitId::new(5);

        assert!(!result.is_block_reachable(AnalysisBlockId::from_slot(foreign, 0)));
        assert!(!result.is_edge_reachable(AnalysisEdgeId::from_slot(foreign, 0)));
    }
}
