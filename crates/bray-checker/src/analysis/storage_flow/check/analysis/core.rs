// rust-style: allow(module-too-large, reason = "storage flow collection is one transition engine whose diagnostics require the same plan, state, and decision context")

use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BorrowCapabilityId, BoundDependencySubject, BoundExpressionId,
    CheckedMemoryOperations, CheckedRefinements, Liveness, MemoryOperationStatus, Refinement,
    StorageAccessId, StorageAccessPlan, StorageAccessPurpose, StorageAccessRoot, StorageBinding,
    StorageExitDecision, StorageExitPoint, StorageFlow, StorageIdentity, StorageOperationDecision,
    StorageOperationStatus, StoragePlan, StorageProjection, StorageRelationship,
    StorageSuspensionState,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, DiagnosticStorageAccess,
    DiagnosticStorageAccessPurpose, DiagnosticStorageProjection, DiagnosticStorageRoot,
    SeverityKind,
};
use bray_symbols::{AnySymbolId, BorrowKind, CallableSignatureQuery};

use crate::analysis::storage_flow::model::StorageAbsence;
use crate::diagnostic::diagnostic_id;
use crate::storage::{StorageScopeOwners, storage_scope_owners};
use crate::{
    CheckerOutcome, CheckerQueryError, CheckerRequestContext, CheckerSemanticQueryProvider,
    CheckerUnitView,
};

use super::super::availability::storage_is_recovered;
use crate::analysis::fixed_point::{FixedPointOutcome, solve_fixed_point};
use crate::analysis::id::AnalysisEdgeId;
use crate::analysis::model::{
    AnalysisCallPhase, AnalysisOperation, AnalysisOperationKind, AnalysisScopeExitPhase,
};
use crate::analysis::reachability::ReachabilityResult;
use crate::analysis::storage_flow::authority::mutable_storage;
use crate::analysis::storage_flow::decision::{diagnostic_kind, more_conservative};
use crate::analysis::storage_flow::model::{StorageFlowDomain, StorageFlowInput, StorageFlowState};

use super::access::BorrowConflict;
use super::snapshot::{ExitState, ReplacementState};

