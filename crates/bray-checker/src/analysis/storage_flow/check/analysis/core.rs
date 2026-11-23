// rust-style: allow(module-too-large, reason = "storage flow collection is one transition engine whose diagnostics require the same plan, state, and decision context")

use crate::analysis::model::AnalysisResultPhase;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BorrowCapabilityId, BoundDependencySubject, BoundExpressionId,
    CheckedMemoryOperations, CheckedRefinements, Liveness, MemoryOperationStatus, Refinement,
    StorageAccessId, StorageAccessPlan, StorageAccessPurpose, StorageAccessRoot, StorageBinding,
    StorageExitPoint, StorageFlow, StorageIdentity, StorageOperationDecision,
    StorageOperationStatus, StoragePath, StoragePlan, StorageProjection, StorageRelationship,
    StorageAddressValue, StorageStateTarget, StorageSuspensionState, StorageOutcomeRows, StorageValueBoundary, StorageValueRetirement,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, DiagnosticStorageAccess,
    DiagnosticStorageAccessPurpose, DiagnosticStorageProjection, DiagnosticStorageRoot,
    SeverityKind,
};
use bray_symbols::{AnySymbolId, BorrowKind, CallableSignatureQuery};

use bray_bound_tree::StorageAbsence;
use crate::{
    CheckerOutcome, CheckerQueryError, CheckerRequestContext, CheckerSemanticQueryProvider,
    CheckerUnitView,
};

use super::super::availability::storage_is_recovered;
use crate::analysis::fixed_point::{FixedPointOutcome, FixedPointResult, solve_fixed_point_with};
use crate::analysis::id::{AnalysisBlockId, AnalysisEdgeId, AnalysisOperationId};
use crate::analysis::model::{
    AnalysisBlock, AnalysisCallPhase, AnalysisOperation, AnalysisOperationKind,
    AnalysisScopeExitPhase, ControlFlowGraph,
};
use crate::analysis::reachability::ReachabilityResult;
use crate::analysis::storage_flow::decision::{diagnostic_kind, more_conservative};
use crate::analysis::storage_flow::memory::{Effect, ReachedAccess, Target};
use crate::analysis::storage_flow::model::{StorageFlowDomain, StorageFlowInput, StorageFlowOutcomes, StorageFlowState};

use super::access::BorrowConflict;
use super::diagnostics::{BlockChecks, DiagnosticArgument, DiagnosticSubject};
use super::snapshot::{BlockRecords, CleanupSnapshots, ReplacementState};

#[expect(
    clippy::too_many_arguments,
    reason = "the analysis consumes correlated durable inputs and one shared control-flow graph"
)]
pub(crate) fn check_storage_flow_with_graph<C>(
    input: &mut StorageFlowInput,
    cleanup_shapes: &mut crate::asynchronous::CleanupShapeResolver<'_, C>,
    request: CheckerUnitView<'_, C>,
    execution: &crate::analysis::guarantee::ExecutionFlowDomain<'_, '_, C>,
    dependencies: &bray_bound_tree::CheckedDependencyContracts,
    values: &crate::dependency::ValueInputs,
    patterns: &bray_bound_tree::CheckedPatterns,
    storage: &StoragePlan,
    liveness: &Liveness,
    refinements: &crate::analysis::refinement::RefinementUniverse,
    memory: &CheckedMemoryOperations,
    graph: &crate::analysis::model::ControlFlowGraph,
    reachability: &ReachabilityResult,
    edge_lifetime_ends: &BTreeMap<AnalysisEdgeId, Vec<BoundDependencySubject>>,
    excluded: &BTreeSet<AnalysisEdgeId>,
) -> CheckerOutcome<(StorageFlow, CheckedRefinements, bool), C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    input.update_liveness(storage, liveness);

    let mut entry = StorageFlowState::entry(request.semantic_values(), storage, input);
    let entry_observations = entry.memory.entry_observations(execution.needs_entry_addresses());

    entry.execution = execution.entry_state(&entry_observations);
    entry.reachable = entry.execution.is_some();
    entry.refinements = crate::analysis::refinement::RefinementSet::empty(refinements.len());
    entry.recovered |= refinements.is_recovered();

    let trusted = crate::analysis::guarantee::TrustedChecks::new(execution, entry.execution.as_ref());

    let mut domain = StorageFlowDomain::new(
        graph,
        entry,
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
    );

    let mut collector = domain.collector(true);

    collector.trusted = trusted;

    let mut publication = BlockPublication::default();

    let mut reached = BTreeSet::new();

    let transfer = |block: &AnalysisBlock, input: StorageFlowOutcomes| {
        if input.states().next().is_some() {
            reached.insert(block.id());
        }

        publication.transfer(&mut collector, graph, block, input)
    };

    let result = match solve_fixed_point_with(graph, &domain, &request, Some(transfer)) {
        FixedPointOutcome::Complete(result) => result,
        FixedPointOutcome::Cancelled => return CheckerOutcome::Cancelled,
        FixedPointOutcome::ConvergenceInvariantViolated => {
            panic!(
                "finite storage-flow analysis exceeded its convergence bound in {:?}",
                request.unit().key()
            )
        }
    };

    let cyclic = domain.take_cyclic_publications();

    for (id, (checks, records)) in cyclic {
        reached.insert(id);
        collector.merge_block_checks(checks);
        publication.merge(records);
    }

    let (failure_effects, reached_cleanup_edges) = domain.into_edge_outcomes();

    for (edge, records) in failure_effects {
        let edge = graph.edge(edge).expect("failure edge belongs to its graph");

        let Some(crate::analysis::model::AnalysisRefinement::InvocationFailure(occurrence, kind)) = edge.refinement() else {
            unreachable!("performed failure effects belong to an invocation failure edge");
        };

        let source = graph.block(edge.source()).expect("failure edge retains its source");

        collector.enter_block(source);

        for record in records {
            collector.publish_failure_edge(edge.id(), occurrence, kind, record);
        }
    }

    let (fallthrough, normal) = collector.collect_terminal_outcomes(graph, &result, &reached);

    let mut diagnostics = match std::mem::take(&mut collector.diagnostics).finish(&mut collector) {
        Ok(diagnostics) => diagnostics,
        Err(error) => return CheckerOutcome::from_query_error(error),
    };

    let result_dependencies = match collector.result_dependencies(&mut diagnostics, &normal) {
        Ok(dependencies) => dependencies,
        Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Upstream(error)) => return CheckerOutcome::UpstreamFailure(error),
    };

    let decisions = collector.decisions().collect::<Vec<_>>();
    let memory_decisions = collector.memory_decisions().collect::<Vec<_>>();
    let exits = collector.exits.decisions(storage, dependencies, graph).collect::<Vec<_>>();

    let replacements = collector
        .replacement_decisions()
        .map(|decision| cleanup_shapes.select_replacement_cleanup(storage, decision))
        .collect::<Vec<_>>();

    let targets = collector.take_state_targets();
    let effects = std::mem::take(&mut collector.effects);
    let observations = collector.operation_invalidations.publish(bray_bound_tree::StorageInvalidation::merge);
    let mut scopes = BTreeMap::new();

    for ((operation, _), effects) in &observations {
        let site = graph.operation(*operation).expect("checked operation belongs to its graph").kind().occurrence();
        let scope = effects.mutation_scope();

        scopes.entry(site).and_modify(|current: &mut bray_bound_tree::StorageMutationScope| *current = (*current).max(scope))
            .or_insert(scope);
    }

    scopes.extend(collector.input_observation_scopes.publish(|scope, incoming| *scope = (*scope).max(*incoming)));

    let analysis = StorageFlow::try_new(
        storage.unit(),
        storage.kind(),
        decisions,
        collector.suspensions.into_values(),
        publication.reachable_blocks,
        exits,
        collector.is_recovered
            || storage_is_recovered(storage)
            || liveness.is_recovered()
            || refinements.is_recovered(),
    )
    .and_then(|analysis| {
        analysis.with_conditions(
            publication.conditions
                .into_iter()
                .filter_map(|(expression, value)| value.map(|value| (expression, value)))
                .collect(),
        )
    })
    .and_then(|analysis| analysis.with_memory_operations(memory, memory_decisions))
    .and_then(|analysis| analysis.with_replacements(replacements))
    .and_then(|analysis| analysis.with_state_targets(targets))
    .expect("durable storage flow must preserve the checked plan's identities")
    .with_exit_reachability(
        excluded.iter().copied(),
        reached_cleanup_edges.into_iter().filter_map(|edge| {
            let edge = graph.edge(edge).expect("reached cleanup edge belongs to its graph");

            crate::analysis::scope_cleanup_failure(graph, edge)
        }),
    )
    .with_cleanup_entries(collector.exits.into_entries())
    .with_cleanup_remainders(collector.cleanup_remainders)
    .with_entry_observations(entry_observations)
    .with_input_observation_scopes(scopes)
    .with_deferred_calls(collector.deferred_calls)
    .with_call_admissions(collector.value_captures, collector.value_transitions)
    .with_pure_effect_premises(collector.pure_effect_premises)
    .with_operation_invalidations(observations)
    .with_operation_completions(collector.normal_completions.publish(bray_bound_tree::StorageCompletion::merge), |operation| {
        graph.operation(operation).expect("normal completion belongs to its graph").kind().occurrence()
    })
    .with_operation_retirements(collector.operation_retirements)
    .with_capture_retirements(collector.capture_retirements)
    .with_dependencies(
        result_dependencies
            .with_possible_effects(effects)
            .with_synchronous_cleanup(collector.synchronous_cleanup),
    )
    .with_report_collections(collector.report_collections)
    .with_reporting_completions(collector.reporting_completions)
    .with_receiver_cleanup(collector.receiver_cleanup)
    .with_outcome_mutations(collector.outcome_mutations.publish(bray_bound_tree::StorageInvalidation::merge))
    .with_completions(collector.completions.publish(bray_bound_tree::StorageCompletion::merge))
    .with_proven_preconditions(
        collector
            .preconditions
            .into_iter()
            .filter_map(|(site, proven)| proven.then_some(site)),
    );

    match crate::analysis::refinement::publish_refinements(
        request,
        graph,
        refinements,
        publication.occurrences,
    ) {
        CheckerOutcome::Complete(result) => {
            let (published, additional) = result.into_parts();

            diagnostics.add_range(additional);

            CheckerOutcome::complete((analysis, published, fallthrough), diagnostics)
        }
        CheckerOutcome::Cancelled => CheckerOutcome::Cancelled,
        CheckerOutcome::UpstreamFailure(error) => CheckerOutcome::UpstreamFailure(error),
    }
}

#[derive(Default)]
pub(in crate::analysis::storage_flow) struct BlockPublication {
    reachable_blocks: BTreeSet<bray_bound_tree::BoundBlockId>,
    occurrences: BTreeMap<AnyBoundNodeId, crate::analysis::refinement::RefinementSet>,
    conditions: BTreeMap<BoundExpressionId, Option<bool>>,
}

impl BlockPublication {
    pub(in crate::analysis::storage_flow) fn transfer<C: CheckerRequestContext + ?Sized>(
        &mut self,
        collector: &mut StorageFlowCollector<'_, C>,
        graph: &ControlFlowGraph,
        block: &AnalysisBlock,
        mut outcomes: StorageFlowOutcomes,
    ) -> StorageFlowOutcomes {
        collector.enter_block(block);

        for state in outcomes.states() {
            collector.report_collections.extend(state.pending_report_boundaries());
        }

        for operation in block.operations() {
            let operation = graph
                .operation(*operation)
                .expect("storage-flow block retains its operation");

            if outcomes.states().next().is_none() { break; }

            if let AnalysisOperationKind::Bound(AnyBoundNodeId::Block(block)) = operation.kind() {
                self.reachable_blocks.insert(block);
            }

            if let AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression)) = operation.kind()
                && (matches!(collector.request.view().expression(expression), Some(bray_bound_tree::BoundExpression::AnonymousCallable(_)))
                    || matches!(collector.execution.semantics.selections().expression(expression), Some(bray_bound_tree::SemanticSelection::CallableReference(_))))
            {
                collector.diagnostics.record_callable_cleanup(expression);
            }

            for state in outcomes.states() {
                record_node_refinements(&mut self.occurrences, operation.kind(), &state.refinements);
            }

            outcomes.apply_operation(collector, block.id(), operation);

        }

        for state in outcomes.states() {
            collector.report_collections.extend(state.pending_report_boundaries());
            collector.collect_conditions(state, block, graph, &mut self.conditions);
        }

        outcomes
    }

    fn merge(&mut self, incoming: Self) {
        self.reachable_blocks.extend(incoming.reachable_blocks);

        merge_records(&mut self.occurrences, incoming.occurrences, |known, incoming| {
            known.intersect(&incoming);
        });

        merge_records(&mut self.conditions, incoming.conditions, retain_common);
    }
}

