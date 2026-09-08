use bray_bound_tree::{AnyBoundNodeId, CheckedAsync, CheckedSemanticSelections, StoragePlan};
use bray_diagnostics::{Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticKind, SeverityKind};
use bray_symbols::{
    CallableConditions, CallableContractClause, CallableExecutionGuarantee, ExecutionProperty,
    SymbolOrdinal,
};

use crate::diagnostic::{bound_node_origin, diagnostic_id};
use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext, CheckerUnitView,
    ExecutionGuaranteeInput,
};

use super::super::fixed_point::{FixedPointOutcome, solve_fixed_point};
use super::super::id::AnalysisBlockId;
use super::super::model::{AnalysisEdgeKind, AnalysisExit, AnalysisExitKind, ControlFlowGraph};
use super::flow::GuaranteeDomain;
use super::operation::{operation_preserves_property, storage_operations_requiring_proof};
use super::postcondition::first_postcondition_failure;

#[derive(Clone, Copy)]
enum Obligation {
    Execution(CallableExecutionGuarantee),
    Postcondition(CallableContractClause),
}

impl Obligation {
    fn proof(self) -> bray_bound_tree::CallableProofObligation {
        match self {
            Self::Execution(guarantee) => {
                bray_bound_tree::CallableProofObligation::Execution(guarantee)
            }
            Self::Postcondition(clause) => {
                bray_bound_tree::CallableProofObligation::Postcondition(clause.ordinal())
            }
        }
    }

    fn guard(self) -> Option<SymbolOrdinal> {
        match self {
            Self::Execution(guarantee) => guarantee.guard(),
            Self::Postcondition(clause) => clause.guard(),
        }
    }
}

enum ProofFailure {
    MissingState(AnalysisBlockId),
    Operation(AnyBoundNodeId),
    AbnormalExit(AnalysisExit),
    UnboundedLoop(AnalysisBlockId),
    Postcondition(AnalysisExit),
}

impl ProofFailure {
    fn origin(&self, graph: &ControlFlowGraph) -> Option<AnyBoundNodeId> {
        match self {
            Self::Operation(node) => Some(*node),
            Self::AbnormalExit(exit) | Self::Postcondition(exit) => exit.origin(),
            Self::MissingState(block) | Self::UnboundedLoop(block) => graph
                .block(*block)?
                .operations()
                .last()
                .and_then(|operation| graph.operation(*operation))
                .map(|operation| operation.kind().node()),
        }
    }
}

