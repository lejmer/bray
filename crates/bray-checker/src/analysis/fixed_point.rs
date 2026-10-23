use std::collections::BTreeSet;

use bray_base::Cancellation;
use bray_bound_tree::BoundUnitId;

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

    fn initial(&self, _: &AnalysisBlock) -> Self::State {
        self.bottom()
    }

    fn should_seed(&self, _: &AnalysisBlock) -> bool {
        false
    }

    fn boundary(&self) -> Self::State;

    fn merge(
        &self,
        block: Option<AnalysisBlockId>,
        target: &mut Self::State,
        boundary: &Self::State,
    ) -> bool;

    fn transfer(&self, block: &AnalysisBlock, source: &Self::State) -> Self::State;

    fn propagate(
        &self,
        source: &Self::State,
        edge: &AnalysisEdge,
        target: &mut Self::State,
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
    cancellation: &dyn Cancellation,
) -> FixedPointOutcome<FixedPointResult<D::State>> {
    let mut states = graph
        .blocks()
        .iter()
        .map(|block| domain.initial(block))
        .collect::<Vec<_>>();

    let mut outputs = std::iter::repeat_with(|| None)
        .take(graph.blocks().len())
        .collect::<Vec<_>>();

    let boundaries = boundary_blocks(graph, domain.direction()).collect::<BTreeSet<_>>();
    let mut worklist = BTreeSet::new();

    for block in graph.blocks() {
        if boundaries.contains(&block.id()) || domain.should_seed(block) {
            enqueue(block.id(), &mut worklist, graph, domain.direction());
        }
    }

    let mut updates = 0_usize;
    let convergence_bound = domain.convergence_bound(graph);

    while let Some((_, block_id)) = worklist.pop_first() {
        if cancellation.is_cancelled() {
            return FixedPointOutcome::Cancelled;
        }

        let block_index = block_id.to_index().expect("scheduled block has an index");

        let block = graph
            .block(block_id)
            .expect("scheduled block belongs to its graph");

        let Some(incoming) = incoming_state(
            graph,
            domain,
            block,
            &outputs,
            &states[block_index],
            boundaries.contains(&block_id),
            cancellation,
        ) else {
            return FixedPointOutcome::Cancelled;
        };

        let changed = states[block_index] != incoming;

        if !changed && outputs[block_index].is_some() {
            continue;
        }

        states[block_index] = incoming;

        let output = domain.transfer(block, &states[block_index]);

        if outputs[block_index].as_ref() == Some(&output) {
            continue;
        }

        outputs[block_index] = Some(output);
        updates = updates.saturating_add(usize::from(changed));

        if updates > convergence_bound {
            return FixedPointOutcome::ConvergenceInvariantViolated;
        }

        for edge_id in traversal_edges(block, domain.direction()) {
            let edge = graph
                .edge(*edge_id)
                .expect("scheduled successor edge belongs to its graph");

            enqueue(
                traversal_target(edge, domain.direction()),
                &mut worklist,
                graph,
                domain.direction(),
            );
        }
    }

    FixedPointOutcome::Complete(FixedPointResult {
        unit: graph.unit(),
        states: states.into_boxed_slice(),
    })
}

