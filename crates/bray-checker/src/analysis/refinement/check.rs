use std::collections::BTreeMap;

use bray_bound_tree::{
    AnyBoundNodeId, CheckedPatterns, CheckedRefinements, CheckedSemanticSelections,
    RefinementOccurrence, StoragePlan,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticLabel,
    DiagnosticLabelKind, DiagnosticRefinementCapacity, SeverityKind,
};

use crate::unit::semantic_input_failure;
use crate::{CheckerInputKind, CheckerOutcome, CheckerRequestContext, CheckerUnitView};

use super::super::build::{ControlFlowGraphBuildOutcome, build_storage_control_flow_graph};
use super::super::fixed_point::{
    FixedPointDomain, FixedPointOutcome, FlowDirection, solve_fixed_point,
};
use super::super::model::{
    AnalysisBlock, AnalysisCallPhase, AnalysisEdge, AnalysisEdgeKind, AnalysisOperation,
    AnalysisOperationKind, AnalysisScopeExitPhase, ControlFlowGraph,
};
use super::super::reachability::{ReachabilityResult, analyze_reachability};
use super::set::RefinementSet;
use super::universe::{RefinementUniverse, RefinementUniverseError};

pub(crate) fn check_refinements<C>(
    request: CheckerUnitView<'_, C>,
    patterns: &CheckedPatterns,
    selections: &CheckedSemanticSelections,
    storage: &StoragePlan,
) -> CheckerOutcome<CheckedRefinements, C::UpstreamError>
where
    C: CheckerRequestContext + ?Sized,
{
    if let Some(error) = semantic_input_failure(
        request,
        [
            (
                CheckerInputKind::Patterns,
                (patterns.unit(), patterns.kind()),
            ),
            (
                CheckerInputKind::SemanticSelections,
                (selections.unit(), selections.kind()),
            ),
            (
                CheckerInputKind::StoragePlan,
                (storage.unit(), storage.kind()),
            ),
        ],
    ) {
        return CheckerOutcome::InfrastructureFailure(error);
    }

    let graph = match build_storage_control_flow_graph(request, storage, selections) {
        ControlFlowGraphBuildOutcome::Complete(graph) => graph,
        ControlFlowGraphBuildOutcome::Cancelled => return CheckerOutcome::Cancelled,
        ControlFlowGraphBuildOutcome::InfrastructureFailure(error) => {
            return CheckerOutcome::InfrastructureFailure(error);
        }
        ControlFlowGraphBuildOutcome::UpstreamFailure(error) => {
            return CheckerOutcome::UpstreamFailure(error);
        }
    };

    check_refinements_with_graph(request, patterns, storage, &graph).with_upstream()
}

pub(crate) fn check_refinements_with_graph<C>(
    request: CheckerUnitView<'_, C>,
    patterns: &CheckedPatterns,
    storage: &StoragePlan,
    graph: &ControlFlowGraph,
) -> CheckerOutcome<CheckedRefinements>
where
    C: CheckerRequestContext + ?Sized,
{
    if !graph.is_well_formed() {
        panic!("checker control-flow graph violated its construction invariants");
    }

    let Some(reachability) = analyze_reachability(graph, request) else {
        return CheckerOutcome::Cancelled;
    };

    let universe = match RefinementUniverse::new(graph, request, patterns, storage) {
        Ok(universe) => universe,
        Err(RefinementUniverseError::CapacityExceeded(capacity)) => {
            return capacity_recovery(request, capacity);
        }
        Err(RefinementUniverseError::CountUnrepresentable) => {
            return CheckerOutcome::InfrastructureFailure(
                crate::CheckerInfrastructureError::RefinementCapacityUnrepresentable,
            );
        }
        Err(RefinementUniverseError::AllocationFailed) => {
            return CheckerOutcome::InfrastructureFailure(
                crate::CheckerInfrastructureError::RefinementStorageUnavailable,
            );
        }
        Err(RefinementUniverseError::Cancelled) => return CheckerOutcome::Cancelled,
    };

    let result = match analyze_refinements(graph, &reachability, &universe, storage, request) {
        Some(result) => result,
        None => return CheckerOutcome::Cancelled,
    };

    let occurrences = result.occurrences(graph, storage);

    let is_recovered = graph
        .operations()
        .iter()
        .any(|operation| matches!(operation.kind(), AnalysisOperationKind::Recovery(_)));

    let refinements = CheckedRefinements::try_new(
        graph.unit(),
        request.view().kind(),
        occurrences,
        is_recovered,
    )
    .unwrap_or_else(|error| panic!("checker produced invalid refinements: {error:?}"));

    CheckerOutcome::without_diagnostics(refinements)
}

