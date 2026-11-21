use bray_bound_tree::{
    CheckedBodySemantics, CheckedExpressionSemantics, CheckedMemoryOperations, CheckedPatterns,
    StoragePlan,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::CallableSignatureQuery;

use crate::analysis::{
    StorageFlowInput, analyze_storage_liveness_with_graph, check_callable_result, check_storage_flow_with_graph,
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
        prepare_refinements(request, patterns, expressions, storage, graph)
    );

    let mut value_inputs = match crate::dependency::ValueInputs::prepare(
        request,
        expressions.types(),
        expressions.selections(),
        patterns,
        |callable| request.callable_result_dependencies(callable),
    ) {
        Ok(inputs) => inputs,
        Err(error) => return CheckerOutcome::from_query_error(error),
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

    if let Err(error) = value_inputs.collect_contract_copies(request, &dependencies) {
        return CheckerOutcome::from_query_error(error);
    }

    let disposal = crate::analysis::guarantee::Cleanup::new(&prepared.cleanup, &dependencies, request.semantic_values());

    let completion_contracts = std::collections::BTreeMap::new();

    let effects = crate::analysis::liveness::OperationEffects::from_checked_inputs(
        request,
        expressions,
        value_inputs,
        &dependencies,
        storage,
        memory,
        graph,
        &prepared.cleanup,
    );

    let (mut storage_inputs, preparation_diagnostics) = match StorageFlowInput::prepare(
        &mut cleanup_shapes,
        request,
        storage,
        &dependencies,
        &effects,
        expressions.selections(),
        expressions.types(),
        &prepared.cleanup,
        memory,
    ) {
        Ok(prepared) => prepared,
        Err(error) => return CheckerOutcome::from_query_error(error),
    };

    let mut excluded = std::collections::BTreeSet::new();

    crate::analysis::exclude_impossible_call_outcomes(graph, request, &mut excluded);

    crate::analysis::exclude_infallible_cleanup_edges(
        graph, request.view(), storage, &prepared.cleanup, &dependencies, &mut excluded,
    );

    let Some(reachability) = crate::analysis::analyze_reachability_excluding(graph, request, &excluded) else {
        return CheckerOutcome::Cancelled;
    };

    let (liveness, edge_lifetime_ends) = complete!(
        diagnostics,
        analyze_storage_liveness_with_graph(request, &effects, storage, graph, &reachability,)
    );

    let execution = crate::analysis::guarantee::prepare_execution_flow(
        prepared,
        request,
        expressions,
        std::borrow::Cow::Borrowed(&[]),
        storage,
        memory,
        &completion_contracts,
        &disposal,
        &dependencies,
        &liveness,
    );

    diagnostics.add_range(preparation_diagnostics);

    let (flow, refinements, fallthrough) = complete!(
        diagnostics,
        check_storage_flow_with_graph(
            &mut storage_inputs,
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
            &excluded,
        )
    );

    let asynchronous = complete!(
        diagnostics,
        check_async_analysis_with_graph(
            &mut cleanup_shapes,
            request,
            expressions.selections(),
            &liveness,
            storage,
            &flow,
            graph,
            &prepared.cleanup,
        )
    );

    complete!(
        diagnostics,
        check_callable_result(request, expressions, fallthrough)
    );

    let implicit_calls = asynchronous.suspensions().iter()
        .filter(|point| !matches!(point.kind(), bray_bound_tree::AsyncSuspensionKind::Await { .. }))
        .flat_map(|point| point.deferred_calls());

    let behavior = behavior.clone().with_deferred_calls(flow.deferred_calls().chain(implicit_calls).cloned());

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
