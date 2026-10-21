use std::collections::BTreeSet;

use bray_bound_tree::{
    AnyBoundNodeId, CheckedBodySemantics, CheckedExpressionSemantics, CheckedMemoryOperations,
    StoragePlan,
};

use super::super::model::{AnalysisEdgeKind, AnalysisExitKind, AnalysisOperationKind};
use super::flow::analyze_execution_flow;
use crate::execution_guarantees::{ExecutionCandidate, ExecutionProperty};
use crate::{CheckerOutcome, CheckerRequestContext, CheckerUnitView};

/// Checks body-local operations and records selected dependencies without certifying them.
pub fn check_execution_candidate<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    prepared: &crate::PreparedBodyAnalysis,
    property: Option<ExecutionProperty>,
    predicate: bool,
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

    let graph = &prepared.graph;
    let mut diagnostics = bray_diagnostics::DiagnosticBag::new();

    let disposal = super::cleanup::Cleanup::new(&prepared.cleanup, body.dependencies())
        .with_selection(body.asynchronous(), body.storage_flow());

    let mut excluded = graph
        .edges()
        .iter()
        .filter(|edge| {
            // Async guarantees describe execution after entry. Pre-entry abandonment
            // remains part of ownership and unwind analysis, not this body proof.
            edge.kind() == AnalysisEdgeKind::EntryCancellation
                || (property != Some(ExecutionProperty::Pure) && edge.is_dependency_failure())
        })
        .map(|edge| edge.id())
        .collect();

    crate::analysis::exclude_non_unwinding_edges(graph, request, &mut excluded);

    let reachable = match analyze_execution_flow(
        prepared,
        excluded,
        request,
        expressions,
        assumptions,
        storage,
        memory,
        body,
        body.refinements().invalidations(),
        contracts,
        &disposal,
        body.dependencies(),
    ) {
        CheckerOutcome::Complete(flow) => {
            let (flow, additional) = flow.into_parts();

            diagnostics.add_range(additional);

            flow
        }
        CheckerOutcome::Cancelled => {
            return CheckerOutcome::Cancelled;
        }
        CheckerOutcome::UpstreamFailure(error) => return CheckerOutcome::UpstreamFailure(error),
    };

    let mut candidate = reachable.candidate(memory, property == Some(ExecutionProperty::Total));

    super::operation::collect_preservation_dependencies(
        candidate.calls.keys().copied(),
        expressions.selections(),
        memory,
        request.trusted_contracts(),
        &mut candidate.dependencies,
    );

    if let Some(property) = property {
        match check_operations(
            &reachable,
            property,
            predicate,
            body,
            memory,
            &mut candidate,
            &mut diagnostics,
        ) {
            Ok(()) => {}
            Err(crate::CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
            Err(crate::CheckerQueryError::Upstream(error)) => {
                return CheckerOutcome::UpstreamFailure(error);
            }
        }
    }

    let bounded_loops = if property == Some(ExecutionProperty::Total)
        && graph
            .edges()
            .iter()
            .any(|edge| edge.kind() == AnalysisEdgeKind::LoopBack)
    {
        super::termination::bounded_loop_headers(
            request,
            graph,
            expressions,
            storage,
            &prepared.literals,
        )
    } else {
        BTreeSet::new()
    };

    check_completion(
        request,
        graph,
        &reachable,
        property,
        &bounded_loops,
        &mut candidate,
    );

    check_postconditions(&reachable, postconditions, &mut candidate);

    CheckerOutcome::complete(candidate, diagnostics)
}

