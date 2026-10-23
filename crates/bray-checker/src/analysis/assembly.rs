use bray_bound_tree::{AnyBoundNodeId, BoundUnitId};

use super::id::{AnalysisBlockId, AnalysisEdgeId, AnalysisOperationId, ProgramPointId};
use super::model::{
    AnalysisBlock, AnalysisCallPhase, AnalysisEdge, AnalysisEdgeKind, AnalysisExit,
    AnalysisExitKind, AnalysisOperation, AnalysisOperationKind, AnalysisRefinement,
    AnalysisScopeExitPhase, AnalysisSuspensionKind, AnalysisTaskOperationKind, ControlFlowGraph,
};

pub(super) struct ControlFlowGraphAssembler {
    unit: BoundUnitId,
    blocks: Vec<(
        Option<bray_bound_tree::BoundExpressionId>,
        Vec<AnalysisOperationId>,
    )>,
    edges: Vec<AnalysisEdge>,
    operations: Vec<AnalysisOperation>,
    exits: Vec<AnalysisExit>,
    report_boundary: Option<bray_bound_tree::BoundExpressionId>,
    report_parents: std::collections::BTreeMap<
        bray_bound_tree::BoundExpressionId,
        Option<bray_bound_tree::BoundExpressionId>,
    >,
}

impl ControlFlowGraphAssembler {
    pub(super) const fn new(unit: BoundUnitId) -> Self {
        Self {
            unit,
            blocks: Vec::new(),
            edges: Vec::new(),
            operations: Vec::new(),
            exits: Vec::new(),
            report_boundary: None,
            report_parents: std::collections::BTreeMap::new(),
        }
    }

    pub(super) fn push_block(&mut self) -> AnalysisBlockId {
        let Some(id) = AnalysisBlockId::try_from_index(self.unit, self.blocks.len()) else {
            return AnalysisBlockId::from_slot(self.unit, u32::MAX);
        };

        self.blocks.push((self.report_boundary, Vec::new()));

        id
    }

    pub(super) fn replace_report_boundary(
        &mut self,
        boundary: Option<bray_bound_tree::BoundExpressionId>,
    ) -> Option<bray_bound_tree::BoundExpressionId> {
        let previous = std::mem::replace(&mut self.report_boundary, boundary);

        if let Some(boundary) = boundary {
            self.report_parents.entry(boundary).or_insert(previous);
        }

        previous
    }

    pub(super) fn push_bound(&mut self, block: AnalysisBlockId, node: AnyBoundNodeId) {
        self.push_operation(block, AnalysisOperationKind::Bound(node));
    }

    pub(super) fn push_call(
        &mut self,
        block: AnalysisBlockId,
        invocation: bray_bound_tree::BoundExecutionSite,
        phase: AnalysisCallPhase,
    ) {
        self.push_operation(block, AnalysisOperationKind::Call { invocation, phase });
    }

    pub(super) fn push_recovery(&mut self, block: AnalysisBlockId, node: AnyBoundNodeId) {
        self.push_operation(block, AnalysisOperationKind::Recovery(node));
    }