fn retain_common<T: Eq>(known: &mut Option<T>, incoming: Option<T>) {
    if *known != incoming {
        *known = None;
    }
}

pub(super) fn merge_records<K: Ord, V>(
    records: &mut BTreeMap<K, V>,
    incoming: BTreeMap<K, V>,
    mut merge: impl FnMut(&mut V, V),
) {
    use std::collections::btree_map::Entry;

    for (key, value) in incoming {
        match records.entry(key) {
            Entry::Occupied(mut entry) => merge(entry.get_mut(), value),
            Entry::Vacant(entry) => { entry.insert(value); }
        }
    }
}

fn merge_suspension(previous: &mut StorageSuspensionState, snapshot: StorageSuspensionState) {
    if !snapshot.is_reachable() { return; }

    if !previous.is_reachable() {
        *previous = snapshot;
        return;
    }

    // An implicit exit has multiple incoming transfers. Retain every possible owner,
    // but promise initialization and active loans only when every path establishes them.
    *previous =
        StorageSuspensionState::new(
            previous.occurrence(),
            previous.live().iter().chain(snapshot.live()).copied(),
            previous.initialized().iter().copied().filter(|identity| {
                snapshot.initialized().binary_search(identity).is_ok()
            }),
            previous.moved().iter().chain(snapshot.moved()).cloned(),
            previous.active_borrows().iter().copied().filter(|borrow| {
                snapshot.active_borrows().binary_search(borrow).is_ok()
            }),
            previous
                .cleanup_retention()
                .iter()
                .chain(snapshot.cleanup_retention())
                .copied(),
        ).with_execution_dependencies(
            previous.execution_retention().iter().chain(snapshot.execution_retention()).copied(),
            previous.unavailable_dependencies().iter().chain(snapshot.unavailable_dependencies()).copied(),
        );
}

fn record_node_refinements(
    occurrences: &mut BTreeMap<AnyBoundNodeId, crate::analysis::refinement::RefinementSet>,
    operation: AnalysisOperationKind,
    refinements: &crate::analysis::refinement::RefinementSet,
) {
    if matches!(
        operation,
        AnalysisOperationKind::Result {
            phase: crate::analysis::model::AnalysisResultPhase::Begin | crate::analysis::model::AnalysisResultPhase::Production,
            ..
        } | AnalysisOperationKind::ScopedResult(_)
    ) {
        return;
    }

    occurrences
        .entry(operation.node())
        .and_modify(|known| {
            known.intersect(refinements);
        })
        .or_insert_with(|| refinements.clone());
}

struct StorageOperationOutcome {
    status: StorageOperationStatus,
    origin: Option<StorageOperationOrigin>,
}

pub(super) type OperationAccesses = BTreeMap<StorageAccessPlan, Option<ReachedAccess>>;

enum StorageOperationOrigin {
    Borrows(Vec<BorrowCapabilityId>),
    Moves(Vec<BoundExpressionId>),
}

impl StorageOperationOutcome {
    fn status(status: StorageOperationStatus) -> Self {
        Self {
            status,
            origin: None,
        }
    }

    fn borrow_conflict(conflict: BorrowConflict) -> Self {
        let origin = match conflict {
            BorrowConflict::Unlocated => None,
            BorrowConflict::Borrows(borrows) => Some(StorageOperationOrigin::Borrows(borrows)),
        };

        Self {
            status: StorageOperationStatus::ConflictingBorrow,
            origin,
        }
    }

    fn moved(origins: Vec<BoundExpressionId>) -> Self {
        Self {
            status: StorageOperationStatus::Moved,
            origin: Some(StorageOperationOrigin::Moves(origins)),
        }
    }
}

pub(crate) struct StorageFlowCollector<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) request: CheckerUnitView<'analysis, C>,
    pub(super) storage: &'analysis StoragePlan,
    pub(super) liveness: &'analysis Liveness,
    pub(super) refinements: &'analysis crate::analysis::refinement::RefinementUniverse,
    pub(super) memory: &'analysis CheckedMemoryOperations,
    pub(super) input: &'analysis StorageFlowInput,
    pub(in crate::analysis::storage_flow) execution:
        &'analysis crate::analysis::guarantee::ExecutionFlowDomain<'analysis, 'analysis, C>,
    pub(super) dependencies: &'analysis bray_bound_tree::CheckedDependencyContracts,
    pub(super) values: &'analysis crate::dependency::ValueInputs,
    pub(super) patterns: &'analysis bray_bound_tree::CheckedPatterns,
    pub(super) results: crate::dependency::ResultState,
    pub(super) trusted: crate::analysis::guarantee::TrustedChecks,
    call_admission: Option<crate::analysis::guarantee::CallAdmissionChecks>,
    preconditions: BTreeMap<bray_bound_tree::BoundExecutionSite, bool>,
    pub(super) statuses: BTreeMap<StorageAccessPlan, StorageOperationStatus>,
    pub(super) suspensions: BTreeMap<bray_bound_tree::BoundExecutionSite, StorageSuspensionState>,
    pub(super) deferred_calls: BTreeMap<bray_bound_tree::BoundExecutionSite, BTreeSet<bray_bound_tree::BodyBehaviorCall>>,
    pub(super) exits: CleanupSnapshots,
    pub(in crate::analysis::storage_flow) cleanup_continuations: Vec<StorageFlowState>,
    pub(super) cleanup_remainders: BTreeMap<bray_bound_tree::StorageMutation, BTreeSet<bray_bound_tree::StorageCleanupRemainder>>,
    replacements: BTreeMap<StorageAccessPlan, ReplacementState>,
    pub(in crate::analysis::storage_flow::check::analysis) input_observation_scopes:
        BlockRecords<bray_bound_tree::BoundExecutionSite, bray_bound_tree::StorageMutationScope>,
    state_targets: BTreeMap<((StorageStateTarget, AnalysisOperationId), StorageOutcomeRows), Option<Target>>,
    pub(super) effects: BTreeSet<bray_symbols::DependencyStorageEffect>,
    pub(super) value_transitions: BTreeMap<(AnalysisOperationId, Option<bray_bound_tree::StorageDeferredProducer>), std::sync::Arc<[bray_bound_tree::StorageValueTransition]>>,
    operation_invalidations: BlockRecords<(AnalysisOperationId, bray_bound_tree::StorageOutcomeRows), bray_bound_tree::StorageInvalidation>,
    pub(super) pure_effect_premises: BTreeSet<AnalysisOperationId>,
    normal_completions: BlockRecords<(AnalysisOperationId, bray_bound_tree::StorageOutcomeRows), bray_bound_tree::StorageCompletion>,
    pub(super) value_captures: BTreeMap<(AnalysisOperationId, Option<bray_bound_tree::StorageDeferredProducer>), std::sync::Arc<[bray_bound_tree::StorageValueCapture]>>,
    pub(super) capture_retirements: BTreeMap<(bray_bound_tree::StorageMutation, StorageValueBoundary), BTreeSet<(Target, StorageValueRetirement)>>,
    pub(super) operation_retirements: BTreeMap<((AnalysisOperationId, StorageValueBoundary), StorageOutcomeRows), BTreeSet<(Target, StorageValueRetirement)>>,
    pub(super) synchronous_cleanup: BTreeSet<bray_symbols::DependencyRequirement>,
    pub(super) receiver_cleanup: BTreeSet<bray_symbols::TypeId>,
    pub(in crate::analysis::storage_flow) report_boundary: Option<BoundExpressionId>,
    block: Option<AnalysisBlockId>,
    report_collections: BTreeSet<Option<BoundExpressionId>>,
    pub(super) reporting_completions: BTreeSet<bray_bound_tree::BoundExecutionSite>,
    pub(super) failure_outcomes: Vec<bray_symbols::CapturedFailureOutcome>,
    outcome_mutations:
        BlockRecords<bray_bound_tree::StorageMutation, bray_bound_tree::StorageInvalidation>,
    pub(super) completions:
        BlockRecords<bray_bound_tree::StorageMutation, bray_bound_tree::StorageCompletion>,
    pub(super) memory_decisions: BTreeMap<BoundExpressionId, MemoryOperationStatus>,
    pub(super) diagnostics: BlockChecks<C::UpstreamError>,
    pub(super) publish: bool,
    pub(super) is_recovered: bool,
    pub(super) query_failed: bool,
}

