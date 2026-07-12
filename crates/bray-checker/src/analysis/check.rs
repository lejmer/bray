use crate::{CheckerOutcome, UnitCheckConclusions, UnitCheckRequest};

use super::build::{TopologyBuildOutcome, build_topology};
use super::reachability::analyze_reachability;

pub(crate) fn check_topology(
    request: UnitCheckRequest<'_>,
) -> CheckerOutcome<UnitCheckConclusions> {
    let topology = match build_topology(request) {
        TopologyBuildOutcome::Complete(topology) => topology,
        TopologyBuildOutcome::Cancelled => return CheckerOutcome::Cancelled,
    };

    if !topology.is_well_formed() {
        panic!("checker analysis topology violated its construction invariants");
    }

    if analyze_reachability(&topology, request).is_none() {
        return CheckerOutcome::Cancelled;
    }

    CheckerOutcome::without_diagnostics(UnitCheckConclusions::new(request.view().unit()))
}
