use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_bound_tree::{BoundExpression, BoundExpressionId, CheckedExpressionSemantics};

use crate::execution_guarantees::ExecutionCondition;
use crate::{CheckerRequestContext, CheckerUnitView};

use super::super::super::fixed_point::FixedPointResult;
use super::super::super::outcomes::FlowOutcomes;
use super::super::super::id::{AnalysisBlockId, AnalysisEdgeId, AnalysisOperationId};
use super::super::super::model::{
    AnalysisEdge, AnalysisEdgeKind, AnalysisRefinement, ControlFlowGraph,
};
use super::super::state::ExecutionState;

pub(in crate::analysis::guarantee) struct ExecutionFlow<
    'a,
    'view,
    C: CheckerRequestContext + ?Sized,
> {
    pub(in crate::analysis::guarantee) domain: ExecutionFlowDomain<'a, 'view, C>,
    pub(super) states: FixedPointResult<FlowOutcomes<ExecutionState>>,
    pub(super) reached_acyclic: BTreeSet<AnalysisBlockId>,
}

/// Evidence produced by the latest transfer of a block's reached alternatives.
#[derive(Default)]
pub(in crate::analysis::guarantee) struct BlockExecutionEvidence {
    pub(in crate::analysis::guarantee) calls: BTreeMap<bray_bound_tree::BoundExecutionSite, BTreeSet<Arc<crate::ExecutionCallEvidence>>>,
    pub(in crate::analysis::guarantee) cleanup: Vec<(AnalysisOperationId, super::super::cleanup::ReachedCleanup)>,
    pub(in crate::analysis::guarantee) operation_domains: BTreeMap<BoundExpressionId, OperationDomainEvidence>,
    pub(in crate::analysis::guarantee) first_unreached: Option<AnalysisOperationId>,
}

impl BlockExecutionEvidence {
    pub(super) fn is_empty(&self) -> bool {
        self.calls.is_empty() && self.cleanup.is_empty() && self.operation_domains.is_empty() && self.first_unreached.is_none()
    }

    pub(super) fn record_call(&mut self, state: &ExecutionState, invocation: bray_bound_tree::BoundExecutionSite) {
        if let Some(entry) = state.entries.get(&invocation) {
            self.calls.entry(invocation).or_default().insert(Arc::clone(entry));
        }
    }
}

#[derive(Clone, Copy)]
pub(in crate::analysis::guarantee) struct OperationDomainEvidence {
    pub(in crate::analysis::guarantee) proven: bool,
    pub(in crate::analysis::guarantee) integer_maximum: Option<(BoundExpressionId, u64)>,
}

impl OperationDomainEvidence {
    pub(in crate::analysis::guarantee) fn merge(&mut self, other: Self) {
        self.proven &= other.proven;
        self.integer_maximum = self.integer_maximum.max(other.integer_maximum);
    }
}