fn capacity_recovery<C>(
    request: CheckerUnitView<'_, C>,
    capacity: DiagnosticRefinementCapacity,
) -> CheckerOutcome<CheckedRefinements>
where
    C: CheckerRequestContext + ?Sized,
{
    let source = match request.source(request.unit().key().source()) {
        Ok(source) => source,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    let diagnostic = capacity_diagnostic(source.span(), capacity);

    let refinements =
        CheckedRefinements::try_new(request.view().unit(), request.view().kind(), [], true)
            .unwrap_or_else(|error| panic!("empty recovery refinements must be valid: {error:?}"));

    CheckerOutcome::complete(refinements, DiagnosticBag::single(diagnostic))
}

fn capacity_diagnostic(
    span: bray_source::SourceSpan,
    capacity: DiagnosticRefinementCapacity,
) -> Diagnostic {
    Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingRefinementCapacityExceeded,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::RefinementCapacityExceeded,
        span,
    ))
    .with_arg(DiagnosticArg::refinement_capacity(capacity))
}

struct RefinementResult<'universe> {
    result: super::super::fixed_point::FixedPointResult<RefinementState>,
    universe: &'universe RefinementUniverse,
}

impl RefinementResult<'_> {
    fn occurrences(
        &self,
        graph: &ControlFlowGraph,
        storage: &StoragePlan,
    ) -> Vec<RefinementOccurrence> {
        let mut occurrence_refinements = BTreeMap::new();

        for block in graph.blocks() {
            let Some(mut state) = self.result.state(block.id()).cloned() else {
                continue;
            };

            for operation in block.operations() {
                let Some(operation) = graph.operation(*operation) else {
                    continue;
                };

                match occurrence_refinements.entry(operation.kind().point()) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(state.refinements.clone());
                    }
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        entry.get_mut().intersect(&state.refinements);
                    }
                }

                transfer_operation(&mut state, operation, self.universe, storage);
            }
        }

        occurrence_refinements
            .into_iter()
            .filter(|(_, refinements)| !refinements.is_empty())
            .map(|(point, refinements)| {
                RefinementOccurrence::at(point, self.universe.active_refinements(&refinements))
            })
            .collect()
    }
}

fn analyze_refinements<'universe, C>(
    graph: &ControlFlowGraph,
    reachability: &ReachabilityResult,
    universe: &'universe RefinementUniverse,
    storage: &StoragePlan,
    request: CheckerUnitView<'_, C>,
) -> Option<RefinementResult<'universe>>
where
    C: CheckerRequestContext + ?Sized,
{
    let domain = RefinementDomain {
        graph,
        reachability,
        universe,
        storage,
    };

    match solve_fixed_point(graph, &domain, &request) {
        FixedPointOutcome::Complete(result) => Some(RefinementResult { result, universe }),
        FixedPointOutcome::Cancelled => None,
        FixedPointOutcome::ConvergenceInvariantViolated => {
            panic!("finite refinement analysis exceeded its convergence bound")
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct RefinementState {
    refinements: RefinementSet,
}

struct RefinementDomain<'analysis> {
    graph: &'analysis ControlFlowGraph,
    reachability: &'analysis ReachabilityResult,
    universe: &'analysis RefinementUniverse,
    storage: &'analysis StoragePlan,
}

impl FixedPointDomain for RefinementDomain<'_> {
    type State = RefinementState;

    fn direction(&self) -> FlowDirection {
        FlowDirection::Forward
    }

    fn bottom(&self) -> Self::State {
        RefinementState {
            refinements: RefinementSet::empty(self.universe.len()),
        }
    }

    fn initial(&self, block: &AnalysisBlock) -> Self::State {
        if self.reachability.is_block_reachable(block.id()) {
            RefinementState {
                refinements: RefinementSet::full(self.universe.len()),
            }
        } else {
            self.bottom()
        }
    }

    fn boundary(&self) -> Self::State {
        self.bottom()
    }

    fn merge_boundary(&self, target: &mut Self::State, boundary: &Self::State) -> bool {
        target.refinements.replace(&boundary.refinements)
    }

    fn transfer(&self, block: &AnalysisBlock, source: &Self::State) -> Self::State {
        self.transfer(block, source)
    }

    fn propagate(
        &self,
        source: &Self::State,
        edge: &AnalysisEdge,
        target: &mut Self::State,
    ) -> bool {
        if !self.reachability.is_edge_reachable(edge.id()) {
            return false;
        }

        let mut incoming = source.clone();

        if edge.kind() == AnalysisEdgeKind::Recovery {
            incoming.refinements.clear();
        } else if let Some(refinement) = edge.refinement() {
            self.universe
                .insert_refinement(&mut incoming.refinements, refinement);
        }

        target.refinements.intersect(&incoming.refinements)
    }

    fn convergence_bound(&self, _: &ControlFlowGraph) -> usize {
        self.graph
            .blocks()
            .len()
            .saturating_mul(self.universe.len().saturating_add(1))
    }
}

impl RefinementDomain<'_> {
    fn transfer(&self, block: &AnalysisBlock, source: &RefinementState) -> RefinementState {
        let mut incoming = source.clone();

        for operation in block.operations() {
            let Some(operation) = self.graph.operation(*operation) else {
                continue;
            };

            transfer_operation(&mut incoming, operation, self.universe, self.storage);
        }

        incoming
    }
}

