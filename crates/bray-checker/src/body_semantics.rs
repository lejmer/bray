use bray_bound_tree::{
    CheckedBodySemantics, CheckedExpressionSemantics, CheckedMemoryOperations, CheckedPatterns,
    StoragePlan,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::CallableSignatureQuery;

use crate::analysis::{
    analyze_storage_liveness_with_graph, check_callable_result, check_storage_flow_with_graph,
    prepare_refinements,
};
use crate::asynchronous::check_async_analysis_with_graph;
use crate::dependency::check_dependency_contracts;
use crate::unit::assert_unit_inputs;
use crate::{CheckerOutcome, CheckerRequestContext, CheckerSemanticQueryProvider, CheckerUnitView};

macro_rules! complete {
    ($diagnostics:ident, $outcome:expr) => {
        match $outcome {
            CheckerOutcome::Complete(result) => {
                let (value, owned) = result.into_parts();

                $diagnostics.add_range(owned);

                value
            }
            CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,

            CheckerOutcome::UpstreamFailure(error) => {
                return CheckerOutcome::UpstreamFailure(error);
            }
        }
    };
}

pub(crate) fn check_body_semantics<C>(
    request: CheckerUnitView<'_, C>,
    prepared: &crate::PreparedBodyAnalysis,
    behavior: &bray_bound_tree::BodyBehaviorContributions,
    expressions: &CheckedExpressionSemantics,
    patterns: &CheckedPatterns,
    storage: &StoragePlan,
    memory: &CheckedMemoryOperations,
) -> CheckerOutcome<CheckedBodySemantics, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    assert_unit_inputs(
        request,
        [
            (
                "prepared analysis",
                (prepared.graph.unit(), prepared.kind()),
            ),
            ("selected behavior", (behavior.unit(), behavior.kind())),
            (
                "expression semantics",
                (expressions.unit(), expressions.kind()),
            ),
            ("patterns", (patterns.unit(), patterns.kind())),
            ("storage plan", (storage.unit(), storage.kind())),
            ("memory operations", (memory.unit(), memory.kind())),
        ],
    );

    let mut cleanup_shapes = prepared.cleanup_shapes(request);
    let graph = &prepared.graph;

    let mut diagnostics = DiagnosticBag::new();

    let refinement_universe = complete!(
        diagnostics,
        prepare_refinements(request, patterns, expressions, storage, graph, memory)
    );

    let value_inputs = match crate::dependency::ValueInputs::prepare(
        request,
        expressions.types(),
        expressions.selections(),
        patterns,
        |callable| request.callable_result_dependencies(callable),
    ) {
        Ok(inputs) => inputs,
        Err(crate::CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(crate::CheckerQueryError::Upstream(error)) => {
            return CheckerOutcome::UpstreamFailure(error);
        }
    };

    let dependencies = complete!(
        diagnostics,
        check_dependency_contracts(
            request,
            prepared,
            expressions.selections(),
            &value_inputs,
            storage
        )
    );

    let disposal = crate::analysis::guarantee::Cleanup::new(&prepared.cleanup, &dependencies);

    let completion_contracts = std::collections::BTreeMap::new();

    let execution = crate::analysis::guarantee::prepare_execution_flow(
        prepared,
        request,
        expressions,
        &[],
        storage,
        memory,
        &refinement_universe.invalidating_accesses,
        &completion_contracts,
        &disposal,
        &dependencies,
    );

    let effects = crate::analysis::liveness::OperationEffects::from_checked_inputs(
        request,
        expressions.selections(),
        value_inputs,
        &dependencies,
        storage,
        memory,
    );

    let mut excluded = std::collections::BTreeSet::new();

    crate::analysis::exclude_non_unwinding_edges(graph, request, &mut excluded);

    let (liveness, flow, refinements, asynchronous, completion) = loop {
        let Some(reachability) =
            crate::analysis::analyze_reachability_excluding(graph, request, &excluded)
        else {
            return CheckerOutcome::Cancelled;
        };

        let mut round_diagnostics = DiagnosticBag::new();

        let (liveness, edge_lifetime_ends) = complete!(
            round_diagnostics,
            analyze_storage_liveness_with_graph(request, &effects, storage, graph, &reachability,)
        );

        let (flow, refinements) = complete!(
            round_diagnostics,
            check_storage_flow_with_graph(
                &mut cleanup_shapes,
                request,
                &execution,
                &dependencies,
                effects.value_inputs(),
                patterns,
                storage,
                &liveness,
                &refinement_universe,
                memory,
                graph,
                &reachability,
                &edge_lifetime_ends,
            )
        );

        let asynchronous = complete!(
            round_diagnostics,
            check_async_analysis_with_graph(
                &mut cleanup_shapes,
                request,
                expressions.types(),
                expressions.selections(),
                &liveness,
                &dependencies,
                storage,
                &refinements,
                &flow,
                graph,
            )
        );

        if !crate::analysis::exclude_cleanup_free_edges(graph, &asynchronous, &mut excluded) {
            diagnostics.add_range(round_diagnostics);

            break (
                liveness,
                flow,
                refinements,
                asynchronous,
                reachability.completion(),
            );
        }

        // Each change removes at least one impossible cleanup edge. Liveness depends
        // on that feasibility, so recompute it before solving the correlated flow again.
        // Structural selection, dependencies, effects and the graph remain prepared once.
    };

    complete!(
        diagnostics,
        check_callable_result(request, expressions, completion.can_complete_normally())
    );

    let behavior = behavior.clone().with_deferred_calls(
        asynchronous
            .suspensions()
            .iter()
            .flat_map(|suspension| suspension.deferred_calls().iter().cloned()),
    );

    let semantics = match CheckedBodySemantics::try_new(
        liveness,
        refinements,
        flow,
        dependencies,
        asynchronous,
        behavior,
    ) {
        Ok(semantics) => semantics,
        Err(error) => {
            panic!(
                "Correlated semantic results describe different bound units or unit categories. in check_body_semantics, value0: {:?}",
                error
            );
        }
    };

    CheckerOutcome::complete(semantics, diagnostics)
}
