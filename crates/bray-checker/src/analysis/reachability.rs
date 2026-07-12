use crate::UnitCheckRequest;

use super::fixed_point::{FixedPointDomain, FixedPointOutcome, FlowDirection, solve_fixed_point};
use super::model::{AnalysisBlock, AnalysisEdge, ControlFlowGraph};

pub(crate) fn analyze_reachability(
    graph: &ControlFlowGraph,
    request: UnitCheckRequest<'_>,
) -> Option<()> {
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

    Some(())
}

impl crate::CheckerCancellation for UnitCheckRequest<'_> {
    fn is_cancelled(&self) -> bool {
        (*self).is_cancelled()
    }
}

struct ReachabilityDomain {
    direction: FlowDirection,
}

impl ReachabilityDomain {
    const fn forward() -> Self {
        Self {
            direction: FlowDirection::Forward,
        }
    }

    const fn backward() -> Self {
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