impl<'analysis, C> StorageFlowCollector<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    fn collect_terminal_outcomes<'solved>(
        &mut self,
        graph: &ControlFlowGraph,
        result: &'solved FixedPointResult<StorageFlowOutcomes>,
        reached: &BTreeSet<AnalysisBlockId>,
    ) -> (bool, Vec<&'solved StorageFlowState>) {
        let mut completed = Vec::new();
        let normal = graph.completion_exits().map(|exit| exit.block()).collect::<BTreeSet<_>>();

        let abnormal = graph.abnormal_exits().map(|exit| {
            let kind = exit.kind().failure_kind().expect("abnormal exit retains its kind");

            (exit.block(), kind)
        }).collect::<BTreeMap<_, _>>();

        for block in graph.blocks() {
            self.enter_block(block);

            if !reached.contains(&block.id()) {
                self.collect_unreachable_suspensions(graph, block);

                continue;
            }

            let Some(output) = result.output(block.id()) else {
                assert!(!normal.contains(&block.id()) && !abnormal.contains_key(&block.id()),
                    "a terminal block must retain its solved output");

                continue;
            };

            for state in output.states() {
                if normal.contains(&block.id()) {
                    completed.push(state);
                } else if let Some(kind) = abnormal.get(&block.id()) {
                    let kind = state.cleanup.as_ref().map_or(*kind, crate::analysis::outcomes::CleanupProgress::failure);

                    self.collect_failure(state, kind);
                }
            }
        }

        let fallthrough = graph.completion_exits().any(|exit| {
            exit.kind() == crate::analysis::model::AnalysisExitKind::NormalFallthrough
                && result.output(exit.block()).is_some_and(|outcomes| outcomes.states().any(|state| !state.recovered))
        });

        (fallthrough, completed)
    }

    fn collect_failure(&mut self, state: &StorageFlowState, kind: bray_symbols::DependencyFailureKind) {
        let execution = state.execution.as_ref().expect("reachable failure retains its execution facts");

        let mut outcome = self.trusted.failure_outcome(self.execution, execution, kind, &state.written_inputs);

        outcome.effects = Some(state.effect_prefix.iter().cloned().collect());
        outcome.cell_changes = self.outcome_cell_changes(state);

        outcome.report = match kind {
            bray_symbols::DependencyFailureKind::Panic => Some(state.completed_report().unwrap_or_default()),
            bray_symbols::DependencyFailureKind::Cancellation => state.completed_report(),
        };

        self.failure_outcomes.push(outcome);
    }

    fn collect_conditions(
        &self,
        state: &StorageFlowState,
        block: &crate::analysis::model::AnalysisBlock,
        graph: &crate::analysis::model::ControlFlowGraph,
        conditions: &mut BTreeMap<BoundExpressionId, Option<bool>>,
    ) {
        for edge in block
            .successors()
            .iter()
            .filter_map(|edge| graph.edge(*edge))
        {
            if let Some(crate::analysis::model::AnalysisRefinement::Condition {
                expression, ..
            }) = edge.refinement()
            {
                let (_, _, value) = self.execution.condition_evidence(
                    state
                        .execution
                        .as_ref()
                        .expect("reachable storage state has execution facts"),
                    expression,
                );

                conditions.entry(expression).and_modify(|known| retain_common(known, value)).or_insert(value);
            }
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the collector borrows separately owned checker tables for one analysis"
    )]
    pub(in crate::analysis::storage_flow) fn new(
        request: CheckerUnitView<'analysis, C>,
        storage: &'analysis StoragePlan,
        liveness: &'analysis Liveness,
        refinements: &'analysis crate::analysis::refinement::RefinementUniverse,
        memory: &'analysis CheckedMemoryOperations,
        input: &'analysis StorageFlowInput,
        execution: &'analysis crate::analysis::guarantee::ExecutionFlowDomain<
            'analysis,
            'analysis,
            C,
        >,
        dependencies: &'analysis bray_bound_tree::CheckedDependencyContracts,
        values: &'analysis crate::dependency::ValueInputs,
        patterns: &'analysis bray_bound_tree::CheckedPatterns,
        publish: bool,
    ) -> Self {
        Self {
            request,
            storage,
            liveness,
            refinements,
            memory,
            input,
            execution,
            dependencies,
            values,
            patterns,
            results: Default::default(),
            trusted: Default::default(),
            call_admission: None,
            preconditions: BTreeMap::new(),
            statuses: BTreeMap::new(),
            suspensions: BTreeMap::new(),
            deferred_calls: BTreeMap::new(),
            exits: CleanupSnapshots::default(),
            cleanup_continuations: Vec::new(),
            cleanup_remainders: BTreeMap::new(),
            replacements: BTreeMap::new(),
            state_targets: BTreeMap::new(),
            value_transitions: BTreeMap::new(),
            operation_invalidations: BlockRecords::default(),
            pure_effect_premises: BTreeSet::new(),
            normal_completions: BlockRecords::default(),
            value_captures: BTreeMap::new(),
            capture_retirements: BTreeMap::new(),
            operation_retirements: BTreeMap::new(),
            input_observation_scopes: BlockRecords::default(),
            outcome_mutations: BlockRecords::default(),
            completions: BlockRecords::default(),
            effects: BTreeSet::new(),
            synchronous_cleanup: BTreeSet::new(),
            receiver_cleanup: BTreeSet::new(),
            report_boundary: None,
            block: None,
            report_collections: BTreeSet::new(),
            reporting_completions: BTreeSet::new(),
            failure_outcomes: Vec::new(),
            memory_decisions: BTreeMap::new(),
            diagnostics: BlockChecks::default(),
            publish,
            is_recovered: false,
            query_failed: false,
        }
    }

    fn merge_block_checks(&mut self, incoming: Self) {
        self.diagnostics.extend(incoming.diagnostics);
        self.trusted.merge_block_failures(incoming.trusted);
        self.exits.merge_block(incoming.exits);
        self.is_recovered |= incoming.is_recovered;

        self.operation_invalidations.extend(incoming.operation_invalidations);
        self.pure_effect_premises.extend(incoming.pure_effect_premises);
        self.normal_completions.extend(incoming.normal_completions);
        self.input_observation_scopes.extend(incoming.input_observation_scopes);
        self.outcome_mutations.extend(incoming.outcome_mutations);
        self.completions.extend(incoming.completions);
        self.value_captures.extend(incoming.value_captures);
        self.value_transitions.extend(incoming.value_transitions);
        self.state_targets.extend(incoming.state_targets);
        self.operation_retirements.extend(incoming.operation_retirements);

        self.effects.extend(incoming.effects);
        self.report_collections.extend(incoming.report_collections);
        self.reporting_completions.extend(incoming.reporting_completions);
        self.synchronous_cleanup.extend(incoming.synchronous_cleanup);
        self.receiver_cleanup.extend(incoming.receiver_cleanup);
        merge_records(&mut self.deferred_calls, incoming.deferred_calls, BTreeSet::extend);
        merge_records(&mut self.cleanup_remainders, incoming.cleanup_remainders, BTreeSet::extend);
        merge_records(&mut self.capture_retirements, incoming.capture_retirements, BTreeSet::extend);
        merge_records(&mut self.preconditions, incoming.preconditions, |known, reached| *known &= reached);
        merge_records(&mut self.statuses, incoming.statuses, |known, reached| *known = more_conservative(*known, reached));
        merge_records(&mut self.replacements, incoming.replacements, ReplacementState::merge);
        merge_records(&mut self.suspensions, incoming.suspensions, merge_suspension);

        merge_records(&mut self.memory_decisions, incoming.memory_decisions, |known, reached| {
            *known = super::memory::conservative_memory_status(*known, reached);
        });
    }

    pub(in crate::analysis::storage_flow) fn apply_operation(
        &mut self,
        state: &mut StorageFlowState,
        operation: &AnalysisOperation,
        task: Option<crate::analysis::guarantee::TaskSelection>,
    ) {
        let kind = operation.kind();

        if let AnalysisOperationKind::ScopedResult(transfer) = kind {
            use crate::analysis::model::ScopedResultDestination;

            match transfer.destination {
                ScopedResultDestination::Region { .. } => state.result_dependencies.select_scoped_region(transfer),
                ScopedResultDestination::Incident { pending } => {
                    self.retain_scoped_result_incident(state, transfer, pending);
                }
                ScopedResultDestination::Discard => { state.result_dependencies.take_value(transfer.invocation); }
                ScopedResultDestination::Capability(_) => {}
            }

            self.transfer_scoped_storage(state, transfer);

            if self.publish && transfer.source.is_some() {
                self.completions.record(self.block(), bray_bound_tree::StorageMutation::ScopedResult {
                    occurrence: transfer.invocation, variant: transfer.selected_variant(),
                }, Default::default());
            }

            return;
        }

        match kind {
            AnalysisOperationKind::Result {
                expression,
                phase: crate::analysis::model::AnalysisResultPhase::Begin | crate::analysis::model::AnalysisResultPhase::Production,
                ..
            } => {
                self.initialize_operation_storage(state, kind);
                self.acquire_generator_storage(state, expression);
                self.publish_capture_retirements(state, (operation.id(), None), StorageValueBoundary::Execution);

                self.execution.transfer_operation(
                    state
                        .execution
                        .as_mut()
                        .expect("reachable storage state has execution facts"),
                    kind,
                    None,
                    None,
                    &[],
                );

                return;
            }
            AnalysisOperationKind::Suspension {
                occurrence,
                kind: crate::analysis::model::AnalysisSuspensionKind::ScopedCall,
            } => {
                // Entry already transferred these inputs. Record live storage
                // without evaluating or transferring them again.
                if self.publish {
                    let dependencies = self.dependencies;

                    let contract = dependencies.deferred_expression(occurrence).map(|id|
                        dependencies.contract(id).expect("scoped suspension retains its checked contract"));

                    let mut unavailable = Vec::new();

                    if let Some(contract) = contract {
                        super::dependencies::collect_unsatisfied_dependencies(
                            self.request.semantic_values(), self.storage,
                            self.refinements.active_refinements(&state.refinements), state, contract,
                            &mut unavailable,
                        );
                    }

                    self.record_suspension(
                        state, occurrence, contract.into_iter().flat_map(|contract| contract.required_subjects()), unavailable,
                    );
                }

                return;
            }
            _ => {}
        }

        if let AnalysisOperationKind::Suspension { occurrence, .. } = kind {
            let (retained, unavailable) = self.awaited_dependencies(state, kind);

            self.record_suspension(state, occurrence, retained, unavailable);
        }

        if let Some(occurrence) = operation.cleanup_suspension() {
            self.record_suspension(state, occurrence, [], []);
        }

        let task = task.or_else(|| self.execution.task_observation_outcome(
            state.execution.as_ref().expect("reachable storage state has execution facts"), kind,
        ));

        let task_outcome = task.as_ref().map(|selection| selection.outcome);

        let panic_report = task.as_ref().and_then(|selection| self.apply_task_observation(state, operation, selection));

        let refinements = std::mem::take(&mut state.refinements);
        let universe = self.refinements;

        self.observe_absence(state, universe.active_refinements(&refinements));

        let mut execution = state
            .execution
            .take()
            .expect("reachable storage state has execution facts");

        if let AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression)) = kind
            && let Some(generator) = self.execution.yield_targets.get(&expression).copied()
        {
            self.append_generator_element(state, expression, generator);
        }

        self.retain_operation_reports(state, *operation, &execution, task_outcome);
        self.transfer_result_dependencies(state, kind, &execution, task_outcome, panic_report.as_deref());

        let accesses = self.capture_transfer_accesses(state, &execution, kind);

        if self.publish
            && !matches!(
                kind,
                AnalysisOperationKind::Result { .. }
                    | AnalysisOperationKind::Call {
                        phase: AnalysisCallPhase::Input,
                        ..
                    }
            )
        {
            self.trusted.before_operation(self.execution, &execution, kind, &|plan| {
                accesses.get(&plan)?.as_ref()?.address.single_location()?.value_observation_path()
            });
        }

        let first_continuation = self.cleanup_continuations.len();

        let displaced = self.apply_cleanup(state, &mut execution, operation);

        for continued in &mut self.cleanup_continuations[first_continuation..] {
            continued.refinements = refinements.clone();
        }

        if !state.reachable {
            return;
        }

        self.apply_operation_effects(state, operation, execution, refinements, task.as_ref(), accesses);
        state.release_displaced_loans(displaced, self.storage, self.input);
    }

    pub(in crate::analysis::storage_flow) fn resume_cleanup(
        &mut self,
        state: &mut StorageFlowState,
        operation: &AnalysisOperation,
    ) {
        let mut execution = state.execution.take().expect("a cleanup continuation owns its reached execution state");
        let first_continuation = self.cleanup_continuations.len();
        let accesses = self.capture_transfer_accesses(state, &execution, operation.kind());

        let displaced = self.apply_cleanup(state, &mut execution, operation);

        if state.reachable {
            let refinements = std::mem::take(&mut state.refinements);

            self.apply_operation_effects(state, operation, execution, refinements, None, accesses);
            state.release_displaced_loans(displaced, self.storage, self.input);
        }

        // A sibling captures the refinements already known when its action ran.
        debug_assert!(self.cleanup_continuations[first_continuation..].iter().all(|continued| continued.execution.is_some()));
    }

    pub(in crate::analysis::storage_flow) fn merge_completed(
        &self,
        state: &mut StorageFlowState,
        incoming: &mut StorageFlowState,
        expression: BoundExpressionId,
    ) {
        let mut execution = state
            .execution
            .take()
            .expect("normal completion retains execution facts");

        let other = incoming
            .execution
            .take()
            .expect("completed alternative retains execution facts");

        self.execution
            .merge_completion(&mut execution, &other, expression);

        state.merge(incoming, None);
        state.execution = Some(execution);
    }

    fn apply_operation_effects(
        &mut self,
        state: &mut StorageFlowState,
        operation: &AnalysisOperation,
        execution: crate::analysis::guarantee::ExecutionState,
        refinements: crate::analysis::refinement::RefinementSet,
        task: Option<&crate::analysis::guarantee::TaskSelection>,
        mut reached_accesses: OperationAccesses,
    ) {
        let kind = operation.kind();
        let occurrence = kind.occurrence();
        let mut execution = execution;

        let completing = matches!(
            kind,
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Input | AnalysisCallPhase::Completion,
                ..
            } | AnalysisOperationKind::Result { .. }
        );

        let assignment_plan = (!completing)
            .then(|| kind.storage_plans(self.storage).find(|plan| plan.purpose() == StorageAccessPurpose::Assignment))
            .flatten();

        let destination = assignment_plan.and_then(|plan| {
            let expression = plan.occurrence().expression().expect("assignment retains its expression");

            self.assignment_destination(&state.memory, expression, plan.access()).map(Cow::into_owned)
        });

        let assignment = destination.as_ref().map(|access| &access.address);

        let single_phase = self.execution.single_phase_invocation(kind);

        let captures = self.capture_operation_inputs(state, operation, &execution, &refinements, &mut reached_accesses);
        let captures = captures.as_deref().unwrap_or_default();

        if let Some(expression) = single_phase {
            self.execution.admit_inline_invocation(&mut execution, expression, captures);
        }

        let attempt = (!completing).then(|| self.call_attempt(kind)).flatten();
        let memory_operation = kind.explicit_attempt_expression().and_then(|expression| self.memory.operation(expression));
        let intrinsic = memory_operation.is_some();

        self.call_admission = if (attempt.is_some() || single_phase.is_some()) && (self.publish || intrinsic) {
            let protected_initialized = memory_operation.is_some_and(|operation|
                self.protected_input_initialized(&state.memory, &execution, operation));

            self.execution.call_admission_checks(&mut execution, occurrence, protected_initialized)
        } else {
            None
        };

        let assignment_mutation = assignment_plan.map(|plan| self.assignment_mutation(kind, plan, &execution));

        let invocation_roots = attempt.map(|_| Self::invocation_roots(&state.memory, execution.admitted_call(occurrence)));

        if kind.awaited_expression().is_none() {
            self.prepare_frame_execution(&state.memory, &mut execution, operation, &[]);
        }

        self.start_frame_effects(state, &mut execution, kind);

        let bind_values = single_phase.is_some() || matches!(kind, AnalysisOperationKind::Call { phase: AnalysisCallPhase::Attempt, .. });
        let commits_cleanup = memory_operation.is_some_and(|operation| crate::memory::execution::commits_buffer_cleanup(operation.kind()));

        let local_effects = if let Some(mutation) = assignment_mutation {
            super::transfers::location_effects(assignment, mutation, None).collect()
        } else if !completing && invocation_roots.is_none() && !intrinsic {
            self.value_transfer_effects(kind, &reached_accesses)
        } else {
            Vec::new()
        };

        let local_effects = local_effects.as_slice();
        let assignment_effects = assignment_mutation.map(|_| local_effects);

        if bind_values && !intrinsic
            && execution.admitted_call(occurrence).is_none_or(|entry| entry.outputs.0.is_empty())
        {
            self.begin_value_transition(&state.memory, &mut execution, operation, &[]);
        }

        let started = kind.explicit_attempt_expression()
            .and_then(|expression| self.execution.started_future(&execution, expression));

        let mut failure_prefixes = super::effect_rows::EffectRows::new();
        let mut returned_referents = BTreeMap::new();

        let assignment_roots = assignment_plan.map(|plan| {
            self.prepare_assignment(state, kind, plan.access(), assignment)
        });

        let outcomes = self.begin_transition(
            state, operation, &execution, invocation_roots.as_deref(), started.as_ref(),
            (&mut failure_prefixes, &mut returned_referents),
        );

        if bind_values && intrinsic {
            self.begin_value_transition(&state.memory, &mut execution, operation, outcomes.as_ref().map_or(&[], bray_symbols::DependencyEffectOutcomes::possible));
        }

        if self.publish {
            self.trusted.capture_returned_witness_transfers(self.execution, &execution, kind);
        }

        let call_effects = outcomes.as_ref().map(bray_symbols::DependencyEffectOutcomes::normal);
        let possible_effects = outcomes.as_ref().map(bray_symbols::DependencyEffectOutcomes::possible);

        if let Some(effects) = possible_effects {
            state.memory.observe_effect_types(effects);
        }

        let (escaped, escape_prefix) = self.operation_escapes(
            state, &execution, kind, (assignment_roots.as_deref(), assignment_effects),
            (invocation_roots.as_deref(), call_effects), started.is_some(),
        );

        if !completing {
            self.check_terminal_operation(
                state,
                kind,
                assignment_effects,
                assignment_mutation,
                possible_effects,
                invocation_roots.as_deref().unwrap_or(&[]),
                &escaped,
            );
        }

        let guarded = self.publish
            .then(|| self.execution.operation_outcome_rows(&execution, kind, task)).flatten();

        let transition = (operation.id(), guarded.as_ref());

        let invalidation = (!completing).then(|| self.prepare_operation_invalidation(
            state, &mut execution, (operation, guarded.as_ref()), (local_effects, call_effects, returned_referents),
            invocation_roots.as_deref(), &escaped, intrinsic,
        )).flatten();

        let unique_assignment = assignment.and_then(StorageAddressValue::single_location).cloned();

        if let Some(plan) = assignment_plan {
            reached_accesses.insert(plan, destination);
        }

        let mut admitted = false;

        if !completing {
            let can_fail = attempt.is_some() && self.capture_failure_effects(
                state, kind, &execution, outcomes.as_ref(), &failure_prefixes,
                invocation_roots.as_deref().unwrap_or(&[]),
            );

            if let Some(expression) = kind.awaited_expression() {
                self.execution.admit_await_inputs(&mut execution, expression);

                admitted = true;
                state.execution = Some(execution);
                self.finish_admission(state, kind, transition, &refinements, can_fail);
                execution = state.execution.take().expect("admitted await retains execution facts");
            }

            let call = if matches!(kind, AnalysisOperationKind::Call { phase: AnalysisCallPhase::Attempt, .. }) && (!intrinsic || commits_cleanup) {
                admitted = true;
                state.execution = Some(execution);

                let inputs = self.admit_call_attempt(state, operation, transition, &refinements, can_fail, &mut reached_accesses);

                execution = state.execution.take().expect("admitted call retains its execution facts");

                Some(inputs)
            } else {
                None
            };

            if !escape_prefix.is_empty() {
                std::sync::Arc::make_mut(&mut state.effect_prefix).extend(escape_prefix.iter().cloned());
            }

            let performed = if started.is_some() {
                // The owning task publishes the actual body prefix when observed.
                Some(&[crate::analysis::storage_flow::memory::Effect::Ambient][..])
            } else {
                call_effects
            };

            self.record_operation_effects(
                state,
                kind,
                performed,
                local_effects,
            );

            if let Some(target) = unique_assignment.as_ref() && assignment_mutation.is_some() {
                self.record_current_write(state, target);
            }

            let assignment = unique_assignment.as_ref()
                .filter(|target| target.is_exact())
                .and_then(Target::value_observation_path)
                .and_then(|path| crate::ExecutionPlace::storage_path(self.storage, &path));

            if let Some(call) = call {
                self.execution.produce_call_attempt(&mut execution, occurrence, call, invalidation.as_ref(), assignment.as_ref());
            } else {
                self.execution.transfer_operation(&mut execution, kind, invalidation.as_ref(), assignment.as_ref(), captures);
            }
        }

        if matches!(
            kind,
            AnalysisOperationKind::Result { .. }
                | AnalysisOperationKind::Call {
                    phase: AnalysisCallPhase::Input,
                    ..
                }
        ) {
            self.execution
                .transfer_operation(&mut execution, kind, None, None, captures);
        }

        self.record_entry_check(&mut execution, kind);

        state.execution = Some(execution);
        state.memory.escape_regions(escaped);

        self.apply_call_effects(state, kind, call_effects, invalidation.as_ref());

        if completing {
            self.end_assignment_operator_inputs(state, kind);
        }

        let completion = self.finish_storage_operation(
            state, operation, refinements, (invalidation.as_ref(), guarded.as_ref()), admitted, &mut reached_accesses,
        );

        if self.publish && let Some(completion) = completion {
            self.normal_completions.record(self.block(), (operation.id(), guarded.clone().unwrap_or_default()), completion);
        }

        if !completing {
            self.record_operation_observations(operation, guarded, unique_assignment, invalidation);
        }
    }

    #[expect(clippy::too_many_arguments, reason = "observation survival uses the already resolved effects and their actual operands")]
    fn prepare_operation_invalidation(
        &mut self,
        state: &StorageFlowState,
        execution: &mut crate::analysis::guarantee::ExecutionState,
        (operation, rows): (&AnalysisOperation, Option<&StorageOutcomeRows>),
        (local, performed, returned_referents): (&[Effect], Option<&[Effect]>, BTreeMap<StoragePath, bray_bound_tree::StorageObservedTarget>),
        roots: Option<&[Target]>,
        escaped: &BTreeSet<Target>,
        intrinsic: bool,
    ) -> Option<bray_bound_tree::StorageInvalidation> {
        let kind = operation.kind();

        let mut invalidation = self.operation_observations(
            state, kind, returned_referents, local, performed, roots, escaped,
        );

        self.observe_memory_read(state, execution, operation, rows, invalidation.as_mut());

        if let Some(invalidation) = &mut invalidation {
            let effects = match performed {
                Some([]) => Some(Cow::Borrowed(local)),
                Some(effects) if local.is_empty() => Some(Cow::Borrowed(effects)),
                Some(effects) => Some(Cow::Owned(effects.iter().chain(local).cloned().collect())),
                None if roots.is_none() && !intrinsic => Some(Cow::Borrowed(local)),
                None => None,
            };

            self.preserve_operation_values(state, execution, kind, effects.as_deref(), invalidation);
        }

        invalidation
    }

    fn finish_storage_operation(
        &mut self, state: &mut StorageFlowState, operation: &AnalysisOperation,
        mut refinements: crate::analysis::refinement::RefinementSet,
        (invalidation, rows): (Option<&bray_bound_tree::StorageInvalidation>, Option<&StorageOutcomeRows>),
        admitted: bool,
        accesses: &mut OperationAccesses,
    ) -> Option<bray_bound_tree::StorageCompletion> {
        let kind = operation.kind();
        let occurrence = kind.occurrence();
        let mut produced = BTreeMap::new();

        let memory = self.apply_storage_operation(
            state,
            operation,
            self.refinements.active_refinements(&refinements),
            (invalidation, rows),
            admitted,
            (accesses, &mut produced),
        );

        let transition = (operation.id(), rows);

        self.publish_capture_retirements(state, transition, StorageValueBoundary::Execution);

        let memory_status = memory.as_ref().map(|(status, _)| *status);
        let write_target = memory.as_ref().and_then(|(_, target)| target.as_ref());
        let mut completion = self.complete_operation(state, kind, memory_status, write_target, &produced);

        if let Some(completion) = &mut completion {
            self.publish_captured_outputs(state, occurrence, completion);
            self.publish_frame_captures(state, occurrence);
        }

        if state.reachable && memory_status.is_none_or(|status| status == MemoryOperationStatus::Valid)
            && matches!(kind, AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(_))
                | AnalysisOperationKind::Result { phase: crate::analysis::model::AnalysisResultPhase::Production, .. }
                | AnalysisOperationKind::Call { phase: AnalysisCallPhase::Completion, .. })
            && let Some(result) = self.invocation_result(occurrence)
        {
            let ty = self.storage.identity_type(result.root);
            let subjects = self.inferred_value_subjects(occurrence, ty).collect();

            state.memory.bound_value_dependencies(result, bray_bound_tree::ReportDependencies {
                subjects, ..Default::default()
            });
        }

        if kind.value_consumption_phase() == Some(bray_bound_tree::StorageValueConsumptionPhase::Completion)
            && let Some(expression) = occurrence.expression()
        {
            if let Some(plan) = self.storage.assignment_destination(expression)
                && self.input.direct_reference_target(self.storage, plan.access()).is_none()
            {
                let assignment = plan.occurrence().expression().expect("destination belongs to an assignment expression");

                let execution = state.execution.as_ref().expect("destination evaluation retains its operands");
                let address = self.evaluated_access(&state.memory, execution, plan.access());

                state.memory.capture_assignment_destination(assignment, address);
            }

            if kind.storage_plans(self.storage).any(|plan| plan.purpose() == StorageAccessPurpose::Assignment) {
                state.memory.finish_assignment(expression);
            }
        }

        if !admitted && memory_status.is_none_or(|status| status == MemoryOperationStatus::Valid) {
            self.retire_consumed_value_slots(state, kind.occurrence(), kind.value_consumption_phase());
        }

        self.end_operation_uses(state, kind);
        self.publish_capture_retirements(state, transition, StorageValueBoundary::Retirement);

        crate::analysis::refinement::transfer_operation(
            &mut refinements,
            operation,
            self.refinements,
            self.storage,
            invalidation,
            &self.input.footprint.discriminants,
        );

        state.refinements = refinements;

        completion
    }

    fn admit_call_attempt(
        &mut self,
        state: &mut StorageFlowState,
        operation: &AnalysisOperation,
        transition: (AnalysisOperationId, Option<&StorageOutcomeRows>),
        refinements: &crate::analysis::refinement::RefinementSet,
        can_fail: bool,
        accesses: &mut OperationAccesses,
    ) -> (Option<crate::ExecutionCondition>, bool) {
        let kind = operation.kind();
        let invocation = kind.occurrence();
        let execution = state.execution.as_mut().expect("planned call inputs retain execution facts");
        let inputs = self.execution.admit_call_inputs(execution, invocation);

        // Apply the resolved move to both physical storage and the current owner.
        self.apply_operation_inputs(state, (operation, transition.1), self.refinements.active_refinements(refinements), None, accesses);

        self.finish_admission(state, kind, transition, refinements, can_fail);

        inputs
    }

    fn finish_admission(&mut self, state: &mut StorageFlowState, operation: AnalysisOperationKind,
        transition: (AnalysisOperationId, Option<&StorageOutcomeRows>),
        refinements: &crate::analysis::refinement::RefinementSet, can_fail: bool,
    ) {
        let invocation = operation.occurrence();

        state.memory.finish_value_transfers();
        self.retire_consumed_value_slots(state, invocation, operation.value_consumption_phase());
        self.publish_capture_retirements(state, transition, StorageValueBoundary::Admission);

        state.failure_entry = None;

        if !can_fail { return; }

        // Abnormal successors inherit the admitted state before normal effects.
        let mut failure = state.clone();

        failure.refinements = refinements.clone();

        if let Some(expression) = operation.awaited_expression() {
            let execution = failure.execution.as_mut().expect("admitted failure retains execution facts");

            self.execution.suspend_awaited_execution(execution, expression);
        }

        state.failure_entry = Some(crate::analysis::storage_flow::model::AdmittedFailure {
            occurrence: invocation, state: std::sync::Arc::new(failure),
        });
    }

    fn end_assignment_operator_inputs(&self, state: &mut StorageFlowState, kind: AnalysisOperationKind) {
        if self.storage.node_plans(kind.node()).any(|plan| plan.purpose() == StorageAccessPurpose::Assignment) {
            // The operator has published retained outputs. End its input loans
            // before the assignment installs the result.
            self.end_last_use_borrows(state, kind.occurrence());
        }
    }

    fn end_operation_uses(&self, state: &mut StorageFlowState, kind: AnalysisOperationKind) {
        if let AnalysisOperationKind::AssignmentInstall(expression) = kind {
            let slot = self.storage.invocation_result(expression.into()).and_then(|access| self.storage.root_identity(access))
                .expect("assignment takes its private operator result");

            state.forget_storage(slot, self.storage, self.input);
        }

        if !matches!(
            kind,
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Input | AnalysisCallPhase::Attempt,
                ..
            } | AnalysisOperationKind::Suspension {
                kind: crate::analysis::model::AnalysisSuspensionKind::Await,
                ..
            }
        ) {
            self.end_last_use_borrows(state, kind.occurrence());

            let operator_completion = matches!(kind, AnalysisOperationKind::Call { phase: AnalysisCallPhase::Completion, .. })
                && kind.occurrence().expression().is_some_and(|expression| {
                    matches!(self.request.view().expression(expression), Some(bray_bound_tree::BoundExpression::Assignment(_)))
                });

            if !operator_completion {
                self.end_last_use_storage(state, kind.occurrence());
            }
        }
    }

    pub(in crate::analysis::storage_flow) fn retire_consumed_value_slots(
        &self,
        state: &mut StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        phase: Option<bray_bound_tree::StorageValueConsumptionPhase>,
    ) {
        let Some(phase) = phase else { return; };

        for path in self.storage.value_retirements(occurrence, phase) {
            let ty = self.storage.storage_type(path.root)
                .expect("consumed evaluated slot has its checked type");

            let origin = self.storage.identity(path.root).and_then(|identity| match identity {
                StorageIdentity::ScopedCapability { expression, .. } => Some(expression),
                _ => identity.definition_occurrence().and_then(bray_bound_tree::BoundExecutionSite::expression),
            })
                .expect("evaluated slot consumption retains its producing expression");

            // Destination transfer has captured the payload. Retire only this output handle.
            // The reached allocation belongs to the destination and remains initialized.
            state.record_move(path.clone(), origin, ty, true);
            state.memory.clear(path);
        }
    }

    fn preserve_operation_values(
        &mut self,
        state: &StorageFlowState,
        execution: &crate::analysis::guarantee::ExecutionState,
        kind: AnalysisOperationKind,
        effects: Option<&[crate::analysis::storage_flow::memory::Effect]>,
        invalidation: &mut bray_bound_tree::StorageInvalidation,
    ) {
        let occurrence = kind.occurrence();

        self.preserve_captured_observations(execution, occurrence, invalidation);

        self.preserve_returned_observations(
            occurrence, execution, &state.memory, effects, invalidation,
        );

        let operation = kind.explicit_attempt_expression().and_then(|expression| self.memory.operation(expression));

        if let Some(pointee) = operation.and_then(|operation| match operation.kind() {
            bray_bound_tree::CheckedMemoryOperationKind::Read { pointee, kind: bray_bound_tree::MemoryReadKind::Move }
            | bray_bound_tree::CheckedMemoryOperationKind::Write { pointee } => Some(*pointee),
            _ => None,
        }) && let bray_bound_tree::StorageInvalidation::Observed { transition, .. } = invalidation
            && *transition == bray_bound_tree::StorageObservationTransition::Contents
        {
            *transition = bray_bound_tree::StorageObservationTransition::CellContents(pointee);
        }

        self.preserve_storage_values(state, operation, effects, invalidation);

        if let Some(contents) = self.input.tag_preserving_assignments.get(&occurrence) {
            self.preserve_store_values(&state.memory, invalidation, true, *contents);
        }

        self.preserve_consumed_input_observations(occurrence, execution.admitted_call(occurrence), invalidation);
    }

    fn capture_operation_inputs(
        &mut self,
        state: &mut StorageFlowState,
        operation: &AnalysisOperation,
        execution: &crate::analysis::guarantee::ExecutionState,
        refinements: &crate::analysis::refinement::RefinementSet,
        accesses: &mut OperationAccesses,
    ) -> Option<std::sync::Arc<[bray_bound_tree::StorageValueCapture]>> {
        let kind = operation.kind();

        let staged = matches!(
            kind,
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Input,
                ..
            }
        );

        if staged {
            self.initialize_operation_storage(state, kind);

            for plan in kind.storage_plans(self.storage) {
                let reached = accesses.remove(&plan).unwrap_or_else(|| {
                    let access = self.operation_access(plan, self.storage.effective_purpose(plan));

                    self.evaluated_access(&state.memory, execution, access)
                });

                let _ = self.apply_plan(
                    state,
                    (operation.id(), None),
                    plan,
                    self.refinements.active_refinements(refinements),
                    None,
                    Some(reached),
                );
            }
        }

        let captures = self.resolve_value_captures(state, operation);

        if self.publish && staged {
            self.record_input_targets(&state.memory, operation, captures.as_deref().unwrap_or_default());
        }

        captures
    }

    fn record_entry_check(
        &mut self,
        execution: &mut crate::analysis::guarantee::ExecutionState,
        kind: AnalysisOperationKind,
    ) {
        if let AnalysisOperationKind::Call { invocation, phase: AnalysisCallPhase::Input } = kind
            && let Some(proven) = self
                .execution
                .capture_entry_check(execution, invocation)
            && self.publish
        {
            self.preconditions
                .entry(invocation)
                .and_modify(|previous| *previous &= proven)
                .or_insert(proven);
        }
    }

    fn enter_block(&mut self, block: &AnalysisBlock) {
        self.query_failed = false;
        self.report_boundary = block.report_boundary();
        self.block = Some(block.id());
        self.diagnostics.block = self.block;
        self.trusted.block = self.block;
    }

    pub(super) fn fail_query(&mut self, error: CheckerQueryError<C::UpstreamError>) {
        if self.publish && !self.query_failed {
            self.diagnostics.record_query_failure(error);
        }

        self.query_failed = true;
    }

    pub(super) fn block(&self) -> AnalysisBlockId {
        self.block.expect("published observation belongs to its reached block")
    }

    pub(super) fn record_outcome_mutation(
        &mut self,
        mutation: bray_bound_tree::StorageMutation,
        invalidation: Cow<'_, bray_bound_tree::StorageInvalidation>,
    ) {
        if self.publish {
            self.outcome_mutations.record(self.block(), mutation, invalidation.into_owned());
        }
    }

    fn record_operation_observations(
        &mut self,
        operation: &AnalysisOperation,
        guarded: Option<bray_bound_tree::StorageOutcomeRows>,
        assignment: Option<Target>,
        invalidation: Option<bray_bound_tree::StorageInvalidation>,
    ) {
        if !self.publish {
            return;
        }

        let kind = operation.kind();

        if let AnyBoundNodeId::Expression(expression) = kind.node()
            && matches!(kind, AnalysisOperationKind::Bound(_) | AnalysisOperationKind::AssignmentInstall(_))
            && matches!(
                self.request.view().expression(expression),
                Some(bray_bound_tree::BoundExpression::Assignment(_))
            )
        {
            self.record_state_target(
                (operation.id(), guarded.as_ref()),
                StorageStateTarget::Assignment(expression),
                assignment,
            );
        }

        if let Some(effects) = invalidation {
            self.record_operation_invalidation(operation.id(), guarded.unwrap_or_default(), effects);
        }
    }

    pub(super) fn record_operation_invalidation(
        &mut self, operation: AnalysisOperationId, guard: StorageOutcomeRows, effects: bray_bound_tree::StorageInvalidation,
    ) {
        if self.publish {
            self.operation_invalidations.record(self.block(), (operation, guard), effects);
        }
    }

    fn complete_operation(
        &mut self,
        state: &mut StorageFlowState,
        kind: AnalysisOperationKind,
        memory_status: Option<MemoryOperationStatus>,
        write_target: Option<&Target>,
        produced: &BTreeMap<StorageAccessPlan, Target>,
    ) -> Option<bray_bound_tree::StorageCompletion> {
        if matches!(
            kind,
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Input,
                ..
            }
        ) {
            return None;
        }

        let valid = memory_status.is_none_or(|status| status == MemoryOperationStatus::Valid);

        let observations = if valid {
            self.completed_references(state, kind)
        } else {
            None
        };

        let execution = state
            .execution
            .as_mut()
            .expect("operation retains execution facts");

        if valid {
            self.execution
                .complete_operation(execution, kind, observations.as_ref(), write_target);

            if matches!(
                kind,
                AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(_))
                    | AnalysisOperationKind::Call {
                        phase: AnalysisCallPhase::Completion,
                        ..
                    }
            ) && let Some(expression) = kind.occurrence().expression()
            {
                let transfer = crate::dependency::ResultTransfer::new(
                    self.request, self.execution.semantics.types(), self.execution.semantics.selections(),
                    self.patterns, self.values, self.storage, &mut state.result_dependencies);

                if let Err(error) = self.execution.capture_deferred_outputs(execution, expression,
                    || transfer.deferred_completion_values(expression),
                    |source| self.deferred_report_dependencies(&transfer, expression, source)) {
                    self.fail_query(error);
                }
            }
        }

        if self.publish
            && !matches!(
                kind,
                AnalysisOperationKind::Result { .. }
                    | AnalysisOperationKind::Call {
                        phase: AnalysisCallPhase::Input,
                        ..
                    }
            )
        {
            self.trusted.after_operation(
                self.execution, execution, kind,
                self.call_admission.as_ref(),
                &|plan| produced.get(&plan)?.value_observation_path(),
            );
        }

        observations
    }

    fn transfer_result_dependencies(
        &mut self,
        state: &mut StorageFlowState,
        kind: AnalysisOperationKind,
        execution: &crate::analysis::guarantee::ExecutionState,
        task_outcome: Option<bray_bound_tree::StorageTaskOutcome>,
        panic_report: Option<&bray_bound_tree::ReportDependencies>,
    ) {
        let completed = match kind {
            AnalysisOperationKind::Call {
                invocation: bray_bound_tree::BoundExecutionSite::Node(AnyBoundNodeId::Expression(expression)),
                phase: AnalysisCallPhase::Completion,
            } => self.execution.awaited_value_dependencies(
                execution, expression, task_outcome,
            ),
            _ => None,
        };

        let combined = panic_report.zip(completed.as_deref())
            .map(|(report, completed)| report.requirements.union(completed).cloned().collect());

        let awaited = combined.as_ref().or_else(|| panic_report.map(|report| &report.requirements)).or(completed.as_deref());

        let mut transfer = crate::dependency::ResultTransfer::new(
            self.request,
            self.execution.semantics.types(),
            self.execution.semantics.selections(),
            self.patterns,
            self.values,
            self.storage,
            &mut state.result_dependencies,
        );

        if let Err(error) = transfer.operation(
            kind,
            kind.occurrence()
                .expression()
                .and_then(|id| self.execution.yield_targets.get(&id).copied()),
            awaited,
        ) {
            self.fail_query(error);
        }
    }

    fn observe_absence<'refinements>(
        &self,
        state: &mut StorageFlowState,
        refinements: impl Iterator<Item = &'refinements Refinement> + Clone,
    ) {
        for refinement in refinements.clone() {
            if let bray_bound_tree::RefinementKind::Pattern {
                access,
                predicate: bray_bound_tree::PatternPredicate::ActiveUnionVariant(active),
                value,
                ..
            } = refinement.kind()
                && let Some(payloads) = self
                    .storage
                    .logical_path(access)
                    .and_then(|path| self.input.variant_payloads.get(path))
            {
                for payload in payloads {
                    if let Some(StorageProjection::ActiveUnionPayloadField { variant, .. }) =
                        payload.projections.last()
                        && (*variant == active) != value
                    {
                        // The absent variant is an ownership fact even when its payload has no source binding.
                        std::sync::Arc::make_mut(&mut state.definitely_absent)
                            .entry(payload.clone())
                            .or_insert(StorageAbsence::NoPayload);
                    }
                }
            }

            let access = match refinement.kind() {
                bray_bound_tree::RefinementKind::Pattern {
                    access,
                    predicate: bray_bound_tree::PatternPredicate::NullableAbsent,
                    value: true,
                    ..
                }
                | bray_bound_tree::RefinementKind::Pattern {
                    access,
                    predicate: bray_bound_tree::PatternPredicate::NullablePresent,
                    value: false,
                    ..
                } => Some(access),
                bray_bound_tree::RefinementKind::NullablePresence {
                    expression,
                    is_present: false,
                } => self
                    .storage
                    .node_plans(expression.into())
                    .find(|plan| plan.purpose() == StorageAccessPurpose::Read)
                    .map(|plan| plan.access()),
                _ => None,
            };

            if let Some(payload) = access.and_then(|access| {
                self.storage
                    .projected_access(access, &[StorageProjection::NullableValue])
            }) {
                state.mark_absent(payload, StorageAbsence::NoPayload, self.storage);
            }
        }
    }

    fn apply_storage_operation<'refinements>(
        &mut self,
        state: &mut StorageFlowState,
        operation: &AnalysisOperation,
        refinements: impl Iterator<Item = &'refinements Refinement> + Clone,
        (invalidation, rows): (Option<&bray_bound_tree::StorageInvalidation>, Option<&StorageOutcomeRows>),
        admitted: bool,
        (accesses, produced): (&mut OperationAccesses, &mut BTreeMap<StorageAccessPlan, Target>),
    ) -> Option<(MemoryOperationStatus, Option<Target>)> {
        if let AnalysisOperationKind::ScopeExit {
            scope, exit, phase, ..
        } = operation.kind()
        {
            state.memory.finish_value_transfers();

            if phase == AnalysisScopeExitPhase::LifecycleResolution {
                self.report_escaping_storage_dependencies(state, scope, exit);

                if self.publish {
                    self.exits.record_lifecycle(StorageExitPoint::new(scope, exit), state);
                }

                self.end_scope(state, scope, exit);
            }

            return None;
        }

        if matches!(
            operation.kind(),
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Input,
                ..
            }
        ) {
            return None;
        }

        if let AnalysisOperationKind::PatternObservation(pattern) = operation.kind() {
            state
                .observed_pattern_bindings
                .extend(self.input.definitions(AnyBoundNodeId::from(pattern).into()));

            return None;
        }

        if matches!(operation.kind(), AnalysisOperationKind::Recovery(_)) {
            state.recovered = true;
            self.is_recovered = true;
        }

        if let Some(invalidation) = invalidation {
            state.invalidate_observed_absence(
                invalidation,
                &self.input.footprint.discriminants,
            );
        }

        if let AnalysisOperationKind::Call { invocation, phase } = operation.kind() {
            match invocation {
                bray_bound_tree::BoundExecutionSite::Node(AnyBoundNodeId::Expression(_))
                    if phase == AnalysisCallPhase::Attempt =>
                {
                    if !admitted {
                        self.apply_operation_inputs(state, (operation, rows), refinements, invalidation, accesses);
                    }

                    state.memory.finish_value_transfers();

                    return None;
                }
                bray_bound_tree::BoundExecutionSite::CallInput { .. }
                | bray_bound_tree::BoundExecutionSite::Iterate(_)
                | bray_bound_tree::BoundExecutionSite::Next(_)
                | bray_bound_tree::BoundExecutionSite::ScopedEnter(_)
                | bray_bound_tree::BoundExecutionSite::ScopedExit(_) => {
                    if phase == AnalysisCallPhase::Attempt {
                        if !admitted {
                            self.apply_operation_inputs(state, (operation, rows), refinements, invalidation, accesses);
                        }
                    } else {
                        if let Some(result) = self.invocation_result(invocation) {
                            state.initialize_path(&result);
                        }

                        state.memory.complete_transition(invocation);

                    }

                    state.memory.finish_value_transfers();

                    return None;
                }
                bray_bound_tree::BoundExecutionSite::Node(_) => {}
                bray_bound_tree::BoundExecutionSite::Cleanup(_) => {
                    panic!("cleanup suspension is not a call phase")
                }
            }
        }

        let expression = operation.kind().occurrence().expression();

        let awaited = expression.filter(|expression| {
            matches!(
                self.request.view().expression(*expression),
                Some(bray_bound_tree::BoundExpression::Await(_))
            )
        });

        if awaited.is_some() && matches!(operation.kind(), AnalysisOperationKind::Suspension { .. })
        {
            return None;
        }

        self.initialize_operation_storage(state, operation.kind());

        let storage = self.storage;

        for plan in operation.kind().storage_plans(storage).filter(|plan|
            !storage.access_is_produced_at(plan.access(), operation.kind().occurrence())) {
            let _ = self.apply_plan(state, (operation.id(), rows), plan, refinements.clone(), invalidation, accesses.remove(&plan));
        }

        let memory = if matches!(operation.kind(), AnalysisOperationKind::Result { .. }) {
            None
        } else {
            self.apply_memory_operation(state, operation, rows, refinements.clone())
        };

        if memory.as_ref().is_none_or(|(status, _)| *status == MemoryOperationStatus::Valid) {
            if memory.is_some() {
                let occurrence = operation.kind().occurrence();

                if let Some(result) = self.invocation_result(occurrence)
                    && state.memory.has_stored_value(&result)
                {
                    let result = crate::analysis::storage_flow::memory::Target::storage(result);

                    // Intrinsics produce current values. Admit that result into the
                    // same pending transfer used by ordinary call completion.
                    state
                        .memory
                        .stage_value(occurrence, &result, result.clone());
                }

                // A completed reference has an identity even when its origin is opaque.
                self.prepare_intrinsic_result_references(state, occurrence);
            }

            state.memory.complete_transition(operation.kind().occurrence());

            self.transfer_memory_result_state(state, operation.kind().node());

            // Result accesses can reach storage through a newly returned borrow.
            // Resolve and validate them only after that borrow has been published.
            for plan in operation.kind().storage_plans(storage).filter(|plan|
                storage.access_is_produced_at(plan.access(), operation.kind().occurrence())) {
                let reached = self.apply_plan(state, (operation.id(), rows), plan, refinements.clone(), invalidation, None);

                if plan.purpose().transfers_value() && let Some(target) = reached {
                    produced.insert(plan, target);
                }
            }
        }

        state.memory.finish_value_transfers();

        if let crate::CheckerUnitRoot::Expression(root) = self.request.root()
            && operation.kind().node() == root.into()
            && matches!(
                self.request.semantic_context(),
                crate::SemanticUnitContext::RuntimeDefault(_)
            )
        {
            self.report_escaping_default_storage(state, root);
        }

        memory
    }

    fn apply_operation_inputs<'refinements>(
        &mut self,
        state: &mut StorageFlowState,
        (operation, rows): (&AnalysisOperation, Option<&StorageOutcomeRows>),
        refinements: impl Iterator<Item = &'refinements Refinement> + Clone,
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
        accesses: &mut OperationAccesses,
    ) {
        for plan in operation.kind().storage_plans(self.storage) {
            let _ = self.apply_plan(state, (operation.id(), rows), plan, refinements.clone(), invalidation, accesses.remove(&plan));
        }
    }

    fn apply_plan<'refinements>(
        &mut self,
        state: &mut StorageFlowState,
        operation: (AnalysisOperationId, Option<&StorageOutcomeRows>),
        plan: StorageAccessPlan,
        refinements: impl Iterator<Item = &'refinements Refinement> + Clone,
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
        reached: Option<Option<ReachedAccess>>,
    ) -> Option<Target> {
        let purpose = self.storage.effective_purpose(plan);
        let access = self.operation_access(plan, purpose);

        let mut targets = reached.unwrap_or_else(|| {
            if matches!(purpose, StorageAccessPurpose::Assignment | StorageAccessPurpose::Write)
                && let Some(expression) = plan.occurrence().expression()
            {
                self.assignment_destination(&state.memory, expression, access).map(Cow::into_owned)
            } else {
                let execution = state.execution.as_ref().expect("a reached storage access retains its evaluated operands");

                self.evaluated_access(&state.memory, execution, access)
            }
        });

        let borrow = self.storage.borrow_for_plan(plan);

        let formed = if matches!(purpose, StorageAccessPurpose::Borrow(_)) {
            borrow.and_then(|loan| self.prepare_formed_borrow(state, plan, loan, &mut targets))
        } else {
            None
        };

        let outcome = self.operation_status(state, plan, purpose, refinements.clone(), targets.as_ref());

        let mut status = outcome.status;

        if matches!(
            status,
            StorageOperationStatus::Uninitialized
                | StorageOperationStatus::InactiveProjection
                | StorageOperationStatus::NotCopyable
        ) {
            if state.recovered {
                // Recovery has cleared these guarantees. Other established storage
                // facts still participate in borrow and authority checking.
                status = StorageOperationStatus::Recovered;
            } else {
                panic!(
                    "Storage-flow analysis produced a state forbidden for one exact planned source operation. in apply_plan, unit: {:?}, origin: {:?}, expression: {:?}, access: {:?}, purpose: {:?}, status: {:?}, root: {:?}, initialized: {:?}, root id: {:?}, projections: {:?}, borrow chain: {:?}, active borrows: {:?}, refinements: {:?}",
                    self.request.unit().key(),
                    self.request
                        .view()
                        .expression(plan.expression())
                        .map(|expression| expression.origin()),
                    plan.expression(),
                    plan.access(),
                    purpose,
                    status,
                    self.storage
                        .root_identity(plan.access())
                        .and_then(|identity| self.storage.identity(identity)),
                    state.initialized,
                    self.storage.root_identity(plan.access()),
                    self.storage.resolved_projections(plan.access()),
                    self.borrow_chain(plan.access()).map(|chain| chain
                        .into_iter()
                        .map(|id| (id, self.storage.borrow_capability(id)))
                        .collect::<Vec<_>>()),
                    state.definitely_active_borrows,
                    refinements.collect::<Vec<_>>()
                );
            }
        }

        let target = if self.publish {
            let target = targets.as_ref().filter(|_| status == StorageOperationStatus::Valid)
                .and_then(|targets| targets.address.single_location());

            // Diagnostics and the transition publication share the same resolved path.
            self.record_state_target(operation, StorageStateTarget::Access(plan), target.cloned());

            target.filter(|_| plan.purpose().transfers_value()
                && self.storage.access_is_produced_at(plan.access(), plan.occurrence())).cloned()
        } else { None };

        if status == StorageOperationStatus::Valid {
            if let Err(error) =
                self.apply_valid_operation(state, plan, purpose, (borrow, formed), targets, invalidation)
            {
                self.fail_query(error);

                return None;
            }
        } else if matches!(status, StorageOperationStatus::Recovered) {
            self.is_recovered = true;
        }

        if !self.publish {
            return None;
        }

        self.statuses
            .entry(plan)
            .and_modify(|current| *current = more_conservative(*current, status))
            .or_insert(status);

        if let Some(kind) = diagnostic_kind(status) {
            self.add_diagnostic(kind, plan, purpose, outcome.origin);
        }

        target
    }

    fn operation_status<'refinements>(
        &self,
        state: &StorageFlowState,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
        refinements: impl Iterator<Item = &'refinements Refinement> + Clone,
        targets: Option<&ReachedAccess>,
    ) -> StorageOperationOutcome {
        let Some(access) = self.storage.access(plan.access()) else {
            return StorageOperationOutcome::status(StorageOperationStatus::Recovered);
        };

        if access.is_recovered() || self.refinements.is_recovered() {
            return StorageOperationOutcome::status(StorageOperationStatus::Recovered);
        }

        if !self.pattern_establishes_projection(plan.access())
            && !self.refinements_allow_access(state, plan.access(), refinements.clone())
        {
            return StorageOperationOutcome::status(StorageOperationStatus::InactiveProjection);
        }

        let Some(root) = self.storage.root_identity(plan.access()) else {
            return StorageOperationOutcome::status(StorageOperationStatus::Recovered);
        };

        let requires_value = matches!(
            purpose,
            StorageAccessPurpose::Read
                | StorageAccessPurpose::Copy
                | StorageAccessPurpose::Move
                | StorageAccessPurpose::Borrow(_)
        );

        let pattern_establishes_projection = self.pattern_establishes_projection(plan.access());

        if requires_value && !pattern_establishes_projection {
            let origins = self.moved_origins(state, plan.access());

            if !origins.is_empty() {
                return StorageOperationOutcome::moved(origins);
            }
        }

        if requires_value
            && !pattern_establishes_projection
            && self.retained_borrow_value(state, plan.access()).is_none()
            && !state.initialized.contains(&root)
            && !state.observed_pattern_bindings.contains(&root)
        {
            return StorageOperationOutcome::status(StorageOperationStatus::Uninitialized);
        }

        let operation_access = self.operation_access(plan, purpose);

        if purpose == StorageAccessPurpose::Move
            && self.access_uses_borrow(operation_access)
            && !self.type_is_borrow(access.reached_type())
        {
            return StorageOperationOutcome::status(StorageOperationStatus::MissingOwnership);
        }

        if let Some(conflict) = self.borrow_conflict(state, plan, purpose, targets) {
            return StorageOperationOutcome::borrow_conflict(conflict);
        }

        if let Some(authority_access) =
            self.input
                .mutation_authority_access(plan, purpose, self.storage)
            && (!self.has_mutation_authority(authority_access)
                || !self.input.fields_allow_mutation(authority_access))
        {
            return StorageOperationOutcome::status(
                StorageOperationStatus::MissingMutationAuthority,
            );
        }

        if purpose == StorageAccessPurpose::Copy
            && !self
                .storage
                .copyable_types()
                .contains(&access.reached_type())
        {
            return StorageOperationOutcome::status(StorageOperationStatus::NotCopyable);
        }

        StorageOperationOutcome::status(StorageOperationStatus::Valid)
    }

    fn apply_valid_operation(
        &self,
        state: &mut StorageFlowState,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
        (borrow, formed): (Option<BorrowCapabilityId>, Option<bray_symbols::TypeId>),
        targets: Option<ReachedAccess>,
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        match purpose {
            StorageAccessPurpose::Move
            | StorageAccessPurpose::Initialize
            | StorageAccessPurpose::Assignment => {
                let reached = targets.as_ref().and_then(|access| access.address.single_location());
                let observed = reached.and_then(|target| target.value_observation_path().or_else(|| target.storage_path().cloned()));

                let logical = observed.as_ref().or_else(|| {
                    (purpose == StorageAccessPurpose::Move).then(|| self.storage.logical_path(plan.access())).flatten()
                });

                let ty = self
                    .storage
                    .access(plan.access())
                    .expect("checked storage transition has its access")
                    .reached_type();

                if let Some(place) = logical {
                    if purpose == StorageAccessPurpose::Move {
                        // Ownership tracking retains the selected path independently of the memory transition.
                        state.record_move(place.clone(), plan.expression(), ty, true);

                    } else {
                        state.initialize_path(place);
                    }
                }

                if purpose == StorageAccessPurpose::Move && let Some(execution) = state.execution.as_mut() {
                    self.execution.observe_storage_move(execution, plan, reached);
                }

                if let Some(ReachedAccess { address: targets, .. }) = targets {
                    if purpose == StorageAccessPurpose::Move {
                        let admitted_input = self.storage.access_plan_role(plan)
                            == bray_bound_tree::StorageAccessPlanRole::PrivateInputAdmission;

                        // Admission transfers the private input to this invocation. Its
                        // protected payload remains initialized until the callee uses it.
                        if !admitted_input {
                            state.memory.consume_locations(&targets, ty);
                        }
                    } else {
                        state
                            .memory
                            .write_locations(&targets, ty, plan.expression(), invalidation);
                    }
                } else {
                    // A lifetime anchor cannot stand in for an unresolved physical target.
                    state.memory.invalidate(
                        &bray_bound_tree::StorageInvalidation::unknown(),
                        if purpose == StorageAccessPurpose::Move {
                            bray_symbols::DependencyMutationKind::Consume
                        } else {
                            bray_symbols::DependencyMutationKind::Write
                        },
                    );
                }
            }
            StorageAccessPurpose::Borrow(_) => {
                if let Some(borrow) = borrow {
                    state.active_borrows.insert(borrow);
                    state.definitely_active_borrows.insert(borrow);

                    if let (Some(ty), Some(reached)) = (formed, targets) {
                        let mut ancestors = crate::analysis::storage_flow::memory::borrow_chain_from(self.storage, Some(&state.memory), reached.loan)
                            .expect("a successfully checked loan retains its authorizing ancestry");

                        ancestors.retain(|ancestor| *ancestor != borrow);

                        state.memory.borrow_reached_storage(
                            self.request.semantic_values(), borrow, reached.address, ty, ancestors.into(),
                        );
                    }
                }
            }
            StorageAccessPurpose::Read
            | StorageAccessPurpose::Write
            | StorageAccessPurpose::Copy
            | StorageAccessPurpose::ValueTransfer
            | StorageAccessPurpose::Member
            | StorageAccessPurpose::Index
            | StorageAccessPurpose::Slice
            | StorageAccessPurpose::Projection => {
                if plan.purpose() != StorageAccessPurpose::Write
                    && let Some((ty, _)) = self.storage.reference_route(plan.access())
                    && let Some(pointee) =
                        crate::representation::reference_target_type(self.request, *ty)
                    && let Some(logical) = self.storage.logical_path(plan.access())
                    && !logical.projections.is_empty()
                    && let Some(ReachedAccess { address: targets, .. }) = targets.filter(|access| access.address.has_values())
                {
                    // Keep the value read from a reference field separate from its slot.
                    for target in targets.targets {
                        self.observe_stored_reference(&mut state.memory, target, logical.clone(), pointee);
                    }
                }
            }
        }

        Ok(())
    }

    fn pattern_establishes_projection(&self, access: StorageAccessId) -> bool {
        let source_is_pattern = |access: StorageAccessId| {
            self.storage.access(access).is_some_and(|access| {
                matches!(
                    access.source().syntax().syntax_kind(),
                    bray_syntax::SyntaxKind::IrrefutablePattern
                        | bray_syntax::SyntaxKind::IrrefutablePatternEntry
                        | bray_syntax::SyntaxKind::CasePattern
                        | bray_syntax::SyntaxKind::CasePatternEntry
                )
            })
        };

        source_is_pattern(access)
            || self.storage.bindings().iter().any(|(_, binding)| {
                let StorageBinding::Access(binding) = binding else {
                    return false;
                };

                source_is_pattern(*binding)
                    && self.storage.relationship(*binding, access) == StorageRelationship::Identical
            })
    }

    fn initialize_operation_storage(
        &self,
        state: &mut StorageFlowState,
        operation: AnalysisOperationKind,
    ) {
        if matches!(operation, AnalysisOperationKind::AssignmentInstall(_)) {
            return;
        }

        let completes = !matches!(
            operation,
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Input,
                ..
            } | AnalysisOperationKind::Result {
                phase: AnalysisResultPhase::Begin,
                ..
            }
        );

        let definitions = self.input.definitions(operation.occurrence());

        let begins = match operation {
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Attempt,
                ..
            } | AnalysisOperationKind::Result {
                phase: AnalysisResultPhase::Production,
                ..
            } => return,
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Completion,
                ..
            }
            | AnalysisOperationKind::Result {
                phase: AnalysisResultPhase::Completion,
                ..
            } => false,
            _ => true,
        };

        if !begins {
            if completes {
                state.live.extend(definitions.iter().copied());
                state.initialized.extend(definitions.iter().copied());
            }

            return;
        }

        let defines =
            |identity: &bray_bound_tree::StorageIdentityId| definitions.contains(identity);

        for identity in definitions {
            state.memory.begin(*identity);
        }

        state
            .observed_pattern_bindings
            .retain(|identity| !defines(identity));

        state.moved.retain(|path, _| !defines(&path.root));

        if !state.definitely_absent.is_empty() {
            std::sync::Arc::make_mut(&mut state.definitely_absent).retain(|path, _| !defines(&path.root));
        }

        state.fully_moved.retain(|storage| !defines(storage));

        // Reserving result storage does not create an owner. A successful commit,
        // including an earlier result yield, establishes its cleanup obligations.
        if completes {
            state.live.extend(definitions.iter().copied());
            state.initialized.extend(definitions.iter().copied());
        } else {
            state.live.retain(|identity| !defines(identity));
            state.initialized.retain(|identity| !defines(identity));
        }
    }

    fn moved_origins(
        &self,
        state: &StorageFlowState,
        access: StorageAccessId,
    ) -> Vec<BoundExpressionId> {
        let logical = self.storage.logical_path(access);

        let retained = self.retained_borrow_value(state, access)
            .and_then(|borrow| state.memory.borrow_regions(borrow));

        let mut origins = state
            .moved
            .iter()
            .flat_map(|(moved, origins)| {
                origins.iter().filter_map(|(origin, ty)| {
                    // Moving a borrow transfers its slot, not the storage it reaches.
                    let relationship = if self.type_is_borrow(*ty) {
                        let descriptor = crate::analysis::storage_flow::memory::Target::storage(moved.clone());

                        // A formed loan retains its reached storage after its source descriptor retires.
                        // Use the captured physical bounds, never the retired descriptor's logical path.
                        if retained.as_ref().is_some_and(|targets| !targets.is_empty()
                            && targets.iter().all(|target| state.memory.target_relationship(target, &descriptor)
                                == StorageRelationship::Disjoint))
                        {
                            StorageRelationship::Disjoint
                        } else {
                            logical.map_or(StorageRelationship::Error, |path| path.relationship(moved))
                        }
                    } else {
                        self.storage.relationship_to_path(access, moved)
                    };

                    (relationship != StorageRelationship::Disjoint).then_some(*origin)
                })
            })
            .collect::<Vec<_>>();

        origins.sort_unstable();
        origins.dedup();

        origins
    }

    fn end_last_use_storage(
        &self,
        state: &mut StorageFlowState,
        operation: bray_bound_tree::BoundExecutionSite,
    ) {
        if state.recovered {
            return;
        }

        for identity in self.input.storage_last_uses(operation) {
            state.end_storage_lifetime(*identity, self.storage, self.input);
        }
    }

    fn end_last_use_borrows(
        &self,
        state: &mut StorageFlowState,
        operation: bray_bound_tree::BoundExecutionSite,
    ) {
        if state.active_borrows.is_empty() {
            return;
        }

        let moved_borrows = state
            .moved
            .keys()
            .filter_map(|access| {
                self.storage
                    .access(access.access_id(self.storage)?)?
                    .root()
                    .borrow_capability()
            })
            .collect::<BTreeSet<_>>();

        let ended = state
            .active_borrows
            .iter()
            .copied()
            .filter(|borrow| {
                let Some(capability) = self.storage.borrow_capability(*borrow) else {
                    return false;
                };

                let subject = BoundDependencySubject::BorrowCapability(*borrow);

                let retained_for_suspension = self.input.suspension_borrows.contains(borrow);

                let completed_retaining_suspension =
                    self.liveness.is_live_across_suspension(operation, subject);

                let reaches_last_use = self.liveness.is_last_use(operation, subject);

                capability.entry_binding().is_none()
                    && ((moved_borrows.contains(borrow)
                        && !self.liveness.is_owner_retained(subject)
                        && (!retained_for_suspension || completed_retaining_suspension))
                        || reaches_last_use)
            })
            .collect::<BTreeSet<_>>();

        state.end_borrows(ended, self.storage, self.input);
    }

    fn end_scope(
        &self,
        state: &mut StorageFlowState,
        block: bray_bound_tree::StorageScope,
        exit: AnyBoundNodeId,
    ) {
        let retiring = match &block {
            bray_bound_tree::StorageScope::AbandonedTransfer { value, .. } => {
                state.result_dependencies.abandon_transfer(self.storage, *value);

                std::slice::from_ref(value)
            }
            _ => self.execution.disposal.requirements.retiring_storage(block),
        };

        state.memory.end_storage(retiring);

        for identity in retiring {
            state.live.remove(identity);
        }

        state.retain_live_storage();

        let ended = state
            .active_borrows
            .iter()
            .copied()
            .filter(|borrow| {
                !self.borrow_is_entry(*borrow) && !self.liveness.is_live_across_scope(
                    block,
                    exit,
                    BoundDependencySubject::BorrowCapability(*borrow),
                )
            }).collect();

        state.end_borrows(ended, self.storage, self.input);
    }

    fn borrow_is_entry(&self, borrow: BorrowCapabilityId) -> bool {
        self.storage
            .borrow_capability(borrow)
            .is_some_and(|capability| capability.entry_binding().is_some())
    }

    fn collect_unreachable_suspensions(
        &mut self,
        graph: &crate::analysis::model::ControlFlowGraph,
        block: &crate::analysis::model::AnalysisBlock,
    ) {
        for operation in block.operations() {
            if let Some(occurrence) = graph
                .operation(*operation)
                .expect("storage-flow block retains its operation")
                .suspension()
            {
                self.suspensions
                    .entry(occurrence)
                    .or_insert_with(|| StorageSuspensionState::unreachable(occurrence));
            }
        }
    }

    pub(super) fn record_state_target(
        &mut self,
        (operation, rows): (AnalysisOperationId, Option<&StorageOutcomeRows>),
        access: StorageStateTarget,
        target: Option<Target>,
    ) {
        self.state_targets
            .entry(((access, operation), rows.cloned().unwrap_or_default()))
            .and_modify(|previous| {
                if previous.as_ref() != target.as_ref() {
                    *previous = None;
                }
            })
            .or_insert(target);
    }

    fn take_state_targets(
        &mut self,
    ) -> BTreeMap<((StorageStateTarget, AnalysisOperationId), StorageOutcomeRows), Option<Target>> {
        std::mem::take(&mut self.state_targets)
    }

    pub(super) fn record_replacement(&mut self, plan: StorageAccessPlan, incoming: ReplacementState) {
        match self.replacements.entry(plan) {
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                entry.get_mut().merge(incoming);
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(incoming);
            }
        }
    }

    fn replacement_decisions(
        &self,
    ) -> impl Iterator<Item = bray_bound_tree::StorageReplacementDecision> + '_ {
        self.replacements
            .iter()
            .filter(|(plan, _)| self.statuses.get(plan) == Some(&StorageOperationStatus::Valid))
            .map(|(plan, state)| {
                bray_bound_tree::StorageReplacementDecision::new(
                    plan.expression(),
                    plan.access(),
                    state.state(),
                    state.moved.iter().cloned(),
                    state.definitely_absent.iter().cloned(),
                    state.initialized,
                    state.recovered,
                )
            })
    }

    fn record_suspension(
        &mut self,
        state: &StorageFlowState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        retained: impl IntoIterator<Item = bray_bound_tree::BoundDependencySubject>,
        unavailable: impl IntoIterator<Item = (bray_bound_tree::BoundDependencySubject, bray_bound_tree::BoundDependencyRequirementKind)>,
    ) {
        if !self.publish {
            return;
        }

        let snapshot = StorageSuspensionState::new(
            occurrence,
            state.live.iter().copied(),
            state.initialized.iter().copied(),
            state.moved.keys().cloned(),
            state.definitely_active_borrows.iter().copied(),
            state.retained_dependencies(self.input),
        ).with_execution_dependencies(retained, unavailable);

        match self.suspensions.entry(occurrence) {
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                merge_suspension(entry.get_mut(), snapshot);
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(snapshot);
            }
        }
    }

    fn add_diagnostic(
        &mut self,
        kind: DiagnosticKind,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
        origin: Option<StorageOperationOrigin>,
    ) {
        if self.query_failed {
            return;
        }

        let access_id = plan.access();

        let Some(access) = self.storage.access(access_id) else {
            panic!(
                "A planned storage access is absent from its source body's storage plan. in add_diagnostic, access: {:?}",
                access_id
            );
        };

        let source = self.request.source(access.source());

        let mut diagnostic = Diagnostic::new(
            bray_diagnostics::DiagnosticId::new(0),
            kind,
            SeverityKind::Error,
        )
        .with_primary_span(source.span())
        .with_label(DiagnosticLabel::primary(
            storage_diagnostic_label(kind),
            source.span(),
        ));

        if let Some(origin) = origin {
            diagnostic = self.with_operation_origins(diagnostic, source.span(), origin);
        }

        self.diagnostics.record_with_argument(
            DiagnosticSubject::Storage(kind, access_id),
            diagnostic,
            Some(DiagnosticArgument::StorageAccess(access_id, purpose)),
        );
    }

    pub(super) fn diagnostic_storage_access(
        &self,
        access_id: StorageAccessId,
        purpose: StorageAccessPurpose,
    ) -> Result<DiagnosticStorageAccess, CheckerQueryError<C::UpstreamError>> {
        let access =
            self.storage
                .access(access_id).unwrap_or_else(|| panic!("diagnostic_storage_access requires planned storage access, access_id: {access_id:?}"));

        let root = match access.root() {
            StorageAccessRoot::Storage(storage) => self.diagnostic_storage_identity(storage),
            StorageAccessRoot::Borrow(_) => DiagnosticStorageRoot::Borrow,
            StorageAccessRoot::BorrowedStorage { .. } => DiagnosticStorageRoot::BorrowedStorage,
            StorageAccessRoot::OwnedIndirection { .. } => DiagnosticStorageRoot::OwnedIndirection,
            StorageAccessRoot::Recovery(_) => DiagnosticStorageRoot::Recovery,
        };

        let projections = access
            .projections()
            .iter()
            .map(|projection| self.diagnostic_storage_projection(*projection))
            .collect::<Result<Vec<_>, _>>()?;

        let reached_type =
            crate::diagnostic::diagnostic_type(self.request.context(), access.reached_type())?;

        Ok(DiagnosticStorageAccess::new(
            diagnostic_storage_purpose(purpose),
            root,
            projections,
            reached_type,
        ))
    }

    fn diagnostic_storage_identity(
        &self,
        identity: bray_bound_tree::StorageIdentityId,
    ) -> DiagnosticStorageRoot {
        match self.storage.identity(identity) {
            Some(StorageIdentity::LocalOwned(_)) => DiagnosticStorageRoot::Local,
            Some(StorageIdentity::Parameter(_) | StorageIdentity::ContractParameter(_)) => {
                DiagnosticStorageRoot::Parameter
            }
            Some(StorageIdentity::Receiver(_)) => DiagnosticStorageRoot::Receiver,
            Some(StorageIdentity::Static(_)) => DiagnosticStorageRoot::Static,
            Some(StorageIdentity::AnonymousParameter(_)) => {
                DiagnosticStorageRoot::AnonymousParameter
            }
            Some(StorageIdentity::PredicateParameter(_)) => {
                DiagnosticStorageRoot::PredicateParameter
            }
            Some(StorageIdentity::PostconditionResult(_)) => {
                DiagnosticStorageRoot::PostconditionResult
            }
            Some(StorageIdentity::Result(_)) => DiagnosticStorageRoot::Result,
            Some(StorageIdentity::Temporary(_) | StorageIdentity::Evaluated(_) | StorageIdentity::InvocationInput { .. }) => {
                DiagnosticStorageRoot::Temporary
            }
            Some(StorageIdentity::CustomIndexBorrow(_)) => DiagnosticStorageRoot::CustomIndexBorrow,
            Some(StorageIdentity::InvocationResult(
                bray_bound_tree::BoundExecutionSite::Iterate(_),
            )) => DiagnosticStorageRoot::IterationCursor,
            Some(StorageIdentity::InvocationResult(bray_bound_tree::BoundExecutionSite::Next(
                _,
            ))) => DiagnosticStorageRoot::IterationElement,
            Some(StorageIdentity::InvocationResult(_) | StorageIdentity::ScopedResultReplacement(_)) => DiagnosticStorageRoot::Temporary,
            Some(StorageIdentity::ScopedCapability { .. }) => {
                DiagnosticStorageRoot::ScopedCapability
            }
            Some(StorageIdentity::Allocation(_)) => DiagnosticStorageRoot::Allocation,
            Some(StorageIdentity::CompilerCreated(_)) => DiagnosticStorageRoot::CompilerCreated,
            Some(StorageIdentity::Alternative { .. }) => DiagnosticStorageRoot::Alternative,
            Some(StorageIdentity::Error(_)) => DiagnosticStorageRoot::Recovery,
            None => {
                panic!(
                    "A planned storage identity is absent from its source body's storage plan. in diagnostic_storage_identity, identity: {:?}",
                    identity
                );
            }
        }
    }

    fn diagnostic_storage_projection(
        &self,
        projection: StorageProjection,
    ) -> Result<DiagnosticStorageProjection, CheckerQueryError<C::UpstreamError>> {
        let projection = match projection {
            StorageProjection::ProductField(field) => DiagnosticStorageProjection::ProductField(
                self.diagnostic_symbol_name(field.into())?,
            ),
            StorageProjection::TupleElement(ordinal) => {
                DiagnosticStorageProjection::TupleElement(u64::from(ordinal.raw()))
            }
            StorageProjection::ElementFromStart(ordinal) => {
                DiagnosticStorageProjection::ElementFromStart(u64::from(ordinal.raw()))
            }
            StorageProjection::ElementFromEnd(ordinal) => {
                DiagnosticStorageProjection::ElementFromEnd(u64::from(ordinal.raw()))
            }
            StorageProjection::ActiveUnionPayloadField { variant, field } => {
                DiagnosticStorageProjection::ActiveUnionPayloadField {
                    variant: self.diagnostic_symbol_name(variant.into())?,
                    field: self.diagnostic_symbol_name(field.into())?,
                }
            }
            StorageProjection::Element(_) | StorageProjection::ElementFromInput(_) | StorageProjection::AnyElement => DiagnosticStorageProjection::IndexedElement,
            StorageProjection::SliceRange { start, end } => {
                DiagnosticStorageProjection::SliceRange {
                    has_start: start.is_some(),
                    has_end: end.is_some(),
                }
            }
            StorageProjection::NullableValue => DiagnosticStorageProjection::NullableValue,
            StorageProjection::OwnedTarget => DiagnosticStorageProjection::OwnedTarget,
            StorageProjection::AddressTarget => DiagnosticStorageProjection::AddressTarget,
            StorageProjection::UninitPayload => DiagnosticStorageProjection::UninitPayload,
        };

        Ok(projection)
    }

    fn diagnostic_symbol_name(
        &self,
        symbol: AnySymbolId,
    ) -> Result<String, CheckerQueryError<C::UpstreamError>> {
        Ok(self
            .request
            .member_name(symbol)?
            .map(|name| name.as_str().to_owned())
            .unwrap_or_else(|| {
                panic!("checked storage symbol {symbol:?} must retain its member name")
            }))
    }

    fn with_operation_origins(
        &self,
        mut diagnostic: Diagnostic,
        primary: bray_source::SourceSpan,
        origin: StorageOperationOrigin,
    ) -> Diagnostic {
        let (kind, sources) = match origin {
            StorageOperationOrigin::Borrows(borrows) => (
                DiagnosticRelatedLocationKind::BorrowOrigin,
                borrows
                    .into_iter()
                    .filter_map(|borrow| self.storage.borrow_capability(borrow)?.source().into())
                    .collect::<Vec<_>>(),
            ),
            StorageOperationOrigin::Moves(expressions) => (
                DiagnosticRelatedLocationKind::MoveOrigin,
                expressions
                    .into_iter()
                    .filter_map(|expression| {
                        Some(
                            self.request
                                .view()
                                .expression(expression)?
                                .origin()
                                .source_anchor(),
                        )
                    })
                    .collect::<Vec<_>>(),
            ),
        };

        for related in sources
            .into_iter()
            .map(|source| self.request.source(source))
            .filter(|related| related.span() != primary)
        {
            diagnostic = diagnostic
                .with_related_location(DiagnosticRelatedLocation::new(kind, related.span()));
        }

        diagnostic
    }

    fn decisions(&self) -> impl Iterator<Item = StorageOperationDecision> + '_ {
        self.storage.access_plans().iter().copied().map(|plan| {
            StorageOperationDecision::new(
                plan,
                self.storage.effective_purpose(plan),
                self.storage.borrow_for_plan(plan),
                self.statuses
                    .get(&plan)
                    .copied()
                    .unwrap_or(StorageOperationStatus::Unreachable),
            )
        })
    }
}

