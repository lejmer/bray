use bray_bound_tree::{AnyBoundNodeId, BoundExpressionId, BoundPatternId, BoundUnitId};

use super::id::{AnalysisBlockId, AnalysisEdgeId, AnalysisOperationId, ProgramPointId};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum AnalysisOperationKind {
    Bound(AnyBoundNodeId),
    DirectAwait(BoundExpressionId),
    TaskOperation {
        expression: BoundExpressionId,
        kind: AnalysisTaskOperationKind,
    },
    ScopeExit {
        block: bray_bound_tree::BoundBlockId,
        phase: AnalysisScopeExitPhase,
    },
    Recovery(AnyBoundNodeId),
}

impl AnalysisOperationKind {
    pub(crate) const fn node(self) -> AnyBoundNodeId {
        match self {
            Self::Bound(node) | Self::Recovery(node) => node,
            Self::DirectAwait(expression) | Self::TaskOperation { expression, .. } => {
                AnyBoundNodeId::Expression(expression)
            }
            Self::ScopeExit { block, .. } => AnyBoundNodeId::Block(block),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum AnalysisTaskOperationKind {
    /// `Future<T>.start()` immediately transfers the future into an owned task.
    Start,
    /// `Task<T>.join()` transfers the task into a lazy observation computation.
    Join,
    /// `Task<T>.cancel()` transfers the task into a lazy cancellation computation.
    Cancel,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum AnalysisScopeExitPhase {
    /// Requests cancellation for every recursively owned unresolved task without waiting.
    TaskCancellationBroadcast,
    /// Resolves joining, finalization, destruction, and related lifecycle obligations.
    LifecycleResolution,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct AnalysisOperation {
    id: AnalysisOperationId,
    kind: AnalysisOperationKind,
    before: ProgramPointId,
    after: ProgramPointId,
}

impl AnalysisOperation {
    pub(crate) const fn new(
        id: AnalysisOperationId,
        kind: AnalysisOperationKind,
        before: ProgramPointId,
        after: ProgramPointId,
    ) -> Self {
        Self {
            id,
            kind,
            before,
            after,
        }
    }

    pub(crate) const fn id(self) -> AnalysisOperationId {
        self.id
    }

    pub(crate) const fn kind(self) -> AnalysisOperationKind {
        self.kind
    }

    pub(crate) const fn before(self) -> ProgramPointId {
        self.before
    }

    pub(crate) const fn after(self) -> ProgramPointId {
        self.after
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum AnalysisEdgeKind {
    Sequential,
    ConditionalTrue,
    ConditionalFalse,
    MatchArm,
    MatchNoMatch,
    LoopEntry,
    LoopBack,
    LoopBreak,
    LoopContinue,
    Return,
    Divergence,
    ResultSuccess,
    ResultErrorPropagation,
    RunResultCompleted,
    RunResultPanicked,
    RunResultCancelled,
    NullablePresent,
    NullableAbsent,
    Catch,
    Panic,
    AwaitSuspend,
    AwaitResume,
    RunCancellation,
    ScopeExit,
    Yield,
    Recovery,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum AnalysisRefinement {
    NullablePresence {
        expression: BoundExpressionId,
        is_present: bool,
    },
    PatternSuccess(BoundPatternId),
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct AnalysisEdge {
    id: AnalysisEdgeId,
    source: AnalysisBlockId,
    target: AnalysisBlockId,
    kind: AnalysisEdgeKind,
    refinement: Option<AnalysisRefinement>,
}

impl AnalysisEdge {
    pub(crate) const fn new(
        id: AnalysisEdgeId,
        source: AnalysisBlockId,
        target: AnalysisBlockId,
        kind: AnalysisEdgeKind,
        refinement: Option<AnalysisRefinement>,
    ) -> Self {
        Self {
            id,
            source,
            target,
            kind,
            refinement,
        }
    }

    pub(crate) const fn id(self) -> AnalysisEdgeId {
        self.id
    }

    pub(crate) const fn source(self) -> AnalysisBlockId {
        self.source
    }

    pub(crate) const fn target(self) -> AnalysisBlockId {
        self.target
    }

    pub(crate) const fn kind(self) -> AnalysisEdgeKind {
        self.kind
    }

    pub(crate) const fn refinement(self) -> Option<AnalysisRefinement> {
        self.refinement
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct AnalysisBlock {
    id: AnalysisBlockId,
    operations: Box<[AnalysisOperationId]>,
    predecessors: Box<[AnalysisEdgeId]>,
    successors: Box<[AnalysisEdgeId]>,
}

impl AnalysisBlock {
    pub(crate) fn new(
        id: AnalysisBlockId,
        operations: impl Into<Box<[AnalysisOperationId]>>,
        predecessors: impl Into<Box<[AnalysisEdgeId]>>,
        successors: impl Into<Box<[AnalysisEdgeId]>>,
    ) -> Self {
        Self {
            id,
            operations: operations.into(),
            predecessors: predecessors.into(),
            successors: successors.into(),
        }
    }

    pub(crate) const fn id(&self) -> AnalysisBlockId {
        self.id
    }

    pub(crate) fn operations(&self) -> &[AnalysisOperationId] {
        &self.operations
    }

    pub(crate) fn predecessors(&self) -> &[AnalysisEdgeId] {
        &self.predecessors
    }

    pub(crate) fn successors(&self) -> &[AnalysisEdgeId] {
        &self.successors
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum AnalysisExitKind {
    NormalFallthrough,
    Return,
    ResultErrorPropagation,
    Divergence,
    Panic,
    Cancellation,
    Yield,
    Recovery,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct AnalysisExit {
    block: AnalysisBlockId,
    kind: AnalysisExitKind,
}

impl AnalysisExit {
    pub(crate) const fn new(block: AnalysisBlockId, kind: AnalysisExitKind) -> Self {
        Self { block, kind }
    }

    pub(crate) const fn block(self) -> AnalysisBlockId {
        self.block
    }

    pub(crate) const fn kind(self) -> AnalysisExitKind {
        self.kind
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ControlFlowGraph {
    unit: BoundUnitId,
    entry: AnalysisBlockId,
    blocks: Box<[AnalysisBlock]>,
    edges: Box<[AnalysisEdge]>,
    operations: Box<[AnalysisOperation]>,
    exits: Box<[AnalysisExit]>,
}

impl ControlFlowGraph {
    pub(crate) fn new(
        unit: BoundUnitId,
        entry: AnalysisBlockId,
        blocks: impl Into<Box<[AnalysisBlock]>>,
        edges: impl Into<Box<[AnalysisEdge]>>,
        operations: impl Into<Box<[AnalysisOperation]>>,
        exits: impl Into<Box<[AnalysisExit]>>,
    ) -> Self {
        Self {
            unit,
            entry,
            blocks: blocks.into(),
            edges: edges.into(),
            operations: operations.into(),
            exits: exits.into(),
        }
    }

    pub(crate) const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    pub(crate) const fn entry(&self) -> AnalysisBlockId {
        self.entry
    }

    pub(crate) fn blocks(&self) -> &[AnalysisBlock] {
        &self.blocks
    }

    pub(crate) fn edges(&self) -> &[AnalysisEdge] {
        &self.edges
    }

    pub(crate) fn operations(&self) -> &[AnalysisOperation] {
        &self.operations
    }

    pub(crate) fn exits(&self) -> &[AnalysisExit] {
        &self.exits
    }

    pub(crate) fn block(&self, id: AnalysisBlockId) -> Option<&AnalysisBlock> {
        (id.unit() == self.unit)
            .then(|| id.to_index())
            .flatten()
            .and_then(|index| self.blocks.get(index))
    }

    pub(crate) fn edge(&self, id: AnalysisEdgeId) -> Option<&AnalysisEdge> {
        (id.unit() == self.unit)
            .then(|| id.to_index())
            .flatten()
            .and_then(|index| self.edges.get(index))
    }

    pub(crate) fn operation(&self, id: AnalysisOperationId) -> Option<&AnalysisOperation> {
        (id.unit() == self.unit)
            .then(|| id.to_index())
            .flatten()
            .and_then(|index| self.operations.get(index))
    }

    pub(crate) fn is_well_formed(&self) -> bool {
        if self.entry.unit() != self.unit() || self.block(self.entry).is_none() {
            return false;
        }

        let blocks_are_valid = self.blocks.iter().all(|block| {
            block.id().unit() == self.unit
                && block.operations().iter().all(|operation| {
                    operation.unit() == self.unit
                        && self
                            .operation(*operation)
                            .is_some_and(|record| record.id() == *operation)
                })
        });

        let operations_are_valid = self.operations().iter().all(|operation| {
            operation.id().unit() == self.unit
                && operation.before().unit() == self.unit
                && operation.after().unit() == self.unit
                && operation.before().to_index().is_some()
                && operation.after().to_index().is_some()
                && operation.kind().node().unit() == self.unit
        });

        let edges_are_valid = self.edges().iter().all(|edge| {
            edge.id().unit() == self.unit
                && self.block(edge.source()).is_some()
                && self.block(edge.target()).is_some()
                && (edge.source() != edge.target() || edge.kind() == AnalysisEdgeKind::LoopBack)
        });

        let refinements_are_valid = self.edges().iter().all(|edge| match edge.refinement() {
            Some(AnalysisRefinement::NullablePresence { expression, .. }) => {
                expression.unit() == self.unit
            }
            Some(AnalysisRefinement::PatternSuccess(pattern)) => pattern.unit() == self.unit,
            None => true,
        });

        let exits_are_valid = self.exits.iter().all(|exit| {
            self.block(exit.block()).is_some()
                && matches!(
                    exit.kind(),
                    AnalysisExitKind::NormalFallthrough
                        | AnalysisExitKind::Return
                        | AnalysisExitKind::ResultErrorPropagation
                        | AnalysisExitKind::Divergence
                        | AnalysisExitKind::Panic
                        | AnalysisExitKind::Cancellation
                        | AnalysisExitKind::Yield
                        | AnalysisExitKind::Recovery
                )
        });

        blocks_are_valid
            && operations_are_valid
            && edges_are_valid
            && refinements_are_valid
            && exits_are_valid
    }
}
