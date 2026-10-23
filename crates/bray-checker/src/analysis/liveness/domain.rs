use std::collections::BTreeSet;

use crate::analysis::fixed_point::{FixedPointDomain, FlowDirection};
use crate::analysis::id::AnalysisBlockId;
use crate::analysis::model::{AnalysisBlock, AnalysisEdge, AnalysisEdgeKind, ControlFlowGraph};
use crate::analysis::reachability::ReachabilityResult;

pub(super) struct SetTransfer<T> {
    pub(super) generated: BTreeSet<T>,
    pub(super) killed: BTreeSet<T>,
}

impl<T> Default for SetTransfer<T> {
    fn default() -> Self {
        Self {
            generated: BTreeSet::new(),
            killed: BTreeSet::new(),
        }
    }
}

/// The same backward use/definition equation serves storage and evaluated-value retention.
pub(super) struct SetLiveness<'a, T> {
    reachability: Option<&'a ReachabilityResult>,
    transfers: Box<[SetTransfer<T>]>,
    universe: BTreeSet<T>,
}

impl<'a, T: Copy + Ord> SetLiveness<'a, T> {
    pub(super) fn new(
        reachability: Option<&'a ReachabilityResult>,
        transfers: Box<[SetTransfer<T>]>,
        universe: BTreeSet<T>,
    ) -> Self {
        Self {
            reachability,
            transfers,
            universe,
        }
    }

    pub(super) fn universe(&self) -> &BTreeSet<T> {
        &self.universe
    }

    fn transfer(&self, block: &AnalysisBlock) -> &SetTransfer<T> {
        &self.transfers[block.id().to_index().expect("flow block has an index")]
    }
}

impl<T: Copy + Ord> FixedPointDomain for SetLiveness<'_, T> {
    type State = BTreeSet<T>;
    fn direction(&self) -> FlowDirection {
        FlowDirection::Backward
    }
    fn bottom(&self) -> Self::State {
        BTreeSet::new()
    }
    fn boundary(&self) -> Self::State {
        BTreeSet::new()
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

    fn should_seed(&self, block: &AnalysisBlock) -> bool {
        self.reachability
            .is_none_or(|reachable| reachable.is_block_reachable(block.id()))
            && !self.transfer(block).generated.is_empty()
    }

    fn transfer(&self, block: &AnalysisBlock, source: &Self::State) -> Self::State {
        if self
            .reachability
            .is_some_and(|reachable| !reachable.is_block_reachable(block.id()))
        {
            return BTreeSet::new();
        }

        let transfer = self.transfer(block);

        // Each predecessor owns its live input set independently of this block's output.
        let mut output = transfer.generated.clone();

        output.extend(
            source
                .iter()
                .filter(|subject| !transfer.killed.contains(subject))
                .copied(),
        );

        output
    }

    fn propagate(
        &self,
        source: &Self::State,
        edge: &AnalysisEdge,
        target: &mut Self::State,
    ) -> bool {
        if self
            .reachability
            .is_some_and(|reachable| !reachable.is_edge_reachable(edge.id()))
        {
            return false;
        }

        let source = if edge.kind() == AnalysisEdgeKind::Recovery {
            &self.universe
        } else {
            source
        };

        self.merge(Some(edge.target()), target, source)
    }

    fn convergence_bound(&self, graph: &ControlFlowGraph) -> usize {
        graph
            .blocks()
            .len()
            .saturating_mul(self.universe.len().saturating_add(1))
    }
}