pub(crate) struct ExecutionFlowDomain<'a, 'view, C: CheckerRequestContext + ?Sized> {
    pub(in crate::analysis::guarantee) proofs: std::sync::Arc<crate::execution_guarantees::ExecutionProofCache>,
    pub(in crate::analysis::guarantee) candidate_evidence: RefCell<BTreeMap<AnalysisBlockId, BlockExecutionEvidence>>,
    pub(in crate::analysis::guarantee) consumed_completion_dependencies: RefCell<BTreeSet<crate::ExecutionCompletionDependency>>,
    pub(super) values: &'a crate::analysis::liveness::ValueRetention,
    pub(in crate::analysis::guarantee) liveness: &'a bray_bound_tree::Liveness,
    pub(in crate::analysis) retained_storage: BTreeSet<bray_bound_tree::StorageAccessId>,
    pub(in crate::analysis::guarantee) dependencies:
        &'a bray_bound_tree::CheckedDependencyContracts,
    pub(in crate::analysis::guarantee) has_bounds_checks: bool,
    pub(in crate::analysis::guarantee) certify_predicate: bool,
    pub(in crate::analysis::guarantee) sequence_length: Option<bray_symbols::AnySymbolId>,
    pub(super) observed_types: &'a std::sync::Arc<BTreeMap<crate::ExecutionPlace, Option<bray_symbols::TypeId>>>,
    pub(in crate::analysis::guarantee) borrowed_inputs: &'a BTreeSet<crate::ExecutionPlace>,
    pub(in crate::analysis::guarantee) scope_places: &'a BTreeMap<
        bray_bound_tree::StorageScope,
        Vec<(crate::ExecutionPlace, bray_bound_tree::StorageIdentityId)>,
    >,
    pub(in crate::analysis::guarantee) generator_observations:
        &'a bray_bound_tree::GeneratorObservations,
    pub(in crate::analysis) yield_targets: &'a BTreeMap<BoundExpressionId, BoundExpressionId>,
    pub(in crate::analysis::guarantee) graph: &'a ControlFlowGraph,
    pub(super) completion_blocks: BTreeSet<AnalysisBlockId>,
    pub(super) excluded_edges: BTreeSet<AnalysisEdgeId>,
    pub(in crate::analysis) result_edges: &'a BTreeSet<AnalysisEdgeId>,
    pub(super) invocation_failures: &'a BTreeSet<(bray_bound_tree::BoundExecutionSite, bray_symbols::DependencyFailureKind)>,
    pub(in crate::analysis::guarantee) request: CheckerUnitView<'view, C>,
    pub(in crate::analysis) semantics: &'a CheckedExpressionSemantics,
    pub(super) assumptions: Cow<'a, [ExecutionCondition]>,
    pub(super) input_invariants: &'a [ExecutionCondition],
    pub(super) union_variants: &'a BTreeMap<bray_symbols::TypeId, std::sync::Arc<[bray_symbols::UnionVariantSymbolId]>>,
    pub(in crate::analysis::guarantee) literals:
        &'a BTreeMap<BoundExpressionId, ExecutionCondition>,
    pub(super) pattern_conditions: &'a BTreeMap<
        bray_bound_tree::BoundPatternId,
        (bray_bound_tree::BoundOperator, ExecutionCondition),
    >,
    pub(in crate::analysis::guarantee) storage: &'a bray_bound_tree::StoragePlan,
    pub(in crate::analysis::guarantee) memory: &'a bray_bound_tree::CheckedMemoryOperations,
    pub(in crate::analysis::guarantee) checked_body:
        Option<&'a bray_bound_tree::CheckedBodySemantics>,
    pub(in crate::analysis::guarantee) contracts:
        &'a BTreeMap<BoundExpressionId, Vec<crate::ExecutionCompletionContract>>,
    pub(in crate::analysis) disposal: &'a super::super::cleanup::Cleanup<'a>,
    pub(in crate::analysis::guarantee) pointer_predicates:
        BTreeMap<bray_symbols::PredicateDefinitionSymbolId, &'static [usize]>,
    pub(in crate::analysis::guarantee) content_predicates:
        BTreeSet<bray_symbols::PredicateDefinitionSymbolId>,
    pub(in crate::analysis::guarantee) owned_allocation:
        Option<bray_symbols::PredicateDefinitionSymbolId>,
    pub(in crate::analysis::guarantee) initialized_range:
        Option<bray_symbols::PredicateDefinitionSymbolId>,
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn replay_edge(&self, state: &ExecutionState, edge: &AnalysisEdge,
        mut emit: impl FnMut(Option<usize>, ExecutionState),
    ) {
        if self.excluded_edges.contains(&edge.id()) {
            return;
        }

        let Some(AnalysisRefinement::InvocationFailure(occurrence, kind)) = edge.refinement() else {
            if let Some(next) = self.finish_edge(state.clone(), edge) { emit(None, next); }

            return;
        };

        let source = state.admitted_failure.as_ref().filter(|(site, _)| *site == occurrence)
            .map_or(state, |(_, state)| state.as_ref());

        if self.invocation_failure_rows(source, occurrence, kind).next().is_none() { return; }

        let flow = self.checked_body.expect("failure replay retains the checked transition").storage_flow();

        for (row, invalidation) in flow.invocation_failure_mutations(occurrence, kind, edge.id()) {
            let mutation = bray_bound_tree::StorageMutation::InvocationFailure { occurrence, kind, edge: edge.id(), row };
            let mut failure = source.clone();

            if !self.assume_failure_row(&mut failure, occurrence, row) { continue; }

            let completion = flow.completion(mutation);

            let mut writes = completion.and_then(|completion|
                self.replay_failure_writes(&failure, mutation, completion.failure_writes()));

            if !invalidation.is_unchanged() {
                self.invalidate_observations(&mut failure, mutation, invalidation);
            }

            let retirements = flow.capture_retirements(mutation, bray_bound_tree::StorageValueBoundary::Execution);

            failure.retire_captured_storage(retirements);

            if let Some((writes, completion)) = writes.as_mut().zip(completion) {
                writes.retire_storage(retirements);
                self.complete_failed_writes(&mut failure, mutation, writes, completion);
            }

            failure.retire_captured_storage(flow.capture_retirements(mutation, bray_bound_tree::StorageValueBoundary::Retirement));

            if let Some(next) = self.finish_edge(failure, edge) { emit(row, next); }
        }
    }

