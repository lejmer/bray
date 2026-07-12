use std::collections::VecDeque;

use bray_bound_tree::BoundUnitId;

use crate::CheckerCancellation;

use super::id::AnalysisBlockId;
use super::model::{AnalysisBlock, AnalysisEdge, ControlFlowGraph};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FlowDirection {
    Forward,
    Backward,
}

pub(crate) trait FixedPointDomain {
    type State: Eq;

    fn direction(&self) -> FlowDirection;

    fn bottom(&self) -> Self::State;

    fn boundary(&self) -> Self::State;

    fn merge_boundary(&self, target: &mut Self::State, boundary: &Self::State) -> bool;

    fn propagate(
        &self,
        block: &AnalysisBlock,
        source: &Self::State,
        edge: &AnalysisEdge,
        target: &mut Self::State,
    ) -> bool;

    fn propagate_self(
        &self,
        block: &AnalysisBlock,
        state: &mut Self::State,
        edge: &AnalysisEdge,
    ) -> bool;

    fn convergence_bound(&self, graph: &ControlFlowGraph) -> usize;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FixedPointResult<State> {
    unit: BoundUnitId,
    states: Box<[State]>,
}

impl<State> FixedPointResult<State> {
    pub(crate) fn state(&self, block: AnalysisBlockId) -> Option<&State> {
        if block.unit() != self.unit {
            return None;
        }

        block.to_index().and_then(|index| self.states.get(index))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FixedPointOutcome<State> {
    Complete(State),
    Cancelled,
    ConvergenceInvariantViolated,
}

pub(crate) fn solve_fixed_point<D: FixedPointDomain>(
    graph: &ControlFlowGraph,
    domain: &D,
    cancellation: &dyn CheckerCancellation,
) -> FixedPointOutcome<FixedPointResult<D::State>> {
    let mut states = std::iter::repeat_with(|| domain.bottom())
        .take(graph.blocks().len())
        .collect::<Vec<_>>();

    let mut queued = vec![false; graph.blocks().len()];
    let mut worklist = VecDeque::new();

    for boundary in boundary_blocks(graph, domain.direction()) {
        let Some(index) = boundary.to_index() else {
            continue;
        };

        let Some(state) = states.get_mut(index) else {
            continue;
        };

        domain.merge_boundary(state, &domain.boundary());
        enqueue(boundary, &mut worklist, &mut queued);
    }

    let mut updates = 0_usize;
    let convergence_bound = domain.convergence_bound(graph);

    while let Some(block_id) = worklist.pop_front() {
        if cancellation.is_cancelled() {
            return FixedPointOutcome::Cancelled;
        }

        let Some(block_index) = block_id.to_index() else {
            continue;
        };

        let Some(marker) = queued.get_mut(block_index) else {
            continue;
        };

        *marker = false;

        let Some(block) = graph.block(block_id) else {
            continue;
        };

        for edge_id in traversal_edges(block, domain.direction()) {
            if cancellation.is_cancelled() {
                return FixedPointOutcome::Cancelled;
            }

            let Some(edge) = graph.edge(*edge_id) else {
                continue;
            };

            let target = traversal_target(edge, domain.direction());
            let Some(target_index) = target.to_index() else {
                continue;
            };

            if block_index == target_index {
                let Some(state) = states.get_mut(block_index) else {
                    continue;
                };

                if domain.propagate_self(block, state, edge) {
                    updates = updates.saturating_add(1);

                    if updates > convergence_bound {
                        return FixedPointOutcome::ConvergenceInvariantViolated;
                    }

                    enqueue(target, &mut worklist, &mut queued);
                }

                continue;
            }

            let Some((source_state, target_state)) =
                two_states(&mut states, block_index, target_index)
            else {
                continue;
            };

            if domain.propagate(block, source_state, edge, target_state) {
                updates = updates.saturating_add(1);

                if updates > convergence_bound {
                    return FixedPointOutcome::ConvergenceInvariantViolated;
                }

                enqueue(target, &mut worklist, &mut queued);
            }
        }
    }

    FixedPointOutcome::Complete(FixedPointResult {
        unit: graph.unit(),
        states: states.into_boxed_slice(),
    })
}

fn two_states<State>(
    states: &mut [State],
    source: usize,
    target: usize,
) -> Option<(&State, &mut State)> {
    if source < target {
        let (before_target, from_target) = states.split_at_mut(target);

        return Some((before_target.get(source)?, from_target.first_mut()?));
    }

    let (before_source, from_source) = states.split_at_mut(source);

    Some((from_source.first()?, before_source.get_mut(target)?))
}

fn boundary_blocks(
    graph: &ControlFlowGraph,
    direction: FlowDirection,
) -> Box<dyn Iterator<Item = AnalysisBlockId> + '_> {
    match direction {
        FlowDirection::Forward => Box::new(std::iter::once(graph.entry())),
        FlowDirection::Backward => Box::new(graph.exits().iter().map(|exit| exit.block())),
    }
}

fn traversal_edges(
    block: &AnalysisBlock,
    direction: FlowDirection,
) -> &[super::id::AnalysisEdgeId] {
    match direction {
        FlowDirection::Forward => block.successors(),
        FlowDirection::Backward => block.predecessors(),
    }
}

const fn traversal_target(edge: &AnalysisEdge, direction: FlowDirection) -> AnalysisBlockId {
    match direction {
        FlowDirection::Forward => edge.target(),
        FlowDirection::Backward => edge.source(),
    }
}

fn enqueue(block: AnalysisBlockId, worklist: &mut VecDeque<AnalysisBlockId>, queued: &mut [bool]) {
    let Some(index) = block.to_index() else {
        return;
    };

    let Some(marker) = queued.get_mut(index) else {
        return;
    };

    if *marker {
        return;
    }

    *marker = true;

    worklist.push_back(block);
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundUnitId;

    use super::{FixedPointDomain, FixedPointOutcome, FlowDirection, solve_fixed_point};
    use crate::analysis::id::{AnalysisBlockId, AnalysisEdgeId};
    use crate::analysis::model::{
        AnalysisBlock, AnalysisEdge, AnalysisEdgeKind, AnalysisExit, AnalysisExitKind,
        ControlFlowGraph,
    };
    use crate::analysis::reachability::ReachabilityDomain;

    #[test]
    fn forward_and_backward_domains_use_the_same_indexes() {
        let graph = linear_graph();

        let forward = solve_fixed_point(&graph, &ReachabilityDomain::forward(), &|| false);
        let backward = solve_fixed_point(&graph, &ReachabilityDomain::backward(), &|| false);

        let FixedPointOutcome::Complete(forward) = forward else {
            panic!("forward analysis must complete");
        };

        let FixedPointOutcome::Complete(backward) = backward else {
            panic!("backward analysis must complete");
        };

        assert_eq!(forward.state(graph.entry()), Some(&true));
        assert_eq!(backward.state(graph.entry()), Some(&true));
    }

    #[test]
    fn cancellation_discards_fixed_point_state() {
        let graph = linear_graph();
        let outcome = solve_fixed_point(&graph, &ReachabilityDomain::forward(), &|| true);

        assert_eq!(outcome, FixedPointOutcome::Cancelled);
    }

    #[test]
    fn fixed_point_results_reject_blocks_from_another_unit() {
        let graph = linear_graph();
        let outcome = solve_fixed_point(&graph, &ReachabilityDomain::forward(), &|| false);

        let FixedPointOutcome::Complete(result) = outcome else {
            panic!("forward analysis must complete");
        };

        let foreign = AnalysisBlockId::from_slot(BoundUnitId::new(99), 0);

        assert_eq!(result.state(foreign), None);
    }

    #[test]
    fn cyclic_domains_converge_under_their_proven_finite_bound() {
        let graph = cyclic_graph();
        let outcome = solve_fixed_point(&graph, &CounterDomain, &|| false);

        let FixedPointOutcome::Complete(result) = outcome else {
            panic!("finite cyclic analysis must converge");
        };

        assert_eq!(result.state(graph.entry()), Some(&3));
    }

    struct CounterDomain;

    impl FixedPointDomain for CounterDomain {
        type State = u8;

        fn direction(&self) -> FlowDirection {
            FlowDirection::Forward
        }

        fn bottom(&self) -> Self::State {
            0
        }

        fn boundary(&self) -> Self::State {
            1
        }

        fn merge_boundary(&self, target: &mut Self::State, boundary: &Self::State) -> bool {
            let changed = *target < *boundary;

            *target = (*target).max(*boundary);

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

        fn propagate_self(
            &self,
            _: &AnalysisBlock,
            state: &mut Self::State,
            _: &AnalysisEdge,
        ) -> bool {
            if *state >= 3 {
                return false;
            }

            *state += 1;

            true
        }

        fn convergence_bound(&self, _: &ControlFlowGraph) -> usize {
            3
        }
    }

    fn linear_graph() -> ControlFlowGraph {
        let unit = BoundUnitId::new(3);
        let first = AnalysisBlockId::from_slot(unit, 0);
        let second = AnalysisBlockId::from_slot(unit, 1);
        let edge = AnalysisEdgeId::from_slot(unit, 0);

        ControlFlowGraph::new(
            unit,
            first,
            [
                AnalysisBlock::new(first, [], [], [edge]),
                AnalysisBlock::new(second, [], [edge], []),
            ],
            [AnalysisEdge::new(
                edge,
                first,
                second,
                AnalysisEdgeKind::Sequential,
                None,
            )],
            [],
            [AnalysisExit::new(
                second,
                AnalysisExitKind::NormalFallthrough,
            )],
        )
    }

    fn cyclic_graph() -> ControlFlowGraph {
        let unit = BoundUnitId::new(4);
        let block = AnalysisBlockId::from_slot(unit, 0);
        let edge = AnalysisEdgeId::from_slot(unit, 0);

        ControlFlowGraph::new(
            unit,
            block,
            [AnalysisBlock::new(block, [], [edge], [edge])],
            [AnalysisEdge::new(
                edge,
                block,
                block,
                AnalysisEdgeKind::LoopBack,
                None,
            )],
            [],
            [AnalysisExit::new(block, AnalysisExitKind::Divergence)],
        )
    }
}