fn check_operations<C: CheckerRequestContext + ?Sized>(
    reachable: &super::flow::ExecutionFlow<'_, '_, C>,
    property: ExecutionProperty,
    predicate: bool,
    body: &CheckedBodySemantics,
    memory: &CheckedMemoryOperations,
    candidate: &mut ExecutionCandidate,
    diagnostics: &mut bray_diagnostics::DiagnosticBag,
) -> Result<(), crate::CheckerQueryError<C::UpstreamError>> {
    let request = reachable.domain.request;
    let graph = reachable.domain.graph;
    let expressions = reachable.domain.semantics;
    let storage = reachable.domain.storage;

    let mut visited = BTreeSet::new();

    for block in graph
        .blocks()
        .iter()
        .filter(|block| reachable.is_block_reachable(block.id()))
    {
        if request.is_cancelled() {
            return Err(crate::CheckerQueryError::Cancelled);
        }

        let mut state = (property == ExecutionProperty::Total
                && (predicate || !reachable.domain.sequence_lengths.is_empty()))
        .then(|| {
            // Replay operations on a copy without changing the solved block entry.
            reachable
                .states
                .state(block.id())
                .and_then(Option::as_ref)
                .expect("reachable block retains its execution state")
                .clone()
        });

        let mut predicate_ranges = super::predicate::IntegerRanges::new();

        for operation in block
            .operations()
            .iter()
            .filter_map(|id| graph.operation(*id))
        {
            let node = operation.kind().node();

            if predicate && property == ExecutionProperty::Total {
                candidate.reachable_nodes.insert(node);
            }

            let preserves = match operation.kind() {
                AnalysisOperationKind::Call { phase: crate::analysis::model::AnalysisCallPhase::Input, .. } => true,
                AnalysisOperationKind::Result {
                    expression,
                    phase: super::super::model::AnalysisResultPhase::Begin,
                } if predicate && property == ExecutionProperty::Pure => {
                    if !super::predicate::expression_is_eligible(
                        request,
                        expression,
                        expressions,
                        storage,
                        body.storage_flow(),
                        reachable.domain.yield_targets,
                    ) {
                        record_constant_failure(request, candidate, expression.into());
                    }

                    true
                }
                AnalysisOperationKind::Result { .. } => true,
                AnalysisOperationKind::ScopeExit {
                    block, exit, phase, ..
                } => super::cleanup::check_scope_cleanup(
                    request,
                    storage,
                    body.asynchronous(),
                    block,
                    exit,
                    phase,
                    property,
                    |access| candidate.inert_cleanup.get(&(exit, access)).copied().unwrap_or(false),
                    &mut candidate.dependencies,
                    diagnostics,
                )?,
                AnalysisOperationKind::Recovery(_)
                | AnalysisOperationKind::Suspension { .. }
                | AnalysisOperationKind::TaskOperation { .. } => false,
                AnalysisOperationKind::Call {
                    invocation: bray_bound_tree::BoundExecutionSite::Node(_),
                    ..
                } if !matches!(node, AnyBoundNodeId::Expression(_)) => {
                    panic!("selected call invocation retains an actual expression owner");
                }
                AnalysisOperationKind::Call {
                    invocation: invocation @ bray_bound_tree::BoundExecutionSite::CallInput { .. },
                    ..
                } => !visited.insert(invocation) || super::operation::check_input(
                    request, invocation, expressions.selections(), storage, property, &mut candidate.dependencies,
                ),
                AnalysisOperationKind::Call {
                    invocation:
                        invocation @ (bray_bound_tree::BoundExecutionSite::Iterate(_)
                        | bray_bound_tree::BoundExecutionSite::Next(_)
                        | bray_bound_tree::BoundExecutionSite::ScopedEnter(_)
                        | bray_bound_tree::BoundExecutionSite::ScopedExit(_)),
                    ..
                } => {
                    if visited.insert(invocation) {
                        super::operation::collect_invocation_dependency(
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
                        bray_bound_tree::BoundExecutionSite::Node(AnyBoundNodeId::Expression(
                            expression,
                        )),
                    ..
                }
                | AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression)) => {
                    let mut valid = true;

                    if visited.insert(expression.into()) {
                        if predicate && property == ExecutionProperty::Pure {
                            if !super::predicate::expression_is_eligible(
                                request,
                                expression,
                                expressions,
                                storage,
                                body.storage_flow(),
                                reachable.domain.yield_targets,
                            ) {
                                record_constant_failure(request, candidate, expression.into());
                            }
                        }

                        let predicate_valid = !predicate
                            || state.as_ref().is_none_or(|state| {
                                reachable.domain.predicate_operation_is_valid(
                                    state,
                                    expression,
                                    &mut predicate_ranges,
                                    &mut candidate.integer_maxima,
                                )
                            });

                        if !predicate_valid {
                            record_constant_failure(request, candidate, expression.into());
                        }

                        valid &= check_expression_execution(
                            request,
                            expression,
                            property,
                            expressions,
                            storage,
                            body,
                            memory,
                            state.as_ref().is_some_and(|state| {
                                reachable.domain.index_in_bounds(state, expression)
                                    || predicate && predicate_valid
                            }),
                            &mut candidate.dependencies,
                            diagnostics,
                        )?;
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
                record_failure(request, candidate, node);
            }

            if let Some(current) = &mut state {
                if !reachable.domain.operation(current, operation.kind(), None) {
                    break;
                }

                if matches!(operation.kind(), AnalysisOperationKind::Call { .. }) {
                    predicate_ranges.clear();
                }
            }
        }
    }

    Ok(())
}

fn check_expression_execution<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expression: bray_bound_tree::BoundExpressionId,
    property: ExecutionProperty,
    expressions: &CheckedExpressionSemantics,
    storage: &StoragePlan,
    body: &CheckedBodySemantics,
    memory: &CheckedMemoryOperations,
    operation_domain_proven: bool,
    dependencies: &mut Vec<crate::execution_guarantees::ExecutionDependency>,
    diagnostics: &mut bray_diagnostics::DiagnosticBag,
) -> Result<bool, crate::CheckerQueryError<C::UpstreamError>> {
    let mut valid = super::operation::check_expression(
        request,
        expression,
        super::operation::ExecutionGoal::Property(property),
        expressions.selections(),
        storage,
        Some(body.storage_flow()),
        memory,
        operation_domain_proven,
        dependencies,
    );

    for replacement in body.asynchronous().replacement(expression).into_iter() {
        valid &= super::cleanup::check_cleanup(
            request,
            storage,
            replacement.access(),
            replacement.parts(),
            property,
            expression.into(),
            dependencies,
            diagnostics,
        )?;
    }

    valid &= super::operation::check_construction_admission(
        request,
        expression,
        property,
        expressions,
        storage,
        dependencies,
        diagnostics,
    )?;

    Ok(valid)
}

fn check_completion<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    graph: &super::super::model::ControlFlowGraph,
    reachable: &super::flow::ExecutionFlow<'_, '_, C>,
    property: Option<ExecutionProperty>,
    bounded_loops: &BTreeSet<super::super::id::AnalysisBlockId>,
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
                && (edge.kind() == AnalysisEdgeKind::LoopContinue
                    || (edge.kind() == AnalysisEdgeKind::LoopBack
                        && !bounded_loops.contains(&edge.target())))
        }) {
            record_failure(request, candidate, request.unit().root().into());
        }
    }
}

fn record_constant_failure<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    candidate: &mut ExecutionCandidate,
    node: AnyBoundNodeId,
) {
    let anchor = crate::diagnostic::bound_node_origin(request, node)
        .map_or(request.unit().key().source().syntax(), |origin| {
            origin.source_anchor().syntax()
        });

    candidate
        .constant_failure
        .get_or_insert(bray_source::SourceSpan::new(
            anchor.source_id(),
            anchor.full_range(),
        ));
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
    flow: &super::flow::ExecutionFlow<'_, '_, C>,
    postconditions: &[(crate::ExecutionCondition, bray_source::SourceSpan)],
    candidate: &mut ExecutionCandidate,
) {
    if let Some(source) =
        super::postcondition::unproven_postconditions(flow, postconditions).first()
    {
        candidate.failure.get_or_insert(*source);
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