pub(crate) fn check_execution_guarantees<C>(
    request: CheckerUnitView<'_, C>,
    input: &ExecutionGuaranteeInput,
    selections: &CheckedSemanticSelections,
    patterns: &bray_bound_tree::CheckedPatterns,
    storage: &StoragePlan,
    asynchronous: &CheckedAsync,
    graph: &ControlFlowGraph,
) -> CheckerOutcome<Vec<bray_bound_tree::CallableProofResult>, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut diagnostics = DiagnosticBag::new();
    let mut proven = Vec::new();

    let boolean = match crate::representation::representation_type(
        request,
        bray_compiler_known::RepresentationRole::ScalarBool,
    ) {
        Ok(ty) => ty,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    let storage_calls = storage_operations_requiring_proof(storage, input.projections_verified());

    let assignments = match super::observation::assignment_observations(request, input, graph) {
        Ok(assignments) => assignments,
        Err(error) => {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::SemanticValueStore(error),
            );
        }
    };

    let observation_accesses =
        match super::observation::observation_accesses(request.semantic_values(), input, storage) {
            Ok(accesses) => accesses,
            Err(error) => {
                return CheckerOutcome::InfrastructureFailure(
                    CheckerInfrastructureError::SemanticValueStore(error),
                );
            }
        };

    let mutations = super::super::storage_index::invalidating_operation_accesses(storage);
    let local_initializers = super::observation::local_initializers(request, input);
    let mut cleanup = std::collections::BTreeMap::new();

    for plan in asynchronous.scope_exits() {
        let empty = !plan.is_recovered()
            && plan.cancellation_broadcast().is_empty()
            && plan.lifecycle_resolution().is_empty();

        cleanup
            .entry((plan.scope(), plan.exit()))
            .and_modify(|previous| *previous &= empty)
            .or_insert(empty);
    }

    let empty_cleanup = cleanup
        .into_iter()
        .filter_map(|(key, empty)| empty.then_some(key))
        .collect();

    let preserving_operations = |property| {
        graph
            .blocks()
            .iter()
            .flat_map(|block| block.operations())
            .filter_map(|operation| graph.operation(*operation))
            .filter(|operation| {
                operation_preserves_property(
                    request,
                    operation.kind(),
                    property,
                    selections,
                    &storage_calls,
                    asynchronous,
                )
            })
            .map(|operation| operation.id())
            .collect()
    };

    let pure_operations = preserving_operations(ExecutionProperty::Pure);
    let total_operations = preserving_operations(ExecutionProperty::Total);

    let obligations_by_guard = obligations_by_guard(input);

    for (guard, obligations) in obligations_by_guard {
        if request.is_cancelled() {
            return CheckerOutcome::Cancelled;
        }

        let mut assumptions = input
            .conditions()
            .invocation_preconditions()
            .iter()
            .filter_map(|clause| {
                clause
                    .predicate()?
                    .condition()
                    .map(|condition| (condition, true))
            })
            .collect::<Vec<_>>();

        let Some(guards) = crate::contract_guard_conditions(input.conditions(), guard) else {
            for obligation in obligations {
                match failure_diagnostic(request, None, obligation, diagnostics.len()) {
                    Ok(diagnostic) => diagnostics.add(diagnostic),
                    Err(error) => return CheckerOutcome::InfrastructureFailure(error),
                }
            }

            continue;
        };

        assumptions.extend(guards.into_iter().map(|guard| (guard, true)));

        let domain = GuaranteeDomain {
            view: request.view(),
            selections,
            patterns,
            boolean,
            result_representation: request
                .available_compiler_known_symbols()
                .result_representation(),
            available: request.available_compiler_known_symbols(),
            asynchronous,
            graph,
            values: request.semantic_values(),
            input,
            assumptions: &assumptions,
            pure_operations: &pure_operations,
            total_operations: &total_operations,
            storage_calls: &storage_calls,
            empty_cleanup: &empty_cleanup,
            assignments: &assignments,
            storage,
            observation_accesses: &observation_accesses,
            mutations: &mutations,
            local_initializers: &local_initializers,
        };

        let result = match solve_fixed_point(graph, &domain, &request) {
            FixedPointOutcome::Complete(result) => result,
            FixedPointOutcome::Cancelled => return CheckerOutcome::Cancelled,
            FixedPointOutcome::ConvergenceInvariantViolated => {
                if guard.is_none() && input.needs_completion_check() {
                    match super::finalization::completion_candidates(
                        request,
                        &domain,
                        None,
                        asynchronous,
                    ) {
                        Ok(result) => diagnostics = diagnostics.merged(result.diagnostics()),
                        Err(error) => return error.into(),
                    }
                }

                for obligation in obligations {
                    match failure_diagnostic(request, None, obligation, diagnostics.len()) {
                        Ok(diagnostic) => diagnostics.add(diagnostic),
                        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
                    }
                }

                continue;
            }
        };

        if guard.is_none() && input.needs_completion_check() {
            match super::finalization::completion_candidates(
                request,
                &domain,
                Some(&result),
                asynchronous,
            ) {
                Ok(candidates) => {
                    let (candidates, completion_diagnostics) = candidates.into_parts();

                    proven.extend(
                        candidates
                            .into_iter()
                            .map(bray_bound_tree::CallableProofResult::Candidate),
                    );

                    diagnostics = diagnostics.merged(&completion_diagnostics);
                }
                Err(error) => return error.into(),
            }
        }

        if let Err(error) = check_obligations(
            request,
            &domain,
            &result,
            &obligations,
            &mut proven,
            &mut diagnostics,
        ) {
            return error.into();
        }
    }

    CheckerOutcome::complete(proven, diagnostics)
}

