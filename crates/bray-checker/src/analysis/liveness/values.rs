use std::collections::{BTreeMap, BTreeSet};

use super::domain::{SetLiveness, SetTransfer};
use crate::analysis::fixed_point::{FixedPointDomain, FixedPointOutcome, solve_fixed_point};
use crate::analysis::id::AnalysisBlockId;
use crate::analysis::model::{
    AnalysisCallPhase, AnalysisOperationKind, AnalysisRefinement, ControlFlowGraph,
};
use bray_bound_tree::{AnyBoundNodeId, BoundExpressionId, BoundUnit, CheckedExpressionSemantics, StoragePlan};

/// Evaluated values have compiler-cache lifetimes, independently of storage ownership.
#[derive(Clone, Debug, Default, Eq, Hash, PartialEq)]
pub(crate) struct ValueRetention {
    incoming: BTreeMap<AnalysisBlockId, BTreeSet<BoundExpressionId>>,
    repeatable: BTreeSet<bray_bound_tree::BoundExecutionSite>,
}

impl ValueRetention {
    pub(crate) fn prepare(
        graph: &ControlFlowGraph,
        unit: &BoundUnit,
        storage: &StoragePlan,
        expressions: &CheckedExpressionSemantics,
        cancellation: &dyn bray_base::Cancellation,
    ) -> Option<Self> {
        let transfers = transfers(graph, unit, storage, expressions);

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
                        .map(|operation| operation.kind().occurrence()),
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

    pub(crate) fn can_repeat(&self, occurrence: bray_bound_tree::BoundExecutionSite) -> bool {
        self.repeatable.contains(&occurrence)
    }
}

fn transfers(
    graph: &ControlFlowGraph,
    unit: &BoundUnit,
    storage: &StoragePlan,
    expressions: &CheckedExpressionSemantics,
) -> Vec<SetTransfer<BoundExpressionId>> {
    let mut transfers = graph
        .blocks()
        .iter()
        .map(|_| SetTransfer::default())
        .collect::<Vec<_>>();

    for block in graph.blocks() {
        let transfer = &mut transfers[block.id().to_index().expect("flow block has an index")];

        if let bray_bound_tree::BoundUnitRoot::Expression(root) = unit.root()
            && graph.exits().iter().any(|exit| {
                exit.block() == block.id()
                    && exit.kind() == crate::analysis::model::AnalysisExitKind::NormalFallthrough
            })
        {
            // Expression units publish this value and its witnesses at normal completion.
            transfer.generated.insert(root);
        }

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
                Some(AnalysisRefinement::CaughtResult {
                    expression,
                    succeeded,
                }) => {
                    if succeeded {
                        transfer.generated.extend(
                            unit.view()
                                .expression(expression)
                                .expect("catch edge retains its expression")
                                .child_expressions(),
                        );
                    }
                }
                // Invocation operands remain live through the sibling admission continuation.
                Some(AnalysisRefinement::PreconditionFailure(_)) => {}
                // These edges select control flow without reading another cached value.
                Some(
                    AnalysisRefinement::TrustBoundary(_)
                    | AnalysisRefinement::GeneratorStorageFailure(_)
                    | AnalysisRefinement::InvocationFailure(_)
                    | AnalysisRefinement::InvocationCancellation(_)
                    | AnalysisRefinement::InvocationCompletion(_)
                    | AnalysisRefinement::InvocationPresence(_, _)
                    | AnalysisRefinement::InvocationBoolean(_, _)
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
                AnalysisOperationKind::Result {
                    expression,
                    phase: crate::analysis::model::AnalysisResultPhase::Begin
                        | crate::analysis::model::AnalysisResultPhase::Completion,
                } => {
                    transfer.generated.remove(&expression);
                    transfer.killed.insert(expression);
                }
                AnalysisOperationKind::ScopeExit { .. } => {}
                AnalysisOperationKind::Call {
                    invocation:
                        occurrence @ (bray_bound_tree::BoundExecutionSite::CallInput { .. }
                        | bray_bound_tree::BoundExecutionSite::Iterate(_)
                        | bray_bound_tree::BoundExecutionSite::Next(_)
                        | bray_bound_tree::BoundExecutionSite::ScopedEnter(_)
                        | bray_bound_tree::BoundExecutionSite::ScopedExit(_)),
                    ..
                } => {
                    transfer.generated.extend(
                        storage
                            .occurrence_plans(occurrence)
                            .filter(|plan| {
                                plan.purpose() != bray_bound_tree::StorageAccessPurpose::Initialize
                            })
                            .map(|plan| plan.expression())
                            .filter(|expression| Some(*expression) != occurrence.expression()),
                    );
                }
                kind => {
                    if let Some(expression) = kind.occurrence().expression() {
                        let node = unit
                            .view()
                            .expression(expression)
                            .expect("value liveness occurrence retains its bound expression");

                        transfer.generated.remove(&expression);
                        transfer.killed.insert(expression);
                        transfer.generated.extend(node.child_expressions());

                        if let Some(selection) = expressions.selections().expression(expression) {
                            // Selected receivers can be nested under the callee syntax.
                            // Preserve actual runtime inputs until entry captures them.
                            transfer.generated.extend(selection.inputs().filter_map(|input| input.expression()));
                        }

                        if let bray_bound_tree::BoundExpression::Structured(index) = node
                            && matches!(
                                index.kind(),
                                bray_bound_tree::BoundStructuredExpressionKind::ElementIndex
                                    | bray_bound_tree::BoundStructuredExpressionKind::SliceIndex
                            )
                        {
                            let mut receiver = index.operands().first().copied();

                            while let Some(bray_bound_tree::BoundExpression::Structured(slice)) =
                                receiver.and_then(|receiver| unit.view().expression(receiver))
                                && slice.kind()
                                    == bray_bound_tree::BoundStructuredExpressionKind::SliceIndex
                            {
                                // Bounds proofs observe the coordinates captured by the inner slice.
                                transfer.generated.extend(slice.operands());
                                receiver = slice.operands().first().copied();
                            }
                        }

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

#[cfg(test)]
mod tests {
    use super::ValueRetention;
    use crate::analysis::assembly::ControlFlowGraphAssembler;
    use crate::analysis::model::{AnalysisEdgeKind, AnalysisExitKind, AnalysisOperationKind, AnalysisRefinement, AnalysisResultPhase};
    use crate::test_support::{error_type, expression_unit, push_expression, semantic_values};
    use bray_bound_tree::{BoundBinaryExpression, BoundErrorExpression, BoundExpression, BoundOperator, BoundUnitId, CheckedExpressionSemantics, CheckedExpressionTypes, CheckedLiteralValues, CheckedSemanticSelections, StoragePlanBuilder};

    #[test]
    fn private_production_retains_inputs_until_their_value_is_captured() {
        let unit = BoundUnitId::new(23);
        let (bound, values) = expression_unit(unit, |tree, origin| {
            let left = push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type())));
            let right = push_expression(tree, BoundExpression::Error(BoundErrorExpression::new(origin, error_type())));
            let result = push_expression(tree, BoundExpression::Binary(BoundBinaryExpression::new(
                origin, BoundOperator::LogicalAnd, [left, right], Some(error_type()), false,
            )));
            vec![left, right, result]
        });
        let [left, right, result] = values.as_slice() else { panic!("fixture has two operands and a result"); };
        let types = CheckedExpressionTypes::new(unit, bound.key().kind(), []);
        let selections = CheckedSemanticSelections::try_new(&bound, &types, []).expect("empty fixture selections");
        let literals = CheckedLiteralValues::try_new(&bound, &types, semantic_values(),
            std::num::NonZeroU16::new(64).expect("fixture width is nonzero"), []).expect("fixture has no literals");
        let semantics = CheckedExpressionSemantics::try_new(types, selections, literals).expect("one fixture unit");
        let storage = StoragePlanBuilder::new(unit, bound.key().kind()).finish();
        let mut graph = ControlFlowGraphAssembler::new(unit);
        let entry = graph.push_block();
        let selected = graph.push_block();
        let publication = graph.push_block();

        graph.push_bound(entry, (*left).into());
        graph.push_edge(entry, selected, AnalysisEdgeKind::ConditionalTrue,
            Some(AnalysisRefinement::Condition { expression: *left, value: true }));
        graph.push_bound(selected, (*right).into());
        graph.push_operation(selected, AnalysisOperationKind::Result { expression: *result, phase: AnalysisResultPhase::Production });
        graph.push_edge(selected, publication, AnalysisEdgeKind::Sequential, None);
        graph.push_operation(publication, AnalysisOperationKind::Result { expression: *result, phase: AnalysisResultPhase::Completion });
        graph.push_exit(publication, AnalysisExitKind::NormalFallthrough, (*result).into());
        let graph = graph.finish(entry);
        let retention = ValueRetention::prepare(&graph, &bound, &storage, &semantics, &|| false)
            .expect("finite fixture retention converges");

        assert!(retention.incoming(selected).contains(left), "the selected producer reads the captured left value");
        assert!(!retention.incoming(publication).contains(left), "the pending result carries its inputs after production");
    }
}
