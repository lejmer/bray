use std::collections::BTreeSet;

use bray_bound_tree::{
    AnyBoundNodeId, CheckedBodySemantics, CheckedExpressionSemantics, CheckedMemoryOperations,
    StoragePlan,
};

use super::super::model::{AnalysisEdgeKind, AnalysisExitKind, AnalysisOperationKind};
use super::flow::analyze_execution_flow;
use super::state::ExecutionState;
use crate::execution_guarantees::{ExecutionCandidate, ExecutionProperty};
use crate::{CheckerOutcome, CheckerRequestContext, CheckerUnitView};

/// Checks body-local operations and records selected dependencies without certifying them.
#[expect(
    clippy::too_many_arguments,
    reason = "execution certification consumes independently computed checker facts in query order"
)]
pub fn check_execution_candidate<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    prepared: &crate::PreparedBodyAnalysis,
    obligation: Option<crate::ExecutionObligation>,
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

    let internal = obligation.is_some_and(crate::ExecutionObligation::is_internal);

    let property = obligation.and_then(|obligation| {
        obligation.property().or(match obligation {
            crate::ExecutionObligation::Observation => Some(ExecutionProperty::Pure),
            crate::ExecutionObligation::Termination
            | crate::ExecutionObligation::FiniteIteration => Some(ExecutionProperty::Total),
            _ => None,
        })
    });

    let mut conditional_invalidations =
        std::borrow::Cow::Borrowed(body.refinements().invalidations());

    if internal {
        for (site, invalidation) in body.storage_flow().observation_invalidations().iter()
            .filter(|(site, _)| body.refinements().invalidations().get(site)
                .is_some_and(|effect| matches!(effect, bray_bound_tree::StorageInvalidation::All { .. }))
                && !body.dependencies().calls().get(site).is_some_and(
                    bray_bound_tree::BoundCallDependencyContracts::preserves_storage_on_normal_return))
        {
            // Copy only when an override must coexist with the immutable checked publication.
            conditional_invalidations.to_mut().insert(*site, invalidation.clone());
        }
    }

    let invalidations = conditional_invalidations.as_ref();

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

    crate::analysis::exclude_impossible_call_outcomes(graph, request, &mut excluded);

    let reachable = match analyze_execution_flow(
        prepared,
        excluded,
        request,
        expressions,
        assumptions,
        storage,
        memory,
        body,
        invalidations,
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

    let mut candidate = reachable.candidate(memory, obligation);

    if internal {
        for occurrence in candidate.calls.keys().copied() {
            if conditional_invalidations.get(&occurrence)
                == body.refinements().invalidations().get(&occurrence)
            {
                continue;
            }

            let Some(selection) = occurrence
                .expression()
                .and_then(|expression| expressions.selections().expression(expression))
            else {
                continue;
            };

            let target = if let bray_bound_tree::SemanticSelection::Call(call) = selection {
                crate::ExecutionDependencyTarget::selected(call)
            } else {
                expressions
                    .selections()
                    .invoked_callable(occurrence)
                    .map(|callable| {
                        bray_bound_tree::BoundCallableTarget::Declaration(callable).into()
                    })
                    .expect("conditional invocation effect retains its selected callable")
            };

            candidate.dependencies.push(crate::ExecutionDependency {
                target,
                obligation: crate::ExecutionObligation::Observation,
                occurrence,
            });
        }
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
            &reachable,
            property,
            predicate,
            internal,
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

    let bounded_loops = super::termination::bounded_loop_ranks(&reachable, &cycles);

    let finite_loops =
        super::termination::finite_protocol_loops(&reachable, &cycles, &mut candidate);

    check_completion(
        request,
        graph,
        &reachable,
        property,
        internal,
        &bounded_loops,
        &cycles,
        &finite_loops,
        &mut candidate,
    );

    check_postconditions(&reachable, postconditions, &mut candidate);

    if internal && property == Some(ExecutionProperty::Total) {
        super::termination::finite_step(&reachable, &mut candidate, &bounded_loops);
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
                // Replay operations on a copy without changing the solved block entry.
                reachable
                    .states
                    .state(block.id())
                    .and_then(Option::as_ref)
                    .expect("reachable block retains its execution state")
                    .clone()
            });

        let mut predicate_ranges = super::integer::IntegerRanges::new();

        for operation in block
            .operations()
            .iter()
            .filter_map(|id| graph.operation(*id))
        {
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
                    |access| {
                        candidate
                            .inert_cleanup
                            .get(&(exit, access))
                            .copied()
                            .unwrap_or(false)
                    },
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
                | AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression)) => {
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
) {
    if property == Some(ExecutionProperty::Total) {
        for exit in graph
            .exits()
            .iter()
            .filter(|exit| reachable.is_block_reachable(exit.block()))
        {
            if matches!(
                exit.kind(),
                AnalysisExitKind::Cancellation
                    | AnalysisExitKind::Divergence
                    | AnalysisExitKind::Recovery
            ) || exit.kind() == AnalysisExitKind::Panic && !internal
            {
                record_failure(request, candidate, request.unit().root().into());
            }
        }

        if cycles.iter().any(|(edge, _)| {
            !bounded_loops.contains_key(&edge.target()) && !finite_loops.contains(&edge.target())
        }) {
            record_failure(request, candidate, request.unit().root().into());
        }
    }
}

fn check_expression_eligibility<C: CheckerRequestContext + ?Sized>(
    flow: &super::flow::ExecutionFlow<'_, '_, C>,
    body: &CheckedBodySemantics,
    expression: bray_bound_tree::BoundExpressionId,
    candidate: &mut ExecutionCandidate,
) {
    let request = flow.domain.request;

    let eligibility = super::predicate::expression_eligibility(
        request,
        expression,
        flow.domain.semantics,
        flow.domain.storage,
        body.storage_flow(),
        flow.domain.yield_targets,
    );

    if eligibility != super::predicate::ExpressionEligibility::Constant {
        record_constant_failure(request, candidate, expression.into());
    }

    if eligibility == super::predicate::ExpressionEligibility::Forbidden {
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

    let effect = body
        .storage_flow()
        .observation_invalidations()
        .get(&occurrence)
        .or_else(|| body.refinements().invalidations().get(&occurrence));

    if !matches!(
        effect,
        Some(bray_bound_tree::StorageInvalidation::Observed {
            scope: StorageMutationScope::Invocation,
            ..
        })
    ) {
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
