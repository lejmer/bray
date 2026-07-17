use crate::{CheckerOutcome, ControlFlowCheckResult, UnitCheckRequest};

use super::build::{ControlFlowGraphBuildOutcome, build_control_flow_graph};
use super::reachability::analyze_reachability;
use super::refinement::analyze_refinements;

pub(crate) fn check_control_flow(
    request: UnitCheckRequest<'_>,
) -> CheckerOutcome<ControlFlowCheckResult> {
    let build = match build_control_flow_graph(request) {
        ControlFlowGraphBuildOutcome::Complete(build) => build,
        ControlFlowGraphBuildOutcome::Cancelled => return CheckerOutcome::Cancelled,
    };

    let (graph, facts) = (*build).into_parts();

    if !graph.is_well_formed() {
        panic!("checker control-flow graph violated its construction invariants");
    }

    let Some(reachability) = analyze_reachability(&graph, request) else {
        return CheckerOutcome::Cancelled;
    };

    let Some(_refinements) = analyze_refinements(&graph, &reachability, request) else {
        return CheckerOutcome::Cancelled;
    };

    let facts = match facts.finish(request.view(), reachability.completion()) {
        Ok(facts) => facts,
        Err(_) => panic!("checker lowering facts violated their construction invariants"),
    };

    let result = ControlFlowCheckResult::new(facts);

    CheckerOutcome::without_diagnostics(result)
}
