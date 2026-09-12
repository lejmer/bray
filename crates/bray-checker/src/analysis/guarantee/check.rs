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
use super::super::reachability::analyze_reachability;
use crate::execution_guarantees::{ExecutionCandidate, ExecutionProperty};
use crate::{CheckerOutcome, CheckerRequestContext, CheckerUnitView};

/// Checks body-local operations and records selected dependencies without certifying them.
pub fn check_execution_candidate<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    property: ExecutionProperty,
    expressions: &CheckedExpressionSemantics,
    storage: &StoragePlan,
    body: &CheckedBodySemantics,
    memory: &CheckedMemoryOperations,
) -> CheckerOutcome<ExecutionCandidate, C::UpstreamError> {
    if let Some(error) = crate::unit::semantic_input_failure(
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
    ) {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    // Purity alone does not discharge cleanup on a dependency's abnormal completion.
    let graph = match property {
        ExecutionProperty::Pure => {
            build_storage_control_flow_graph(request, storage, expressions.selections())
        }
        ExecutionProperty::Total => {
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

    let Some(reachable) = analyze_reachability(&graph, request) else {
        return CheckerOutcome::Cancelled;
    };

    let mut candidate = ExecutionCandidate::default();
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

    if let Err(error) = check_completion(
        request,
        &graph,
        &reachable,
        expressions,
        property,
        &mut candidate,
    ) {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    CheckerOutcome::complete(candidate, diagnostics)
}

fn check_completion<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    graph: &super::super::model::ControlFlowGraph,
    reachable: &super::super::reachability::ReachabilityResult,
    expressions: &CheckedExpressionSemantics,
    property: ExecutionProperty,
    candidate: &mut ExecutionCandidate,
) -> Result<(), crate::CheckerInfrastructureError> {
    // TODO(BRA-509): Remove this safeguard when ordinary body checking rejects missing results.
    if graph.exits().iter().any(|exit| {
        reachable.is_block_reachable(exit.block())
            && exit.kind() == AnalysisExitKind::NormalFallthrough
    }) {
        let result = match expressions.types().callable_result_type() {
            Some(ty) => crate::representation::type_representation(request, ty),
            None => Ok(None),
        };

        match result {
            Ok(Some(bray_compiler_known::RepresentationRole::Unit)) => {}
            Ok(_) => record_failure(request, candidate, request.unit().root().into()),
            Err(error) => return Err(error),
        }
    }

    if property == ExecutionProperty::Total {
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

    Ok(())
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