#[expect(
    clippy::too_many_arguments,
    reason = "the analysis consumes correlated durable inputs and one shared control-flow graph"
)]
pub(crate) fn check_storage_flow_with_graph<C>(
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
) -> CheckerOutcome<(StorageFlow, CheckedRefinements), C::UpstreamError>
where
    C: CheckerRequestContext + CheckerSemanticQueryProvider<CallableSignatureQuery> + ?Sized,
{
    let ((input, cleanup_diagnostics), authority_diagnostics) =
        match mutable_storage(request, storage).and_then(|(mutable, diagnostics)| {
            StorageFlowInput::new(
                cleanup_shapes,
                request,
                storage,
                mutable,
                liveness,
                dependencies,
                values,
                execution.semantics.selections(),
                execution.disposal.plan,
                memory,
            )
            .map(|prepared| (prepared, diagnostics))
        }) {
            Ok(result) => result,
            Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
            Err(CheckerQueryError::Upstream(error)) => {
                return CheckerOutcome::UpstreamFailure(error);
            }
        };

    let owners = match storage_scope_owners(request).map_err(CheckerQueryError::with_upstream) {
        Ok(owners) => owners,
        Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Upstream(error)) => {
            return CheckerOutcome::UpstreamFailure(error);
        }
    };

    let domain = StorageFlowDomain::new(
        &graph,
        edge_lifetime_ends,
        &reachability,
        storage,
        liveness,
        refinements,
        memory,
        &input,
        &owners,
        request,
        execution,
        dependencies,
        values,
        patterns,
    );

    let result = match solve_fixed_point(&graph, &domain, &request) {
        FixedPointOutcome::Complete(result) => result,
        FixedPointOutcome::Cancelled => return CheckerOutcome::Cancelled,
        FixedPointOutcome::ConvergenceInvariantViolated => {
            panic!("finite storage-flow analysis exceeded its convergence bound")
        }
    };

    let mut collector = StorageFlowCollector::new(
        request,
        storage,
        liveness,
        refinements,
        memory,
        &input,
        &owners,
        execution,
        dependencies,
        values,
        patterns,
    );

    collector.trusted = crate::analysis::guarantee::TrustedChecks::new(execution);
    collector.diagnostics.add_range(authority_diagnostics);
    collector.diagnostics.add_range(cleanup_diagnostics);

    let mut conditions = BTreeMap::<_, Option<bool>>::new();
    let mut reachable_blocks = BTreeSet::new();
    let mut reachable_exits = BTreeSet::new();
    let mut occurrences = BTreeMap::<_, crate::analysis::refinement::RefinementSet>::new();

    let normal_exits = graph
        .completion_exits()
        .map(|exit| exit.block())
        .collect::<BTreeSet<_>>();

    let normal_paths = graph.completion_predecessors();

    for block in graph.blocks() {
        collector.on_normal_path = normal_paths.contains(&block.id());

        // Publication evaluates operations against an independent final block-entry state.
        let state = result.state(block.id()).filter(|state| state.reachable);

        let Some(mut state) = state.cloned() else {
            collector.collect_unreachable_suspensions(graph, block);

            continue;
        };

        for operation in block.operations() {
            let operation = graph
                .operation(*operation)
                .expect("storage-flow block retains its operation");

            if let AnalysisOperationKind::Bound(AnyBoundNodeId::Block(block)) = operation.kind() {
                reachable_blocks.insert(block);
            }

            if let AnalysisOperationKind::ScopeExit {
                block,
                exit,
                phase: AnalysisScopeExitPhase::LifecycleResolution,
                ..
            } = operation.kind()
            {
                reachable_exits.insert(StorageExitPoint::new(block, exit));
            }

            occurrences
                .entry(operation.kind().node())
                .and_modify(|set| {
                    set.intersect(&state.refinements);
                })
                // Each occurrence retains its converged entry facts while replay advances the state.
                .or_insert_with(|| state.refinements.clone());

            collector.apply_operation(&mut state, operation);
        }

        collector.collect_conditions(&state, block, graph, &mut conditions);

        if normal_exits.contains(&block.id()) {
            collector.collect_completion(&state);
        }
    }

    let carriers = collector.trusted.result_carriers(request);

    let returned_input = (!collector.has_normal_effects)
        .then_some(collector.trusted.returned_input)
        .flatten()
        .flatten();

    collector
        .diagnostics
        .add_range(std::mem::take(&mut collector.trusted).finish(execution));

    if let Some(error) = collector.query_failure.take() {
        return match error {
            CheckerQueryError::Cancelled => CheckerOutcome::Cancelled,
            CheckerQueryError::Upstream(error) => CheckerOutcome::UpstreamFailure(error),
        };
    }

    let dependencies = match collector.result_dependencies() {
        Ok(dependencies) => dependencies,
        Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
        Err(CheckerQueryError::Upstream(error)) => return CheckerOutcome::UpstreamFailure(error),
    };

    let decisions = collector.decisions().collect::<Vec<_>>();
    let memory_decisions = collector.memory_decisions().collect::<Vec<_>>();
    let exits = collector.exit_decisions().collect::<Vec<_>>();
    let replacements = collector.replacement_decisions().collect::<Vec<_>>();

    let assignments = collector.take_assignment_targets();
    let effects = std::mem::take(&mut collector.effects);

    let analysis = StorageFlow::try_new(
        storage.unit(),
        storage.kind(),
        decisions,
        collector.suspensions.into_values(),
        reachable_blocks,
        reachable_exits,
        exits,
        collector.is_recovered
            || storage_is_recovered(storage)
            || liveness.is_recovered()
            || refinements.is_recovered(),
    )
    .and_then(|analysis| {
        analysis.with_conditions(
            conditions
                .into_iter()
                .filter_map(|(expression, value)| value.map(|value| (expression, value)))
                .collect(),
        )
    })
    .and_then(|analysis| analysis.with_memory_operations(memory, memory_decisions))
    .and_then(|analysis| analysis.with_replacements(replacements))
    .and_then(|analysis| analysis.with_assignment_targets(assignments))
    .expect("durable storage flow must preserve the checked plan's identities")
    .with_dependencies(
        dependencies
            .with_effects(effects)
            .with_carriers(carriers)
            .with_returned_input(returned_input),
    )
    .with_cleanup_mutations(collector.cleanup_mutations)
    .with_completions(collector.completions);

    let published = match crate::analysis::refinement::publish_refinements(
        request,
        graph,
        refinements,
        occurrences,
        collector.invalidations,
    ) {
        CheckerOutcome::Complete(result) => {
            let (published, diagnostics) = result.into_parts();

            collector.diagnostics.add_range(diagnostics);

            published
        }
        CheckerOutcome::Cancelled => return CheckerOutcome::Cancelled,
        CheckerOutcome::UpstreamFailure(error) => return CheckerOutcome::UpstreamFailure(error),
    };

    CheckerOutcome::complete((analysis, published), collector.diagnostics)
}

