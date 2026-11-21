use super::{StorageFlowInput, StorageFlowState};
use super::outcomes::StorageFlowOutcomes;
use crate::analysis::outcomes::{cleanup_successor, edge_path};
use crate::analysis::fixed_point::{FixedPointDomain, FlowDirection};
use crate::analysis::guarantee::ExecutionJoin;
use crate::analysis::id::{AnalysisBlockId, AnalysisEdgeId};
use crate::analysis::model::{AnalysisBlock, AnalysisEdge, AnalysisEdgeKind, ControlFlowGraph};
use crate::analysis::reachability::ReachabilityResult;
use crate::analysis::storage_flow::check::{BlockPublication, FailureEdgeEffects, StorageFlowCollector};
use crate::{CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{BoundDependencySubject, CheckedMemoryOperations, Liveness, StoragePlan};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::analysis::storage_flow) struct StorageFlowDomain<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    graph: &'analysis ControlFlowGraph,
    terminal_outputs: BTreeSet<AnalysisBlockId>,
    entry: RefCell<Option<StorageFlowState>>,
    failure_effects: RefCell<BTreeMap<AnalysisEdgeId, Vec<FailureEdgeEffects>>>,
    reached_cleanup_edges: RefCell<BTreeSet<AnalysisEdgeId>>,
    cyclic_publications: RefCell<BTreeMap<AnalysisBlockId, (StorageFlowCollector<'analysis, C>, BlockPublication)>>,
    edge_lifetime_ends: &'analysis BTreeMap<AnalysisEdgeId, Vec<BoundDependencySubject>>,
    reachability: &'analysis ReachabilityResult,
    storage: &'analysis StoragePlan,
    liveness: &'analysis Liveness,
    refinements: &'analysis crate::analysis::refinement::RefinementUniverse,
    memory: &'analysis CheckedMemoryOperations,
    input: &'analysis StorageFlowInput,
    request: CheckerUnitView<'analysis, C>,
    execution: &'analysis crate::analysis::guarantee::ExecutionFlowDomain<'analysis, 'analysis, C>,
    dependencies: &'analysis bray_bound_tree::CheckedDependencyContracts,
    values: &'analysis crate::dependency::ValueInputs,
    patterns: &'analysis bray_bound_tree::CheckedPatterns,
}

