use crate::{CheckerOutcome, UnitCheckConclusions, UnitCheckRequest};

use super::build::{ControlFlowGraphBuildOutcome, build_control_flow_graph};
use super::reachability::analyze_reachability;

pub(crate) fn check_control_flow(
    request: UnitCheckRequest<'_>,
) -> CheckerOutcome<UnitCheckConclusions> {
    let graph = match build_control_flow_graph(request) {
        ControlFlowGraphBuildOutcome::Complete(graph) => graph,
        ControlFlowGraphBuildOutcome::Cancelled => return CheckerOutcome::Cancelled,
    };

    if !graph.is_well_formed() {
        panic!("checker control-flow graph violated its construction invariants");
    }

    if analyze_reachability(&graph, request).is_none() {
        return CheckerOutcome::Cancelled;
    }

    CheckerOutcome::without_diagnostics(UnitCheckConclusions::new(request.view().unit()))
}