struct StorageOperationOutcome {
    status: StorageOperationStatus,
    origin: Option<StorageOperationOrigin>,
}

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
    pub(super) owners: &'analysis StorageScopeOwners,
    pub(super) execution:
        &'analysis crate::analysis::guarantee::ExecutionFlowDomain<'analysis, 'analysis, C>,
    pub(super) dependencies: &'analysis bray_bound_tree::CheckedDependencyContracts,
    values: &'analysis crate::dependency::ValueInputs,
    patterns: &'analysis bray_bound_tree::CheckedPatterns,
    pub(super) results: crate::dependency::ResultState,
    pub(super) trusted: crate::analysis::guarantee::TrustedChecks,
    pub(super) memory_evidence: Option<bool>,
    pub(super) statuses: BTreeMap<StorageAccessPlan, StorageOperationStatus>,
    pub(super) suspensions: BTreeMap<bray_bound_tree::BoundExecutionSite, StorageSuspensionState>,
    exits: Vec<(bray_bound_tree::BoundBlockId, AnyBoundNodeId, ExitState)>,
    exit_indices: BTreeMap<(bray_bound_tree::BoundBlockId, AnyBoundNodeId), usize>,
    replacements: BTreeMap<StorageAccessPlan, ReplacementState>,
    assignments: BTreeMap<BoundExpressionId, Option<crate::analysis::storage_flow::memory::Place>>,
    pub(super) effects: BTreeSet<bray_symbols::DependencyStorageEffect>,
    pub(super) on_normal_path: bool,
    pub(super) has_normal_effects: bool,
    pub(super) references: Option<crate::analysis::storage_flow::memory::ReferenceExit>,
    pub(super) cleanup_mutations:
        BTreeMap<bray_bound_tree::StorageMutation, bray_bound_tree::StorageInvalidation>,
    pub(super) completions:
        BTreeMap<bray_bound_tree::StorageMutation, bray_bound_tree::StorageCompletion>,
    invalidations:
        BTreeMap<bray_bound_tree::BoundExecutionSite, bray_bound_tree::StorageInvalidation>,
    pub(super) memory_decisions: BTreeMap<BoundExpressionId, MemoryOperationStatus>,
    pub(super) diagnostics: DiagnosticBag,
    pub(super) reported_diagnostics: BTreeSet<(DiagnosticKind, StorageAccessId)>,
    pub(super) reported_memory_diagnostics: BTreeSet<(DiagnosticKind, BoundExpressionId)>,
    pub(super) publish: bool,
    pub(super) is_recovered: bool,
    pub(super) query_failure: Option<CheckerQueryError<C::UpstreamError>>,
}