impl<'analysis, C> StorageFlowDomain<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    #[expect(
        clippy::too_many_arguments,
        reason = "the domain borrows each independently validated analysis input"
    )]
    pub(in crate::analysis::storage_flow) fn new(
        graph: &'analysis ControlFlowGraph,
        entry: StorageFlowState,
        edge_lifetime_ends: &'analysis BTreeMap<AnalysisEdgeId, Vec<BoundDependencySubject>>,
        reachability: &'analysis ReachabilityResult,
        storage: &'analysis StoragePlan,
        liveness: &'analysis Liveness,
        refinements: &'analysis crate::analysis::refinement::RefinementUniverse,
        memory: &'analysis CheckedMemoryOperations,
        input: &'analysis StorageFlowInput,
        request: CheckerUnitView<'analysis, C>,
        execution: &'analysis crate::analysis::guarantee::ExecutionFlowDomain<
            'analysis,
            'analysis,
            C,
        >,
        dependencies: &'analysis bray_bound_tree::CheckedDependencyContracts,
        values: &'analysis crate::dependency::ValueInputs,
        patterns: &'analysis bray_bound_tree::CheckedPatterns,
    ) -> Self {
        Self {
            graph,
            terminal_outputs: graph.exits().iter().map(|exit| exit.block()).collect(),
            entry: RefCell::new(Some(entry)),
            failure_effects: RefCell::default(),
            reached_cleanup_edges: RefCell::default(),
            cyclic_publications: RefCell::default(),
            edge_lifetime_ends,
            reachability,
            storage,
            liveness,
            refinements,
            memory,
            input,
            request,
            execution,
            dependencies,
            values,
            patterns,
        }
    }

    pub(in crate::analysis::storage_flow) fn collector(&self, publish: bool) -> StorageFlowCollector<'analysis, C> {
        StorageFlowCollector::new(
            self.request,
            self.storage,
            self.liveness,
            self.refinements,
            self.memory,
            self.input,
            self.execution,
            self.dependencies,
            self.values,
            self.patterns,
            publish,
        )
    }

    pub(in crate::analysis::storage_flow) fn take_cyclic_publications(
        &mut self,
    ) -> BTreeMap<AnalysisBlockId, (StorageFlowCollector<'analysis, C>, BlockPublication)> {
        std::mem::take(self.cyclic_publications.get_mut())
    }

    pub(in crate::analysis::storage_flow) fn into_edge_outcomes(
        self,
    ) -> (BTreeMap<AnalysisEdgeId, Vec<FailureEdgeEffects>>, BTreeSet<AnalysisEdgeId>) {
        (self.failure_effects.into_inner(), self.reached_cleanup_edges.into_inner())
    }

    fn propagate_state(
        &self,
        source: Cow<'_, StorageFlowState>,
        edge: &AnalysisEdge,
        row: Option<usize>,
    ) -> Option<(StorageFlowState, Option<FailureEdgeEffects>)> {
        let cleanup = cleanup_successor(source.cleanup.as_ref(), edge)?;

        let recovered = source.recovered;

        let (mut incoming, failure_effects) = match edge.refinement() {
            Some(crate::analysis::model::AnalysisRefinement::InvocationFailure(occurrence, kind)) => {
                let mut collector = self.collector(false);

                collector.report_boundary = self.graph.block(edge.source()).expect("call source exists").report_boundary();

                let (state, record) = collector.failure_edge_state(&source, edge.id(), occurrence, kind, row)?;

                (state, Some(record))
            }
            _ => (source.into_owned(), None),
        };

        incoming.cleanup = cleanup;

        if edge.kind() != AnalysisEdgeKind::Suspension {
            incoming.failure_entry = None;
        }

        if !incoming.reachable
            || !incoming
                .execution
                .as_mut()
                .is_some_and(|state| self.execution.refine_edge(state, edge))
        {
            return None;
        }

        if edge.kind() == AnalysisEdgeKind::Recovery {
            incoming.recovered = true;
            incoming.refinements.clear();
        } else if let Some(refinement) = edge.refinement() {
            self.refinements
                .insert_refinement(&mut incoming.refinements, refinement);

            if let Some(execution) = incoming.execution.as_mut()
                && !self
                    .execution
                    .assume_patterns(execution, self.refinements.edge_facts(refinement))
            {
                return None;
            }
        }

        use crate::analysis::model::AnalysisPropagationOutcome;

        if let Some(crate::analysis::model::AnalysisRefinement::PropagationOutcome { expression, outcome }) = edge.refinement()
            && matches!(outcome, AnalysisPropagationOutcome::Failure | AnalysisPropagationOutcome::RunFailure(_))
        {
            let mut collector = self.collector(false);

            match outcome {
                AnalysisPropagationOutcome::Failure => collector.propagate_reference_result(&mut incoming, expression),
                AnalysisPropagationOutcome::RunFailure(bray_symbols::DependencyFailureKind::Panic) => {
                    collector.report_boundary = self.graph.block(edge.source()).expect("propagation retains its source").report_boundary();
                    collector.propagate_run_report(&mut incoming, expression);
                }
                _ => {}
            }

            collector.retire_consumed_value_slots(&mut incoming, expression.into(),
                Some(bray_bound_tree::StorageValueConsumptionPhase::Completion));
        }

        use crate::analysis::model::AnalysisCaughtResult;

        let caught = match edge.refinement() {
            Some(crate::analysis::model::AnalysisRefinement::CaughtResult { expression, outcome }) => Some((expression, outcome)),
            _ => None,
        };

        let mut report = None;

        for boundary in self.graph.exited_report_boundaries(edge) {
            let transferred = incoming.transfer_report_boundary(
                boundary,
                self.graph.report_parent(boundary),
                caught == Some((boundary, AnalysisCaughtResult::Panicked)),
            );

            if transferred.is_some() { report = transferred; }
        }

        if let Some((expression, outcome)) = caught {
            self.collector(false).publish_caught_value(&mut incoming, expression, outcome, report);
        }

        let ends = self
            .edge_lifetime_ends
            .get(&edge.id())
            .map(Vec::as_slice)
            .unwrap_or_default();

        if !ends.is_empty() && !recovered && !self.liveness.is_recovered() {
            let ended_borrows = ends
                .iter()
                .filter_map(|end| {
                    let bray_bound_tree::BoundDependencySubject::BorrowCapability(id) = *end else {
                        return None;
                    };

                    (self
                        .storage
                        .borrow_capability(id)
                        .is_some_and(|borrow| borrow.entry_binding().is_none()))
                    .then_some(id)
                })
                .collect::<BTreeSet<_>>();

            incoming.end_borrows(ended_borrows, self.storage, self.input);

            for end in ends {
                if let bray_bound_tree::BoundDependencySubject::Storage(id) = *end
                    && self.input.cleanup_requirements.ends_at_last_use(id)
                {
                    incoming.end_storage_lifetime(id, self.storage, self.input);
                }
            }
        }

        Some((incoming, failure_effects))
    }

    fn propagate_states<'state, Path: std::borrow::Borrow<crate::analysis::outcomes::OutcomePath>>(
        &self,
        source: impl IntoIterator<Item = (Option<Path>, Cow<'state, StorageFlowState>)>,
        edge: &AnalysisEdge,
        target: &mut StorageFlowOutcomes,
    ) -> bool {
        if !self.reachability.is_edge_reachable(edge.id()) {
            return false;
        }

        let mut changed = false;
        let mut reached_edge = false;
        let mut failure_effects = Vec::new();
        let join = ExecutionJoin::incoming_block(self.graph, edge.target());

        for (path, state) in source {
            let path = path.as_ref().map(std::borrow::Borrow::borrow);

            let mut merge = |reached, row| {
                let Some((incoming, performed)) = reached else { return; };

                reached_edge = true;
                failure_effects.extend(performed);

                let path = edge_path(self.graph, edge, path, self.execution.result_edges.contains(&edge.id()), row);

                changed |= target.merge(join, path, Cow::Owned(incoming));
            };

            if let Some(crate::analysis::model::AnalysisRefinement::InvocationFailure(occurrence, kind)) = edge.refinement() {
                let admitted = state.failure_at(occurrence).map_or(state.as_ref(), |entry| entry.state.as_ref());

                if let Some(execution) = &admitted.execution {
                    for row in self.execution.invocation_failure_rows(execution, occurrence, kind) {
                        merge(self.propagate_state(Cow::Borrowed(state.as_ref()), edge, row), row);
                    }
                }
            } else {
                merge(self.propagate_state(state, edge, None), None);
            }
        }

        if matches!(edge.refinement(),
            Some(crate::analysis::model::AnalysisRefinement::CleanupOutcome { failure: Some(_), .. }
                | crate::analysis::model::AnalysisRefinement::InvocationFailure(bray_bound_tree::BoundExecutionSite::ScopedExit(_), _)
                | crate::analysis::model::AnalysisRefinement::InvocationEntry { invocation: bray_bound_tree::BoundExecutionSite::ScopedExit(_), admitted: false })) {
            let mut edges = self.reached_cleanup_edges.borrow_mut();

            if reached_edge { edges.insert(edge.id()); }
            else { edges.remove(&edge.id()); }
        }

        if matches!(edge.refinement(), Some(crate::analysis::model::AnalysisRefinement::InvocationFailure(..))) {
            // A changed loop contribution supersedes the effects of its earlier approximation.
            let mut records = self.failure_effects.borrow_mut();

            if failure_effects.is_empty() {
                records.remove(&edge.id());
            } else {
                records.insert(edge.id(), failure_effects);
            }
        }

        changed
    }
}

