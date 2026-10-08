use std::collections::BTreeSet;

use bray_bound_tree::{
    AnyBoundNodeId, CheckedBodySemantics, CheckedExpressionSemantics, CheckedMemoryOperations,
    StoragePlan,
};

use super::super::build::{
    ControlFlowGraphBuildOutcome, build_execution_control_flow_graph,
    build_storage_control_flow_graph,
};
use super::super::model::{AnalysisEdgeKind, AnalysisExitKind, AnalysisOperationKind};
use super::flow::analyze_execution_flow;
use crate::execution_guarantees::{ExecutionCandidate, ExecutionProperty};
use crate::{CheckerOutcome, CheckerRequestContext, CheckerUnitView};

/// Checks body-local operations and records selected dependencies without certifying them.
pub fn check_execution_candidate<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    property: Option<ExecutionProperty>,
    assumptions: &[crate::ExecutionCondition],
    postconditions: &[(crate::ExecutionCondition, bray_source::SourceSpan)],
    contracts: &std::collections::BTreeMap<
        bray_bound_tree::BoundExpressionId,
        Vec<crate::ExecutionCompletionContract>,
    >,
    expressions: &CheckedExpressionSemantics,
    storage: &StoragePlan,
    body: &CheckedBodySemantics,
    memory: &CheckedMemoryOperations,
) -> CheckerOutcome<ExecutionCandidate, C::UpstreamError> {
    assert_execution_inputs(request, expressions, storage, body, memory);

    // Purity alone does not discharge cleanup on a dependency's abnormal completion.
    let graph = match property {
        Some(ExecutionProperty::Pure) => {
            build_storage_control_flow_graph(request, storage, expressions.selections(), None)
        }
        Some(ExecutionProperty::Total) | None => {
            build_execution_control_flow_graph(request, storage, expressions.selections())
        }
    };

    let graph = match graph {
        ControlFlowGraphBuildOutcome::Complete(graph) => graph,
        ControlFlowGraphBuildOutcome::Cancelled => return CheckerOutcome::Cancelled,
        ControlFlowGraphBuildOutcome::InfrastructureFailure(error) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        ControlFlowGraphBuildOutcome::UpstreamFailure(error) => {
            return CheckerOutcome::UpstreamFailure(error);
        }
    };

    let literals =
        crate::execution_guarantees::condition_literals(expressions, request.semantic_values());

    let (reachable, mut diagnostics) = match analyze_execution_flow(
        &graph,
        request,
        expressions,
        assumptions,
        &literals,
        storage,
        contracts,
        Some(body.asynchronous()),
    ) {
        CheckerOutcome::Complete(flow) => flow.into_parts(),
        CheckerOutcome::Cancelled => {
            return CheckerOutcome::Cancelled;
        }
        CheckerOutcome::InfrastructureFailure(error) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        CheckerOutcome::UpstreamFailure(error) => return CheckerOutcome::UpstreamFailure(error),
    };

    macro_rules! checked {
        ($result:expr) => {
            match $result {
                Ok(value) => value,
                Err(crate::CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
                Err(crate::CheckerQueryError::Infrastructure(error)) => {
                    return CheckerOutcome::InfrastructureFailure(error)
                }
                Err(crate::CheckerQueryError::Upstream(error)) => {
                    return CheckerOutcome::UpstreamFailure(error)
                }
            }
        };
    }

    let mut candidate = checked!(reachable.candidate(
        body.asynchronous(),
        property == Some(ExecutionProperty::Total)
    ));

    super::operation::collect_preservation_dependencies(
        candidate.calls.keys().copied(),
        expressions.selections(),
        memory,
        request.semantic_values(),
        &mut candidate.dependencies,
    );

    let mut visited = BTreeSet::new();

    for block in graph
        .blocks()
        .iter()
        .filter(|block| reachable.is_block_reachable(block.id()))
    {
        let Some(property) = property else {
            break;
        };

        if request.is_cancelled() {
            return CheckerOutcome::Cancelled;
        }

        for operation in block
            .operations()
            .iter()
            .filter_map(|id| graph.operation(*id))
        {
            let node = operation.kind().node();

            let preserves = match operation.kind() {
                AnalysisOperationKind::ScopeExit { block, exit, phase } => {
                    checked!(super::cleanup::check_scope_cleanup(
                        request,
                        storage,
                        body.asynchronous(),
                        block,
                        exit,
                        phase,
                        property,
                        &mut candidate.dependencies,
                        &mut diagnostics
                    ))
                }
                AnalysisOperationKind::Recovery(_)
                | AnalysisOperationKind::Suspension { .. }
                | AnalysisOperationKind::TaskOperation { .. } => false,
                AnalysisOperationKind::Call {
                    invocation: bray_bound_tree::SemanticOccurrence::Node(_),
                    ..
                } if !matches!(node, AnyBoundNodeId::Expression(_)) => {
                    panic!("selected call invocation retains an actual expression owner");
                }
                AnalysisOperationKind::Call {
                    invocation:
                        invocation @ (bray_bound_tree::SemanticOccurrence::ScopeEnter(_)
                        | bray_bound_tree::SemanticOccurrence::ScopeExit(_)),
                    ..
                } => {
                    if visited.insert(invocation) {
                        super::operation::collect_scoped_dependency(
                            expressions.selections(),
                            invocation,
                            property,
                            &mut candidate.dependencies,
                        );

                        super::operation::check_storage_accesses(
                            request,
                            invocation,
                            storage,
                            property,
                            &mut candidate.dependencies,
                        )
                    } else {
                        true
                    }
                }
                AnalysisOperationKind::Call {
                    invocation:
                        bray_bound_tree::SemanticOccurrence::Node(
                            bray_bound_tree::AnyBoundNodeId::Expression(expression),
                        ),
                    ..
                }
                | AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression)) => {
                    let mut valid = true;

                    if visited.insert(expression.into()) {
                        valid &= super::operation::check_expression(
                            request,
                            expression,
                            property,
                            expressions.selections(),
                            storage,
                            memory,
                            &mut candidate.dependencies,
                        );

                        for replacement in body.asynchronous().replacement(expression).into_iter() {
                            valid &= checked!(super::cleanup::check_cleanup(
                                request,
                                storage,
                                replacement.access(),
                                replacement.parts(),
                                property,
                                node,
                                &mut candidate.dependencies,
                                &mut diagnostics
                            ));
                        }

                        valid &= checked!(super::operation::check_construction_admission(
                            request,
                            expression,
                            property,
                            expressions,
                            storage,
                            &mut candidate.dependencies,
                            &mut diagnostics
                        ));
                    }

                    valid
                }
                AnalysisOperationKind::PatternObservation(_) | AnalysisOperationKind::Bound(_) => {
                    super::operation::check_storage_accesses(
                        request,
                        node.into(),
                        storage,
                        property,
                        &mut candidate.dependencies,
                    )
                }
                AnalysisOperationKind::Call { .. } => {
                    unreachable!("all selected invocation occurrences are handled above")
                }
            };

            if !preserves {
                record_failure(request, &mut candidate, node);
            }
        }
    }

    check_completion(request, &graph, &reachable, property, &mut candidate);
    check_postconditions(&graph, &reachable, postconditions, &mut candidate);

    CheckerOutcome::complete(candidate, diagnostics)
}

