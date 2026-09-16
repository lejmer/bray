use bray_bound_tree::{
    CheckedBodySemantics, CheckedControlFlow, CheckedExpressionSemantics, CheckedMemoryOperations,
    CheckedPatterns, StoragePlan,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::CallableSignatureQuery;

use crate::analysis::{
    ControlFlowGraphBuildOutcome, analyze_storage_liveness_with_graph,
    build_storage_control_flow_graph, check_callable_result, check_refinements_with_graph,
    check_storage_flow_with_graph,
};
use crate::asynchronous::check_async_analysis_with_graph;
use crate::behavior::collect_body_behavior;
use crate::dependency::check_dependency_contracts;
use crate::unit::assert_unit_inputs;
use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext,
    CheckerSemanticQueryProvider, CheckerUnitView,
};

macro_rules! complete {
    ($diagnostics:ident, $outcome:expr) => {
        match $outcome {
            CheckerOutcome::Complete(result) => {
                let (value, owned) = result.into_parts();

                $diagnostics.add_range(owned);

                value
            }
            CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
            CheckerOutcome::InfrastructureFailure(error) => {
                return CheckerOutcome::InfrastructureFailure(error);
            }
            CheckerOutcome::UpstreamFailure(error) => {
                return CheckerOutcome::UpstreamFailure(error);
            }
        }
    };
}

pub(crate) fn check_body_semantics<C>(
    request: CheckerUnitView<'_, C>,
    control_flow: &CheckedControlFlow,
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
            ("control flow", (control_flow.unit(), control_flow.kind())),
            (
                "expression semantics",
                (expressions.unit(), expressions.kind()),
            ),
            ("patterns", (patterns.unit(), patterns.kind())),
            ("storage plan", (storage.unit(), storage.kind())),
            ("memory operations", (memory.unit(), memory.kind())),
        ],
    );

    let graph =
        match build_storage_control_flow_graph(request, storage, expressions.selections(), None) {
            ControlFlowGraphBuildOutcome::Complete(graph) => graph,
            ControlFlowGraphBuildOutcome::Cancelled => return CheckerOutcome::Cancelled,
            ControlFlowGraphBuildOutcome::InfrastructureFailure(error) => {
                return CheckerOutcome::InfrastructureFailure(error);
            }
            ControlFlowGraphBuildOutcome::UpstreamFailure(error) => {
                return CheckerOutcome::UpstreamFailure(error);
            }
        };

    let mut diagnostics = DiagnosticBag::new();

    let semantics = complete!(
        diagnostics,
        check_body_semantics_with_graph(
            request,
            control_flow,
            expressions,
            patterns,
            storage,
            memory,
            &graph,
        )
    );

    complete!(
        diagnostics,
        check_callable_result(
            request,
            control_flow,
            expressions,
            patterns,
            storage,
            memory,
            &semantics,
        )
    );

    CheckerOutcome::complete(semantics, diagnostics)
}

pub(crate) fn check_body_semantics_with_graph<C>(
    request: CheckerUnitView<'_, C>,
    control_flow: &CheckedControlFlow,
    expressions: &CheckedExpressionSemantics,
    patterns: &CheckedPatterns,
    storage: &StoragePlan,
    memory: &CheckedMemoryOperations,
    graph: &crate::analysis::ControlFlowGraph,
) -> CheckerOutcome<CheckedBodySemantics, C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    let mut diagnostics = DiagnosticBag::new();

    let liveness = complete!(
        diagnostics,
        analyze_storage_liveness_with_graph(
            request,
            expressions.selections(),
            expressions.types(),
            patterns,
            storage,
            memory,
            graph,
        )
    );

    let refinements = complete!(
        diagnostics,
        check_refinements_with_graph(request, patterns, expressions.selections(), storage, graph)
            .with_upstream()
    );

    let flow = complete!(
        diagnostics,
        check_storage_flow_with_graph(request, storage, &liveness, &refinements, memory, graph,)
    );

    let dependencies = complete!(
        diagnostics,
        check_dependency_contracts(request, expressions.selections(), patterns, storage, &flow,)
    );

    let asynchronous = complete!(
        diagnostics,
        check_async_analysis_with_graph(
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

    let behavior = complete!(
        diagnostics,
        collect_body_behavior(
            request,
            control_flow,
            expressions.selections(),
            &asynchronous,
        )
        .with_upstream()
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
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::SemanticSnapshot(error),
            );
        }
    };

    CheckerOutcome::complete(semantics, diagnostics)
}