    pub(super) fn finish_edge(&self, mut next: ExecutionState, edge: &AnalysisEdge) -> Option<ExecutionState> {
        next.cleanup = super::super::super::outcomes::cleanup_successor(next.cleanup.as_ref(), edge)?;

        if edge.kind() != AnalysisEdgeKind::Suspension {
            next.admitted_failure = None;
        }

        self.refine_edge(&mut next, edge).then_some(next)
    }

    pub(in crate::analysis) fn invocation_failure_rows<'a>(
        &'a self, state: &'a ExecutionState, occurrence: bray_bound_tree::BoundExecutionSite, kind: bray_symbols::DependencyFailureKind,
    ) -> impl Iterator<Item = Option<usize>> + 'a {
        let reachable = self.invocation_failures.contains(&(occurrence, kind));

        let deferred = (reachable && kind == bray_symbols::DependencyFailureKind::Panic)
            .then(|| self.direct_await_body(state, occurrence))
            .flatten()
            .and_then(|(_, _, frame)| frame.entry.failure_rows.as_deref());

        let rows = if deferred.is_some() || !reachable {
            Some(&[][..])
        } else {
            self.dependencies.calls().get(&occurrence).and_then(|call| call.reports().failures())
        };

        let entry = state.admitted_call(occurrence);

        let limit = crate::analysis::outcomes::FlowOutcomes::<ExecutionState>::MAX_ALTERNATIVES;

        let calls = rows.into_iter().flatten().enumerate().filter_map(move |(index, row)|
            (row.covers(kind) && entry.is_none_or(|entry| entry.admits_failure(index))).then_some(Some(index))
        );

        let frames = deferred.into_iter().flatten().filter_map(move |row| {
            (row.covers(kind) && row.guard.as_ref().is_none_or(|guard| state.ordinary_proof().can_assume(guard, true)))
                .then_some(Some(row.index))
        });

        let admitted = calls.chain(frames);

        let widen = rows.is_some_and(|rows| rows.iter().filter(|row| row.covers(kind)).nth(limit).is_some())
            || deferred.is_some() && admitted.clone().nth(limit).is_some();

        let opaque = rows.is_none() || (widen && admitted.clone().next().is_some());

        admitted.take(if widen { 0 } else { limit }).chain(opaque.then_some(None))
    }

    pub(in crate::analysis) fn assume_failure_row(&self, state: &mut ExecutionState,
        occurrence: bray_bound_tree::BoundExecutionSite, row: Option<usize>,
    ) -> bool {
        let Some(row) = row else { return true; };

        let Some(entry) = self.direct_await_body(state, occurrence).map(|(_, _, frame)| frame.entry.as_ref())
            .or_else(|| state.admitted_call(occurrence)) else {
            return true;
        };

        if !entry.admits_failure(row) { return false; }

        let Some(guard) = entry.failure_guard(row).cloned() else { return true; };

        if !state.ordinary_proof().can_assume(&guard, true) { return false; }

        guard.assume(true, &mut state.assumptions);
        self.refine_witnesses(state);

        true
    }

    pub(in crate::analysis) fn normal_completion(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> bool {
        let awaited = invocation.expression().filter(|&expression| {
            invocation == expression.into()
                && matches!(self.request.view().expression(expression), Some(BoundExpression::Await(_)))
        });

        let result = awaited.map_or_else(
            || self.invocation_value(state, invocation),
            |expression| ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression)),
        );

        if state.entries.get(&invocation).is_some_and(|entry| !self.outcome_reachable(&state.assumptions, entry, &result)) {
            return false;
        }

        if awaited.is_some() {
            let value = state.admitted_receiver(invocation);

            return value.deferred_value().is_none_or(|frame| {
                self.deferred_normal_completion(&state.assumptions, frame, &result)
            });
        }

        self.dependencies.calls().get(&invocation).is_none_or(|contract| contract.normal_completion())
    }

    pub(super) fn deferred_normal_completion(
        &self,
        assumptions: &crate::execution_guarantees::ExecutionAssumptions,
        frame: &crate::ExecutionDeferredValue,
        result: &ExecutionCondition,
    ) -> bool {
        let proof = self.proofs.ordinary_proof(assumptions);

        frame.unknown_possible(&proof) || frame.producers.iter().any(|(source, candidate)| {
            super::super::deferred::possible(&candidate.selection, &proof)
                && self.frame_normal_completion(assumptions, source, candidate, result)
        })
    }

    pub(super) fn frame_normal_completion(
        &self,
        assumptions: &crate::execution_guarantees::ExecutionAssumptions,
        source: &bray_bound_tree::StorageDeferredProducer,
        candidate: &crate::ExecutionDeferredInvocation,
        result: &ExecutionCondition,
    ) -> bool {
        let declared = if let Some(source) = source.local() {
            self.dependencies.calls()[&source].deferred_completion()
        } else {
            candidate.callee.as_deref().and_then(|callee| self.dependencies.frame_callees().get(callee))
                .is_none_or(|body| body.summary.normal_completion())
        };

        declared && self.outcome_reachable(assumptions, &candidate.entry, result)
            && self.task_observation_body(source, candidate).is_none_or(|body| {
            let completed = result.clone().matches_variant(self.task_result_variant(None));

            completed.prove_ordinary(assumptions, Some(&self.proofs)) != Some(true)
                || self.deferred_normal_completion(assumptions, body, &result.clone().project(self.task_completed_payload()))
        })
    }

    pub(in crate::analysis) fn condition_evidence(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> (ExecutionCondition, ExecutionCondition, Option<bool>) {
        let condition = state
            .expressions
            .get(&expression)
            .or_else(|| state.pending_results.get(&expression))
            .cloned()
            .unwrap_or_else(|| {
                match self.request.view().expression(expression) {
                    // These CFG results have begun but do not exist until their
                    // outcome edge publishes the completed Boolean.
                    Some(BoundExpression::Binary(binary)) if matches!(binary.operator(),
                        bray_bound_tree::BoundOperator::LogicalAnd | bray_bound_tree::BoundOperator::LogicalOr) => {
                        ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression))
                    }
                    Some(BoundExpression::Unary(unary))
                        if unary.operator() == bray_bound_tree::BoundOperator::LogicalNot => {
                        ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression))
                    }
                    Some(BoundExpression::Structured(condition))
                        if condition.kind() == bray_bound_tree::BoundStructuredExpressionKind::Condition => {
                        ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression))
                    }
                    _ => self.value(state, expression),
                }
            });

        // Branches inspect Boolean truth, not ownership identity. Use checked
        // equalities to observe a call result through its immutable input snapshot.
        let equalities = state.proofs.equalities(&state.assumptions, Some(expression));
        let observed = condition.with_equalities(&equalities);

        let value = self
            .checked_body
            .map(bray_bound_tree::CheckedBodySemantics::storage_flow)
            .and_then(|flow| flow.condition_value(expression))
            .or_else(|| {
                observed.prove_ordinary(&state.assumptions, Some(&state.proofs))
            });

        // Preserve the evaluated condition across joins. Substitution can differ
        // between the first loop iteration and subsequent visits to the same branch.
        (condition, observed, value)
    }

    pub(in crate::analysis) fn refine_edge(
        &self,
        state: &mut ExecutionState,
        edge: &AnalysisEdge,
    ) -> bool {
        if self.excluded_edges.contains(&edge.id()) {
            return false;
        }

        if let Some(AnalysisRefinement::InvocationEntry { invocation, admitted }) = edge.refinement()
            && !self.refine_invocation_entry(state, invocation, admitted)
        {
            return false;
        }

        if let Some(AnalysisRefinement::InvocationCompletion(invocation)) = edge.refinement() {
            let possible = self.normal_completion(state, invocation);

            if !possible {
                return false;
            }
        }

        if let Some(AnalysisRefinement::TrustBoundary(expression)) = edge.refinement() {
            state.trust_boundaries.insert(expression);
        }

        if let Some(AnalysisRefinement::Condition { expression, value }) = edge.refinement() {
            let (condition, observed, known) = self.condition_evidence(state, expression);

            if known.is_some_and(|known| known != value)
                || known.is_none() && !state.ordinary_proof().can_assume(&condition, value)
            {
                return false;
            }

            self.assume_integer_order(state, expression, &condition, value);

            // Keep both the stable evaluated branch and its checked Boolean meaning.
            // The latter exposes conjunctions carried by completion observations.
            observed.assume(value, &mut state.assumptions);

            let prospective = condition == ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression));

            condition.assume(value, &mut state.assumptions);

            if !state.expressions.contains_key(&expression)
                && !state.pending_results.contains_key(&expression)
                && prospective
            {
                // A prospective Boolean has no owner or initialized storage.
                // This outcome is its first actual value, published by Result Completion.
                state.pending_results.insert(expression, ExecutionCondition::new(crate::ExecutionConditionKind::Boolean(value)));
            }
        }

        if let Some((condition, value)) = self.branch_condition(state, edge) {
            if !state.ordinary_proof().can_assume(&condition, value) {
                return false;
            }

            condition.assume(value, &mut state.assumptions);
        }

        if let Some(AnalysisRefinement::PropagationOutcome {
            expression,
            outcome,
        }) = edge.refinement()
        {
            use crate::analysis::model::AnalysisPropagationOutcome;

            if outcome == AnalysisPropagationOutcome::Failure {
                self.propagate_result(state, expression);
            }

            if matches!(outcome, AnalysisPropagationOutcome::Failure | AnalysisPropagationOutcome::RunFailure(_)) {
                self.consume_value_slots(state, expression.into(), bray_bound_tree::StorageValueConsumptionPhase::Completion);
            }
        }

        if let Some(AnalysisRefinement::CaughtResult {
            expression,
            outcome,
        }) = edge.refinement()
        {
            self.complete_caught_result(state, expression, outcome);
        }

        self.refine_witnesses(state);
        self.retain_successor_values(state, edge);

        true
    }
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn entry_state(&self) -> Option<ExecutionState> {
        let mut state = ExecutionState {
            proofs: std::sync::Arc::clone(&self.proofs),
            ..Default::default()
        };

        state.current = super::super::current::CurrentValues::with_observed_types(
            std::sync::Arc::clone(self.observed_types));

        for place in self.observed_types.keys() {
            if place.is_callable_input()
                || place.reference().is_some_and(|reference| {
                    self.request
                        .trusted_contracts()
                        .is_some_and(|contracts| contracts.inputs.contains(&reference))
                })
            {
                // Entry places and their immutable snapshots retain the shared field path.
                let input = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone()));

                state.current.observe_input(place);

                if self.observed_types[place].is_some_and(|ty|
                    matches!(self.request.semantic_values().type_data(ty).as_ref(), bray_symbols::TypeData::Borrow { .. }))
                {
                    // The entry value records its loan role before any capture.
                    // Its referent retains the same formal data used by entry facts.
                    state.current.insert(place.clone(), ExecutionCondition::reference_value(input.clone().borrowed(), input.clone()));
                }

                // Only the formal value owns storage at entry. Observations of
                // its components can cross loans and do not establish ownership.
                if place.is_callable_input() && place.projections.is_empty()
                    && !self.borrowed_inputs.iter().any(|borrow| borrow.contains(place))
                {
                    state.owned_results.insert(input.clone(), bray_symbols::DependencyOwnedResult::Transfer(
                        bray_symbols::DependencyOwnedTransfer::new(input.clone(), [])
                            .with_preservation(bray_symbols::DependencyValuePreservation::all()),
                    ));
                }

                if self.unsigned_value(&input) {
                    let ty = self.request.semantic_values().unborrowed_type(self.observed_types[place]
                        .expect("typed unsigned entry retains its scalar type"));

                    let zero = self
                        .request
                        .semantic_values()
                        .intern_constant_value(bray_symbols::ConstantValueData::new(
                            ty,
                            bray_symbols::ConstantValueKind::Integer(
                                bray_symbols::IntegerConstant::from_u64(0),
                            ),
                        ))
                        .expect("zero belongs to its checked unsigned scalar type");

                    ExecutionCondition::operation(
                        bray_bound_tree::BoundOperator::GreaterEqual,
                        vec![
                            input,
                            ExecutionCondition::new(crate::ExecutionConditionKind::Literal(
                                self.request.semantic_values().constant_value_data(zero),
                            )),
                        ],
                    )
                    .assume(true, &mut state.assumptions);
                }
            }
        }

        for (identity, storage) in self.storage.identity_entries() {
            if crate::ExecutionPlace::tracks_local_storage(&storage) {
                if let Some(place) = crate::ExecutionPlace::storage_identity(self.storage, identity) {
                    state.current.set_slot(place, super::super::current::CurrentValue {
                        value: ExecutionCondition::unknown(),
                        availability: super::super::current::ValueAvailability::Absent,
                    });
                }
            } else if matches!(storage, bray_bound_tree::StorageIdentity::PostconditionResult(_)) {
                let place = crate::ExecutionPlace::storage_identity(self.storage, identity)
                    .expect("a postcondition result retains its formal storage identity");

                // The caller's produced result is an input to this clause. Its
                // symbolic contents exist at entry without creating an owner.
                state.current.insert(place, ExecutionCondition::result());
            }
        }

        for assumption in self.assumptions.iter().chain(
            self.request
                .trusted_contracts()
                .into_iter()
                .flat_map(|contracts| &contracts.requirements),
        ) {
            for place in assumption.inputs() {
                state.current.observe_input(place);
            }
        }

        for assumption in self.input_invariants.iter().chain(self.assumptions.iter()) {
            if assumption.prove_ordinary(&state.assumptions, Some(&state.proofs)) == Some(false)
            {
                return None;
            }

            // Entry assumptions remain immutable when body storage changes.
            assumption.clone().assume(true, &mut state.assumptions);
        }

        if let Some(contracts) = self.request.trusted_contracts() {
            for (selection, carriers) in self.dependencies.static_carriers() {
                let place = crate::ExecutionPlace::static_instance(selection.clone());
                let value = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone()));

                state.current.insert(place.clone(), value.clone());

                for carrier in carriers.iter() {
                    let guarantee = crate::execution_condition_from_type_term(
                        self.request.semantic_values(),
                        carrier.guarantee(),
                        0,
                    )
                    .expect("checked initializer condition belongs to the same semantic store")
                    .substitute(
                        &|_| ExecutionCondition::unknown(),
                        &value,
                        &mut { ExecutionCondition::WORK_LIMIT },
                    ).pointer_observations(&self.pointer_predicates);

                    let subject = super::super::trusted::static_carrier(
                        self.request.semantic_values(),
                        self.dependencies,
                        carrier.path(),
                    )
                    .substitute(
                        &|_| ExecutionCondition::unknown(),
                        &value,
                        &mut { ExecutionCondition::WORK_LIMIT },
                    );

                    guarantee
                        .clone()
                        .assume(true, &mut state.trusted_assumptions);

                    state.carry_guarantee(subject, guarantee);
                }
            }

            state.current.extend(
                contracts
                    .constants
                    .iter()
                    .map(|(place, value)| (place.clone(), value.clone())),
            );

            for condition in &contracts.preconditions {
                condition.clone().assume(true, &mut state.assumptions);
            }

            for condition in &contracts.requirements {
                let condition = condition.pointer_observations(&self.pointer_predicates);

                if condition.prove_trusted(&state.trusted_assumptions, &state.assumptions)
                    == Some(false)
                {
                    return None;
                }

                condition.assume(true, &mut state.trusted_assumptions);
            }

            for (condition, subject, containing_value) in &contracts.required_witnesses {
                for subject in subject.carrier_inputs(self.request.semantic_values(), self.dependencies, *containing_value) {
                    state.current.observe_input(&subject);
                    state.carry_guarantee(ExecutionCondition::new(crate::ExecutionConditionKind::Input(subject)), condition.pointer_observations(&self.pointer_predicates));
                }
            }

            for (condition, owner) in self.allocation_subjects(&contracts.requirements) {
                let owner = ExecutionCondition::new(crate::ExecutionConditionKind::Input(owner));

                state
                    .allocation_owners
                    .insert(condition.clone(), owner.clone());

                state.carry_guarantee(owner, condition);
            }
        }

        Some(state)
    }
}
