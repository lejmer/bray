use super::{StorageFlowInput, StorageFlowState};
use super::outcomes::StorageFlowOutcomes;
use crate::analysis::outcomes::{edge_path, result_production_edges};
use crate::analysis::fixed_point::{FixedPointDomain, FlowDirection};
use crate::analysis::id::{AnalysisBlockId, AnalysisEdgeId};
use crate::analysis::model::{AnalysisBlock, AnalysisEdge, AnalysisEdgeKind, ControlFlowGraph};
use crate::analysis::reachability::ReachabilityResult;
use crate::analysis::storage_flow::check::StorageFlowCollector;
use crate::storage::StorageScopeOwners;
use crate::{CheckerRequestContext, CheckerUnitView};
use bray_bound_tree::{BoundDependencySubject, CheckedMemoryOperations, Liveness, StoragePlan};
use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::analysis::storage_flow) struct StorageFlowDomain<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    graph: &'analysis ControlFlowGraph,
    result_edges: BTreeSet<AnalysisEdgeId>,
    edge_lifetime_ends: &'analysis BTreeMap<AnalysisEdgeId, Vec<BoundDependencySubject>>,
    reachability: &'analysis ReachabilityResult,
    storage: &'analysis StoragePlan,
    liveness: &'analysis Liveness,
    refinements: &'analysis crate::analysis::refinement::RefinementUniverse,
    memory: &'analysis CheckedMemoryOperations,
    input: &'analysis StorageFlowInput,
    owners: &'analysis StorageScopeOwners,
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
        edge_lifetime_ends: &'analysis BTreeMap<AnalysisEdgeId, Vec<BoundDependencySubject>>,
        reachability: &'analysis ReachabilityResult,
        storage: &'analysis StoragePlan,
        liveness: &'analysis Liveness,
        refinements: &'analysis crate::analysis::refinement::RefinementUniverse,
        memory: &'analysis CheckedMemoryOperations,
        input: &'analysis StorageFlowInput,
        owners: &'analysis StorageScopeOwners,
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
            result_edges: result_production_edges(graph, storage),
            edge_lifetime_ends,
            reachability,
            storage,
            liveness,
            refinements,
            memory,
            input,
            owners,
            request,
            execution,
            dependencies,
            values,
            patterns,
        }
    }

    fn collector(&self) -> StorageFlowCollector<'_, C> {
        StorageFlowCollector::without_publication(
            self.request,
            self.storage,
            self.liveness,
            self.refinements,
            self.memory,
            self.input,
            self.owners,
            self.execution,
            self.dependencies,
            self.values,
            self.patterns,
        )
    }

    fn transfer_block(&self, block: &AnalysisBlock, source: &StorageFlowOutcomes) -> StorageFlowOutcomes {
        let mut state = source.clone();

        let mut collector = self.collector();

        collector.report_boundary = block.report_boundary();

        for operation in block.operations() {
            let operation = self
                .graph
                .operation(*operation)
                .expect("storage-flow block retains its operation");

            state.apply_operation(&mut collector, block.id(), operation);
        }

        state
    }
    fn propagate_state(
        &self,
        source: &StorageFlowState,
        edge: &AnalysisEdge,
    ) -> Option<StorageFlowState> {
        let mut incoming = match edge.refinement() {
            Some(crate::analysis::model::AnalysisRefinement::InvocationFailure(occurrence, kind)) =>
                self.collector().failure_edge_state(source, occurrence, kind)?,
            _ => source.clone(),
        };

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

        if let Some(crate::analysis::model::AnalysisRefinement::PropagationOutcome {
            expression,
            outcome: crate::analysis::model::AnalysisPropagationOutcome::Failure,
        }) = edge.refinement()
        {
            self.collector()
                .propagate_reference_result(&mut incoming, expression);
        }

        if let Some(crate::analysis::model::AnalysisRefinement::InvocationFailure(occurrence, kind)) =
            edge.refinement()
        {
            let mut collector = self.collector();

            collector.report_boundary = self
                .graph
                .block(edge.source())
                .expect("call source exists")
                .report_boundary();

            collector.retain_call_reports(
                &mut incoming,
                occurrence,
                bray_bound_tree::CallReportOutput::Abnormal(Some(kind)),
                false,
            );
        }

        for boundary in self.graph.exited_report_boundaries(edge) {
            incoming.transfer_report_boundary(
                boundary,
                self.graph.report_parent(boundary),
                edge.kind() == AnalysisEdgeKind::Catch,
            );
        }

        let ends = self
            .edge_lifetime_ends
            .get(&edge.id())
            .map(Vec::as_slice)
            .unwrap_or_default();

        if !ends.is_empty() && !source.recovered && !self.liveness.is_recovered() {
            let ended_borrows = ends
                .iter()
                .filter_map(|end| {
                    let bray_bound_tree::BoundDependencySubject::BorrowCapability(id) = *end else {
                        return None;
                    };

                    (self
                        .storage
                        .borrow_capability(id)
                        .is_some_and(|borrow| borrow.entry_binding().is_none())
                        && !incoming.retains_borrow(id, self.input))
                    .then_some(id)
                })
                .collect::<BTreeSet<_>>();

            if !ended_borrows.is_empty() {
                incoming.end_borrows(&ended_borrows, self.storage);
            }

            for end in ends {
                if let bray_bound_tree::BoundDependencySubject::Storage(id) = *end
                    && self.input.cleanup_free_storage.contains(&id)
                {
                    incoming.end_storage_lifetime(id, self.storage);
                }
            }
        }

        Some(incoming)
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
        let mut state = StorageFlowState::entry(self.request.semantic_values(), self.storage, self.input);

        state.execution = self.execution.entry_state();
        state.reachable = state.execution.is_some();

        state.refinements =
            crate::analysis::refinement::RefinementSet::empty(self.refinements.len());

        state.recovered |= self.refinements.is_recovered();

        StorageFlowOutcomes::entry(state)
    }

    fn merge(
        &self,
        block: Option<AnalysisBlockId>,
        target: &mut Self::State,
        boundary: &Self::State,
    ) -> bool {
        let mut changed = false;

        for (path, state) in boundary.iter() {
            changed |= target.merge(block, path.cloned(), Cow::Borrowed(state));
        }

        changed
    }

    fn transfer(&self, block: &AnalysisBlock, source: &Self::State) -> Self::State {
        self.transfer_block(block, source)
    }

    fn propagate(&self, source: &Self::State, edge: &AnalysisEdge, target: &mut Self::State) -> bool {
        if !self.reachability.is_edge_reachable(edge.id()) {
            return false;
        }

        let mut changed = false;

        for (path, state) in source.iter() {
            let Some(incoming) = self.propagate_state(state, edge) else { continue; };

            let path = edge_path(self.graph, edge, path, self.result_edges.contains(&edge.id()));

            changed |= target.merge(Some(edge.target()), path, Cow::Owned(incoming));
        }

        changed
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
}