impl<'analysis, C> StorageFlowCollector<'analysis, C>
where
    C: CheckerRequestContext + ?Sized,
{
    fn collect_completion(&mut self, state: &StorageFlowState) {
        self.collect_exit_references(state);
        self.results.merge_returns(&state.result_dependencies);

        self.trusted.exit(
            self.execution,
            state
                .execution
                .as_ref()
                .expect("reachable storage state has execution facts"),
        );
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
                let (_, value) = self.execution.condition_evidence(
                    state
                        .execution
                        .as_ref()
                        .expect("reachable storage state has execution facts"),
                    expression,
                );

                conditions
                    .entry(expression)
                    .and_modify(|known| {
                        if *known != value {
                            *known = None;
                        }
                    })
                    .or_insert(value);
            }
        }
    }

    fn new(
        request: CheckerUnitView<'analysis, C>,
        storage: &'analysis StoragePlan,
        liveness: &'analysis Liveness,
        refinements: &'analysis crate::analysis::refinement::RefinementUniverse,
        memory: &'analysis CheckedMemoryOperations,
        input: &'analysis StorageFlowInput,
        owners: &'analysis StorageScopeOwners,
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
            request,
            storage,
            liveness,
            refinements,
            memory,
            input,
            owners,
            execution,
            dependencies,
            values,
            patterns,
            results: Default::default(),
            trusted: Default::default(),
            memory_evidence: None,
            statuses: BTreeMap::new(),
            suspensions: BTreeMap::new(),
            exits: Vec::new(),
            exit_indices: BTreeMap::new(),
            replacements: BTreeMap::new(),
            assignments: BTreeMap::new(),
            invalidations: BTreeMap::new(),
            cleanup_mutations: BTreeMap::new(),
            completions: BTreeMap::new(),
            effects: BTreeSet::new(),
            on_normal_path: false,
            has_normal_effects: false,
            references: None,
            memory_decisions: BTreeMap::new(),
            diagnostics: DiagnosticBag::new(),
            reported_diagnostics: BTreeSet::new(),
            reported_memory_diagnostics: BTreeSet::new(),
            publish: true,
            is_recovered: false,
            query_failure: None,
        }
    }

    pub(in crate::analysis::storage_flow) fn without_publication(
        request: CheckerUnitView<'analysis, C>,
        storage: &'analysis StoragePlan,
        liveness: &'analysis Liveness,
        refinements: &'analysis crate::analysis::refinement::RefinementUniverse,
        memory: &'analysis CheckedMemoryOperations,
        input: &'analysis StorageFlowInput,
        owners: &'analysis StorageScopeOwners,
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
            publish: false,
            ..Self::new(
                request,
                storage,
                liveness,
                refinements,
                memory,
                input,
                owners,
                execution,
                dependencies,
                values,
                patterns,
            )
        }
    }

    pub(in crate::analysis::storage_flow) fn apply_operation(
        &mut self,
        state: &mut StorageFlowState,
        operation: &AnalysisOperation,
    ) {
        if let AnalysisOperationKind::Suspension {
            occurrence,
            kind: crate::analysis::model::AnalysisSuspensionKind::ScopedCall,
        } = operation.kind()
        {
            // Entry already transferred the invocation's inputs. Suspension records
            // their live storage without evaluating or transferring them again.
            self.record_suspension(state, occurrence);
            return;
        }

        if let Some(occurrence) = operation.cleanup_suspension() {
            self.record_suspension(state, occurrence);
        }

        let mut refinements = std::mem::take(&mut state.refinements);
        let universe = self.refinements;

        self.observe_absence(state, universe.active_refinements(&refinements));

        let mut execution = state
            .execution
            .take()
            .expect("reachable storage state has execution facts");

        let kind = operation.kind();

        self.transfer_result_dependencies(state, kind);

        if self.publish {
            self.trusted
                .before_operation(self.execution, &execution, kind);
        }

        self.apply_cleanup(state, &mut execution, kind);

        let completing = matches!(
            kind,
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Completion,
                ..
            }
        );

        let assignment = (!completing)
            .then(|| self.assignment_target(state, kind.node()))
            .flatten();

        if matches!(
            kind,
            AnalysisOperationKind::Call {
                invocation: bray_bound_tree::BoundExecutionSite::ScopedEnter(_),
                phase: AnalysisCallPhase::Attempt
            }
        ) {
            for plan in self
                .storage
                .occurrence_plans(kind.occurrence())
                .filter(|plan| matches!(plan.purpose(), StorageAccessPurpose::Borrow(_)))
            {
                self.apply_plan(state, plan, universe.active_refinements(&refinements));
            }
        }

        let call_effects = self.begin_transition(state, kind, assignment.as_ref(), &execution);

        let assignment_path = assignment
            .as_ref()
            .and_then(|target| target.storage_path())
            .cloned();

        let mut invalidation = if completing {
            None
        } else {
            let call_effects = call_effects
                .as_ref()
                .map(|effects| self.call_observations(state, kind.occurrence(), effects));

            call_effects
                .or_else(|| {
                    // Assignment publication and mutation share one resolved path.
                    assignment.as_ref().map(|target| {
                        self.target_observations(&state.memory, std::iter::once(target))
                    })
                })
                .or_else(|| {
                    self.refinements
                        .invalidation_at(kind.occurrence())
                        .map(|planned| self.planned_observations(state, planned, kind.occurrence()))
                        .or_else(|| {
                            matches!(
                                kind,
                                AnalysisOperationKind::Suspension {
                                    kind: crate::analysis::model::AnalysisSuspensionKind::Await,
                                    ..
                                }
                            )
                            .then(|| {
                                self.opaque_observations(
                                    &state.memory,
                                    self.invocation_roots(state, kind.occurrence()),
                                )
                            })
                        })
                })
        };

        if let Some(invalidation) = &mut invalidation {
            self.preserve_borrowed_carriers(state, &state.memory, invalidation);
        }

        if !completing {
            self.record_operation_effects(kind, call_effects.as_deref(), assignment.as_ref());

            if self.publish {
                if let AnyBoundNodeId::Expression(expression) = kind.node()
                    && matches!(
                        self.request.view().expression(expression),
                        Some(bray_bound_tree::BoundExpression::Assignment(_))
                    )
                {
                    self.assignments
                        .entry(expression)
                        .and_modify(|previous| {
                            if *previous != assignment_path {
                                *previous = None;
                            }
                        })
                        // Target paths share immutable projections with the transition.
                        .or_insert_with(|| assignment_path.clone());
                }

                if let Some(effects) = invalidation
                    .as_ref()
                    .or_else(|| self.refinements.invalidation_at(kind.occurrence()))
                {
                    self.invalidations
                        .entry(kind.occurrence())
                        .and_modify(|previous| previous.merge(effects))
                        // Publication owns the final may-effects independently of preparation.
                        .or_insert_with(|| effects.clone());
                }
            }

            let assignment = assignment_path
                .as_ref()
                .and_then(|path| crate::ExecutionPlace::storage_path(self.storage, path));

            self.execution.transfer_operation(
                &mut execution,
                kind,
                invalidation.as_ref(),
                assignment.as_ref(),
            );
        }

        self.memory_evidence = self
            .execution
            .trusted_memory_evidence(&execution, kind.occurrence());

        state.execution = Some(execution);

        self.apply_call_effects(state, kind, call_effects.as_deref(), invalidation.as_ref());

        let memory_status = self.apply_storage_operation(
            state,
            operation,
            universe.active_refinements(&refinements),
            invalidation.as_ref(),
        );

        let observations = if !matches!(
            kind,
            AnalysisOperationKind::Call {
                phase: crate::analysis::model::AnalysisCallPhase::Attempt,
                ..
            }
        ) {
            self.completed_references(state, kind)
        } else {
            bray_bound_tree::StorageCompletion::default()
        };

        let execution = state
            .execution
            .as_mut()
            .expect("operation retains execution facts");

        if memory_status.is_none_or(|status| status == MemoryOperationStatus::Valid) {
            self.execution
                .complete_operation(execution, kind, Some(&observations));
        }

        if self.publish {
            self.trusted
                .after_operation(self.execution, execution, kind, self.memory_evidence);
        }

        if !matches!(
            kind,
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Attempt,
                ..
            } | AnalysisOperationKind::Suspension {
                kind: crate::analysis::model::AnalysisSuspensionKind::Await,
                ..
            }
        ) {
            self.end_last_use_borrows(state, kind.occurrence());
            self.end_last_use_storage(state, kind.occurrence());
        }

        crate::analysis::refinement::transfer_operation(
            &mut refinements,
            operation,
            universe,
            self.storage,
            invalidation.as_ref(),
            &self.input.discriminants,
        );

        state.refinements = refinements;
    }

    fn transfer_result_dependencies(
        &mut self,
        state: &mut StorageFlowState,
        kind: AnalysisOperationKind,
    ) {
        let mut transfer = crate::dependency::ResultTransfer::new(
            self.request,
            self.execution.semantics.types(),
            self.execution.semantics.selections(),
            self.patterns,
            self.values,
            &mut state.result_dependencies,
        );

        if let Err(error) = transfer.operation(
            kind,
            kind.occurrence()
                .expression()
                .and_then(|id| self.execution.yield_targets.get(&id).copied()),
        ) {
            self.query_failure = Some(error);
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
                    if let Some(bray_bound_tree::StorageProjection::ActiveUnionPayloadField {
                        variant,
                        ..
                    }) = payload.projections.last()
                        && (*variant == active) != value
                    {
                        // The absent variant is an ownership fact even when its payload has no source binding.
                        state
                            .definitely_absent
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
                    .projected_access(access, &[bray_bound_tree::StorageProjection::NullableValue])
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
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
    ) -> Option<MemoryOperationStatus> {
        if let AnalysisOperationKind::PatternObservation(pattern) = operation.kind() {
            state
                .observed_pattern_bindings
                .extend(self.input.definitions(pattern.into()));

            return None;
        }

        if let AnalysisOperationKind::Suspension { occurrence, .. } = operation.kind() {
            self.record_suspension(state, occurrence);
        }

        if matches!(operation.kind(), AnalysisOperationKind::Recovery(_)) {
            state.recovered = true;
            self.is_recovered = true;
        }

        if !matches!(
            operation.kind(),
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Completion,
                ..
            } | AnalysisOperationKind::ScopeExit { .. }
        ) && let Some(invalidation) = invalidation.or_else(|| {
            self.refinements
                .invalidation_at(operation.kind().occurrence())
        }) {
            state.invalidate_observed_absence(
                invalidation,
                self.storage,
                &self.input.discriminants,
            );
        }

        if let AnalysisOperationKind::Call { invocation, phase } = operation.kind() {
            match invocation {
                bray_bound_tree::BoundExecutionSite::Node(
                    bray_bound_tree::AnyBoundNodeId::Expression(_),
                ) if phase == AnalysisCallPhase::Attempt => {
                    return None;
                }
                bray_bound_tree::BoundExecutionSite::ScopedEnter(_)
                | bray_bound_tree::BoundExecutionSite::ScopedExit(_) => {
                    if phase == AnalysisCallPhase::Attempt {
                        for plan in self
                            .storage
                            .occurrence_plans(operation.kind().occurrence())
                            .filter(|plan| {
                                !matches!(plan.purpose(), StorageAccessPurpose::Borrow(_))
                            })
                        {
                            self.apply_plan(state, plan, refinements.clone());
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

        let deferred = self
            .dependencies
            .calls()
            .get(&operation.kind().occurrence())
            .is_some_and(|call| call.deferred_effects().is_some());

        if awaited.is_some() && matches!(operation.kind(), AnalysisOperationKind::Suspension { .. })
        {
            return None;
        }

        self.initialize_operation_storage(state, operation.kind().node());

        for plan in self.storage.node_plans(operation.kind().node()) {
            self.apply_plan(state, plan, refinements.clone());
        }

        let memory_status =
            self.apply_memory_operation(state, operation.kind().node(), refinements.clone());

        if memory_status
            .is_none_or(|status| status == bray_bound_tree::MemoryOperationStatus::Valid)
        {
            if let Some(expression) = awaited {
                if let Some(source) = state
                    .execution
                    .as_ref()
                    .and_then(|execution| self.execution.awaited_invocation(execution, expression))
                {
                    state.memory.complete_transition(source);

                    if let Some(source) = source
                        .expression()
                        .and_then(|id| self.expression_result_storage(id))
                        && let Some(result) = self.expression_result_storage(expression)
                    {
                        state.memory.transfer(&source, result);
                    }
                }
            } else if !deferred {
                state
                    .memory
                    .complete_transition(operation.kind().occurrence());
            }

            self.transfer_memory_result_state(state, operation.kind().node());
        }

        state.memory.finish_value_transfers();

        if let AnalysisOperationKind::ScopeExit {
            block,
            exit,
            phase: AnalysisScopeExitPhase::LifecycleResolution,
            ..
        } = operation.kind()
        {
            self.report_escaping_storage_dependencies(state, block, exit);
        }

        if let crate::CheckerUnitRoot::Expression(root) = self.request.root()
            && operation.kind().node() == root.into()
            && matches!(
                self.request.semantic_context(),
                crate::SemanticUnitContext::RuntimeDefault(_)
            )
        {
            self.report_escaping_default_storage(state, root);
        }

        if let AnalysisOperationKind::ScopeExit {
            block,
            exit,
            phase: AnalysisScopeExitPhase::LifecycleResolution,
            ..
        } = operation.kind()
        {
            self.record_exit(state, block, exit);
            self.end_scope(state, block, exit);
        }

        memory_status
    }

    fn apply_plan<'refinements>(
        &mut self,
        state: &mut StorageFlowState,
        plan: StorageAccessPlan,
        refinements: impl Iterator<Item = &'refinements Refinement> + Clone,
    ) {
        let purpose = self.storage.effective_purpose(plan);

        let outcome = self.operation_status(state, plan, purpose, refinements.clone());

        let status = outcome.status;
        let borrow = self.storage.borrow_for_plan(plan);

        if matches!(
            status,
            StorageOperationStatus::Uninitialized
                | StorageOperationStatus::InactiveProjection
                | StorageOperationStatus::NotCopyable
        ) {
            panic!(
                "Storage-flow analysis produced a state forbidden for one exact planned source operation. in apply_plan, unit: {:?}, origin: {:?}, expression: {:?}, access: {:?}, purpose: {:?}, status: {:?}, root: {:?}, initialized: {:?}",
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
                state.initialized
            );
        }

        if matches!(status, StorageOperationStatus::Valid) {
            if self.publish && purpose == StorageAccessPurpose::Assignment {
                let incoming = ReplacementState::new(state, plan, self.storage);

                match self.replacements.entry(plan) {
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        entry.get_mut().merge(incoming);
                    }
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(incoming);
                    }
                }
            }

            if let Err(error) = self.apply_valid_operation(state, plan, purpose, borrow) {
                self.query_failure = Some(error);

                return;
            }
        } else if matches!(status, StorageOperationStatus::Recovered) {
            self.is_recovered = true;
        }

        if !self.publish {
            return;
        }

        self.statuses
            .entry(plan)
            .and_modify(|current| *current = more_conservative(*current, status))
            .or_insert(status);

        if let Some(kind) = diagnostic_kind(status) {
            self.add_diagnostic(kind, plan, purpose, outcome.origin);
        }
    }

    fn operation_status<'refinements>(
        &self,
        state: &StorageFlowState,
        plan: StorageAccessPlan,
        purpose: StorageAccessPurpose,
        refinements: impl Iterator<Item = &'refinements Refinement> + Clone,
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

        if let Some(conflict) = self.borrow_conflict(state, plan, purpose) {
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
        borrow: Option<BorrowCapabilityId>,
    ) -> Result<(), CheckerQueryError<C::UpstreamError>> {
        match purpose {
            StorageAccessPurpose::Move
            | StorageAccessPurpose::Initialize
            | StorageAccessPurpose::Assignment => {
                let target =
                    self.input
                        .reference_target(&state.memory, self.storage, plan.access());

                let observed = target
                    .as_ref()
                    .and_then(|target| target.storage_path())
                    .cloned();

                let logical = observed
                    .as_ref()
                    .or_else(|| self.storage.logical_path(plan.access()));

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

                if let Some(target) = target {
                    if purpose == StorageAccessPurpose::Move {
                        state.memory.consume_target(target, ty);
                    } else {
                        state.memory.write_target(target, ty, plan.expression());
                    }
                } else {
                    // A lifetime anchor cannot stand in for an unresolved physical target.
                    state.memory.invalidate(
                        &bray_bound_tree::StorageInvalidation::unknown(),
                        self.storage,
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

                    let capability = self
                        .storage
                        .borrow_capability(borrow)
                        .expect("checked borrow retains its capability");

                    if capability.expression() == Some(plan.expression()) {
                        let access = capability.access();

                        if let Some(source) =
                            self.input
                                .reference_target(&state.memory, self.storage, access)
                        {
                            let ty = self
                                .storage
                                .access(access)
                                .expect("borrow has its source access")
                                .reached_type();

                            let result = self
                                .storage
                                .access(plan.access())
                                .expect("planned loan has its passed value type")
                                .reached_type();

                            let values = self.request.semantic_values();

                            let reborrow = matches!((values.type_data(ty).as_ref(), values.type_data(result).as_ref()),
                                (bray_symbols::TypeData::Borrow { target: source, .. }, bray_symbols::TypeData::Borrow { target: result, .. }) if source == result);

                            if reborrow {
                                state
                                    .memory
                                    .reborrow_value(borrow, &source, capability.parent());
                            } else {
                                state.memory.borrow_target_storage(borrow, source, ty);
                            }
                        }
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
            | StorageAccessPurpose::Projection => {}
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

    fn initialize_operation_storage(&self, state: &mut StorageFlowState, node: AnyBoundNodeId) {
        let definitions = self.input.definitions(node);

        for identity in definitions {
            state.memory.begin(*identity);
        }

        state
            .observed_pattern_bindings
            .retain(|identity| !definitions.contains(identity));

        state
            .moved
            .retain(|path, _| !definitions.contains(&path.root));

        state
            .definitely_absent
            .retain(|path, _| !definitions.contains(&path.root));

        state
            .fully_moved
            .retain(|storage| !definitions.contains(storage));

        state.live.extend(definitions.iter().copied());
        state.initialized.extend(definitions.iter().copied());
    }

    fn moved_origins(
        &self,
        state: &StorageFlowState,
        access: StorageAccessId,
    ) -> Vec<BoundExpressionId> {
        let logical = self.storage.logical_path(access);

        let mut origins = state
            .moved
            .iter()
            .flat_map(|(moved, origins)| {
                origins.iter().filter_map(|(origin, ty)| {
                    // Moving a borrow transfers its slot, not the storage it reaches.
                    let relationship = if self.type_is_borrow(*ty) {
                        logical.map_or(StorageRelationship::Error, |path| path.relationship(moved))
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
            state.end_storage_lifetime(*identity, self.storage);
        }
    }

    fn end_last_use_borrows(
        &self,
        state: &mut StorageFlowState,
        operation: bray_bound_tree::BoundExecutionSite,
    ) {
        let moved_borrows = state
            .moved
            .keys()
            .filter_map(|access| {
                match self.storage.access(access.access_id(self.storage)?)?.root() {
                    root => root.borrow_capability(),
                }
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
                    && !state.retains_borrow_for_cleanup(*borrow, self.input)
                    && ((moved_borrows.contains(borrow)
                        && !self.liveness.is_owner_retained(subject)
                        && (!retained_for_suspension || completed_retaining_suspension))
                        || reaches_last_use)
            })
            .collect::<BTreeSet<_>>();

        state.end_borrows(&ended, self.storage);
    }

    fn end_scope(
        &self,
        state: &mut StorageFlowState,
        block: bray_bound_tree::BoundBlockId,
        exit: AnyBoundNodeId,
    ) {
        let leaves_scope = |identity| {
            !bray_bound_tree::storage_identity_transfers_at_unit_exit(self.storage, identity)
                && self.owners.identity_scope(self.storage, identity) == Some(block)
        };

        state.memory.end_storage(leaves_scope);
        state.live.retain(|identity| !leaves_scope(*identity));

        state.retain_live_storage();

        let retained = state
            .active_borrows
            .iter()
            .copied()
            .filter(|borrow| state.retains_borrow_for_cleanup(*borrow, self.input))
            .collect::<BTreeSet<_>>();

        state.active_borrows.retain(|borrow| {
            retained.contains(borrow)
                || self.borrow_is_entry(*borrow)
                || self.liveness.is_live_across_scope(
                    block,
                    exit,
                    BoundDependencySubject::BorrowCapability(*borrow),
                )
        });

        state.definitely_active_borrows.retain(|borrow| {
            retained.contains(borrow)
                || self.borrow_is_entry(*borrow)
                || self.liveness.is_live_across_scope(
                    block,
                    exit,
                    BoundDependencySubject::BorrowCapability(*borrow),
                )
        });
    }

    fn borrow_is_entry(&self, borrow: BorrowCapabilityId) -> bool {
        self.storage
            .borrow_capability(borrow)
            .is_some_and(|capability| capability.entry_binding().is_some())
    }

    fn record_exit(
        &mut self,
        state: &StorageFlowState,
        block: bray_bound_tree::BoundBlockId,
        exit: AnyBoundNodeId,
    ) {
        if !self.publish {
            return;
        }

        let key = (block, exit);

        if let Some(index) = self.exit_indices.get(&key).copied() {
            self.exits[index].2.merge(state);
            return;
        }

        self.exit_indices.insert(key, self.exits.len());

        self.exits.push((block, exit, ExitState::new(state)));
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

    fn take_assignment_targets(
        &mut self,
    ) -> BTreeMap<BoundExpressionId, bray_bound_tree::StoragePath> {
        std::mem::take(&mut self.assignments)
            .into_iter()
            .filter_map(|(expression, target)| target.map(|target| (expression, target)))
            .collect()
    }

    pub(super) fn exit_decisions(&self) -> impl Iterator<Item = StorageExitDecision> + '_ {
        self.exits.iter().map(|(block, exit, state)| {
            StorageExitDecision::new(
                *block,
                *exit,
                state.live.iter().copied(),
                state.initialized.iter().copied(),
                state.moved.iter().cloned(),
                state.definitely_absent.iter().cloned(),
                state.fully_moved.iter().copied(),
                state.active_borrows.iter().copied(),
                state.recovered,
            )
        })
    }

    fn replacement_decisions(
        &self,
    ) -> impl Iterator<Item = bray_bound_tree::StorageReplacementDecision> + '_ {
        self.replacements.iter().map(|(plan, state)| {
            let initialization = state.state(self.storage.root_identity(plan.access()).is_some());

            bray_bound_tree::StorageReplacementDecision::new(
                plan.expression(),
                plan.access(),
                initialization,
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
            state.retained_cleanup_dependencies(self.input),
        );

        match self.suspensions.entry(occurrence) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(snapshot);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let previous = entry.get_mut();

                if !previous.is_reachable() {
                    *previous = snapshot;
                    return;
                }

                // An implicit exit has multiple incoming transfers. Retain every possible owner,
                // but promise initialization and active loans only when every path establishes them.
                *previous =
                    StorageSuspensionState::new(
                        occurrence,
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
                    );
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
        if self.query_failure.is_some() {
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

        let access_argument = match self.diagnostic_storage_access(access_id, purpose) {
            Ok(access) => access,
            Err(error) => {
                self.query_failure = Some(error);

                return;
            }
        };

        if !self.reported_diagnostics.insert((kind, access_id)) {
            return;
        }

        let mut diagnostic = Diagnostic::new(
            diagnostic_id(self.diagnostics.len()),
            kind,
            SeverityKind::Error,
        )
        .with_primary_span(source.span())
        .with_label(DiagnosticLabel::primary(
            storage_diagnostic_label(kind),
            source.span(),
        ))
        .with_arg(DiagnosticArg::storage_access(access_argument));

        if let Some(origin) = origin {
            diagnostic = self.with_operation_origins(diagnostic, source.span(), origin);
        }

        self.diagnostics.add(diagnostic);
    }

    fn diagnostic_storage_access(
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
        let root = match self.storage.identity(identity) {
            Some(StorageIdentity::LocalOwned(_)) => DiagnosticStorageRoot::Local,
            Some(StorageIdentity::Parameter(_)) => DiagnosticStorageRoot::Parameter,
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
            Some(StorageIdentity::Temporary(_)) => DiagnosticStorageRoot::Temporary,
            Some(StorageIdentity::CustomIndexBorrow(_)) => DiagnosticStorageRoot::CustomIndexBorrow,
            Some(StorageIdentity::IterationCursor(_)) => DiagnosticStorageRoot::IterationCursor,
            Some(StorageIdentity::IterationElement(_)) => DiagnosticStorageRoot::IterationElement,
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
        };

        root
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
            StorageProjection::Element(_) => DiagnosticStorageProjection::IndexedElement,
            StorageProjection::SliceRange { start, end } => {
                DiagnosticStorageProjection::SliceRange {
                    has_start: start.is_some(),
                    has_end: end.is_some(),
                }
            }
            StorageProjection::NullableValue => DiagnosticStorageProjection::NullableValue,
            StorageProjection::OwnedTarget => DiagnosticStorageProjection::OwnedTarget,
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
                plan.node(),
                plan.expression(),
                self.storage.effective_purpose(plan),
                plan.access(),
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