fn obligations_by_guard(
    input: &ExecutionGuaranteeInput,
) -> std::collections::BTreeMap<Option<SymbolOrdinal>, Vec<Obligation>> {
    let mut obligations_by_guard = std::collections::BTreeMap::<_, Vec<_>>::new();

    if input.needs_completion_check() {
        obligations_by_guard.entry(None).or_default();
    }

    for obligation in input
        .conditions()
        .execution_guarantees()
        .iter()
        .copied()
        .map(Obligation::Execution)
        .chain(
            input
                .conditions()
                .normal_completion_postconditions()
                .iter()
                .chain(input.conditions().guarded_postconditions())
                .copied()
                .map(Obligation::Postcondition),
        )
    {
        obligations_by_guard
            .entry(obligation.guard())
            .or_default()
            .push(obligation);
    }

    obligations_by_guard
}

fn check_obligations<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    domain: &GuaranteeDomain<'_>,
    result: &super::super::fixed_point::FixedPointResult<
        Result<super::flow::DomainState, CheckerInfrastructureError>,
    >,
    obligations: &[Obligation],
    proven: &mut Vec<bray_bound_tree::CallableProofResult>,
    diagnostics: &mut DiagnosticBag,
) -> Result<(), crate::CheckerQueryError<C::UpstreamError>> {
    for &obligation in obligations {
        if request.is_cancelled() {
            return Err(crate::CheckerQueryError::Cancelled);
        }

        let mut dependencies = std::collections::BTreeSet::new();

        let failure = match obligation {
            Obligation::Execution(guarantee) => {
                first_failure(domain, result, guarantee.property(), &mut dependencies)
            }
            Obligation::Postcondition(clause) => {
                first_postcondition_failure(request, domain, result, clause)
                    .map(|failure| failure.map(ProofFailure::Postcondition))
            }
        };

        let failure = match failure {
            Ok(failure) => failure,
            Err(error) => return Err(error.into()),
        };

        if let Some(failure) = failure {
            // General postconditions contribute evidence when independently proved. A declared
            // execution guarantee or conditional guarantee requires this static verification.
            let optional = domain.input.conditions().execution_guarantees().is_empty()
                && domain
                    .input
                    .conditions()
                    .guarded_postconditions()
                    .is_empty();

            let ordinal = if optional {
                proven.len()
            } else {
                diagnostics.len()
            };

            let diagnostic = match failure_diagnostic(
                request,
                failure.origin(domain.graph),
                obligation,
                ordinal,
            ) {
                Ok(diagnostic) => diagnostic,
                Err(error) => return Err(error.into()),
            };

            if optional {
                proven.push(bray_bound_tree::CallableProofResult::Unproven {
                    obligation: obligation.proof(),
                    diagnostic,
                });
            } else {
                diagnostics.add(diagnostic);
            }
        } else {
            let obligation = obligation.proof();

            for block in domain.graph.blocks() {
                if let Some(Ok(state)) = result.state(block.id())
                    && state.reachable
                {
                    dependencies.extend(state.dependencies.iter().copied());
                }
            }

            proven.push(bray_bound_tree::CallableProofResult::Candidate(
                bray_bound_tree::CallableProofCandidate::new(obligation, dependencies),
            ));
        }
    }

    Ok(())
}

