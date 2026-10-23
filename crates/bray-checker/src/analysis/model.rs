use bray_bound_tree::{AnyBoundNodeId, BoundExpressionId, BoundPatternId, BoundUnitId};
use bray_compiler_known::ImplementationHook;

use super::id::{AnalysisBlockId, AnalysisEdgeId, AnalysisOperationId, ProgramPointId};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum AnalysisOperationKind {
    Bound(AnyBoundNodeId),
    PatternObservation(BoundPatternId),
    Result {
        expression: BoundExpressionId,
        phase: AnalysisResultPhase,
    },
    Call {
        invocation: bray_bound_tree::BoundExecutionSite,
        phase: AnalysisCallPhase,
    },
    Suspension {
        occurrence: bray_bound_tree::BoundExecutionSite,
        kind: AnalysisSuspensionKind,
    },
    TaskOperation {
        expression: BoundExpressionId,
        kind: AnalysisTaskOperationKind,
    },
    ScopeExit {
        block: bray_bound_tree::BoundBlockId,
        exit: AnyBoundNodeId,
        phase: AnalysisScopeExitPhase,
        preserves_exit_value: bool,
    },
    Recovery(AnyBoundNodeId),
}

impl AnalysisOperationKind {
    pub(crate) const fn is_normal_cleanup(self) -> bool {
        matches!(
            self,
            Self::ScopeExit {
                preserves_exit_value: true,
                ..
            } | Self::Bound(AnyBoundNodeId::Expression(_))
                | Self::Result {
                    phase: AnalysisResultPhase::Completion,
                    ..
                }
                | Self::Call {
                    invocation: bray_bound_tree::BoundExecutionSite::Node(
                        AnyBoundNodeId::Expression(_)
                    ),
                    phase: AnalysisCallPhase::Completion,
                }
        )
    }

    pub(crate) const fn occurrence(self) -> bray_bound_tree::BoundExecutionSite {
        match self {
            Self::Call { invocation, .. } => invocation,
            Self::Suspension { occurrence, .. } => occurrence,
            Self::ScopeExit { exit, .. } => bray_bound_tree::BoundExecutionSite::Cleanup(exit),
            _ => bray_bound_tree::BoundExecutionSite::Node(self.node()),
        }
    }

