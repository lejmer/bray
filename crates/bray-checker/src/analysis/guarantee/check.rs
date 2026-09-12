use std::collections::BTreeSet;

use bray_bound_tree::{
    AnyBoundNodeId, CheckedBodySemantics, CheckedExpressionSemantics, CheckedMemoryOperations,
    StoragePlan,
};
use bray_diagnostics::DiagnosticBag;

use super::super::build::{
    ControlFlowGraphBuildOutcome, build_execution_control_flow_graph,
    build_storage_control_flow_graph,
};
use super::super::model::{
    AnalysisEdgeKind, AnalysisExitKind, AnalysisOperationKind, AnalysisScopeExitPhase,
};
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
    if let Some(error) = execution_input_failure(request, expressions, storage, body, memory) {
        return CheckerOutcome::InfrastructureFailure(error);
    }

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

    let literals = match crate::execution_guarantees::condition_literals(
        expressions,
        request.semantic_values(),
    ) {
        Ok(literals) => literals,
        Err(error) => {
            return CheckerOutcome::InfrastructureFailure(
                crate::CheckerInfrastructureError::SemanticValueStore(error),
            );
        }
    };

    let reachable = match analyze_execution_flow(
        &graph,
        request,
        expressions,
        assumptions,
        &literals,
        storage,
        contracts,
    ) {
        super::super::fixed_point::FixedPointOutcome::Complete(flow) => flow,
        super::super::fixed_point::FixedPointOutcome::Cancelled => {
            return CheckerOutcome::Cancelled;
        }
        super::super::fixed_point::FixedPointOutcome::ConvergenceInvariantViolated => {
            let mut candidate = ExecutionCandidate::default();
            record_failure(request, &mut candidate, request.unit().root().into());

            return CheckerOutcome::complete(candidate, DiagnosticBag::new());
        }
    };

    let mut candidate = ExecutionCandidate {
        calls: reachable.calls(),
        completion_dependencies: reachable.completion_dependencies(),
        ..ExecutionCandidate::default()
    };

    let mut diagnostics = DiagnosticBag::new();

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
                    let mut valid = true;

                    for plan in body
                        .asynchronous()
                        .scope_exits()
                        .iter()
                        .filter(|plan| plan.scope() == block && plan.exit() == exit)
                    {
                        valid &= !plan.is_recovered() && plan.cancellation_broadcast().is_empty();

                        if phase == AnalysisScopeExitPhase::LifecycleResolution {
                            for access in plan.lifecycle_resolution() {
                                let identity = storage.root_identity(*access);

                                let parts = body
                                    .asynchronous()
                                    .storage_requirements()
                                    .iter()
                                    .find(|requirement| Some(requirement.identity()) == identity)
                                    .and_then(|requirement| requirement.parts());

                                valid &= checked!(super::cleanup::check_cleanup(
                                    request,
                                    storage,
                                    *access,
                                    parts,
                                    property,
                                    exit,
                                    &mut candidate.dependencies,
                                    &mut diagnostics
                                ));
                            }
                        }
                    }

                    valid
                }
                AnalysisOperationKind::Recovery(_)
                | AnalysisOperationKind::Suspension { .. }
                | AnalysisOperationKind::TaskOperation { .. } => false,
                AnalysisOperationKind::PatternObservation(_) => true,
                AnalysisOperationKind::Call { expression, .. }
                | AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression)) => {
                    let mut valid = true;

                    if visited.insert(expression) {
                        valid &= super::operation::check_expression(
                            request,
                            expression,
                            property,
                            expressions.selections(),
                            storage,
                            memory,
                            &mut candidate.dependencies,
                        );

                        for replacement in body
                            .asynchronous()
                            .replacements()
                            .iter()
                            .filter(|replacement| replacement.expression() == expression)
                        {
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

                        if matches!(expressions.selections().expression(expression), Some(bray_bound_tree::SemanticSelection::Operation(bray_bound_tree::SelectedOperation::Construction(construction))) if !matches!(construction.target(), bray_bound_tree::ConstructionTarget::TypeForm { .. }))
                        {
                            if let Some(ty) = expressions
                                .types()
                                .expression(expression)
                                .map(|entry| entry.ty())
                            {
                                let admission =
                                    checked!(crate::asynchronous::execution_cleanup_dependencies(
                                        request,
                                        ty,
                                        property,
                                        crate::asynchronous::ExecutionCleanupMode::Admission,
                                        node
                                    ));

                                valid &= admission.value().is_some();
                                diagnostics.add_range(admission.into_parts().1);
                            } else {
                                valid = false;
                            }
                        }
                    }

                    valid
                }
                AnalysisOperationKind::Bound(_) => true,
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

fn execution_input_failure<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expressions: &CheckedExpressionSemantics,
    storage: &StoragePlan,
    body: &CheckedBodySemantics,
    memory: &CheckedMemoryOperations,
) -> Option<crate::CheckerInfrastructureError> {
    crate::unit::semantic_input_failure(
        request,
        [
            (
                crate::CheckerInputKind::ExpressionTypes,
                (expressions.types().unit(), expressions.types().kind()),
            ),
            (
                crate::CheckerInputKind::SemanticSelections,
                (
                    expressions.selections().unit(),
                    expressions.selections().kind(),
                ),
            ),
            (
                crate::CheckerInputKind::StoragePlan,
                (storage.unit(), storage.kind()),
            ),
            (
                crate::CheckerInputKind::MemoryOperations,
                (memory.unit(), memory.kind()),
            ),
            (
                crate::CheckerInputKind::AsyncAnalysis,
                (body.asynchronous().unit(), body.asynchronous().kind()),
            ),
        ],
    )
}