fn first_failure(
    domain: &GuaranteeDomain<'_>,
    result: &super::super::fixed_point::FixedPointResult<
        Result<super::flow::DomainState, CheckerInfrastructureError>,
    >,
    property: ExecutionProperty,
    dependencies: &mut std::collections::BTreeSet<bray_bound_tree::CallableProofDependency>,
) -> Result<Option<ProofFailure>, CheckerInfrastructureError> {
    for block in domain.graph.blocks() {
        let Some(state) = result.state(block.id()) else {
            return Ok(Some(ProofFailure::MissingState(block.id())));
        };

        let state = state.as_ref().map_err(|error| *error)?;

        if !state.reachable {
            continue;
        }

        // The replay observes each call before its effects invalidate entry observations.
        let mut output = state.clone();
        output.total_calls.clear();
        output.total_cleanup.clear();

        for operation in block
            .operations()
            .iter()
            .filter_map(|operation| domain.graph.operation(*operation))
        {
            let Some(required) = domain.operation_dependencies(operation, property, &output)?
            else {
                return Ok(Some(ProofFailure::Operation(operation.kind().node())));
            };

            dependencies.extend(required);
            domain.transfer_operation(operation, &mut output)?;
        }

        dependencies.extend(output.dependencies.iter().copied());

        if property != ExecutionProperty::Total {
            continue;
        }

        let abnormal_exit = domain.graph.exits().iter().find(|exit| {
            exit.block() == block.id()
                && !matches!(
                    exit.kind(),
                    AnalysisExitKind::NormalFallthrough
                        | AnalysisExitKind::Return
                        | AnalysisExitKind::ResultErrorPropagation
                        | AnalysisExitKind::Yield
                )
                && !inert_replacement_exit(domain, **exit)
        });

        if let Some(exit) = abnormal_exit {
            return Ok(Some(ProofFailure::AbnormalExit(*exit)));
        }

        for edge in block
            .successors()
            .iter()
            .filter_map(|edge| domain.graph.edge(*edge))
        {
            if matches!(
                edge.kind(),
                AnalysisEdgeKind::LoopBack | AnalysisEdgeKind::LoopContinue
            ) && domain.edge_state(&output, edge)?.reachable
            {
                return Ok(Some(ProofFailure::UnboundedLoop(block.id())));
            }
        }
    }

    Ok(None)
}

fn inert_replacement_exit(domain: &GuaranteeDomain<'_>, exit: AnalysisExit) -> bool {
    let Some(AnyBoundNodeId::Expression(expression)) = exit.origin() else {
        return false;
    };

    if !matches!(
        exit.kind(),
        AnalysisExitKind::Panic | AnalysisExitKind::Cancellation
    ) || !matches!(
        domain.view.expression(expression),
        Some(bray_bound_tree::BoundExpression::Assignment(_))
    ) {
        return false;
    }

    let replacements = domain.asynchronous.replacements();

    replacements
        .binary_search_by_key(
            &expression,
            bray_bound_tree::StorageReplacementPlan::expression,
        )
        .ok()
        .and_then(|index| replacements.get(index))
        .is_some_and(|plan| plan.cleanup() == bray_bound_tree::AsyncStorageCleanupRequirement::None)
}

fn failure_diagnostic<C>(
    request: CheckerUnitView<'_, C>,
    node: Option<AnyBoundNodeId>,
    obligation: Obligation,
    ordinal: usize,
) -> Result<Diagnostic, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let source = node
        .and_then(|node| bound_node_origin(request, node))
        .map_or_else(
            || request.unit().key().source(),
            |origin| origin.source_anchor(),
        );

    let span = request.source(source)?.span();

    let diagnostic = match obligation {
        Obligation::Execution(guarantee) => Diagnostic::new(
            diagnostic_id(ordinal),
            DiagnosticKind::CheckingUnprovenExecutionGuarantee,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::referenced_name(
            guarantee.property().as_str(),
        )),
        Obligation::Postcondition(_) => Diagnostic::new(
            diagnostic_id(ordinal),
            DiagnosticKind::CheckingUnprovenPostcondition,
            SeverityKind::Error,
        ),
    };

    Ok(diagnostic.with_primary_span(span))
}
