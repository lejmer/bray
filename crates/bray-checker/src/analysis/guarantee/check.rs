use bray_diagnostics::DiagnosticExecutionProofFailure as FailureCause;
use std::collections::BTreeSet;

use bray_bound_tree::{
    AnyBoundNodeId, CheckedBodySemantics, CheckedExpressionSemantics, CheckedMemoryOperations,
    StoragePlan,
};

use super::super::model::{
    AnalysisEdgeKind, AnalysisExitKind, AnalysisOperationKind, AnalysisRefinement,
};
use super::flow::analyze_execution_flow;
use super::state::ExecutionState;
use crate::execution_guarantees::{ExecutionCandidate, ExecutionProperty};
use crate::{CheckerOutcome, CheckerRequestContext, CheckerUnitView};

type CandidateOutcome<C> = CheckerOutcome<
    ExecutionCandidate,
    <C as CheckerRequestContext>::UpstreamError,
>;

/// Checks obligations against the same solved flow while their input facts and edges agree.
#[expect(
    clippy::too_many_arguments,
    reason = "candidate checks share independently computed body and call inputs"
)]
pub fn with_execution_candidate_checker<C: CheckerRequestContext + ?Sized, R>(
    request: CheckerUnitView<'_, C>,
    prepared: &crate::PreparedBodyAnalysis,
    predicate: bool,
    contracts: &std::collections::BTreeMap<
        bray_bound_tree::BoundExpressionId,
        Vec<crate::ExecutionCompletionContract>,
    >,
    expressions: &CheckedExpressionSemantics,
    storage: &StoragePlan,
    body: &CheckedBodySemantics,
    memory: &CheckedMemoryOperations,
    check: impl FnOnce(&mut dyn FnMut(
        Option<crate::ExecutionObligation>,
        &[crate::ExecutionCondition],
        &[(crate::ExecutionCondition, bray_source::SourceSpan)],
    ) -> CandidateOutcome<C>) -> R,
) -> R {
    assert_execution_inputs(request, expressions, storage, body, memory);

    let graph = &prepared.graph;

    let disposal = super::cleanup::Cleanup::new(
        &prepared.cleanup,
        body.dependencies(),
        request.semantic_values(),
    );

    let checked_exclusions: BTreeSet<_> = body.storage_flow().excluded_edges().iter().copied().collect();

    let mut previous = None::<super::flow::ExecutionFlow<'_, '_, C>>;

    check(&mut |obligation, assumptions, postconditions| {
        if request.is_cancelled() {
            return CheckerOutcome::Cancelled;
        }

        let internal = obligation.is_some_and(crate::ExecutionObligation::is_internal);

        let property = obligation.and_then(|obligation| {
            obligation.property().or(match obligation {
                crate::ExecutionObligation::Observation => Some(ExecutionProperty::Pure),
                crate::ExecutionObligation::Termination
                | crate::ExecutionObligation::FiniteIteration => Some(ExecutionProperty::Total),
                _ => None,
            })
        });

        let mut excluded = checked_exclusions.clone();

        excluded.extend(graph.edges().iter().filter(|edge| {
            // Pre-entry abandonment belongs to ownership and unwind analysis.
            edge.kind() == AnalysisEdgeKind::EntryCancellation
                || (property != Some(ExecutionProperty::Pure) && edge.is_dependency_failure())
                // Template observation removes materialization failures for checked producers.
                || (predicate || internal)
                    && matches!(edge.refinement(), Some(AnalysisRefinement::GeneratorStorageFailure(producer))
                        if prepared.generator_observations.producer(producer).is_some())
        }).map(|edge| edge.id()));

        if previous.as_ref().is_none_or(|flow| !flow.matches_inputs(assumptions, &excluded)) {
            // Release the old states before solving a new proof environment.
            drop(previous.take());

            previous = analyze_execution_flow(
                prepared, excluded, request, expressions, assumptions, storage, memory,
                body, contracts, &disposal, body.dependencies(),
            );
        }

        match &previous {
            Some(flow) => check_execution_candidate(flow, obligation, property, predicate, internal, postconditions),
            None => CheckerOutcome::Cancelled,
        }
    })
}

