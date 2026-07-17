use bray_bound_tree::{AnyBoundNodeId, BoundUnitId};

use super::id::{AnalysisBlockId, AnalysisEdgeId, AnalysisOperationId, ProgramPointId};
use super::model::{
    AnalysisBlock, AnalysisEdge, AnalysisEdgeKind, AnalysisExit, AnalysisExitKind,
    AnalysisOperation, AnalysisOperationKind, AnalysisRefinement, AnalysisScopeExitPhase,
    AnalysisTaskOperationKind, ControlFlowGraph,
};

pub(super) struct ControlFlowGraphAssembler {
    unit: BoundUnitId,
    blocks: Vec<Vec<AnalysisOperationId>>,
    edges: Vec<AnalysisEdge>,
    operations: Vec<AnalysisOperation>,
    exits: Vec<AnalysisExit>,
}

impl ControlFlowGraphAssembler {
    pub(super) const fn new(unit: BoundUnitId) -> Self {
        Self {
            unit,
            blocks: Vec::new(),
            edges: Vec::new(),
            operations: Vec::new(),
            exits: Vec::new(),
        }
    }

    pub(super) fn push_block(&mut self) -> AnalysisBlockId {
        let Some(id) = AnalysisBlockId::try_from_index(self.unit, self.blocks.len()) else {
            return AnalysisBlockId::from_slot(self.unit, u32::MAX);
        };

        self.blocks.push(Vec::new());

        id
    }

    pub(super) fn push_bound(&mut self, block: AnalysisBlockId, node: AnyBoundNodeId) {
        self.push_operation(block, AnalysisOperationKind::Bound(node));
    }

    pub(super) fn push_recovery(&mut self, block: AnalysisBlockId, node: AnyBoundNodeId) {
        self.push_operation(block, AnalysisOperationKind::Recovery(node));
    }

    pub(super) fn push_direct_await(
        &mut self,
        block: AnalysisBlockId,
        expression: bray_bound_tree::BoundExpressionId,
    ) {
        self.push_operation(block, AnalysisOperationKind::DirectAwait(expression));
    }

    pub(super) fn push_task_operation(
        &mut self,
        block: AnalysisBlockId,
        expression: bray_bound_tree::BoundExpressionId,
        kind: AnalysisTaskOperationKind,
    ) {
        self.push_operation(
            block,
            AnalysisOperationKind::TaskOperation { expression, kind },
        );
    }

    pub(super) fn push_scope_exit(
        &mut self,
        current: AnalysisBlockId,
        block: bray_bound_tree::BoundBlockId,
    ) -> AnalysisBlockId {
        let cancellation = self.push_block();

        self.push_edge(current, cancellation, AnalysisEdgeKind::ScopeExit, None);

        self.push_operation(
            cancellation,
            AnalysisOperationKind::ScopeExit {
                block,
                phase: AnalysisScopeExitPhase::TaskCancellationBroadcast,
            },
        );

        let lifecycle = self.push_block();

        self.push_edge(cancellation, lifecycle, AnalysisEdgeKind::Sequential, None);

        self.push_operation(
            lifecycle,
            AnalysisOperationKind::ScopeExit {
                block,
                phase: AnalysisScopeExitPhase::LifecycleResolution,
            },
        );

        lifecycle
    }

    pub(super) fn push_edge(
        &mut self,
        source: AnalysisBlockId,
        target: AnalysisBlockId,
        kind: AnalysisEdgeKind,
        refinement: Option<AnalysisRefinement>,
    ) {
        let Some(id) = AnalysisEdgeId::try_from_index(self.unit, self.edges.len()) else {
            return;
        };

        self.edges
            .push(AnalysisEdge::new(id, source, target, kind, refinement));
    }

    pub(super) fn push_exit(&mut self, block: AnalysisBlockId, kind: AnalysisExitKind) {
        let exit = self.push_block();
        let edge = exit_edge_kind(kind);

        self.push_edge(block, exit, edge, None);
        self.exits.push(AnalysisExit::new(exit, kind));
    }

    pub(super) fn finish(self, entry: AnalysisBlockId) -> ControlFlowGraph {
        let mut predecessors = vec![Vec::new(); self.blocks.len()];
        let mut successors = vec![Vec::new(); self.blocks.len()];

        for edge in &self.edges {
            if let Some(source) = edge
                .source()
                .to_index()
                .and_then(|index| successors.get_mut(index))
            {
                source.push(edge.id());
            }

            if let Some(target) = edge
                .target()
                .to_index()
                .and_then(|index| predecessors.get_mut(index))
            {
                target.push(edge.id());
            }
        }

        let blocks = self
            .blocks
            .into_iter()
            .enumerate()
            .filter_map(|(index, operations)| {
                let id = AnalysisBlockId::try_from_index(self.unit, index)?;

                Some(AnalysisBlock::new(
                    id,
                    operations,
                    std::mem::take(&mut predecessors[index]),
                    std::mem::take(&mut successors[index]),
                ))
            })
            .collect::<Box<[_]>>();

        ControlFlowGraph::new(
            self.unit,
            entry,
            blocks,
            self.edges,
            self.operations,
            self.exits,
        )
    }

    fn push_operation(&mut self, block: AnalysisBlockId, kind: AnalysisOperationKind) {
        let Some(id) = AnalysisOperationId::try_from_index(self.unit, self.operations.len()) else {
            return;
        };

        let Some(before_index) = self.operations.len().checked_mul(2) else {
            return;
        };

        let Some(after_index) = before_index.checked_add(1) else {
            return;
        };

        let Some(before) = ProgramPointId::try_from_index(self.unit, before_index) else {
            return;
        };

        let Some(after) = ProgramPointId::try_from_index(self.unit, after_index) else {
            return;
        };

        self.operations
            .push(AnalysisOperation::new(id, kind, before, after));

        if let Some(operations) = block
            .to_index()
            .and_then(|index| self.blocks.get_mut(index))
        {
            operations.push(id);
        }
    }
}

const fn exit_edge_kind(kind: AnalysisExitKind) -> AnalysisEdgeKind {
    match kind {
        AnalysisExitKind::NormalFallthrough => AnalysisEdgeKind::Sequential,
        AnalysisExitKind::Return => AnalysisEdgeKind::Return,
        AnalysisExitKind::ResultErrorPropagation => AnalysisEdgeKind::ResultErrorPropagation,
        AnalysisExitKind::Divergence => AnalysisEdgeKind::Divergence,
        AnalysisExitKind::Panic => AnalysisEdgeKind::Panic,
        AnalysisExitKind::Cancellation => AnalysisEdgeKind::RunCancellation,
        AnalysisExitKind::Yield => AnalysisEdgeKind::Yield,
        AnalysisExitKind::Recovery => AnalysisEdgeKind::Recovery,
    }
}
