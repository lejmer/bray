use super::merge::merge;
use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundExpression, BoundExpressionId, CheckedExpressionSemantics};

use crate::execution_guarantees::ExecutionCondition;
use crate::{CheckerRequestContext, CheckerUnitView};

use super::super::super::fixed_point::{FixedPointDomain, FixedPointResult, FlowDirection};
use super::super::super::id::{AnalysisBlockId, AnalysisEdgeId};
use super::super::super::model::{
    AnalysisBlock, AnalysisEdge, AnalysisRefinement, ControlFlowGraph,
};
use super::super::state::ExecutionState;

pub(in crate::analysis::guarantee) struct ExecutionFlow<
    'a,
    'view,
    C: CheckerRequestContext + ?Sized,
> {
    pub(in crate::analysis::guarantee) domain: ExecutionFlowDomain<'a, 'view, C>,
    pub(in crate::analysis::guarantee) states: FixedPointResult<Option<ExecutionState>>,
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlow<'_, '_, C> {
    pub(in crate::analysis::guarantee) fn is_block_reachable(
        &self,
        block: AnalysisBlockId,
    ) -> bool {
        self.states.state(block).is_some_and(Option::is_some)
    }

    pub(in crate::analysis::guarantee) fn edge_output(
        &self,
        edge: AnalysisEdgeId,
    ) -> Option<ExecutionState> {
        let edge = self.domain.graph.edge(edge)?;
        let state = self.output(edge.source())?;

        self.domain.edge_state(state, edge)
    }

    pub(in crate::analysis::guarantee) fn output(
        &self,
        block: AnalysisBlockId,
    ) -> Option<&ExecutionState> {
        self.states.output(block)?.as_ref()
    }
}

pub(crate) struct ExecutionFlowDomain<'a, 'view, C: CheckerRequestContext + ?Sized> {
    pub(in crate::analysis::guarantee) proofs: std::sync::Arc<crate::execution_guarantees::ExecutionProofCache>,
    pub(super) values: &'a crate::analysis::liveness::ValueRetention,
    pub(in crate::analysis::guarantee) dependencies:
        &'a bray_bound_tree::CheckedDependencyContracts,
    pub(in crate::analysis::guarantee) has_bounds_checks: bool,
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
    pub(super) excluded_edges: BTreeSet<AnalysisEdgeId>,
    pub(in crate::analysis::guarantee) request: CheckerUnitView<'view, C>,
    pub(in crate::analysis) semantics: &'a CheckedExpressionSemantics,
    pub(super) assumptions: &'a [ExecutionCondition],
    pub(super) input_invariants: &'a [ExecutionCondition],
    pub(super) union_variants: &'a BTreeMap<bray_symbols::TypeId, std::sync::Arc<[bray_symbols::UnionVariantSymbolId]>>,
    pub(in crate::analysis::guarantee) literals:
        &'a BTreeMap<BoundExpressionId, ExecutionCondition>,
    pub(super) pattern_conditions: &'a BTreeMap<
        bray_bound_tree::BoundPatternId,
        (bray_bound_tree::BoundOperator, ExecutionCondition),
    >,
    pub(in crate::analysis::guarantee) storage: &'a bray_bound_tree::StoragePlan,
    pub(super) memory: &'a bray_bound_tree::CheckedMemoryOperations,
    pub(in crate::analysis::guarantee) checked_body:
        Option<&'a bray_bound_tree::CheckedBodySemantics>,
    pub(in crate::analysis::guarantee) contracts:
        &'a BTreeMap<BoundExpressionId, Vec<crate::ExecutionCompletionContract>>,
    pub(in crate::analysis) disposal: &'a super::super::cleanup::Cleanup<'a>,
    pub(in crate::analysis::guarantee) refinements: &'a BTreeMap<
        bray_bound_tree::BoundExecutionSite,
        bray_bound_tree::StorageInvalidation,
    >,
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
    fn edge_state(&self, state: &ExecutionState, edge: &AnalysisEdge) -> Option<ExecutionState> {
        // Each outgoing path owns its independently refined flow environment.
        let mut next = state.clone();

        if let Some(AnalysisRefinement::InvocationFailure(occurrence)) = edge.refinement()
            && self.call_preserves_normal_storage(state, occurrence)
        {
            self.invalidate_call_effects(&mut next, occurrence, None);
        }

        self.refine_edge(&mut next, edge).then_some(next)
    }

    pub(in crate::analysis) fn normal_completion(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> bool {
        let deferred = invocation
            .expression()
            .into_iter()
            .flat_map(|expression| self.awaited_invocations(state, expression))
            .collect::<Vec<_>>();

        let result = self.invocation_value(state, invocation);

        if invocation
            .expression()
            .is_some_and(|expression| self.awaited_unknown(state, expression))
        {
            true
        } else if invocation.expression().is_none_or(|expression| !matches!(
            self.request.view().expression(expression), Some(BoundExpression::Await(_))
        )) {
            self.dependencies
                .calls()
                .get(&invocation)
                .is_none_or(|contract| {
                    contract.normal_completion()
                        && (contract.deferred_effects().is_some()
                            || state.entries.get(&invocation).is_none_or(|entry| {
                                self.owned_normal_completion(
                                    &state.assumptions,
                                    invocation,
                                    entry,
                                    &result,
                                )
                            }))
                })
        } else {
            deferred.into_iter().any(|(source, entry)| {
                self.dependencies.calls()[&source].deferred_completion()
                    && self.owned_normal_completion(&state.assumptions, source, entry, &result)
            })
        }
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
                        ExecutionCondition::Expression(expression)
                    }
                    Some(BoundExpression::Unary(unary))
                        if unary.operator() == bray_bound_tree::BoundOperator::LogicalNot => {
                        ExecutionCondition::Expression(expression)
                    }
                    Some(BoundExpression::Structured(condition))
                        if condition.kind() == bray_bound_tree::BoundStructuredExpressionKind::Condition => {
                        ExecutionCondition::Expression(expression)
                    }
                    _ => self.value(state, expression),
                }
            });

        // Branches inspect Boolean truth, not ownership identity. Use checked
        // equalities to observe a call result through its immutable input snapshot.
        let equalities = ExecutionCondition::equalities(&state.assumptions, Some(expression));
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

        if let Some(AnalysisRefinement::PreconditionFailure(invocation)) = edge.refinement()
            && let Some(contract) = self
                .request
                .trusted_contracts()
                .and_then(|contracts| contracts.calls.get(&invocation))
            && state.entries.get(&invocation).is_some_and(|entry| {
                entry.prove_requirements(
                    contract
                        .preconditions
                        .iter()
                        .map(|(condition, _)| condition),
                ).is_ok()
            })
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
                || known.is_none() && !state.proofs.ordinary_proof(&state.assumptions).can_assume(&condition, value)
            {
                return false;
            }

            if !value
                && let Some(BoundExpression::Binary(binary)) =
                    self.request.view().expression(expression)
                && binary.operands().iter().all(|operand| {
                    self.semantics
                        .types()
                        .expression(*operand)
                        .and_then(|entry| {
                            crate::representation::type_representation(self.request, entry.ty())
                        })
                        .and_then(bray_compiler_known::RepresentationRole::integer_representation)
                        .is_some()
                })
                && let ExecutionCondition::Operation(operator, operands) = &condition
                && let Some(complement) = match operator {
                    bray_bound_tree::BoundOperator::Less => {
                        Some(bray_bound_tree::BoundOperator::GreaterEqual)
                    }
                    bray_bound_tree::BoundOperator::LessEqual => {
                        Some(bray_bound_tree::BoundOperator::Greater)
                    }
                    bray_bound_tree::BoundOperator::Greater => {
                        Some(bray_bound_tree::BoundOperator::LessEqual)
                    }
                    bray_bound_tree::BoundOperator::GreaterEqual => {
                        Some(bray_bound_tree::BoundOperator::Less)
                    }
                    _ => None,
                }
            {
                // Integer ordering has no unordered outcome; floating-point NaN does.
                ExecutionCondition::operation(complement, operands.iter().cloned().collect())
                    .assume(true, &mut state.assumptions);
            }

            // Keep both the stable evaluated branch and its checked Boolean meaning.
            // The latter exposes conjunctions carried by completion observations.
            observed.assume(value, &mut state.assumptions);

            let prospective = condition == ExecutionCondition::Expression(expression);

            condition.assume(value, &mut state.assumptions);

            if !state.expressions.contains_key(&expression)
                && !state.pending_results.contains_key(&expression)
                && prospective
            {
                // A prospective Boolean has no owner or initialized storage.
                // This outcome is its first actual value, published by Result Completion.
                state.pending_results.insert(expression, ExecutionCondition::Boolean(value));
            }
        }

        if let Some((condition, value)) = self.branch_condition(state, edge) {
            if !state.proofs.ordinary_proof(&state.assumptions).can_assume(&condition, value) {
                return false;
            }

            condition.assume(value, &mut state.assumptions);
        }

        if let Some(AnalysisRefinement::PropagationOutcome {
            expression,
            succeeded: false,
        }) = edge.refinement()
        {
            self.propagate_result(state, expression);
        }

        if let Some(AnalysisRefinement::CaughtResult {
            expression,
            succeeded,
        }) = edge.refinement()
        {
            self.complete_caught_result(state, expression, !succeeded);
        }

        self.refine_witnesses(state);
        self.retain_successor_values(state, edge);

        true
    }
}