    pub(crate) const fn node(self) -> AnyBoundNodeId {
        match self {
            Self::Bound(node) | Self::Recovery(node) => node,
            Self::PatternObservation(pattern) => AnyBoundNodeId::Pattern(pattern),
            Self::Call { invocation, .. } => invocation.node(),
            Self::Suspension { occurrence, .. } => occurrence.node(),
            Self::TaskOperation { expression, .. } | Self::Result { expression, .. } => {
                AnyBoundNodeId::Expression(expression)
            }
            Self::ScopeExit { block, .. } => AnyBoundNodeId::Block(block),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum AnalysisResultPhase {
    Begin,
    Completion,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum AnalysisCallPhase {
    Input,
    Attempt,
    Completion,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum AnalysisSuspensionKind {
    Await,
    Yield,
    ScopedCall,
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

impl AnalysisTaskOperationKind {
    pub(crate) const fn from_implementation_hook(hook: ImplementationHook) -> Option<Self> {
        match hook {
            ImplementationHook::FutureStart => Some(Self::Start),
            ImplementationHook::TaskJoin => Some(Self::Join),
            ImplementationHook::TaskCancel => Some(Self::Cancel),
            _ => None,
        }
    }
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
    cleanup_suspension: bool,
    scoped_exit: Option<(bray_bound_tree::StorageExitPoint, bool)>,
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
            cleanup_suspension: false,
            scoped_exit: None,
        }
    }

    pub(super) fn with_scoped_exit(
        mut self,
        exit: bray_bound_tree::StorageExitPoint,
        preserves_exit_value: bool,
    ) -> Self {
        self.scoped_exit = Some((exit, preserves_exit_value));

        self
    }

    pub(crate) fn scoped_exit(self) -> Option<bray_bound_tree::StorageExitPoint> {
        self.scoped_exit.map(|(exit, _)| exit)
    }

    pub(crate) fn is_normal_cleanup(self) -> bool {
        self.scoped_exit
            .map_or_else(|| self.kind.is_normal_cleanup(), |(_, normal)| normal)
    }

    pub(crate) fn cleanup_suspension(self) -> Option<bray_bound_tree::BoundExecutionSite> {
        self.cleanup_suspension
            .then(|| bray_bound_tree::BoundExecutionSite::Cleanup(self.kind.occurrence().node()))
    }

    pub(crate) fn suspension(self) -> Option<bray_bound_tree::BoundExecutionSite> {
        match self.kind {
            AnalysisOperationKind::Suspension { occurrence, .. } => Some(occurrence),
            _ => self.cleanup_suspension(),
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
    Suspension,
    Resume,
    /// Cancellation before an asynchronous body begins execution.
    EntryCancellation,
    RunCancellation,
    ScopeExit,
    Yield,
    Recovery,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(crate) enum AnalysisRefinement {
    Condition {
        expression: BoundExpressionId,
        value: bool,
    },
    PropagationOutcome {
        expression: BoundExpressionId,
        succeeded: bool,
    },
    PatternOutcome {
        subject: BoundExpressionId,
        pattern: BoundPatternId,
        value: bool,
    },
    TrustBoundary(BoundExpressionId),
    PreconditionFailure(bray_bound_tree::BoundExecutionSite),
    InvocationFailure(bray_bound_tree::BoundExecutionSite),
    InvocationCancellation(bray_bound_tree::BoundExecutionSite),
    InvocationCompletion(bray_bound_tree::BoundExecutionSite),
    InvocationPresence(bray_bound_tree::BoundExecutionSite, bool),
    InvocationBoolean(bray_bound_tree::BoundExecutionSite, bool),
    CleanupRequired {
        scope: bray_bound_tree::BoundBlockId,
        exit: AnyBoundNodeId,
    },
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

    pub(crate) const fn is_dependency_failure(self) -> bool {
        matches!(
            self.refinement,
            Some(
                AnalysisRefinement::InvocationFailure(_)
                    | AnalysisRefinement::InvocationCancellation(_)
                    | AnalysisRefinement::CleanupRequired { .. }
            )
        )
    }

    pub(crate) const fn refinement(self) -> Option<AnalysisRefinement> {
        self.refinement
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct AnalysisBlock {
    id: AnalysisBlockId,
    operations: Box<[AnalysisOperationId]>,
    predecessors: Box<[AnalysisEdgeId]>,
    successors: Box<[AnalysisEdgeId]>,
    report_boundary: Option<BoundExpressionId>,
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
            report_boundary: None,
        }
    }

    pub(crate) fn with_report_boundary(mut self, boundary: Option<BoundExpressionId>) -> Self {
        self.report_boundary = boundary;

        self
    }

    pub(crate) const fn report_boundary(&self) -> Option<BoundExpressionId> {
        self.report_boundary
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

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct ControlFlowGraph {
    unit: BoundUnitId,
    entry: AnalysisBlockId,
    blocks: Box<[AnalysisBlock]>,
    edges: Box<[AnalysisEdge]>,
    operations: Box<[AnalysisOperation]>,
    exits: Box<[AnalysisExit]>,
    component_order: Box<[usize]>,
    report_parents: std::collections::BTreeMap<BoundExpressionId, Option<BoundExpressionId>>,
}

impl ControlFlowGraph {
    pub(crate) fn select_cleanup_suspensions(
        &mut self,
        scopes: &std::collections::BTreeSet<bray_bound_tree::BoundBlockId>,
        replacements: &std::collections::BTreeSet<BoundExpressionId>,
        inputs: &std::collections::BTreeMap<
            BoundExpressionId,
            std::collections::BTreeSet<bray_bound_tree::BodyBehaviorCall>,
        >,
    ) {
        for operation in &mut self.operations {
            operation.cleanup_suspension = match operation.kind {
                AnalysisOperationKind::ScopeExit {
                    block,
                    phase: AnalysisScopeExitPhase::LifecycleResolution,
                    ..
                } => scopes.contains(&block),
                AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression)) => {
                    replacements.contains(&expression) || inputs.contains_key(&expression)
                }
                AnalysisOperationKind::Call {
                    invocation,
                    phase: AnalysisCallPhase::Attempt,
                } => invocation
                    .expression()
                    .is_some_and(|expression| inputs.contains_key(&expression)),
                AnalysisOperationKind::Call {
                    invocation,
                    phase: AnalysisCallPhase::Completion,
                } => invocation
                    .expression()
                    .is_some_and(|expression| replacements.contains(&expression)),
                _ => false,
            };
        }
    }

    pub(crate) fn new(
        unit: BoundUnitId,
        entry: AnalysisBlockId,
        blocks: impl Into<Box<[AnalysisBlock]>>,
        edges: impl Into<Box<[AnalysisEdge]>>,
        operations: impl Into<Box<[AnalysisOperation]>>,
        exits: impl Into<Box<[AnalysisExit]>>,
    ) -> Self {
        let mut graph = Self {
            unit,
            entry,
            blocks: blocks.into(),
            edges: edges.into(),
            operations: operations.into(),
            exits: exits.into(),
            component_order: Box::new([]),
            report_parents: std::collections::BTreeMap::new(),
        };

        assert!(
            graph.is_well_formed(),
            "control-flow graph construction violated its invariants for {unit:?}"
        );

        let components = bray_base::strongly_connected_components(
            graph.blocks.iter().map(|block| block.id()),
            |id| {
                graph
                    .block(id)
                    .expect("graph member exists")
                    .successors()
                    .iter()
                    .map(|edge| graph.edge(*edge).expect("successor exists").target())
                    .collect::<Vec<_>>()
            },
        );

        let mut order = vec![0; graph.blocks.len()];

        for (rank, component) in components.into_iter().enumerate() {
            for block in component {
                order[block.to_index().expect("component block has an index")] = rank;
            }
        }

        graph.component_order = order.into();

        graph
    }

    pub(super) fn with_report_parents(
        mut self,
        parents: std::collections::BTreeMap<BoundExpressionId, Option<BoundExpressionId>>,
    ) -> Self {
        self.report_parents = parents;

        self
    }

    pub(crate) fn report_parent(&self, boundary: BoundExpressionId) -> Option<BoundExpressionId> {
        self.report_parents[&boundary]
    }

    pub(crate) fn exited_report_boundaries(
        &self,
        edge: &AnalysisEdge,
    ) -> impl Iterator<Item = BoundExpressionId> + '_ {
        let source = self
            .block(edge.source())
            .expect("report source belongs to its graph")
            .report_boundary();

        let target = self
            .block(edge.target())
            .expect("report destination belongs to its graph")
            .report_boundary();

        std::iter::successors(source, |boundary| self.report_parent(*boundary)).take_while(
            move |boundary| {
                !std::iter::successors(target, |parent| self.report_parent(*parent))
                    .any(|parent| parent == *boundary)
            },
        )
    }

    /// Components precede every successor outside their own cycle.
    pub(crate) fn component_order(&self, block: AnalysisBlockId) -> usize {
        self.component_order[block.to_index().expect("graph block has an index")]
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

    /// Exits that publish a normal result, including the error variant of a Result.
    pub(crate) fn completion_exits(&self) -> impl Iterator<Item = &AnalysisExit> {
        self.exits.iter().filter(|exit| {
            matches!(
                exit.kind(),
                AnalysisExitKind::Return
                    | AnalysisExitKind::NormalFallthrough
                    | AnalysisExitKind::ResultErrorPropagation
            )
        })
    }

    /// Exits that transfer the run's panic or cancellation outcome.
    pub(crate) fn abnormal_exits(&self) -> impl Iterator<Item = &AnalysisExit> {
        self.exits.iter().filter(|exit| {
            matches!(
                exit.kind(),
                AnalysisExitKind::Panic | AnalysisExitKind::Cancellation
            )
        })
    }

    /// Blocks that can lead to a normal function completion.
    pub(crate) fn completion_predecessors(&self) -> std::collections::BTreeSet<AnalysisBlockId> {
        let mut pending = self
            .completion_exits()
            .map(|exit| exit.block())
            .collect::<Vec<_>>();

        let mut blocks = std::collections::BTreeSet::new();

        while let Some(block) = pending.pop() {
            if blocks.insert(block) {
                pending.extend(
                    self.block(block)
                        .expect("completion path belongs to graph")
                        .predecessors()
                        .iter()
                        .map(|edge| {
                            self.edge(*edge)
                                .expect("predecessor belongs to graph")
                                .source()
                        }),
                );
            }
        }

        blocks
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
            Some(
                AnalysisRefinement::InvocationFailure(invocation)
                | AnalysisRefinement::InvocationCancellation(invocation)
                | AnalysisRefinement::PreconditionFailure(invocation)
                | AnalysisRefinement::InvocationCompletion(invocation)
                | AnalysisRefinement::InvocationPresence(invocation, _)
                | AnalysisRefinement::InvocationBoolean(invocation, _),
            ) => invocation.node().unit() == self.unit,
            Some(AnalysisRefinement::CleanupRequired { scope, exit }) => {
                scope.unit() == self.unit && exit.unit() == self.unit
            }
            Some(
                AnalysisRefinement::Condition { expression, .. }
                | AnalysisRefinement::PropagationOutcome { expression, .. }
                | AnalysisRefinement::TrustBoundary(expression),
            ) => expression.unit() == self.unit,
            Some(AnalysisRefinement::PatternOutcome {
                subject, pattern, ..
            }) => subject.unit() == self.unit && pattern.unit() == self.unit,
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
