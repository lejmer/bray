use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundAssignmentOperator, BoundControlTransferKind,
    BoundExpression, BoundExpressionId, BoundReferenceTarget, CheckedExpressionSemantics,
};

use crate::execution_guarantees::{ExecutionCondition, expression_condition};
use crate::{CheckerOutcome, CheckerQueryError, CheckerRequestContext, CheckerUnitView};

use super::super::fixed_point::{
    FixedPointDomain, FixedPointOutcome, FixedPointResult, FlowDirection, solve_fixed_point,
};
use super::super::id::{AnalysisBlockId, AnalysisEdgeId};
use super::super::model::{AnalysisBlock, AnalysisEdge, AnalysisRefinement, ControlFlowGraph};
use super::super::storage_invalidation::StorageInvalidation;
use super::state::ExecutionState;

pub(super) struct ExecutionFlow<'a, 'view, C: CheckerRequestContext + ?Sized> {
    pub(super) domain: ExecutionFlowDomain<'a, 'view, C>,
    pub(super) states: FixedPointResult<Option<ExecutionState>>,
    pub(super) copied_types: BTreeSet<bray_symbols::TypeId>,
}

pub(super) fn analyze_execution_flow<'a, 'view, C: CheckerRequestContext + ?Sized>(
    graph: &'a ControlFlowGraph,
    request: CheckerUnitView<'view, C>,
    semantics: &'a CheckedExpressionSemantics,
    assumptions: &'a [ExecutionCondition],
    literals: &'a BTreeMap<BoundExpressionId, ExecutionCondition>,
    storage: &'a bray_bound_tree::StoragePlan,
    contracts: &'a BTreeMap<BoundExpressionId, Vec<crate::ExecutionCompletionContract>>,
    cleanup: Option<&'a bray_bound_tree::CheckedAsync>,
) -> CheckerOutcome<ExecutionFlow<'a, 'view, C>, C::UpstreamError> {
    let mut copies = super::super::storage_flow::copyability::CopyabilityResolver::new(request);

    for plan in storage.access_plans().iter().filter(|plan| matches!(plan.purpose(),
        bray_bound_tree::StorageAccessPurpose::ValueTransfer | bray_bound_tree::StorageAccessPurpose::Copy)) {
        let ty = storage.access(plan.access()).expect("checked transfer has committed storage").reached_type();

        match copies.resolve(ty) {
            Ok(_) => {},
            Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
            Err(CheckerQueryError::Infrastructure(error)) => return CheckerOutcome::InfrastructureFailure(error),
            Err(CheckerQueryError::Upstream(error)) => return CheckerOutcome::UpstreamFailure(error),
        }
    }

    let (copied_types, diagnostics) = copies.into_parts();

    let mut invalidating = super::super::storage_invalidation::invalidating_operation_accesses(
        request, semantics.selections(), storage, &copied_types);

    for (expression, node) in request.unit().tree().expressions() {
        if let BoundExpression::Call(call) = node
            && semantics.types().expression(call.callee()).is_some_and(|entry|
                matches!(request.semantic_values().type_data(entry.ty()).as_ref(), bray_symbols::TypeData::Callable(callable)
                    if callable.constness() == bray_symbols::CallableConstness::Constant)) {
            invalidating.remove(&expression.into());
        }

        if let Some(bray_bound_tree::SemanticSelection::Call(call)) = semantics.selections().expression(expression)
            && (!matches!(call.resolution().result(), bray_bound_tree::BoundCallResult::Immediate(_))
                || matches!(call.implementation_hook(), Some(
                bray_compiler_known::ImplementationHook::RawAllocate
                | bray_compiler_known::ImplementationHook::Allocate
                | bray_compiler_known::ImplementationHook::AddressOf
                | bray_compiler_known::ImplementationHook::AddressOfMut
                | bray_compiler_known::ImplementationHook::RawBufferCapacity
                | bray_compiler_known::ImplementationHook::RawBufferInitializedCount
                | bray_compiler_known::ImplementationHook::RawBufferPointer
                | bray_compiler_known::ImplementationHook::RawBufferSparePointer))) {
            // These closed operations observe existing storage or create a fresh allocation.
            // They cannot change existing owner epochs or initialized contents.
            invalidating.remove(&expression.into());
        }
    }

    let targets = request.unit().tree().expressions().map(|(id, expression)|
        (expression.origin().source_anchor().syntax(), id)).collect::<BTreeMap<_, _>>();

    let yield_targets = request.unit().tree().expressions().filter_map(|(id, expression)| {
        let BoundExpression::ControlTransfer(transfer) = expression else { return None; };

        if transfer.kind() != BoundControlTransferKind::Yield { return None; }

        transfer.target().and_then(|target| targets.get(&target).copied()).map(|target| (id, target))
    }).collect();

    let domain = ExecutionFlowDomain {
        yield_targets,
        graph,
        request,
        semantics,
        assumptions,
        literals,
        storage,
        contracts,
        cleanup,
        owners: request.trusted_contracts().filter(|contracts|
            !contracts.requirements.is_empty() || !contracts.guarantees.is_empty() || !contracts.calls.is_empty()).map(|_| {
            bray_bound_tree::StorageScopeOwners::collect(request.unit())
                .expect("checked bound unit must have balanced lexical scopes")
        }),
        invalidating,
        spatial_predicates: request.trusted_contracts().filter(|contracts|
            !contracts.requirements.is_empty() || !contracts.calls.is_empty()).into_iter().flat_map(|_| [
            "ValidRead", "ValidWrite", "AlignedFor", "DeviceValidRead", "DeviceValidWrite", "DeviceAlignedFor"])
            .filter_map(|name| {
            let key = bray_compiler_known::CompilerKnownDeclarationKey::try_new(name)
                .expect("closed spatial predicate key must be valid");

            request.context().available_compiler_known_symbols().provider()
                .declaration_symbol::<bray_symbols::PredicateSymbolId>(&key).map(Into::into)
        }).collect(),
    };

    let states = match solve_fixed_point(graph, &domain, &request) {
        FixedPointOutcome::Complete(states) => states,
        FixedPointOutcome::Cancelled => return CheckerOutcome::Cancelled,
        FixedPointOutcome::ConvergenceInvariantViolated => {
            panic!("execution condition flow exceeded its finite convergence bound");
        }
    };

    CheckerOutcome::complete(ExecutionFlow { domain, states, copied_types }, diagnostics)
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
    yield_targets: BTreeMap<BoundExpressionId, BoundExpressionId>,
    pub(super) graph: &'a ControlFlowGraph,
    pub(super) request: CheckerUnitView<'view, C>,
    pub(super) semantics: &'a CheckedExpressionSemantics,
    assumptions: &'a [ExecutionCondition],
    literals: &'a BTreeMap<BoundExpressionId, ExecutionCondition>,
    pub(super) storage: &'a bray_bound_tree::StoragePlan,
    pub(super) contracts: &'a BTreeMap<BoundExpressionId, Vec<crate::ExecutionCompletionContract>>,
    cleanup: Option<&'a bray_bound_tree::CheckedAsync>,
    pub(super) owners: Option<bray_bound_tree::StorageScopeOwners>,
    invalidating: BTreeMap<AnyBoundNodeId, StorageInvalidation>,
    pub(super) spatial_predicates: BTreeSet<bray_symbols::PredicateDefinitionSymbolId>,
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn value(&self, state: &ExecutionState, expression: BoundExpressionId) -> ExecutionCondition {
        expression_condition(
            self.request.unit(),
            self.semantics,
            self.request.semantic_values(),
            self.literals,
            &state.current,
            &state.expressions,
            expression,
            &mut { crate::ExecutionCondition::WORK_LIMIT },
        )
    }

    pub(super) fn storage_value(&self, state: &ExecutionState, plan: bray_bound_tree::StorageAccessPlan) -> ExecutionCondition {
        let mut value = self.value(state, plan.expression());

        let source = self.storage.expression_plans(plan.expression()).filter(|source|
            source.node() == AnyBoundNodeId::Expression(plan.expression())
                && self.storage.access_contains(source.access(), plan.access()))
            .max_by_key(|source| self.storage.resolved_projections(source.access()).map_or(0, <[_]>::len));

        let source_path = source.and_then(|source| self.storage.resolved_projections(source.access())).unwrap_or(&[]);
        let path = self.storage.resolved_projections(plan.access()).and_then(|path| path.strip_prefix(source_path));

        if let Some(path) = path {
            for projection in path { value = value.project(*projection); }
        } else { value = ExecutionCondition::Unknown; }

        value
    }

    fn edge_state(&self, state: &ExecutionState, edge: &AnalysisEdge) -> Option<ExecutionState> {
        // Each outgoing path owns its independently refined flow environment.
        let mut next = state.clone();

        if let Some(AnalysisRefinement::TrustBoundary(expression)) = edge.refinement() {
            next.trust_boundaries.insert(expression);
        }

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

        self.complete_trusted_await(&mut next, edge);

        Some(next)
    }

    pub(super) fn operation(
        &self,
        state: &mut ExecutionState,
        operation: super::super::model::AnalysisOperationKind,
    ) {
        if let super::super::model::AnalysisOperationKind::ScopeExit { block, exit, phase } =
            operation
        {
            if phase == super::super::model::AnalysisScopeExitPhase::LifecycleResolution {
                self.expire_trusted_scope(state, block);
            }

            if self.cleanup.is_some_and(|cleanup| cleanup.scope_exits().iter().any(|plan| {
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
            })) {
                state.invalidate_cleanup();
            }

            return;
        }

        match operation.node() {
            AnyBoundNodeId::Expression(expression) => {
                if self.cleanup.is_some_and(|cleanup| cleanup.replacements().iter().any(|plan| {
                    plan.expression() == expression
                        && matches!(
                            plan.cleanup(),
                            bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(_)
                        )
                })) {
                    state.invalidate_cleanup();
                }

                if !matches!(
                    operation,
                    super::super::model::AnalysisOperationKind::Call {
                        phase: super::super::model::AnalysisCallPhase::Completion,
                        ..
                    }
                ) {
                    state.witness_carriers.retain(|value| !value.depends_on(expression));
                    state.trusted_assumptions.retain(|(condition, _)| !condition.depends_on(expression));

                    state
                        .assumptions
                        .retain(|(condition, _)| !condition.depends_on(expression));

                    for value in state
                        .current
                        .values_mut()
                        .chain(state.expressions.values_mut())
                        .chain(state.pending_results.values_mut())
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

                state.trust_boundaries.remove(&expression);

                if self.request.trusted_contracts().and_then(|contracts| contracts.calls.get(&expression))
                    .is_some_and(|contract| !contract.completes)
                    && let Some(entry) = state.entries.get_mut(&expression) {
                    entry.pending_execution = true;
                }
            }
            AnyBoundNodeId::Pattern(pattern) => self.pattern(state, pattern),
            _ => {}
        }
    }
    fn expression(&self, state: &mut ExecutionState, expression: BoundExpressionId) {
        state.expressions.remove(&expression);

        let value = state.pending_results.remove(&expression).unwrap_or_else(|| self.value(state, expression));

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
            Some(BoundExpression::ControlTransfer(transfer)) if transfer.kind() == BoundControlTransferKind::Yield => {
                if let Some(target) = self.yield_targets.get(&expression) {
                    let value = transfer.operand().map(|operand| self.value(state, operand))
                        .unwrap_or(ExecutionCondition::Unknown);

                    state.pending_results.insert(*target, value);
                }
            }
            _ => {}
        }
    }

    fn invalidate(&self, state: &mut ExecutionState, node: AnyBoundNodeId) {
        let Some(invalidation) = self.invalidating.get(&node) else {
            return;
        };

        if matches!(invalidation, StorageInvalidation::All) {
            let preserves_storage = match node {
                AnyBoundNodeId::Expression(expression) => matches!(self.semantics.selections().expression(expression),
                    Some(bray_bound_tree::SemanticSelection::Call(call)) if matches!(call.implementation_hook(), Some(
                        bray_compiler_known::ImplementationHook::RawPointerWrite
                        | bray_compiler_known::ImplementationHook::RawPointerRead
                        | bray_compiler_known::ImplementationHook::VolatileStore
                        | bray_compiler_known::ImplementationHook::DeviceVolatileStore))),
                _ => false,
            };

            let spatial = if preserves_storage {
                state.trusted_assumptions.iter().filter(|(condition, _)| matches!(condition,
                    ExecutionCondition::Predicate(predicate, _, _) if self.spatial_predicates.contains(predicate)))
                    .cloned().collect()
            } else { BTreeSet::new() };

            let dependencies = if preserves_storage {
                // Reached addresses keep their owner lifetimes even when their contents change.
                // Retain the complete dependency chain, including intermediate address views.
                std::mem::take(&mut state.witness_dependencies)
            } else { BTreeMap::new() };

            state.invalidate_cleanup();
            state.trusted_assumptions = spatial;
            state.witness_dependencies = dependencies;

            for (expression, _) in self.request.unit().tree().expressions() {
                if let Some(place) = crate::execution_guarantees::expression_place(
                    self.request.unit(), self.semantics, expression)
                    && !place.fields.is_empty() {
                    state.current.insert(place, ExecutionCondition::Unknown);
                }
            }

            return;
        }

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

            if !invalidation.invalidates(access, self.storage) {
                continue;
            }

            let Some(reference) = crate::execution_guarantees::storage_binding_reference(*target)
            else {
                continue;
            };

            state.invalidate_trusted_place(&crate::ExecutionPlace::from(reference));

            let transfers = !self.storage.access_plans().iter().filter(|plan| plan.node() == node)
                .any(|plan| matches!(plan.purpose(), bray_bound_tree::StorageAccessPurpose::Assignment
                    | bray_bound_tree::StorageAccessPurpose::Initialize));

            if !transfers {
                let place = crate::ExecutionPlace::from(reference);
                let value = place.value_in(&state.current).unwrap_or_else(|| ExecutionCondition::Input(place));

                state.invalidate_trusted(&value);
            }

            let StorageInvalidation::Accesses(accesses) = invalidation else {
                state
                    .current
                    .insert(reference.into(), ExecutionCondition::Unknown);

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
                    if if transfers { changed.contains(place) } else { place.overlaps(&changed) } {
                        *value = ExecutionCondition::Unknown;
                    }
                }

                state.current.insert(changed, ExecutionCondition::Unknown);
            }

            if !transfers || accesses.iter().any(|access|
                self.storage.resolved_projections(*access).is_none_or(|projections| projections.is_empty())) {
                state.current.insert(reference.into(), ExecutionCondition::Unknown);
            }
        }
    }

    fn pattern(&self, state: &mut ExecutionState, pattern: bray_bound_tree::BoundPatternId) {
        let bound = self.request.view().pattern(pattern).expect("checked pattern must exist");

        let mut plans = self.storage.access_plans().iter().filter(|plan| plan.node() == pattern.into()
            && !matches!(plan.purpose(), bray_bound_tree::StorageAccessPurpose::Projection));

        for binding in bound.bindings().iter().copied().chain(bound.entries().iter().filter_map(|entry| entry.binding())) {
            if self.storage.binding(bray_bound_tree::StorageBindingTarget::Local(binding)).is_none() {
                continue;
            }

            let Some(plan) = plans.next() else { continue; };

            let value = self.storage_value(state, *plan);

            // The storage plan supplies exact components for declarations, matches, and conditional bindings.
            state.assign(BoundReferenceTarget::Local(binding.into()).into(), value);
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
                    place.reference(),
                    Some(BoundReferenceTarget::Surface(
                        bray_symbols::AnySymbolId::CallableParameter(_)
                            | bray_symbols::AnySymbolId::ReceiverParameter(_)
                    ))
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

        if let Some(contracts) = self.request.trusted_contracts() {
            for condition in &contracts.preconditions {
                condition.clone().assume(true, &mut state.assumptions);
            }

            for condition in &contracts.requirements {
                condition.clone().assume(true, &mut state.trusted_assumptions);
            }
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
        match target.entries.entry(*expression) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(evidence.clone());
                changed = true;
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                changed |= entry.get_mut().intersect(evidence);
            }
        }
    }

    let before = target.assumptions.len();

    let boundaries_before = target.trust_boundaries.len();

    target.trust_boundaries.retain(|boundary| incoming.trust_boundaries.contains(boundary));
    changed |= boundaries_before != target.trust_boundaries.len();

    target
        .assumptions
        .retain(|condition| incoming.assumptions.contains(condition));

    changed |= before != target.assumptions.len();

    let before = target.witness_carriers.len();

    target.witness_carriers.retain(|value| incoming.witness_carriers.contains(value));
    changed |= before != target.witness_carriers.len();

    let before = target.trusted_assumptions.len();

    target.trusted_assumptions.retain(|condition| incoming.trusted_assumptions.contains(condition));
    changed |= before != target.trusted_assumptions.len();

    for (carrier, dependencies) in &incoming.witness_dependencies {
        let retained = target.witness_dependencies.entry(carrier.clone()).or_default();
        let before = retained.len();

        retained.extend(dependencies.iter().cloned());
        changed |= before != retained.len();
    }

    changed |= merge_values(&mut target.current, &incoming.current);
    changed |= merge_values(&mut target.expressions, &incoming.expressions);
    changed |= merge_values(&mut target.pending_results, &incoming.pending_results);

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
