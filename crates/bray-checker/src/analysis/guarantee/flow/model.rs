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

    pub(in crate::analysis::guarantee) fn is_edge_reachable(&self, edge: AnalysisEdgeId) -> bool {
        let Some(edge) = self.domain.graph.edge(edge) else {
            return false;
        };

        let Some(state) = self.output(edge.source()) else {
            return false;
        };

        self.domain.edge_state(&state, edge).is_some()
    }

    pub(in crate::analysis::guarantee) fn output(
        &self,
        block: AnalysisBlockId,
    ) -> Option<ExecutionState> {
        let state = self.states.state(block)?;

        self.domain.transfer(self.domain.graph.block(block)?, state)
    }
}

pub(crate) struct ExecutionFlowDomain<'a, 'view, C: CheckerRequestContext + ?Sized> {
    pub(super) values: &'a crate::analysis::liveness::ValueRetention,
    pub(in crate::analysis::guarantee) dependencies:
        &'a bray_bound_tree::CheckedDependencyContracts,
    pub(in crate::analysis::guarantee) sequence_lengths: &'a [BoundExpressionId],
    pub(super) observed_types: &'a BTreeMap<crate::ExecutionPlace, Option<bray_symbols::TypeId>>,
    pub(in crate::analysis::guarantee) borrowed_inputs: &'a BTreeSet<crate::ExecutionPlace>,
    pub(super) assigned_places: &'a BTreeSet<crate::ExecutionPlace>,
    pub(in crate::analysis::guarantee) scope_places: &'a BTreeMap<
        bray_bound_tree::BoundBlockId,
        Vec<(crate::ExecutionPlace, bray_bound_tree::StorageIdentityId)>,
    >,
    pub(in crate::analysis) yield_targets: &'a BTreeMap<BoundExpressionId, BoundExpressionId>,
    pub(in crate::analysis::guarantee) graph: &'a ControlFlowGraph,
    pub(super) excluded_edges: BTreeSet<AnalysisEdgeId>,
    pub(in crate::analysis::guarantee) request: CheckerUnitView<'view, C>,
    pub(in crate::analysis) semantics: &'a CheckedExpressionSemantics,
    pub(super) assumptions: &'a [ExecutionCondition],
    pub(super) literals: &'a BTreeMap<BoundExpressionId, ExecutionCondition>,
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
    pub(in crate::analysis::guarantee) refinements: &'a std::collections::BTreeMap<
        bray_bound_tree::BoundExecutionSite,
        bray_bound_tree::StorageInvalidation,
    >,
    pub(in crate::analysis::guarantee) spatial_predicates:
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

        self.refine_edge(&mut next, edge).then_some(next)
    }

    pub(in crate::analysis) fn condition_evidence(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> (ExecutionCondition, Option<bool>) {
        let condition = state
            .expressions
            .get(&expression)
            .cloned()
            .unwrap_or_else(|| self.value(state, expression));

        // Branches inspect Boolean truth, not ownership identity. Use checked
        // equalities to observe a call result through its immutable input snapshot.
        let equalities = ExecutionCondition::equalities(&state.assumptions, Some(expression));
        let condition = condition.with_equalities(&equalities);

        let value = self
            .checked_body
            .map(bray_bound_tree::CheckedBodySemantics::storage_flow)
            .and_then(|flow| flow.condition_value(expression))
            .or_else(|| {
                condition.prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
            });

        (condition, value)
    }

    pub(in crate::analysis) fn refine_edge(
        &self,
        state: &mut ExecutionState,
        edge: &AnalysisEdge,
    ) -> bool {
        if self.excluded_edges.contains(&edge.id()) {
            return false;
        }

        if let Some(AnalysisRefinement::InvocationCompletion(invocation)) = edge.refinement() {
            let possible = match invocation
                .expression()
                .and_then(|expression| self.awaited_invocation(state, expression))
            {
                Some(source) => self.dependencies.calls()[&source].deferred_completion(),
                None => self
                    .dependencies
                    .calls()
                    .get(&invocation)
                    .is_none_or(|contract| contract.normal_completion()),
            };

            if !possible {
                return false;
            }
        }

        if let Some(AnalysisRefinement::TrustBoundary(expression)) = edge.refinement() {
            state.trust_boundaries.insert(expression);
        }

        if let Some(AnalysisRefinement::Condition { expression, value }) = edge.refinement() {
            let (condition, known) = self.condition_evidence(state, expression);

            if known.is_some_and(|known| known != value) {
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

            condition.assume(value, &mut state.assumptions);
        }

        if let Some((condition, value)) = self.branch_condition(state, edge) {
            if condition
                .prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
                .is_some_and(|known| known != value)
            {
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

        state.refine_witnesses();
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
        let mut state = ExecutionState::default();

        for place in self.observed_types.keys() {
            if place.is_callable_input() {
                // Entry places and their immutable snapshots retain the shared field path.
                state
                    .current
                    .insert(place.clone(), ExecutionCondition::Input(place.clone()));
            }
        }

        for assumption in self.assumptions.iter().chain(
            self.request
                .trusted_contracts()
                .into_iter()
                .flat_map(|contracts| &contracts.requirements),
        ) {
            for place in assumption.inputs() {
                // The state owns a key and immutable entry snapshot for each observed input field.
                state
                    .current
                    .entry(place.clone())
                    .or_insert_with(|| ExecutionCondition::Input(place.clone()));
            }
        }

        for assumption in self.assumptions {
            if assumption.prove(&state.assumptions, &mut {
                crate::ExecutionCondition::WORK_LIMIT
            }) == Some(false)
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
                    );

                    let subject =
                        super::super::trusted::static_carrier(self.dependencies, carrier.path())
                            .substitute(&|_| ExecutionCondition::Unknown, &value, &mut {
                                ExecutionCondition::WORK_LIMIT
                            });

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
                if condition.prove_trusted(&state.trusted_assumptions, &state.assumptions)
                    == Some(false)
                {
                    return None;
                }

                condition
                    .clone()
                    .assume(true, &mut state.trusted_assumptions);
            }

            for (condition, subject) in &contracts.required_witnesses {
                let mut root = subject.clone();

                root.projections = std::sync::Arc::new([]);

                let subject = ExecutionCondition::Input(subject.clone());

                let carrier = self
                    .observed_types
                    .get(&root)
                    .copied()
                    .flatten()
                    .and_then(|ty| subject.witness_carrier(self.request.semantic_values(), ty))
                    .map_or(subject, |(carrier, _)| carrier);

                state.carry_guarantee(carrier, condition.clone());
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

    fn merge_boundary(&self, target: &mut Self::State, boundary: &Self::State) -> bool {
        merge(target, boundary, None)
    }

    fn transfer(&self, block: &AnalysisBlock, source: &Self::State) -> Self::State {
        // Transfer updates a block-local copy without changing its incoming fixed-point state.
        let mut state = source.clone()?;

        for operation in block
            .operations()
            .iter()
            .filter_map(|id| self.graph.operation(*id))
        {
            self.operation(&mut state, operation.kind(), None);
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

        if self
            .graph
            .block(edge.target())
            .expect("flow edge has a target")
            .predecessors()
            .len()
            == 1
        {
            if *target == incoming {
                return false;
            }

            *target = incoming;

            return true;
        }

        merge(target, &incoming, Some(edge.target()))
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