    pub(super) fn push_suspension(
        &mut self,
        block: AnalysisBlockId,
        occurrence: bray_bound_tree::BoundExecutionSite,
        kind: AnalysisSuspensionKind,
    ) {
        self.push_operation(
            block,
            AnalysisOperationKind::Suspension { occurrence, kind },
        );
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
        exit: bray_bound_tree::AnyBoundNodeId,
        preserves_exit_value: bool,
        scoped: Option<(
            bray_bound_tree::BoundExpressionId,
            bray_bound_tree::BoundCallResult,
        )>,
    ) -> AnalysisBlockId {
        let entered = self.push_block();

        self.push_edge(current, entered, AnalysisEdgeKind::ScopeExit, None);

        let completed = if let Some((expression, result)) = scoped {
            self.push_call(
                entered,
                bray_bound_tree::BoundExecutionSite::ScopedExit(expression),
                AnalysisCallPhase::Attempt,
            );

            let completed = self.push_block();

            if matches!(result, bray_bound_tree::BoundCallResult::LazyFuture(_)) {
                self.push_suspension(
                    entered,
                    bray_bound_tree::BoundExecutionSite::ScopedExit(expression),
                    AnalysisSuspensionKind::ScopedCall,
                );

                let suspended = self.push_block();

                self.push_edge(entered, suspended, AnalysisEdgeKind::Suspension, None);

                // Cleanup is shielded, so cancellation waits until exit completes.
                self.push_edge(
                    suspended,
                    completed,
                    AnalysisEdgeKind::Resume,
                    Some(super::model::AnalysisRefinement::InvocationCompletion(
                        bray_bound_tree::BoundExecutionSite::ScopedExit(expression),
                    )),
                );
            } else {
                self.push_edge(
                    entered,
                    completed,
                    AnalysisEdgeKind::Sequential,
                    Some(super::model::AnalysisRefinement::InvocationCompletion(
                        bray_bound_tree::BoundExecutionSite::ScopedExit(expression),
                    )),
                );
            }

            self.push_call(
                completed,
                bray_bound_tree::BoundExecutionSite::ScopedExit(expression),
                AnalysisCallPhase::Completion,
            );

            let operation = self
                .operations
                .last_mut()
                .expect("scoped completion was just emitted");

            *operation = operation.with_scoped_exit(
                bray_bound_tree::StorageExitPoint::new(block, exit),
                preserves_exit_value,
            );

            completed
        } else {
            entered
        };

        let cancellation = if let Some((expression, _)) = scoped {
            let cancellation = self.push_block();

            self.push_edge(completed, cancellation, AnalysisEdgeKind::Sequential, None);

            if !preserves_exit_value {
                // A failing exit still resolves the remaining cleanup, without its normal guarantees.
                self.push_edge(
                    entered,
                    cancellation,
                    AnalysisEdgeKind::Sequential,
                    Some(AnalysisRefinement::InvocationFailure(
                        bray_bound_tree::BoundExecutionSite::ScopedExit(expression),
                    )),
                );
            }

            cancellation
        } else {
            entered
        };

        self.push_operation(
            cancellation,
            AnalysisOperationKind::ScopeExit {
                block,
                exit,
                phase: AnalysisScopeExitPhase::TaskCancellationBroadcast,
                preserves_exit_value,
            },
        );

        let lifecycle = self.push_block();

        self.push_edge(cancellation, lifecycle, AnalysisEdgeKind::Sequential, None);

        self.push_operation(
            lifecycle,
            AnalysisOperationKind::ScopeExit {
                block,
                exit,
                phase: AnalysisScopeExitPhase::LifecycleResolution,
                preserves_exit_value,
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
            .filter_map(|(index, (report_boundary, operations))| {
                let id = AnalysisBlockId::try_from_index(self.unit, index)?;

                Some(
                    AnalysisBlock::new(
                        id,
                        operations,
                        std::mem::take(&mut predecessors[index]),
                        std::mem::take(&mut successors[index]),
                    )
                    .with_report_boundary(report_boundary),
                )
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
        .with_report_parents(self.report_parents)
    }

    pub(super) fn push_operation(&mut self, block: AnalysisBlockId, kind: AnalysisOperationKind) {
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

        if let Some((_, operations)) = block
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

#[cfg(test)]
mod tests {
    use super::{
        AnalysisCallPhase, AnalysisExitKind, AnalysisOperationKind, AnalysisScopeExitPhase,
        BoundUnitId, ControlFlowGraphAssembler,
    };
    use crate::test_support::{callable_key, error_type, push_block, push_expression};
    use bray_bound_tree::{
        BoundCallResult, BoundErrorExpression, BoundExpression, BoundFutureConstruction,
        BoundNodeOrigin, BoundTreeBuilder,
    };

    #[test]
    fn scoped_exit_completes_before_body_task_cancellation_and_destruction() {
        let unit = BoundUnitId::new(83);
        let origin = BoundNodeOrigin::source(callable_key().source());
        let mut tree = BoundTreeBuilder::new(unit);

        let expression = push_expression(
            &mut tree,
            BoundExpression::Error(BoundErrorExpression::new(origin, error_type())),
        );

        let scope = push_block(&mut tree, origin, []);

        for result in [
            BoundCallResult::Immediate(error_type()),
            BoundCallResult::LazyFuture(BoundFutureConstruction::new(error_type(), error_type())),
        ] {
            for preserves_exit_value in [false, true] {
                let mut assembler = ControlFlowGraphAssembler::new(unit);
                let entry = assembler.push_block();

                let end = assembler.push_scope_exit(
                    entry,
                    scope,
                    expression.into(),
                    preserves_exit_value,
                    Some((expression, result)),
                );

                assembler.push_exit(end, AnalysisExitKind::Return);

                let graph = assembler.finish(entry);

                for failed in [false, true]
                    .into_iter()
                    .filter(|failed| !*failed || !preserves_exit_value)
                {
                    let mut cursor = graph.entry();
                    let mut events = Vec::new();

                    loop {
                        let block = graph.block(cursor).unwrap();

                        for operation in block.operations() {
                            match graph.operation(*operation).unwrap().kind() {
                                AnalysisOperationKind::Call { phase, .. } => {
                                    events.push(match phase {
                                        AnalysisCallPhase::Attempt => "exit attempt",
                                        AnalysisCallPhase::Completion => "exit complete",
                                    })
                                }
                                AnalysisOperationKind::ScopeExit { phase, .. } => {
                                    events.push(match phase {
                                        AnalysisScopeExitPhase::TaskCancellationBroadcast => {
                                            "cancel tasks"
                                        }
                                        AnalysisScopeExitPhase::LifecycleResolution => {
                                            "destroy locals"
                                        }
                                    })
                                }
                                AnalysisOperationKind::Suspension { .. } => {
                                    events.push("await exit")
                                }
                                operation => panic!("unexpected cleanup operation: {operation:?}"),
                            }
                        }

                        let successors = block
                            .successors()
                            .iter()
                            .map(|id| graph.edge(*id).unwrap())
                            .collect::<Vec<_>>();

                        let failure = successors.iter().find(|edge| {
                            matches!(
                                edge.refinement(),
                                Some(
                                    crate::analysis::model::AnalysisRefinement::InvocationFailure(
                                        _
                                    )
                                )
                            )
                        });

                        if failure.is_some() {
                            assert!(!preserves_exit_value);
                            assert_eq!(successors.len(), 2);
                        } else {
                            assert!(successors.len() <= 1);
                        }

                        let Some(edge) = failure.filter(|_| failed).copied().or_else(|| successors.iter().copied().find(|edge| !matches!(edge.refinement(), Some(crate::analysis::model::AnalysisRefinement::InvocationFailure(_))))) else { break; };

                        cursor = edge.target();
                    }

                    let mut expected = if matches!(result, BoundCallResult::LazyFuture(_)) {
                        vec![
                            "exit attempt",
                            "await exit",
                            "exit complete",
                            "cancel tasks",
                            "destroy locals",
                        ]
                    } else {
                        vec![
                            "exit attempt",
                            "exit complete",
                            "cancel tasks",
                            "destroy locals",
                        ]
                    };

                    if failed {
                        expected.retain(|event| *event != "exit complete");
                    }

                    assert_eq!(events, expected);
                }
            }
        }
    }
}