fn storage_diagnostic_label(kind: DiagnosticKind) -> DiagnosticLabelKind {
    match kind {
        DiagnosticKind::CheckingUseOfMovedStorage => DiagnosticLabelKind::MovedStorageUse,
        DiagnosticKind::CheckingConflictingBorrow => {
            DiagnosticLabelKind::ConflictingBorrowOperation
        }
        DiagnosticKind::CheckingMissingMutationAuthority => {
            DiagnosticLabelKind::MissingMutationAuthority
        }
        DiagnosticKind::CheckingMissingStorageOwnership => {
            DiagnosticLabelKind::MissingStorageOwnership
        }
        _ => unreachable!("storage-flow diagnostics must describe a storage operation"),
    }
}

fn diagnostic_storage_purpose(purpose: StorageAccessPurpose) -> DiagnosticStorageAccessPurpose {
    match purpose {
        StorageAccessPurpose::Read => DiagnosticStorageAccessPurpose::Read,
        StorageAccessPurpose::Initialize => DiagnosticStorageAccessPurpose::Initialize,
        StorageAccessPurpose::Write => DiagnosticStorageAccessPurpose::Write,
        StorageAccessPurpose::Move => DiagnosticStorageAccessPurpose::Move,
        StorageAccessPurpose::Copy | StorageAccessPurpose::ValueTransfer => {
            DiagnosticStorageAccessPurpose::Copy
        }
        StorageAccessPurpose::Borrow(BorrowKind::Shared) => {
            DiagnosticStorageAccessPurpose::SharedBorrow
        }
        StorageAccessPurpose::Borrow(BorrowKind::Mutable) => {
            DiagnosticStorageAccessPurpose::MutableBorrow
        }
        StorageAccessPurpose::Assignment => DiagnosticStorageAccessPurpose::Assignment,
        StorageAccessPurpose::Member => DiagnosticStorageAccessPurpose::MemberSelection,
        StorageAccessPurpose::Index => DiagnosticStorageAccessPurpose::IndexSelection,
        StorageAccessPurpose::Slice => DiagnosticStorageAccessPurpose::SliceSelection,
        StorageAccessPurpose::Projection => DiagnosticStorageAccessPurpose::Projection,
    }
}
