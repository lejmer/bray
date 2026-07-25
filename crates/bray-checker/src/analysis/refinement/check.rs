use std::collections::BTreeMap;

use bray_bound_tree::{
    AnyBoundNodeId, CheckedPatternFacts, CheckedRefinementFacts, RefinementOccurrence, StoragePlan,
};
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};

use crate::{CheckerOutcome, CheckerRequestContext, CheckerUnitView};

use super::super::build::{ControlFlowGraphBuildOutcome, build_control_flow_graph};
use super::super::fixed_point::{
    FixedPointDomain, FixedPointOutcome, FlowDirection, solve_fixed_point,
};
use super::super::model::{
    AnalysisBlock, AnalysisEdge, AnalysisEdgeKind, AnalysisOperation, AnalysisOperationKind,
    AnalysisScopeExitPhase, ControlFlowGraph,
};
use super::super::reachability::{ReachabilityResult, analyze_reachability};
use super::set::FactSet;
use super::universe::{RefinementUniverse, RefinementUniverseError};

pub(crate) fn check_refinements<C>(
    request: CheckerUnitView<'_, C>,
    patterns: &CheckedPatternFacts,
    storage: &StoragePlan,
) -> CheckerOutcome<CheckedRefinementFacts>
where
    C: CheckerRequestContext + ?Sized,
{
    if patterns.unit() != request.view().unit()
        || patterns.kind() != request.view().kind()
        || storage.unit() != request.view().unit()
        || storage.kind() != request.view().kind()
    {
        return CheckerOutcome::InfrastructureFailure(
            crate::CheckerInfrastructureError::InvalidRefinementInput,
        );
    }

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

    let universe = match RefinementUniverse::new(&graph, request, patterns, storage) {
        Ok(universe) => universe,
        Err(RefinementUniverseError::CapacityExceeded) => {
            return capacity_recovery(request);
        }
        Err(RefinementUniverseError::Cancelled) => return CheckerOutcome::Cancelled,
    };

    let result = match analyze_refinements(&graph, &reachability, &universe, storage, request) {
        Some(result) => result,
        None => return CheckerOutcome::Cancelled,
    };

    let occurrences = result.occurrences(&graph, storage);

    let is_recovered = graph
        .operations()
        .iter()
        .any(|operation| matches!(operation.kind(), AnalysisOperationKind::Recovery(_)));

    let facts = CheckedRefinementFacts::try_new(
        graph.unit(),
        request.view().kind(),
        occurrences,
        is_recovered,
    )
    .unwrap_or_else(|error| panic!("checker produced invalid refinement facts: {error:?}"));

    CheckerOutcome::without_diagnostics(facts)
}

fn capacity_recovery<C>(request: CheckerUnitView<'_, C>) -> CheckerOutcome<CheckedRefinementFacts>
where
    C: CheckerRequestContext + ?Sized,
{
    let source = match request.source(request.unit().key().source()) {
        Ok(source) => source,
        Err(error) => return CheckerOutcome::InfrastructureFailure(error),
    };

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingRefinementCapacityExceeded,
        SeverityKind::Error,
    )
    .with_primary_span(source.span());

    let facts =
        CheckedRefinementFacts::try_new(request.view().unit(), request.view().kind(), [], true)
            .unwrap_or_else(|error| {
                panic!("empty recovery refinement facts must be valid: {error:?}")
            });

    CheckerOutcome::complete(facts, DiagnosticBag::single(diagnostic))
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
        let mut occurrence_facts = BTreeMap::new();

        for block in graph.blocks() {
            let Some(mut state) = self.result.state(block.id()).cloned() else {
                continue;
            };

            for operation in block.operations() {
                let Some(operation) = graph.operation(*operation) else {
                    continue;
                };

                match occurrence_facts.entry(operation.kind().node()) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(state.facts.clone());
                    }
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        entry.get_mut().intersect(&state.facts);
                    }
                }

                transfer_operation(&mut state, operation, self.universe, storage);
            }
        }

        occurrence_facts
            .into_iter()
            .filter(|(_, facts)| !facts.is_empty())
            .map(|(node, facts)| {
                RefinementOccurrence::new(node, self.universe.active_facts(&facts))
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
    facts: FactSet,
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
            facts: FactSet::empty(self.universe.len()),
        }
    }

    fn initial(&self, block: &AnalysisBlock) -> Self::State {
        if self.reachability.is_block_reachable(block.id()) {
            RefinementState {
                facts: FactSet::full(self.universe.len()),
            }
        } else {
            self.bottom()
        }
    }

    fn boundary(&self) -> Self::State {
        self.bottom()
    }

    fn merge_boundary(&self, target: &mut Self::State, boundary: &Self::State) -> bool {
        target.facts.replace(&boundary.facts)
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
            incoming.facts.clear();
        } else if let Some(refinement) = edge.refinement() {
            self.universe
                .insert_refinement(&mut incoming.facts, refinement);
        }

        target.facts.intersect(&incoming.facts)
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
        AnalysisOperationKind::Recovery(_) => state.facts.clear(),
        AnalysisOperationKind::ScopeExit {
            phase: AnalysisScopeExitPhase::LifecycleResolution,
            ..
        } => {}
        AnalysisOperationKind::Bound(node) => {
            universe.invalidate_for_operation(&mut state.facts, node, storage);
            universe.finish_operation(&mut state.facts, node);
        }
        AnalysisOperationKind::DirectAwait(expression)
        | AnalysisOperationKind::TaskOperation { expression, .. } => {
            let node = AnyBoundNodeId::Expression(expression);

            universe.invalidate_for_operation(&mut state.facts, node, storage);
            universe.finish_operation(&mut state.facts, node);
        }
        AnalysisOperationKind::ScopeExit {
            phase: AnalysisScopeExitPhase::TaskCancellationBroadcast,
            ..
        } => {}
    }
}
