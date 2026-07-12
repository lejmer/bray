use crate::outcome::WholeUnitFlowConclusions;
use crate::{CheckerOutcome, UnitCheckConclusions, UnitCheckRequest};

use super::build::{ControlFlowGraphBuildOutcome, build_control_flow_graph};
use super::reachability::analyze_reachability;
use super::refinement::analyze_refinements;

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

    let Some(reachability) = analyze_reachability(&graph, request) else {
        return CheckerOutcome::Cancelled;
    };

    let Some(_refinements) = analyze_refinements(&graph, &reachability, request) else {
        return CheckerOutcome::Cancelled;
    };

    let flow = WholeUnitFlowConclusions::new(
        graph.unit(),
        reachability.completion(),
        reachability.is_recovered(),
    );

    let conclusions = UnitCheckConclusions::new(request.view().kind(), flow);

    CheckerOutcome::without_diagnostics(conclusions)
}