fn incoming_state<D: FixedPointDomain>(
    graph: &ControlFlowGraph,
    domain: &D,
    block: &AnalysisBlock,
    outputs: &[Option<D::State>],
    previous: &D::State,
    boundary: bool,
    cancellation: &dyn Cancellation,
) -> Option<D::State> {
    let mut incoming = domain.initial(block);

    if boundary {
        domain.merge(None, &mut incoming, &domain.boundary());
    }

    let reverse = match domain.direction() {
        FlowDirection::Forward => FlowDirection::Backward,
        FlowDirection::Backward => FlowDirection::Forward,
    };

    // Join only the latest predecessor outputs. Accumulating old contributions
    // invents values at unchanged branches inside cycles and loses their relations.
    // Each output is transferred once and retained here without cloning it.
    let mut cyclic = false;

    for edge_id in traversal_edges(block, reverse) {
        if cancellation.is_cancelled() {
            return None;
        }

        let edge = graph
            .edge(*edge_id)
            .expect("incoming edge belongs to its graph");

        cyclic |= matches!(
            edge.kind(),
            super::model::AnalysisEdgeKind::LoopBack | super::model::AnalysisEdgeKind::LoopContinue
        );

        let source = traversal_target(edge, reverse);
        let index = source.to_index().expect("incoming source has an index");

        if let Some(output) = &outputs[index] {
            domain.propagate(output, edge, &mut incoming);
        }
    }

    if cyclic {
        // Loop headers retain their accumulated approximation. Otherwise a newly
        // excluded back edge can restore the entry literal and oscillate forever.
        domain.merge(Some(block.id()), &mut incoming, previous);
    }

    Some(incoming)
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

fn enqueue(
    block: AnalysisBlockId,
    worklist: &mut BTreeSet<(usize, AnalysisBlockId)>,
    graph: &ControlFlowGraph,
    direction: FlowDirection,
) {
    let rank = graph.component_order(block);

    let priority = match direction {
        FlowDirection::Forward => rank,
        FlowDirection::Backward => usize::MAX - rank,
    };

    worklist.insert((priority, block));
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

    #[test]
    fn forward_and_backward_domains_use_the_same_indexes() {
        let graph = linear_graph();

        let forward = solve_fixed_point(&graph, &CounterDomain(FlowDirection::Forward), &|| false);

        let backward =
            solve_fixed_point(&graph, &CounterDomain(FlowDirection::Backward), &|| false);

        let FixedPointOutcome::Complete(forward) = forward else {
            panic!("forward analysis must complete");
        };

        let FixedPointOutcome::Complete(backward) = backward else {
            panic!("backward analysis must complete");
        };

        assert!(
            forward
                .state(graph.entry())
                .is_some_and(|state| *state == 1)
        );

        assert!(
            backward
                .state(graph.entry())
                .is_some_and(|state| *state == 1)
        );
    }

    #[test]
    fn cancellation_discards_fixed_point_state() {
        let graph = linear_graph();
        let outcome = solve_fixed_point(&graph, &CounterDomain(FlowDirection::Forward), &|| true);

        assert_eq!(outcome, FixedPointOutcome::Cancelled);
    }

    #[test]
    fn fixed_point_results_reject_blocks_from_another_unit() {
        let graph = linear_graph();
        let outcome = solve_fixed_point(&graph, &CounterDomain(FlowDirection::Forward), &|| false);

        let FixedPointOutcome::Complete(result) = outcome else {
            panic!("forward analysis must complete");
        };

        let foreign = AnalysisBlockId::from_slot(BoundUnitId::new(99), 0);

        assert_eq!(result.state(foreign), None);
    }

    #[test]
    fn cyclic_domains_converge_under_their_proven_finite_bound() {
        let graph = cyclic_graph();
        let outcome = solve_fixed_point(&graph, &CounterDomain(FlowDirection::Forward), &|| false);

        let FixedPointOutcome::Complete(result) = outcome else {
            panic!("finite cyclic analysis must converge");
        };

        assert_eq!(result.state(graph.entry()), Some(&3));
    }

    #[test]
    fn unequal_branch_lengths_settle_joins_once_in_both_directions() {
        use std::cell::Cell;
        use std::collections::BTreeSet;

        struct Visits {
            direction: FlowDirection,
            transfers: Cell<usize>,
        }

        impl FixedPointDomain for Visits {
            type State = BTreeSet<AnalysisBlockId>;
            fn direction(&self) -> FlowDirection {
                self.direction
            }
            fn bottom(&self) -> Self::State {
                BTreeSet::new()
            }
            fn boundary(&self) -> Self::State {
                BTreeSet::new()
            }
            fn should_seed(&self, _: &AnalysisBlock) -> bool {
                true
            }
            fn merge(
                &self,
                _: Option<AnalysisBlockId>,
                target: &mut Self::State,
                source: &Self::State,
            ) -> bool {
                let before = target.len();

                target.extend(source.iter().copied());

                target.len() != before
            }
            fn transfer(&self, block: &AnalysisBlock, source: &Self::State) -> Self::State {
                self.transfers.set(self.transfers.get() + 1);

                source.iter().copied().chain([block.id()]).collect()
            }
            fn propagate(
                &self,
                source: &Self::State,
                edge: &AnalysisEdge,
                target: &mut Self::State,
            ) -> bool {
                self.merge(Some(edge.target()), target, source)
            }
            fn convergence_bound(&self, graph: &ControlFlowGraph) -> usize {
                graph.blocks().len() * graph.blocks().len()
            }
        }

        let unit = BoundUnitId::new(22);
        let block = |slot| AnalysisBlockId::from_slot(unit, slot);
        let pairs = [(0, 1), (0, 2), (2, 3), (3, 1), (1, 4)];

        let edges = pairs
            .iter()
            .enumerate()
            .map(|(index, (source, target))| {
                AnalysisEdge::new(
                    AnalysisEdgeId::from_slot(unit, u32::try_from(index).unwrap()),
                    block(*source),
                    block(*target),
                    AnalysisEdgeKind::Sequential,
                    None,
                )
            })
            .collect::<Vec<_>>();

        let blocks = (0..5)
            .map(|slot| {
                AnalysisBlock::new(
                    block(slot),
                    [],
                    edges
                        .iter()
                        .filter(|edge| edge.target() == block(slot))
                        .map(|edge| edge.id())
                        .collect::<Vec<_>>(),
                    edges
                        .iter()
                        .filter(|edge| edge.source() == block(slot))
                        .map(|edge| edge.id())
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();

        let graph = ControlFlowGraph::new(
            unit,
            block(0),
            blocks,
            edges,
            [],
            [AnalysisExit::new(
                block(4),
                AnalysisExitKind::NormalFallthrough,
            )],
        );

        for direction in [FlowDirection::Forward, FlowDirection::Backward] {
            let domain = Visits {
                direction,
                transfers: Cell::new(0),
            };

            let FixedPointOutcome::Complete(result) = solve_fixed_point(&graph, &domain, &|| false)
            else {
                panic!("finite diamond must converge")
            };

            let (target, expected) = match direction {
                FlowDirection::Forward => (
                    block(4),
                    BTreeSet::from([block(0), block(1), block(2), block(3)]),
                ),
                FlowDirection::Backward => (
                    block(0),
                    BTreeSet::from([block(1), block(2), block(3), block(4)]),
                ),
            };

            assert_eq!(result.state(target), Some(&expected));
            assert_eq!(domain.transfers.get(), 5);
        }
    }

    struct CounterDomain(FlowDirection);

    impl FixedPointDomain for CounterDomain {
        type State = u8;

        fn direction(&self) -> FlowDirection {
            self.0
        }

        fn bottom(&self) -> Self::State {
            0
        }

        fn boundary(&self) -> Self::State {
            1
        }

        fn merge(
            &self,
            _: Option<AnalysisBlockId>,
            target: &mut Self::State,
            boundary: &Self::State,
        ) -> bool {
            let changed = *target < *boundary;

            *target = (*target).max(*boundary);

            changed
        }

        fn transfer(&self, _: &AnalysisBlock, source: &Self::State) -> Self::State {
            (*source).min(3)
        }

        fn propagate(
            &self,
            source: &Self::State,
            edge: &AnalysisEdge,
            target: &mut Self::State,
        ) -> bool {
            let incoming = if edge.source() == edge.target() {
                source.saturating_add(1).min(3)
            } else {
                *source
            };

            self.merge(Some(edge.target()), target, &incoming)
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
