use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundAssignmentOperator, BoundBlockItem, BoundControlTransferKind,
    BoundExpression, BoundExpressionId, BoundReferenceTarget, CheckedExpressionSemantics,
};

use crate::execution_guarantees::{ExecutionCondition, expression_condition};
use crate::{CheckerRequestContext, CheckerUnitView};

use super::super::fixed_point::{
    FixedPointDomain, FixedPointOutcome, FixedPointResult, FlowDirection, solve_fixed_point,
};
use super::super::id::{AnalysisBlockId, AnalysisEdgeId};
use super::super::model::{AnalysisBlock, AnalysisEdge, AnalysisRefinement, ControlFlowGraph};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ExecutionState {
    pub(super) assumptions: BTreeSet<(ExecutionCondition, bool)>,
    pub(super) current: BTreeMap<crate::ExecutionPlace, ExecutionCondition>,
    expressions: BTreeMap<BoundExpressionId, ExecutionCondition>,
    pub(super) result: ExecutionCondition,
    pub(super) entries: BTreeMap<BoundExpressionId, crate::ExecutionCallEvidence>,
    pub(super) completion_dependencies: BTreeSet<crate::ExecutionCompletionDependency>,
}

impl Default for ExecutionState {
    fn default() -> Self {
        Self {
            assumptions: BTreeSet::new(),
            current: BTreeMap::new(),
            expressions: BTreeMap::new(),
            result: ExecutionCondition::Unknown,
            entries: BTreeMap::new(),
            completion_dependencies: BTreeSet::new(),
        }
    }
}

impl ExecutionState {
    pub(super) fn invalidate_cleanup(&mut self) {
        // Cleanup can mutate observations through owned capabilities, just like an opaque call.
        for value in self.current.values_mut() {
            *value = ExecutionCondition::Unknown;
        }
    }

    pub(super) fn assign(&mut self, place: crate::ExecutionPlace, value: ExecutionCondition) {
        self.current.retain(|observed, _| !place.contains(observed));
        self.current.insert(place, value);
    }
}
pub(super) struct ExecutionFlow<'a, 'view, C: CheckerRequestContext + ?Sized> {
    pub(super) domain: ExecutionFlowDomain<'a, 'view, C>,
    pub(super) states: FixedPointResult<Option<ExecutionState>>,
}

