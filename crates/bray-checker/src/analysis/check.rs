use crate::{CheckerOutcome, CheckerRequestContext, CheckerUnitView, ControlFlowCheckResult};

use super::build::{ControlFlowGraphBuildOutcome, build_control_flow_graph};
use super::reachability::analyze_reachability;

pub(crate) fn check_control_flow<C>(
    request: CheckerUnitView<'_, C>,
) -> CheckerOutcome<ControlFlowCheckResult>
where
    C: CheckerRequestContext + ?Sized,
{
    let graph = match build_control_flow_graph(request) {
        ControlFlowGraphBuildOutcome::Complete(graph) => graph,
        ControlFlowGraphBuildOutcome::Cancelled => return CheckerOutcome::Cancelled,
    };

    if !graph.is_well_formed() {
        panic!("checker control-flow graph violated its construction invariants");
    }

    let Some(reachability) = analyze_reachability(&graph, request) else {
        return CheckerOutcome::Cancelled;
    };

    let result = ControlFlowCheckResult::new(
        graph.unit(),
        request.view().kind(),
        reachability.completion(),
    );

    CheckerOutcome::without_diagnostics(result)
}