fn check_completion<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    graph: &super::super::model::ControlFlowGraph,
    reachable: &super::flow::ExecutionFlow<'_, '_, C>,
    property: Option<ExecutionProperty>,
    candidate: &mut ExecutionCandidate,
) {
    if property == Some(ExecutionProperty::Total) {
        for exit in graph
            .exits()
            .iter()
            .filter(|exit| reachable.is_block_reachable(exit.block()))
        {
            if matches!(
                exit.kind(),
                AnalysisExitKind::Panic
                    | AnalysisExitKind::Cancellation
                    | AnalysisExitKind::Divergence
                    | AnalysisExitKind::Recovery
            ) {
                record_failure(request, candidate, request.unit().root().into());
            }
        }

        if graph.edges().iter().any(|edge| {
            reachable.is_edge_reachable(edge.id())
                && matches!(
                    edge.kind(),
                    AnalysisEdgeKind::LoopBack | AnalysisEdgeKind::LoopContinue
                )
        }) {
            record_failure(request, candidate, request.unit().root().into());
        }
    }
}

fn record_failure<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    candidate: &mut ExecutionCandidate,
    node: AnyBoundNodeId,
) {
    if candidate.failure.is_some() {
        return;
    }

    let anchor = crate::diagnostic::bound_node_origin(request, node)
        .map_or(request.unit().key().source().syntax(), |origin| {
            origin.source_anchor().syntax()
        });

    candidate.failure = Some(bray_source::SourceSpan::new(
        anchor.source_id(),
        anchor.full_range(),
    ));
}

fn check_postconditions<C: CheckerRequestContext + ?Sized>(
    graph: &super::super::model::ControlFlowGraph,
    flow: &super::flow::ExecutionFlow<'_, '_, C>,
    postconditions: &[(crate::ExecutionCondition, bray_source::SourceSpan)],
    candidate: &mut ExecutionCandidate,
) {
    for (condition, source) in postconditions {
        let proven = graph
            .exits()
            .iter()
            .filter(|exit| {
                matches!(
                    exit.kind(),
                    AnalysisExitKind::Return
                        | AnalysisExitKind::NormalFallthrough
                        | AnalysisExitKind::ResultErrorPropagation
                )
            })
            .all(|exit| {
                let Some(state) = flow.output(exit.block()) else {
                    return true;
                };

                let condition = condition.substitute(
                    &|input| {
                        input
                            .value_in(&state.current)
                            .unwrap_or_else(|| crate::ExecutionCondition::Input(input.clone()))
                    },
                    &state.result,
                    &mut { crate::ExecutionCondition::WORK_LIMIT },
                );

                condition.prove(&state.assumptions, &mut {
                    crate::ExecutionCondition::WORK_LIMIT
                }) == Some(true)
            });

        if !proven {
            candidate.failure.get_or_insert(*source);
        }
    }
}

fn assert_execution_inputs<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expressions: &CheckedExpressionSemantics,
    storage: &StoragePlan,
    body: &CheckedBodySemantics,
    memory: &CheckedMemoryOperations,
) {
    crate::unit::assert_unit_inputs(
        request,
        [
            (
                "expression types",
                (expressions.types().unit(), expressions.types().kind()),
            ),
            (
                "semantic selections",
                (
                    expressions.selections().unit(),
                    expressions.selections().kind(),
                ),
            ),
            ("storage plan", (storage.unit(), storage.kind())),
            ("memory operations", (memory.unit(), memory.kind())),
            (
                "async analysis",
                (body.asynchronous().unit(), body.asynchronous().kind()),
            ),
        ],
    )
}