fn check_execution_candidate<C: CheckerRequestContext + ?Sized>(
    reachable: &super::flow::ExecutionFlow<'_, '_, C>,
    obligation: Option<crate::ExecutionObligation>,
    property: Option<ExecutionProperty>,
    predicate: bool,
    internal: bool,
    postconditions: &[(crate::ExecutionCondition, bray_source::SourceSpan)],
) -> CandidateOutcome<C> {
    let request = reachable.domain.request;
    let graph = reachable.domain.graph;
    let expressions = reachable.domain.semantics;
    let body = reachable.domain.checked_body.expect("candidate checks retain checked body semantics");
    let memory = reachable.domain.memory;
    let mut diagnostics = bray_diagnostics::DiagnosticBag::new();

    let (mut candidate, reached_cleanup) = reachable.candidate(memory, obligation, property);

    if property == Some(ExecutionProperty::Pure) && !internal {
        reachable.domain.check_predicate_blocks(body.storage_flow(), &mut diagnostics);
    }

    super::operation::collect_preservation_dependencies(
        candidate.calls.keys().copied(),
        expressions.selections(),
        memory,
        request.trusted_contracts(),
        &mut candidate.dependencies,
    );

    if let Some(property) = property {
        match check_operations(
            reachable,
            property,
            predicate,
            internal,
            body,
            memory,
            &reached_cleanup,
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

    let cycles = if property == Some(ExecutionProperty::Total) {
        graph
            .edges()
            .iter()
            .filter(|edge| {
                matches!(
                    edge.kind(),
                    AnalysisEdgeKind::LoopBack | AnalysisEdgeKind::LoopContinue
                )
            })
            .filter_map(|edge| reachable.edge_output(edge.id()).map(|state| (edge, state)))
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let bounded_loops = super::termination::bounded_loop_ranks(reachable, &cycles);

    let finite_loops =
        super::termination::finite_protocol_loops(reachable, &cycles, &mut candidate);

    match check_completion(
        request,
        graph,
        reachable,
        property,
        internal,
        &bounded_loops,
        &cycles,
        &finite_loops,
        &mut candidate,
    ) {
        Ok(()) => {}
        Err(crate::CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(crate::CheckerQueryError::Upstream(error)) => return CheckerOutcome::UpstreamFailure(error),
    }

    match check_postconditions(reachable, postconditions, &mut candidate) {
        Ok(()) => {}
        Err(crate::CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(crate::CheckerQueryError::Upstream(error)) => return CheckerOutcome::UpstreamFailure(error),
    }

    if internal && property == Some(ExecutionProperty::Total) {
        super::termination::finite_step(reachable, &mut candidate, &bounded_loops);
    }

    if internal && property == Some(ExecutionProperty::Total) {
        for dependency in &mut candidate.dependencies {
            if dependency.obligation
                == crate::ExecutionObligation::Property(ExecutionProperty::Total, None)
            {
                dependency.obligation = crate::ExecutionObligation::Termination;
            }
        }
    }

    CheckerOutcome::complete(candidate, diagnostics)
}

#[expect(
    clippy::too_many_arguments,
    reason = "operation certification keeps its execution mode, checked effects, proof output and diagnostics explicit"
)]
fn check_operations<C: CheckerRequestContext + ?Sized>(
    reachable: &super::flow::ExecutionFlow<'_, '_, C>,
    property: ExecutionProperty,
    predicate: bool,
    internal: bool,
    body: &CheckedBodySemantics,
    memory: &CheckedMemoryOperations,
    reached_cleanup: &std::collections::BTreeMap<super::super::id::AnalysisOperationId, super::cleanup::ReachedScopeCleanup>,
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
            && (predicate || internal || reachable.domain.has_bounds_checks))
            .then(|| {
                reachable
                    .entry(block.id())
                    .expect("reachable block retains its execution state")
            });

        let mut predicate_ranges = super::integer::IntegerRanges::new();

        let mut operations = block
            .operations()
            .iter()
            .filter_map(|id| graph.operation(*id))
            .peekable();

        while let Some(operation) = operations.next() {
            let node = operation.kind().node();

            if property == ExecutionProperty::Total {
                candidate.reachable_nodes.insert(node);
            }

            let dependency_start = candidate.dependencies.len();

            let preserves = match operation.kind() {
                AnalysisOperationKind::Call {
                    phase: crate::analysis::model::AnalysisCallPhase::Input,
                    ..
                } => true,
                AnalysisOperationKind::Result {
                    expression,
                    phase: super::super::model::AnalysisResultPhase::Begin,
                } if property == ExecutionProperty::Pure => {
                    check_expression_eligibility(reachable, body, expression, candidate);

                    true
                }
                AnalysisOperationKind::Result { .. } | AnalysisOperationKind::ScopedResult(_) => true,
                AnalysisOperationKind::ScopeExit {
                    scope, exit, ..
                } => super::cleanup::check_scope_cleanup(
                    request,
                    body.storage_flow(),
                    scope,
                    exit,
                    reached_cleanup.get(&operation.id()),
                    property,
                    |access| {
                        (predicate || internal)
                            && reachable.domain.generator_observations.eliminates(storage, access)
                    },
                    &mut candidate.dependencies,
                    diagnostics,
                )?,
                AnalysisOperationKind::Recovery(_)
                | AnalysisOperationKind::Suspension { .. } => false,
                AnalysisOperationKind::Call {
                    invocation: bray_bound_tree::BoundExecutionSite::Node(_),
                    ..
                } if !matches!(node, AnyBoundNodeId::Expression(_)) => {
                    panic!("selected call invocation retains an actual expression owner");
                }
                AnalysisOperationKind::Call {
                    invocation: invocation @ bray_bound_tree::BoundExecutionSite::CallInput { .. },
                    ..
                } => {
                    !visited.insert(invocation)
                        || super::operation::check_input(
                            request,
                            invocation,
                            expressions.selections(),
                            storage,
                            property,
                            &mut candidate.dependencies,
                        )
                }
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
                | AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression))
                | AnalysisOperationKind::AssignmentInstall(expression) => {
                    let mut valid = true;

                    if visited.insert(expression.into()) {
                        if property == ExecutionProperty::Pure {
                            check_expression_eligibility(reachable, body, expression, candidate);
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
                                request.unit().key().is_static_initializer()
                                    || reachable.domain.index_in_bounds(state, expression)
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

            if property == ExecutionProperty::Pure {
                classify_mutation(
                    request,
                    operation.kind(),
                    expressions,
                    storage,
                    body,
                    dependency_start,
                    candidate,
                );
            }

            if !preserves {
                record_failure(request, candidate, node, FailureCause::Unproven);
            }

            if let Some(current) = &mut state
                && operations.peek().is_some()
            {
                if !reachable.domain.operation(current.to_mut(), operation, block.id(), None) {
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

#[expect(
    clippy::too_many_arguments,
    reason = "expression certification retains its checked facts and separate proof and diagnostic outputs"
)]
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

    if let Some(replacement) = body.storage_flow().replacement(expression) {
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

#[expect(
    clippy::too_many_arguments,
    reason = "termination certification keeps loop bounds, cycle states and accumulated proofs separate"
)]
fn check_completion<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    graph: &super::super::model::ControlFlowGraph,
    reachable: &super::flow::ExecutionFlow<'_, '_, C>,
    property: Option<ExecutionProperty>,
    internal: bool,
    bounded_loops: &std::collections::BTreeMap<
        super::super::id::AnalysisBlockId,
        (crate::ExecutionPlace, std::cmp::Ordering),
    >,
    cycles: &[(&super::super::model::AnalysisEdge, ExecutionState)],
    finite_loops: &BTreeSet<super::super::id::AnalysisBlockId>,
    candidate: &mut ExecutionCandidate,
) -> crate::CheckerQueryResult<(), C::UpstreamError> {
    if property == Some(ExecutionProperty::Total) {
        for exit in graph
            .exits()
            .iter()
            .filter(|exit| reachable.is_block_reachable(exit.block()))
        {
            let cause = match exit.kind() {
                AnalysisExitKind::Cancellation => FailureCause::Cancellation,
                AnalysisExitKind::Divergence => {
                    let loop_exit = matches!(exit.node(), AnyBoundNodeId::Expression(expression)
                        if matches!(request.view().expression(expression), Some(bray_bound_tree::BoundExpression::Structured(value))
                            if value.kind() == bray_bound_tree::BoundStructuredExpressionKind::Loop));

                    if loop_exit { FailureCause::UnboundedLoop } else { FailureCause::NonReturningPath }
                }
                AnalysisExitKind::Recovery => FailureCause::Recovery,
                AnalysisExitKind::Panic if !internal => FailureCause::Panic,
                _ => continue,
            };

            if candidate.failure.is_none() && cause == FailureCause::Panic {
                if let Some(condition) = super::diagnostic::panic_requirement(reachable, exit.node())? {
                    candidate.failure = Some(crate::ExecutionVerificationFailure {
                        source: node_span(request, exit.node()), cause,
                        condition: Some(std::sync::Arc::new(condition)),
                    });
                }
            }

            record_failure(request, candidate, exit.node(), cause);
        }

        if let Some((edge, _)) = cycles.iter().find(|(edge, _)| {
            !bounded_loops.contains_key(&edge.target()) && !finite_loops.contains(&edge.target())
        }) {
            let expression = graph.loop_expression(edge.target())
                .expect("a cyclic edge targets its producer-owned loop header");

            record_failure(request, candidate, expression.into(), FailureCause::UnboundedLoop);
        }
    }

    Ok(())
}

fn check_expression_eligibility<C: CheckerRequestContext + ?Sized>(
    flow: &super::flow::ExecutionFlow<'_, '_, C>,
    body: &CheckedBodySemantics,
    expression: bray_bound_tree::BoundExpressionId,
    candidate: &mut ExecutionCandidate,
) {
    let request = flow.domain.request;

    let eligibility = flow
        .domain
        .expression_eligibility(expression, body.storage_flow());

    if eligibility != super::predicate::ExpressionEligibility::Constant {
        record_constant_failure(request, candidate, expression.into());
    }

    if matches!(eligibility, super::predicate::ExpressionEligibility::Forbidden | super::predicate::ExpressionEligibility::ForbiddenBlock(_)) {
        candidate
            .observation_failure
            .get_or_insert(node_span(request, expression.into()));
    }

}

fn classify_mutation<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    operation: AnalysisOperationKind,
    expressions: &CheckedExpressionSemantics,
    storage: &StoragePlan,
    body: &CheckedBodySemantics,
    dependency_start: usize,
    candidate: &mut ExecutionCandidate,
) {
    use bray_bound_tree::StorageMutationScope;

    let occurrence = match operation {
        AnalysisOperationKind::Call { invocation, .. } => invocation,
        AnalysisOperationKind::Bound(node) => node.into(),
        _ => return,
    };

    if let Some(expression) = occurrence.expression()
        && matches!(
            request.view().expression(expression),
            Some(bray_bound_tree::BoundExpression::Assignment(_))
        )
    {
        if body
            .storage_flow()
            .assignment_target(expression)
            .is_some_and(|target| {
                target.mutation_scope(storage, request.semantic_values())
                    == StorageMutationScope::Inputs
            })
        {
            candidate
                .input_mutation
                .get_or_insert(node_span(request, occurrence.node()));
        }

        return;
    }

    let Some(target) = expressions.selections().invoked_callable(occurrence) else {
        return;
    };

    if occurrence.expression().and_then(|expression| expressions.selections().expression(expression))
        .is_some_and(|selection| matches!(selection, bray_bound_tree::SemanticSelection::Call(call)
            if call.phase_behaviors().invocation().execution_properties().contains(&ExecutionProperty::Pure))) {
        return;
    }

    let scope = body.storage_flow().input_observation_scope(occurrence);

    if scope != Some(StorageMutationScope::Invocation) {
        return;
    }

    // Effects on exclusively owned local storage may use observational confinement.
    // Calls reaching inputs retain Pure, including any applicable guarded promise.
    for dependency in &mut candidate.dependencies[dependency_start..] {
        if dependency.target.callable()
            == Some(bray_bound_tree::BoundCallableTarget::Declaration(target))
            && dependency.obligation
                == crate::ExecutionObligation::Property(ExecutionProperty::Pure, None)
        {
            dependency.obligation = crate::ExecutionObligation::Observation;
        }
    }
}

fn node_span<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    node: AnyBoundNodeId,
) -> bray_source::SourceSpan {
    let anchor = crate::diagnostic::bound_node_origin(request, node)
        .map_or(request.unit().key().source().syntax(), |origin| {
            origin.source_anchor().syntax()
        });

    bray_source::SourceSpan::new(anchor.source_id(), anchor.full_range())
}

fn record_constant_failure<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    candidate: &mut ExecutionCandidate,
    node: AnyBoundNodeId,
) {
    candidate
        .constant_failure
        .get_or_insert_with(|| node_span(request, node));
}

fn record_failure<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    candidate: &mut ExecutionCandidate,
    node: AnyBoundNodeId,
    cause: FailureCause,
) {
    if candidate.failure.is_some() {
        return;
    }

    candidate.failure = Some(crate::ExecutionVerificationFailure { condition: None, source: node_span(request, node), cause });
}

fn check_postconditions<C: CheckerRequestContext + ?Sized>(
    flow: &super::flow::ExecutionFlow<'_, '_, C>,
    postconditions: &[(crate::ExecutionCondition, bray_source::SourceSpan)],
    candidate: &mut ExecutionCandidate,
) -> crate::CheckerQueryResult<(), C::UpstreamError> {
    if candidate.failure.is_none()
        && let Some((source, failed)) = super::postcondition::unproven_postcondition(flow, postconditions)
    {
        let condition = super::diagnostic::obligation(&flow.domain,
            bray_diagnostics::DiagnosticContractObligationKind::Postcondition, &failed)?;

        candidate.failure = Some(crate::ExecutionVerificationFailure {
            source, cause: FailureCause::Unproven, condition: Some(std::sync::Arc::new(condition)),
        });
    }

    Ok(())
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