fn transfer_operation(
    state: &mut RefinementState,
    operation: &AnalysisOperation,
    universe: &RefinementUniverse,
    storage: &StoragePlan,
) {
    match operation.kind() {
        AnalysisOperationKind::Recovery(_) => state.refinements.clear(),
        AnalysisOperationKind::PatternObservation(_) => {}
        AnalysisOperationKind::ScopeExit {
            phase: AnalysisScopeExitPhase::LifecycleResolution,
            ..
        } => {}
        AnalysisOperationKind::Bound(_)
        | AnalysisOperationKind::Suspension { .. }
        | AnalysisOperationKind::TaskOperation { .. } => {
            universe.invalidate_for_operation(
                &mut state.refinements,
                operation.kind().point(),
                storage,
            );

            universe.finish_operation(&mut state.refinements, operation.kind().node());
        }
        AnalysisOperationKind::Call {
            phase: AnalysisCallPhase::Attempt,
            ..
        }
        | AnalysisOperationKind::PropagationFailure(_) => {
            universe.invalidate_for_operation(
                &mut state.refinements,
                operation.kind().point(),
                storage,
            );
        }
        AnalysisOperationKind::Call {
            expression,
            phase: AnalysisCallPhase::Completion,
        } => {
            universe.finish_operation(
                &mut state.refinements,
                AnyBoundNodeId::Expression(expression),
            );
        }
        AnalysisOperationKind::ScopeExit {
            phase: AnalysisScopeExitPhase::TaskCancellationBroadcast,
            ..
        } => {}
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticRefinementCapacity,
        DiagnosticRefinementCapacitySurface, SeverityKind,
    };
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};
    use bray_testing::assert_goal_state_diagnostic_kind;

    use super::super::universe::{MAX_REFINEMENT_CELLS, MAX_REFINEMENT_REFINEMENTS};
    use super::capacity_diagnostic;

    #[test]
    fn refinement_capacity_recovery_preserves_each_exact_configured_surface() {
        let span = SourceSpan::new(
            SourceId::new(1),
            TextRange::new(TextSize::new(2), TextSize::new(3)),
        );

        let capacities = [
            capacity(
                DiagnosticRefinementCapacitySurface::RefinementEntries,
                MAX_REFINEMENT_REFINEMENTS,
            ),
            capacity(
                DiagnosticRefinementCapacitySurface::RetainedStateCells,
                MAX_REFINEMENT_CELLS,
            ),
            capacity(
                DiagnosticRefinementCapacitySurface::PublishedRefinements,
                MAX_REFINEMENT_CELLS,
            ),
        ];

        for capacity in capacities {
            let diagnostic = capacity_diagnostic(span, capacity);

            assert_eq!(
                diagnostic.kind(),
                DiagnosticKind::CheckingRefinementCapacityExceeded
            );

            assert_eq!(diagnostic.severity(), SeverityKind::Error);
            assert_eq!(diagnostic.primary_span(), Some(span));

            assert_eq!(
                diagnostic.args(),
                [DiagnosticArg::refinement_capacity(capacity)]
            );

            assert_goal_state_diagnostic_kind(
                &DiagnosticBag::single(diagnostic),
                DiagnosticKind::CheckingRefinementCapacityExceeded,
            );
        }
    }

    fn capacity(
        surface: DiagnosticRefinementCapacitySurface,
        maximum: usize,
    ) -> DiagnosticRefinementCapacity {
        let maximum = u64::try_from(maximum)
            .unwrap_or_else(|_| panic!("refinement capacity maximum must fit the protocol"));

        DiagnosticRefinementCapacity::try_new(surface, maximum + 1, maximum)
            .unwrap_or_else(|| panic!("limit plus one must be a capacity violation"))
    }
}
