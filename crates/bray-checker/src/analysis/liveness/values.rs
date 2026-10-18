use std::collections::{BTreeMap, BTreeSet};

use super::domain::{SetLiveness, SetTransfer};
use crate::analysis::fixed_point::{FixedPointDomain, FixedPointOutcome, solve_fixed_point};
use crate::analysis::id::AnalysisBlockId;
use crate::analysis::model::{
    AnalysisCallPhase, AnalysisOperationKind, AnalysisRefinement, ControlFlowGraph,
};
use bray_bound_tree::{AnyBoundNodeId, BoundExpressionId, BoundUnit, StoragePlan};

/// Evaluated values have compiler-cache lifetimes, independently of storage ownership.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct ValueRetention {
    incoming: BTreeMap<AnalysisBlockId, BTreeSet<BoundExpressionId>>,
    repeatable: BTreeSet<BoundExpressionId>,
}

impl ValueRetention {
    pub(crate) fn prepare(
        graph: &ControlFlowGraph,
        unit: &BoundUnit,
        storage: &StoragePlan,
        cancellation: &dyn bray_base::Cancellation,
    ) -> Option<Self> {
        let transfers = transfers(graph, unit, storage);

        let domain = SetLiveness::new(
            None,
            transfers.into(),
            unit.tree().expressions().map(|(id, _)| id).collect(),
        );

        let solved = match solve_fixed_point(graph, &domain, cancellation) {
            FixedPointOutcome::Complete(result) => result,
            FixedPointOutcome::Cancelled => return None,
            FixedPointOutcome::ConvergenceInvariantViolated => {
                panic!("finite expression liveness exceeded its bound")
            }
        };

        let incoming = graph
            .blocks()
            .iter()
            .map(|block| {
                let state = solved
                    .state(block.id())
                    .expect("solved block has a live output set");

                (
                    block.id(),
                    FixedPointDomain::transfer(&domain, block, state),
                )
            })
            .collect();

        let mut repeatable = BTreeSet::new();

        for block in graph.blocks() {
            if block
                .successors()
                .iter()
                .map(|edge| {
                    graph
                        .edge(*edge)
                        .expect("value liveness successor belongs to its graph")
                })
                .any(|edge| {
                    graph.component_order(edge.target()) == graph.component_order(block.id())
                })
            {
                repeatable.extend(
                    block
                        .operations()
                        .iter()
                        .map(|id| {
                            graph
                                .operation(*id)
                                .expect("value liveness block retains its operation")
                        })
                        .filter_map(|operation| operation.kind().occurrence().expression()),
                );
            }
        }

        Some(Self {
            incoming,
            repeatable,
        })
    }

    pub(crate) fn incoming(&self, block: AnalysisBlockId) -> &BTreeSet<BoundExpressionId> {
        &self.incoming[&block]
    }

    pub(crate) fn can_repeat(&self, expression: BoundExpressionId) -> bool {
        self.repeatable.contains(&expression)
    }
}

fn transfers(
    graph: &ControlFlowGraph,
    unit: &BoundUnit,
    storage: &StoragePlan,
) -> Vec<SetTransfer<BoundExpressionId>> {
    let mut transfers = graph
        .blocks()
        .iter()
        .map(|_| SetTransfer::default())
        .collect::<Vec<_>>();

    for block in graph.blocks() {
        let transfer = &mut transfers[block.id().to_index().expect("flow block has an index")];

        for edge in block.successors().iter().map(|id| {
            graph
                .edge(*id)
                .expect("value liveness successor belongs to its graph")
        }) {
            match edge.refinement() {
                Some(
                    AnalysisRefinement::Condition { expression, .. }
                    | AnalysisRefinement::PropagationOutcome { expression, .. },
                ) => {
                    transfer.generated.insert(expression);
                }
                Some(AnalysisRefinement::PatternOutcome { subject, .. }) => {
                    transfer.generated.insert(subject);
                }
                // These edges select control flow without reading another cached value.
                Some(
                    AnalysisRefinement::TrustBoundary(_)
                    | AnalysisRefinement::InvocationFailure(_)
                    | AnalysisRefinement::InvocationCompletion(_)
                    | AnalysisRefinement::CleanupRequired { .. },
                )
                | None => {}
            }
        }

        for operation in block.operations().iter().rev().map(|id| {
            graph
                .operation(*id)
                .expect("value liveness block retains its operation")
        }) {
            match operation.kind() {
                AnalysisOperationKind::Result { expression, .. } => {
                    transfer.generated.remove(&expression);
                    transfer.killed.insert(expression);
                }
                AnalysisOperationKind::ScopeExit { .. } => {}
                kind => {
                    if let Some(expression) = kind.occurrence().expression() {
                        let node = unit
                            .view()
                            .expression(expression)
                            .expect("value liveness occurrence retains its bound expression");

                        transfer.generated.remove(&expression);
                        transfer.killed.insert(expression);
                        transfer.generated.extend(node.child_expressions());

                        if matches!(
                            kind,
                            AnalysisOperationKind::Call {
                                phase: AnalysisCallPhase::Completion,
                                ..
                            }
                        ) {
                            transfer.generated.insert(expression);
                        }
                    }

                    if let AnyBoundNodeId::Pattern(pattern) = kind.node() {
                        transfer.generated.extend(
                            storage
                                .node_plans(pattern.into())
                                .map(|plan| plan.expression()),
                        );
                    }
                }
            }
        }
    }

    transfers
}