impl<C> FixedPointDomain for StorageFlowDomain<'_, C>
where
    C: CheckerRequestContext + ?Sized,
{
    type State = StorageFlowOutcomes;

    fn direction(&self) -> FlowDirection {
        FlowDirection::Forward
    }

    fn bottom(&self) -> Self::State {
        StorageFlowOutcomes::default()
    }

    fn boundary(&self) -> Self::State {
        StorageFlowOutcomes::entry(self.entry.borrow_mut().take().expect("the fixed-point solver takes the body entry once"))
    }

    fn merge(
        &self,
        block: Option<AnalysisBlockId>,
        target: &mut Self::State,
        boundary: &Self::State,
    ) -> bool {
        let mut changed = false;
        let join = block.map_or(ExecutionJoin::Boundary, |block| ExecutionJoin::incoming_block(self.graph, block));

        for (path, state) in boundary.iter() {
            changed |= target.merge(join, path.cloned(), Cow::Borrowed(state));
        }

        changed
    }

    fn transfer(&self, block: &AnalysisBlock, source: &Self::State) -> Self::State {
        if source.states().next().is_none() {
            self.cyclic_publications.borrow_mut().remove(&block.id());

            return Self::State::default();
        }

        let mut collector = self.collector(true);
        let mut publication = BlockPublication::default();
        let output = publication.transfer(&mut collector, self.graph, block, source.clone());

        self.cyclic_publications.borrow_mut().insert(block.id(), (collector, publication));

        output
    }

    fn propagate(&self, source: &Self::State, edge: &AnalysisEdge, target: &mut Self::State) -> bool {
        self.propagate_states(source.iter().map(|(path, state)| (path, Cow::Borrowed(state))), edge, target)
    }

    fn can_consume_settled_output(&self, source: &AnalysisBlock, edge: &AnalysisEdge) -> bool {
        self.reachability.is_edge_reachable(edge.id())
            && !self.terminal_outputs.contains(&source.id())
            && self.graph.is_last_successor(source, edge, |successor| self.reachability.is_edge_reachable(successor))
    }

    fn propagate_settled_output(
        &self,
        source: &mut Option<Self::State>,
        edge: &AnalysisEdge,
        target: &mut Self::State,
    ) -> bool {
        let source = source.take().expect("settled predecessor has an output");

        self.propagate_states(source.into_states().map(|(path, state)| (path, Cow::Owned(state))), edge, target)
    }

    fn convergence_bound(&self, graph: &ControlFlowGraph) -> usize {
        let domain_size = self
            .storage
            .identities()
            .len()
            .saturating_add(self.storage.accesses().len().saturating_mul(2))
            .saturating_add(self.storage.borrow_capabilities().len())
            .saturating_add(self.storage.identities().len().saturating_mul(3))
            .saturating_add(self.refinements.len())
            .saturating_add(2);

        graph
            .blocks()
            .len()
            .saturating_mul(domain_size)
            .saturating_add(self.execution.convergence_bound(graph))
    }

    fn trace_cycle_delta(&self, block: AnalysisBlockId, previous: &Self::State, incoming: &Self::State) {
        let incoming = incoming.iter().collect::<BTreeMap<_, _>>();

        for (path, previous) in previous.iter() {
            let Some(next) = incoming.get(&path) else { continue; };

            eprintln!("cycle storage delta {block:?}: physical={} refinements={} deps={} failure={} moved={} effects={}",
                previous.memory != next.memory, previous.refinements != next.refinements,
                previous.result_dependencies != next.result_dependencies, previous.failure_entry != next.failure_entry,
                previous.moved != next.moved, previous.effect_prefix != next.effect_prefix);

            if let (Some(previous), Some(next)) = (&previous.execution, &next.execution) {
                previous.trace_cycle_delta(next);
            }
        }
    }
}
