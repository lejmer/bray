use bray_bound_tree::{
    CheckedBodySemantics, CheckedControlFlow, CheckedExpressionSemantics, CheckedMemoryOperations,
    CheckedPatterns, StoragePlan,
};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::CallableSignatureQuery;

use crate::analysis::{
    ControlFlowGraphBuildOutcome, analyze_storage_liveness_with_graph,
    build_storage_control_flow_graph, check_refinements_with_graph, check_storage_flow_with_graph,
};
use crate::asynchronous::check_async_analysis_with_graph;
use crate::behavior::collect_body_behavior;
use crate::dependency::check_dependency_contracts;
use crate::unit::semantic_inputs_match;
use crate::{
    CheckerInfrastructureError, CheckerOutcome, CheckerRequestContext,
    CheckerSemanticQueryProvider, CheckerUnitView,
};

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
    if !semantic_inputs_match(
        request,
        [
            (control_flow.unit(), control_flow.kind()),
            (expressions.unit(), expressions.kind()),
            (patterns.unit(), patterns.kind()),
            (storage.unit(), storage.kind()),
            (memory.unit(), memory.kind()),
        ],
    ) {
        return CheckerOutcome::InfrastructureFailure(
            CheckerInfrastructureError::InvalidBodySemantics,
        );
    }

    let graph = match build_storage_control_flow_graph(request, storage, expressions.selections()) {
        ControlFlowGraphBuildOutcome::Complete(graph) => graph,
        ControlFlowGraphBuildOutcome::Cancelled => return CheckerOutcome::Cancelled,
        ControlFlowGraphBuildOutcome::InfrastructureFailure(error) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
    };

    let mut diagnostics = DiagnosticBag::new();

    macro_rules! complete {
        ($outcome:expr) => {
            match $outcome {
                CheckerOutcome::Complete(result) => {
                    let (value, owned) = result.into_parts();

                    diagnostics.add_range(owned);

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

    let liveness = complete!(analyze_storage_liveness_with_graph(
        request,
        expressions.selections(),
        storage,
        memory,
        &graph,
    ));

    let refinements = complete!(
        check_refinements_with_graph(request, patterns, storage, &graph,).with_upstream()
    );

    let flow = complete!(check_storage_flow_with_graph(
        request,
        storage,
        &liveness,
        &refinements,
        memory,
        &graph,
    ));

    let dependencies = complete!(check_dependency_contracts(
        request,
        expressions.selections(),
        storage,
        &flow,
    ));

    let asynchronous = complete!(check_async_analysis_with_graph(
        request,
        expressions.types(),
        expressions.selections(),
        &liveness,
        &dependencies,
        storage,
        &refinements,
        &flow,
        &graph,
    ));

    let behavior = complete!(
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
        Err(_) => {
            return CheckerOutcome::InfrastructureFailure(
                CheckerInfrastructureError::InvalidBodySemantics,
            );
        }
    };

    CheckerOutcome::complete(semantics, diagnostics)
}