impl<C: CheckerRequestContext + ?Sized> FixedPointDomain for ExecutionFlowDomain<'_, '_, C> {
    type State = Option<ExecutionState>;

    fn direction(&self) -> FlowDirection {
        FlowDirection::Forward
    }
    fn bottom(&self) -> Self::State {
        None
    }

    fn boundary(&self) -> Self::State {
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
                let input = ExecutionCondition::Input(place.clone());

                state.current.observe_input(place);

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
                    let ty = self.observed_types[place]
                        .expect("typed unsigned entry retains its scalar type");

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
                            ExecutionCondition::Literal(
                                self.request.semantic_values().constant_value_data(zero),
                            ),
                        ],
                    )
                    .assume(true, &mut state.assumptions);
                }
            }
        }

        for (identity, storage) in self.storage.identity_entries() {
            if matches!(storage,
                bray_bound_tree::StorageIdentity::LocalOwned(_)
                    | bray_bound_tree::StorageIdentity::Result(_)
                    | bray_bound_tree::StorageIdentity::Temporary(_)
                    | bray_bound_tree::StorageIdentity::Evaluated(_)
                    | bray_bound_tree::StorageIdentity::InvocationInput { .. }
                    | bray_bound_tree::StorageIdentity::InvocationResult(_)
                    | bray_bound_tree::StorageIdentity::ScopedCapability { .. }
            ) {
                if let Some(place) = crate::ExecutionPlace::storage_identity(self.storage, identity) {
                    state.current.set_slot(place, super::super::current::CurrentValue {
                        value: ExecutionCondition::Unknown,
                        availability: super::super::current::ValueAvailability::Absent,
                    });
                }
            } else if matches!(storage, bray_bound_tree::StorageIdentity::PostconditionResult(_)) {
                let place = crate::ExecutionPlace::storage_identity(self.storage, identity)
                    .expect("a postcondition result retains its formal storage identity");

                // The caller's produced result is an input to this clause. Its
                // symbolic contents exist at entry without creating an owner.
                state.current.insert(place, ExecutionCondition::Result);
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

        for assumption in self.input_invariants.iter().chain(self.assumptions) {
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
                let value = ExecutionCondition::Input(place.clone());

                state.current.insert(place.clone(), value.clone());

                for carrier in carriers.iter() {
                    let guarantee = crate::execution_condition_from_type_term(
                        self.request.semantic_values(),
                        carrier.guarantee(),
                        0,
                    )
                    .expect("checked initializer condition belongs to the same semantic store")
                    .substitute(
                        &|_| ExecutionCondition::Unknown,
                        &value,
                        &mut { ExecutionCondition::WORK_LIMIT },
                    ).pointer_observations(&self.pointer_predicates);

                    let subject = super::super::trusted::static_carrier(
                        self.request.semantic_values(),
                        self.dependencies,
                        carrier.path(),
                    )
                    .substitute(
                        &|_| ExecutionCondition::Unknown,
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
                    state.carry_guarantee(ExecutionCondition::Input(subject), condition.pointer_observations(&self.pointer_predicates));
                }
            }

            for (condition, owner) in self.allocation_subjects(&contracts.requirements) {
                let owner = ExecutionCondition::Input(owner);

                state
                    .allocation_owners
                    .insert(condition.clone(), owner.clone());

                state.carry_guarantee(owner, condition);
            }
        }

        Some(state)
    }

    fn merge(
        &self,
        block: Option<AnalysisBlockId>,
        target: &mut Self::State,
        boundary: &Self::State,
    ) -> bool {
        merge(target, boundary, block.into())
    }

    fn transfer(&self, block: &AnalysisBlock, source: &Self::State) -> Self::State {
        // Transfer updates a block-local copy without changing its incoming fixed-point state.
        let mut state = source.clone()?;

        for operation in block
            .operations()
            .iter()
            .filter_map(|id| self.graph.operation(*id))
        {
            if !self.operation(&mut state, operation.kind(), None) {
                return None;
            }
        }

        Some(state)
    }

    fn propagate(
        &self,
        source: &Self::State,
        edge: &AnalysisEdge,
        target: &mut Self::State,
    ) -> bool {
        let incoming = source
            .as_ref()
            .and_then(|source| self.edge_state(source, edge));

        self.merge(Some(edge.target()), target, &incoming)
    }

    fn convergence_bound(&self, graph: &ControlFlowGraph) -> usize {
        graph
            .blocks()
            .len()
            .saturating_mul(
                graph
                    .operations()
                    .len()
                    .saturating_add(graph.edges().len())
                    .saturating_add(1),
            )
            .saturating_mul(16)
    }
}
