use bray_bound_tree::{AnyBoundNodeId, BoundUnitId};

use super::id::{AnalysisBlockId, AnalysisEdgeId, AnalysisOperationId, ProgramPointId};
use super::model::{
    AnalysisBlock, AnalysisCallPhase, AnalysisEdge, AnalysisEdgeKind, AnalysisExit,
    AnalysisExitKind, AnalysisOperation, AnalysisOperationKind, AnalysisRefinement,
    AnalysisScopeExitPhase, AnalysisSuspensionKind, ControlFlowGraph,
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
    loop_expressions: std::collections::BTreeMap<AnalysisBlockId, bray_bound_tree::BoundExpressionId>,
    report_boundary: Option<bray_bound_tree::BoundExpressionId>,
    report_parents: std::collections::BTreeMap<
        bray_bound_tree::BoundExpressionId,
        Option<bray_bound_tree::BoundExpressionId>,
    >,
}

#[derive(Clone, Copy)]
pub(super) struct ScopeFailureTargets {
    pub(super) panic: AnalysisBlockId,
    pub(super) cancellation: AnalysisBlockId,
    pub(super) local_cleanup: bool,
}

impl ControlFlowGraphAssembler {
    pub(super) const fn new(unit: BoundUnitId) -> Self {
        Self {
            unit,
            blocks: Vec::new(),
            edges: Vec::new(),
            operations: Vec::new(),
            exits: Vec::new(),
            loop_expressions: std::collections::BTreeMap::new(),
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

    pub(super) fn push_loop_header(&mut self, expression: bray_bound_tree::BoundExpressionId) -> AnalysisBlockId {
        let header = self.push_block();

        self.loop_expressions.insert(header, expression);

        header
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

    pub(super) fn push_scope_broadcast(
        &mut self,
        current: AnalysisBlockId,
        scope: bray_bound_tree::StorageScope,
        exit: AnyBoundNodeId,
        preserves_exit_value: bool,
    ) -> AnalysisBlockId {
        let broadcast = self.push_block();

        self.push_edge(current, broadcast, AnalysisEdgeKind::ScopeExit, None);

        self.push_operation(
            broadcast,
            AnalysisOperationKind::ScopeExit {
                scope,
                exit,
                phase: AnalysisScopeExitPhase::TaskCancellationBroadcast,
                preserves_exit_value,
            },
        );

        broadcast
    }

    pub(super) fn push_scope_lifecycle(
        &mut self,
        current: AnalysisBlockId,
        scope: bray_bound_tree::StorageScope,
        exit: AnyBoundNodeId,
        preserves_exit_value: bool,
        scoped: Option<(
            bray_bound_tree::BoundExpressionId,
            bray_bound_tree::BoundCallResult,
            Option<[super::model::ScopedResultTransfer; 2]>,
            bool,
        )>,
        failures: Option<ScopeFailureTargets>,
    ) -> AnalysisBlockId {
        let entered = self.push_block();

        self.push_edge(current, entered, AnalysisEdgeKind::Sequential, None);

        let lifecycle = if let Some((expression, result, transfers, checks_requirements)) = scoped {
            let invocation = bray_bound_tree::BoundExecutionSite::ScopedExit(expression);
            let attempt = self.push_block();

            self.push_call(entered, invocation, AnalysisCallPhase::Input);
            self.mark_scoped_exit(scope, exit, preserves_exit_value);

            self.push_edge(entered, attempt, AnalysisEdgeKind::Sequential,
                Some(AnalysisRefinement::InvocationEntry { invocation, admitted: true }));

            self.push_call(attempt, invocation, AnalysisCallPhase::Attempt);
            self.mark_scoped_exit(scope, exit, preserves_exit_value);

            let completed = self.push_block();

            let (preceding, edge) = if matches!(result, bray_bound_tree::BoundCallResult::LazyFuture(_)) {
                self.push_suspension(attempt, invocation, AnalysisSuspensionKind::ScopedCall);

                let suspended = self.push_block();

                self.push_edge(attempt, suspended, AnalysisEdgeKind::Suspension, None);

                // Cleanup is shielded, so cancellation waits until exit completes.
                (suspended, AnalysisEdgeKind::Resume)
            } else {
                (attempt, AnalysisEdgeKind::Sequential)
            };

            self.push_edge(
                preceding,
                completed,
                edge,
                Some(AnalysisRefinement::InvocationCompletion(invocation)),
            );

            self.push_call(completed, invocation, AnalysisCallPhase::Completion);
            self.mark_scoped_exit(scope, exit, preserves_exit_value);

            let lifecycle = self.push_block();

            if let Some([success, error]) = transfers {
                self.push_scoped_exit_results(completed, lifecycle, exit, [success, error], failures);
            } else {
                self.push_edge(completed, lifecycle, AnalysisEdgeKind::Sequential, None);
            }

            if let Some((panic, cancellation)) = failures
                .map(|targets| (targets.panic, targets.cancellation))
                .or_else(|| (!preserves_exit_value).then_some((lifecycle, lifecycle)))
            {
                // Only a runtime precondition on the selected exit can fail its admission.
                if checks_requirements {
                    self.push_edge(
                        entered,
                        panic,
                        AnalysisEdgeKind::Sequential,
                        Some(AnalysisRefinement::InvocationEntry { invocation, admitted: false }),
                    );
                }

                for (kind, continuation) in bray_symbols::DependencyFailureKind::ALL.into_iter().zip([panic, cancellation]) {
                    self.push_edge(
                        attempt,
                        continuation,
                        AnalysisEdgeKind::Sequential,
                        Some(AnalysisRefinement::InvocationFailure(invocation, kind)),
                    );
                }
            }

            lifecycle
        } else {
            entered
        };

        self.push_operation(
            lifecycle,
            AnalysisOperationKind::ScopeExit {
                scope,
                exit,
                phase: AnalysisScopeExitPhase::LifecycleResolution,
                preserves_exit_value,
            },
        );

        let Some(failures) = failures.filter(|targets| targets.local_cleanup) else {
            return lifecycle;
        };

        for (kind, target) in bray_symbols::DependencyFailureKind::ALL.into_iter().zip([failures.panic, failures.cancellation]) {
            self.push_edge(lifecycle, target, AnalysisEdgeKind::Sequential,
                Some(AnalysisRefinement::CleanupOutcome { scope, exit, failure: Some(kind) }));
        }

        let resolved = self.push_block();

        self.push_edge(lifecycle, resolved, AnalysisEdgeKind::Sequential,
            Some(AnalysisRefinement::CleanupOutcome { scope, exit, failure: None }));

        resolved
    }

    fn mark_scoped_exit(&mut self, scope: bray_bound_tree::StorageScope, exit: AnyBoundNodeId, preserves_exit_value: bool) {
        let operation = self.operations.last_mut().expect("scoped call phase was just emitted");

        *operation = operation.with_scoped_exit(bray_bound_tree::StorageExitPoint::new(scope, exit), preserves_exit_value);
    }

    fn push_scoped_exit_results(
        &mut self,
        completed: AnalysisBlockId,
        lifecycle: AnalysisBlockId,
        exit: AnyBoundNodeId,
        transfers: [super::model::ScopedResultTransfer; 2],
        failures: Option<ScopeFailureTargets>,
    ) {
        for (transfer, kind) in transfers.into_iter().zip([AnalysisEdgeKind::ResultSuccess, AnalysisEdgeKind::ResultErrorPropagation]) {
            let selected = self.push_block();

            self.push_edge(completed, selected, kind,
                Some(AnalysisRefinement::InvocationVariant(transfer.invocation, transfer.selected_variant())));

            self.push_operation(selected, AnalysisOperationKind::ScopedResult(transfer));

            let after = if matches!(transfer.destination, super::model::ScopedResultDestination::Region { displaced: Some(_), .. }) {
                let expression = transfer.invocation.expression().expect("scoped replacement retains its with expression");
                let scope = bray_bound_tree::StorageScope::ScopedResultReplacement(expression);
                let broadcast = self.push_scope_broadcast(selected, scope, exit, true);

                let failures = failures.map(|outer| {
                    let [panic, cancellation] = [outer.panic, outer.cancellation].map(|target| {
                        let entry = self.push_block();
                        let resolved = self.push_scope_lifecycle(entry, scope, exit, false, None, None);

                        self.push_edge(resolved, target, AnalysisEdgeKind::Sequential, None);

                        entry
                    });

                    ScopeFailureTargets { panic, cancellation, local_cleanup: true }
                });

                self.push_scope_lifecycle(broadcast, scope, exit, true, None, failures)
            } else {
                selected
            };

            self.push_edge(after, lifecycle, AnalysisEdgeKind::Sequential, None);
        }
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

    pub(super) fn push_exit(&mut self, block: AnalysisBlockId, kind: AnalysisExitKind, node: AnyBoundNodeId) {
        let exit = self.push_block();
        let edge = exit_edge_kind(kind);

        self.push_edge(block, exit, edge, None);
        self.exits.push(AnalysisExit::new(exit, kind, node));
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
            self.loop_expressions,
        )
        .with_report_parents(self.report_parents)
    }

    pub(super) fn push_operation(&mut self, block: AnalysisBlockId, kind: AnalysisOperationKind) {
        let Ok(slot) = u32::try_from(self.operations.len()) else {
            return;
        };

        let id = AnalysisOperationId::new(self.unit, slot);

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
    fn scoped_exit_runs_after_cancellation_broadcast_and_before_local_destruction() {
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

                let broadcast = assembler.push_scope_broadcast(
                    entry,
                    scope.into(),
                    expression.into(),
                    preserves_exit_value,
                );

                let end = assembler.push_scope_lifecycle(
                    broadcast,
                    scope.into(),
                    expression.into(),
                    preserves_exit_value,
                    Some((expression, result, None, true)),
                    None,
                );

                assembler.push_exit(end, AnalysisExitKind::Return, expression.into());

                let graph = assembler.finish(entry);

                for failed in [false, true]
                    .into_iter()
                    .filter(|failed| !*failed || !preserves_exit_value)
                {
                    let mut cursor = graph.entry();
                    let mut events = Vec::new();

                    loop {
                        let block = graph.block(cursor).expect("fixture MIR block must exist");

                        for operation in block.operations() {
                            match graph
                                .operation(*operation)
                                .expect("fixture MIR operation must exist")
                                .kind()
                            {
                                AnalysisOperationKind::Call { phase, .. } => {
                                    if phase == AnalysisCallPhase::Input {
                                        continue;
                                    }

                                    events.push(match phase {
                                        AnalysisCallPhase::Input => unreachable!(),
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
                            .map(|id| graph.edge(*id).expect("fixture `edge` call must succeed"))
                            .collect::<Vec<_>>();

                        let failure = successors.iter().find(|edge| {
                            matches!(
                                edge.refinement(),
                                Some(
                                    crate::analysis::model::AnalysisRefinement::InvocationFailure(..)
                                )
                            )
                        });

                        let admission_failure =
                            successors.iter().any(|edge| {
                                matches!(
                            edge.refinement(),
                            Some(crate::analysis::model::AnalysisRefinement::InvocationEntry { admitted: false, .. })
                        )
                            });

                        if failure.is_some() {
                            assert!(!preserves_exit_value);
                            assert!(!admission_failure);
                            assert_eq!(successors.len(), 3);
                        } else if admission_failure {
                            assert_eq!(successors.len(), 2);
                        } else {
                            assert!(successors.len() <= 1);
                        }

                        let Some(edge) = failure.filter(|_| failed).copied().or_else(|| successors.iter().copied().find(|edge| !matches!(
                            edge.refinement(),
                            Some(crate::analysis::model::AnalysisRefinement::InvocationFailure(..)
                                | crate::analysis::model::AnalysisRefinement::InvocationEntry { admitted: false, .. })
                        ))) else { break; };

                        cursor = edge.target();
                    }

                    let mut expected = if matches!(result, BoundCallResult::LazyFuture(_)) {
                        vec![
                            "cancel tasks",
                            "exit attempt",
                            "await exit",
                            "exit complete",
                            "destroy locals",
                        ]
                    } else {
                        vec![
                            "cancel tasks",
                            "exit attempt",
                            "exit complete",
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