pub(super) fn analyze_execution_flow<'a, 'view, C: CheckerRequestContext + ?Sized>(
    graph: &'a ControlFlowGraph,
    request: CheckerUnitView<'view, C>,
    semantics: &'a CheckedExpressionSemantics,
    assumptions: &'a [ExecutionCondition],
    literals: &'a BTreeMap<BoundExpressionId, ExecutionCondition>,
    storage: &'a bray_bound_tree::StoragePlan,
    contracts: &'a BTreeMap<BoundExpressionId, Vec<crate::ExecutionCompletionContract>>,
    cleanup: &'a bray_bound_tree::CheckedAsync,
) -> FixedPointOutcome<ExecutionFlow<'a, 'view, C>> {
    let domain = ExecutionFlowDomain {
        graph,
        request,
        semantics,
        assumptions,
        literals,
        storage,
        contracts,
        cleanup,
        invalidating: super::super::storage_invalidation::invalidating_operation_accesses(
            request,
            semantics.selections(),
            storage,
        ),
    };

    let states = match solve_fixed_point(graph, &domain, &request) {
        FixedPointOutcome::Complete(states) => states,
        FixedPointOutcome::Cancelled => return FixedPointOutcome::Cancelled,
        FixedPointOutcome::ConvergenceInvariantViolated => {
            return FixedPointOutcome::ConvergenceInvariantViolated;
        }
    };

    FixedPointOutcome::Complete(ExecutionFlow { domain, states })
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlow<'_, '_, C> {
    pub(super) fn is_block_reachable(&self, block: AnalysisBlockId) -> bool {
        self.states.state(block).is_some_and(Option::is_some)
    }

    pub(super) fn is_edge_reachable(&self, edge: AnalysisEdgeId) -> bool {
        let Some(edge) = self.domain.graph.edge(edge) else {
            return false;
        };

        let Some(state) = self.output(edge.source()) else {
            return false;
        };

        self.domain.edge_state(&state, edge).is_some()
    }

    pub(super) fn output(&self, block: AnalysisBlockId) -> Option<ExecutionState> {
        let state = self.states.state(block)?;

        self.domain.transfer(self.domain.graph.block(block)?, state)
    }
}

pub(super) struct ExecutionFlowDomain<'a, 'view, C: CheckerRequestContext + ?Sized> {
    pub(super) graph: &'a ControlFlowGraph,
    pub(super) request: CheckerUnitView<'view, C>,
    pub(super) semantics: &'a CheckedExpressionSemantics,
    assumptions: &'a [ExecutionCondition],
    literals: &'a BTreeMap<BoundExpressionId, ExecutionCondition>,
    pub(super) storage: &'a bray_bound_tree::StoragePlan,
    contracts: &'a BTreeMap<BoundExpressionId, Vec<crate::ExecutionCompletionContract>>,
    cleanup: &'a bray_bound_tree::CheckedAsync,
    invalidating: BTreeMap<AnyBoundNodeId, Box<[bray_bound_tree::StorageAccessId]>>,
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    fn value(&self, state: &ExecutionState, expression: BoundExpressionId) -> ExecutionCondition {
        expression_condition(
            self.request.unit(),
            self.semantics,
            self.literals,
            &state.current,
            &state.expressions,
            expression,
            &mut { crate::ExecutionCondition::WORK_LIMIT },
        )
    }

    fn edge_state(&self, state: &ExecutionState, edge: &AnalysisEdge) -> Option<ExecutionState> {
        // Each outgoing path owns its independently refined flow environment.
        let mut next = state.clone();

        if let Some(AnalysisRefinement::Condition { expression, value }) = edge.refinement() {
            let condition = state
                .expressions
                .get(&expression)
                .cloned()
                .unwrap_or_else(|| self.value(state, expression));

            if condition
                .prove(&state.assumptions, &mut {
                    crate::ExecutionCondition::WORK_LIMIT
                })
                .is_some_and(|known| known != value)
            {
                return None;
            }

            condition.assume(value, &mut next.assumptions);
        }

        Some(next)
    }

    fn call_entry(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<crate::ExecutionCallEvidence> {
        let bray_bound_tree::SemanticSelection::Call(call) =
            self.semantics.selections().expression(expression)?
        else {
            return None;
        };

        let mut arguments = BTreeMap::new();

        for argument in call.arguments() {
            if let bray_bound_tree::SelectedArgument::Explicit {
                expression,
                parameter: Some(parameter),
                conversion,
                ..
            } = argument
            {
                let value = if matches!(
                    conversion.target(),
                    bray_bound_tree::ConversionTarget::Identity
                        | bray_bound_tree::ConversionTarget::CallableContract
                ) {
                    state
                        .expressions
                        .get(expression)
                        .cloned()
                        .unwrap_or_else(|| self.value(state, *expression))
                } else {
                    ExecutionCondition::Unknown
                };

                arguments.insert(
                    BoundReferenceTarget::Surface((*parameter).into()).into(),
                    value,
                );
            }
        }

        if let Some(receiver) = call.receiver() {
            arguments.insert(
                BoundReferenceTarget::Surface(receiver.parameter().into()).into(),
                self.value(state, receiver.expression()),
            );
        }

        // Call evidence retains the immutable entry conditions independently of subsequent mutation.
        Some(crate::ExecutionCallEvidence {
            arguments,
            assumptions: state.assumptions.clone(),
        })
    }

    fn complete_call(&self, state: &mut ExecutionState, expression: BoundExpressionId) {
        let Some(entry) = state.entries.get(&expression) else {
            return;
        };

        let Some(bray_bound_tree::SemanticSelection::Call(call)) =
            self.semantics.selections().expression(expression)
        else {
            return;
        };

        if matches!(
            call.target(),
            bray_bound_tree::BoundCallableTarget::Predicate(_)
        ) {
            return;
        }

        // TODO(BRA-500): Carry async completion evidence through the future and await boundary.
        if !matches!(
            call.resolution().result(),
            bray_bound_tree::BoundCallResult::Immediate(_)
        ) {
            return;
        }

        let result = ExecutionCondition::Expression(expression);

        let contracts = self
            .contracts
            .get(&expression)
            .into_iter()
            .flatten()
            .filter(|contract| entry.proves(&contract.entry))
            .collect::<Vec<_>>();

        let post_state = self.receiver_post_state(state, expression, call, &contracts);

        for contract in contracts {
            for (postcondition, source) in &contract.postconditions {
                let condition = postcondition.substitute(
                    &|input| {
                        input
                            .value_in(&post_state)
                            .unwrap_or(ExecutionCondition::Unknown)
                    },
                    &result,
                    &mut { crate::ExecutionCondition::WORK_LIMIT },
                );

                if condition == ExecutionCondition::Unknown {
                    continue;
                }

                condition.assume(true, &mut state.assumptions);

                state
                    .completion_dependencies
                    .insert(crate::ExecutionCompletionDependency {
                        target: call.target(),
                        source: *source,
                        node: expression.into(),
                    });
            }
        }

        state.expressions.insert(expression, result);
    }

    pub(super) fn operation(
        &self,
        state: &mut ExecutionState,
        operation: super::super::model::AnalysisOperationKind,
    ) {
        if let super::super::model::AnalysisOperationKind::ScopeExit { block, exit, phase } =
            operation
        {
            if self.cleanup.scope_exits().iter().any(|plan| {
                plan.scope() == block
                    && plan.exit() == exit
                    && match phase {
                        super::super::model::AnalysisScopeExitPhase::TaskCancellationBroadcast => {
                            !plan.cancellation_broadcast().is_empty()
                        }
                        super::super::model::AnalysisScopeExitPhase::LifecycleResolution => {
                            !plan.lifecycle_resolution().is_empty()
                        }
                    }
            }) {
                state.invalidate_cleanup();
            }

            return;
        }

        match operation.node() {
            AnyBoundNodeId::Expression(expression) => {
                if self.cleanup.replacements().iter().any(|plan| {
                    plan.expression() == expression
                        && matches!(
                            plan.cleanup(),
                            bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(_)
                        )
                }) {
                    state.invalidate_cleanup();
                }

                if !matches!(
                    operation,
                    super::super::model::AnalysisOperationKind::Call {
                        phase: super::super::model::AnalysisCallPhase::Completion,
                        ..
                    }
                ) {
                    state
                        .assumptions
                        .retain(|(condition, _)| !condition.depends_on(expression));

                    for value in state
                        .current
                        .values_mut()
                        .chain(state.expressions.values_mut())
                    {
                        if value.depends_on(expression) {
                            *value = ExecutionCondition::Unknown;
                        }
                    }

                    if let Some(entry) = self.call_entry(state, expression) {
                        state.entries.insert(expression, entry);
                    }
                }

                self.expression(state, expression);

                if !matches!(
                    operation,
                    super::super::model::AnalysisOperationKind::Call {
                        phase: super::super::model::AnalysisCallPhase::Attempt,
                        ..
                    }
                ) {
                    self.complete_call(state, expression);
                }
            }
            AnyBoundNodeId::Pattern(pattern) => self.pattern(state, pattern),
            _ => {}
        }
    }
    fn expression(&self, state: &mut ExecutionState, expression: BoundExpressionId) {
        state.expressions.remove(&expression);
        let value = self.value(state, expression);
        state.expressions.insert(expression, value);
        self.invalidate(state, expression.into());

        match self.request.view().expression(expression) {
            Some(BoundExpression::Assignment(assignment)) => {
                if let [target, source] = assignment.operands() {
                    if let Some(place) = crate::execution_guarantees::expression_place(
                        self.request.unit(),
                        self.semantics,
                        *target,
                    ) {
                        let value = if assignment.operator() == BoundAssignmentOperator::Assign {
                            state
                                .expressions
                                .get(source)
                                .cloned()
                                .unwrap_or_else(|| self.value(state, *source))
                        } else {
                            ExecutionCondition::Unknown
                        };

                        state.assign(place, value);
                    }
                }
            }
            Some(BoundExpression::ControlTransfer(transfer))
                if transfer.kind() == BoundControlTransferKind::Return =>
            {
                state.result = transfer
                    .operand()
                    .map(|operand| {
                        state
                            .expressions
                            .get(&operand)
                            .cloned()
                            .unwrap_or_else(|| self.value(state, operand))
                    })
                    .unwrap_or(ExecutionCondition::Unknown);
            }
            _ => {}
        }
    }

    fn invalidate(&self, state: &mut ExecutionState, node: AnyBoundNodeId) {
        let Some(accesses) = self.invalidating.get(&node) else {
            return;
        };

        for (target, binding) in self.storage.bindings() {
            let access = match binding {
                bray_bound_tree::StorageBinding::Access(access) => *access,
                bray_bound_tree::StorageBinding::Identity(identity) => {
                    let Some(access) = self.storage.root_access(*identity) else {
                        continue;
                    };

                    access
                }
            };

            if !accesses.iter().any(|invalidated| {
                self.storage.relationship(*invalidated, access)
                    != bray_bound_tree::StorageRelationship::Disjoint
            }) {
                continue;
            }

            let Some(reference) = crate::execution_guarantees::storage_binding_reference(*target)
            else {
                continue;
            };

            for invalidated in accesses {
                if self.storage.relationship(*invalidated, access)
                    == bray_bound_tree::StorageRelationship::Disjoint
                {
                    continue;
                }

                let changed = self
                    .storage
                    .resolved_projections(*invalidated)
                    .and_then(|path| crate::ExecutionPlace::from(reference).project(path))
                    .unwrap_or_else(|| reference.into());

                for (place, value) in &mut state.current {
                    if place.overlaps(&changed) {
                        *value = ExecutionCondition::Unknown;
                    }
                }

                state.current.insert(changed, ExecutionCondition::Unknown);
            }

            state
                .current
                .insert(reference.into(), ExecutionCondition::Unknown);
        }
    }

    fn pattern(&self, state: &mut ExecutionState, pattern: bray_bound_tree::BoundPatternId) {
        for (_, block) in self.request.unit().tree().blocks() {
            for item in block.items() {
                let BoundBlockItem::LocalBinding(binding) = item else {
                    continue;
                };

                if binding.pattern() != pattern {
                    continue;
                }

                let value = if binding.bindings().len() == 1 {
                    state
                        .expressions
                        .get(&binding.initializer())
                        .cloned()
                        .unwrap_or_else(|| self.value(state, binding.initializer()))
                } else {
                    ExecutionCondition::Unknown
                };

                for binding in binding.bindings() {
                    // Pattern bindings independently retain the initializer's immutable term.
                    state.assign(
                        BoundReferenceTarget::Local((*binding).into()).into(),
                        value.clone(),
                    );
                }
            }
        }
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

        for (expression, _) in self.request.unit().tree().expressions() {
            if let Some(place) = crate::execution_guarantees::expression_place(
                self.request.unit(),
                self.semantics,
                expression,
            ) {
                if matches!(
                    place.root,
                    BoundReferenceTarget::Surface(
                        bray_symbols::AnySymbolId::CallableParameter(_)
                            | bray_symbols::AnySymbolId::ReceiverParameter(_)
                    )
                ) {
                    // Entry places and their immutable snapshots retain the shared field path.
                    state
                        .current
                        .insert(place.clone(), ExecutionCondition::Input(place));
                }
            }
        }

        for assumption in self.assumptions {
            for place in assumption.inputs() {
                // The state owns a key and immutable entry snapshot for each observed input field.
                state
                    .current
                    .entry(place.clone())
                    .or_insert_with(|| ExecutionCondition::Input(place.clone()));
            }

            if assumption.prove(&state.assumptions, &mut {
                crate::ExecutionCondition::WORK_LIMIT
            }) == Some(false)
            {
                return None;
            }

            // Entry assumptions remain immutable when body storage changes.
            assumption.clone().assume(true, &mut state.assumptions);
        }

        Some(state)
    }

    fn merge_boundary(&self, target: &mut Self::State, boundary: &Self::State) -> bool {
        merge(target, boundary)
    }

    fn transfer(&self, block: &AnalysisBlock, source: &Self::State) -> Self::State {
        // Transfer updates a block-local copy without changing its incoming fixed-point state.
        let mut state = source.clone()?;

        for operation in block
            .operations()
            .iter()
            .filter_map(|id| self.graph.operation(*id))
        {
            self.operation(&mut state, operation.kind());
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

        merge(target, &incoming)
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

fn merge(target: &mut Option<ExecutionState>, incoming: &Option<ExecutionState>) -> bool {
    let Some(incoming) = incoming else {
        return false;
    };

    let Some(target) = target else {
        // The first reachable predecessor supplies an independently owned flow environment.
        *target = Some(incoming.clone());

        return true;
    };

    let mut changed = false;
    let dependencies = target.completion_dependencies.len();

    target
        .completion_dependencies
        .extend(incoming.completion_dependencies.iter().copied());

    changed |= dependencies != target.completion_dependencies.len();

    for (expression, evidence) in &incoming.entries {
        if let std::collections::btree_map::Entry::Vacant(entry) = target.entries.entry(*expression)
        {
            // The join retains an independently owned snapshot for each preceding call.
            entry.insert(evidence.clone());
            changed = true;
        }
    }

    let before = target.assumptions.len();

    target
        .assumptions
        .retain(|condition| incoming.assumptions.contains(condition));

    changed |= before != target.assumptions.len();
    changed |= merge_values(&mut target.current, &incoming.current);
    changed |= merge_values(&mut target.expressions, &incoming.expressions);

    if target.result != incoming.result && target.result != ExecutionCondition::Unknown {
        target.result = ExecutionCondition::Unknown;
        changed = true;
    }

    changed
}

fn merge_values<K: Ord + Clone>(
    target: &mut BTreeMap<K, ExecutionCondition>,
    incoming: &BTreeMap<K, ExecutionCondition>,
) -> bool {
    let mut changed = false;

    for (key, value) in target.iter_mut() {
        if incoming.get(key) != Some(value) && *value != ExecutionCondition::Unknown {
            *value = ExecutionCondition::Unknown;
            changed = true;
        }
    }

    for key in incoming.keys() {
        // Join keys are small identities or share immutable field paths.
        if let std::collections::btree_map::Entry::Vacant(entry) = target.entry(key.clone()) {
            entry.insert(ExecutionCondition::Unknown);
            changed = true;
        }
    }

    changed
}
