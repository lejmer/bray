use std::collections::VecDeque;

use crate::CheckerCancellation;

use super::id::AnalysisBlockId;
use super::model::{AnalysisBlock, AnalysisEdge, AnalysisTopology};

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

    fn convergence_bound(&self, topology: &AnalysisTopology) -> usize;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FixedPointResult<State> {
    states: Box<[State]>,
}

impl<State> FixedPointResult<State> {
    pub(crate) fn state(&self, block: AnalysisBlockId) -> Option<&State> {
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
    topology: &AnalysisTopology,
    domain: &D,
    cancellation: &dyn CheckerCancellation,
) -> FixedPointOutcome<FixedPointResult<D::State>> {
    let mut states = std::iter::repeat_with(|| domain.bottom())
        .take(topology.blocks().len())
        .collect::<Vec<_>>();
    let mut queued = vec![false; topology.blocks().len()];
    let mut worklist = VecDeque::new();

    for boundary in boundary_blocks(topology, domain.direction()) {
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
    let convergence_bound = domain.convergence_bound(topology);

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

        let Some(block) = topology.block(block_id) else {
            continue;
        };

        for edge_id in traversal_edges(block, domain.direction()) {
            if cancellation.is_cancelled() {
                return FixedPointOutcome::Cancelled;
            }

            let Some(edge) = topology.edge(*edge_id) else {
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
    topology: &AnalysisTopology,
    direction: FlowDirection,
) -> Box<dyn Iterator<Item = AnalysisBlockId> + '_> {
    match direction {
        FlowDirection::Forward => Box::new(std::iter::once(topology.entry())),
        FlowDirection::Backward => Box::new(topology.exits().iter().map(|exit| exit.block())),
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
    use super::{FixedPointDomain, FixedPointOutcome, FlowDirection, solve_fixed_point};
    use crate::analysis::id::{AnalysisBlockId, AnalysisEdgeId};
    use crate::analysis::model::{
        AnalysisBlock, AnalysisEdge, AnalysisEdgeKind, AnalysisExit, AnalysisExitKind,
        AnalysisTopology,
    };
    use bray_bound_tree::BoundUnitId;

    #[test]
    fn forward_and_backward_domains_use_the_same_indexes() {
        let topology = linear_topology();

        let forward = solve_fixed_point(&topology, &BooleanDomain::forward(), &|| false);
        let backward = solve_fixed_point(&topology, &BooleanDomain::backward(), &|| false);

        let FixedPointOutcome::Complete(forward) = forward else {
            panic!("forward analysis must complete");
        };

        let FixedPointOutcome::Complete(backward) = backward else {
            panic!("backward analysis must complete");
        };

        assert_eq!(forward.state(topology.entry()), Some(&true));
        assert_eq!(backward.state(topology.entry()), Some(&true));
    }

    #[test]
    fn cancellation_discards_fixed_point_state() {
        let topology = linear_topology();
        let outcome = solve_fixed_point(&topology, &BooleanDomain::forward(), &|| true);

        assert_eq!(outcome, FixedPointOutcome::Cancelled);
    }

    #[test]
    fn cyclic_domains_converge_under_their_proven_finite_bound() {
        let topology = cyclic_topology();
        let outcome = solve_fixed_point(&topology, &CounterDomain, &|| false);

        let FixedPointOutcome::Complete(result) = outcome else {
            panic!("finite cyclic analysis must converge");
        };

        assert_eq!(result.state(topology.entry()), Some(&3));
    }

    struct BooleanDomain {
        direction: FlowDirection,
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

        fn convergence_bound(&self, _: &AnalysisTopology) -> usize {
            3
        }
    }

    impl BooleanDomain {
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

    impl FixedPointDomain for BooleanDomain {
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

        fn convergence_bound(&self, topology: &AnalysisTopology) -> usize {
            topology.blocks().len()
        }
    }

    fn linear_topology() -> AnalysisTopology {
        let unit = BoundUnitId::new(3);
        let first = AnalysisBlockId::from_slot(unit, 0);
        let second = AnalysisBlockId::from_slot(unit, 1);
        let edge = AnalysisEdgeId::from_slot(unit, 0);

        AnalysisTopology::new(
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

    fn cyclic_topology() -> AnalysisTopology {
        let unit = BoundUnitId::new(4);
        let block = AnalysisBlockId::from_slot(unit, 0);
        let edge = AnalysisEdgeId::from_slot(unit, 0);

        AnalysisTopology::new(
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
