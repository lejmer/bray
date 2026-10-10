use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    AnyBoundNodeId, BoundAssignmentOperator, BoundControlTransferKind, BoundExpression,
    BoundExpressionId, BoundReferenceTarget, CheckedExpressionSemantics,
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
    let (copied_types, diagnostics) =
        match crate::analysis::storage_flow::copyability::storage_copyable_types(request, storage) {
            Ok(result) => result,
            Err(CheckerQueryError::Cancelled) => return CheckerOutcome::Cancelled,
            Err(CheckerQueryError::Upstream(error)) => {
                return CheckerOutcome::UpstreamFailure(error);
            }
        };

    let mut invalidating = super::super::storage_invalidation::invalidating_operation_accesses(
        request,
        semantics.selections(),
        storage,
        &copied_types,
    );

    let mut observed_types = BTreeMap::new();
    let mut assigned_places = BTreeSet::new();
    let mut targets = BTreeMap::new();
    let mut yields = Vec::new();

    for (expression, node) in request.unit().tree().expressions() {
        targets.insert(node.origin().source_anchor().syntax(), expression);

        if let BoundExpression::ControlTransfer(transfer) = node
            && transfer.kind() == BoundControlTransferKind::Yield
            && let Some(target) = transfer.target()
        {
            yields.push((expression, target));
        }

        if let BoundExpression::Assignment(assignment) = node
            && let Some(target) = assignment.operands().first()
            && let Some(BoundExpression::Name(name)) = request.view().expression(*target)
        {
            assigned_places.insert(name.target().into());
        }

        if let Some(place) = crate::execution_guarantees::expression_place(
            request.unit(),
            semantics,
            request.semantic_values(),
            expression,
        ) {
            let mut ty = semantics
                .types()
                .expression(expression)
                .map(|entry| entry.ty());

            while let Some(current) = ty
                && let bray_symbols::TypeData::Borrow { target, .. } =
                    request.semantic_values().type_data(current).as_ref()
            {
                ty = Some(*target);
            }

            observed_types
                .entry(place)
                .and_modify(|known| {
                    if *known != ty {
                        *known = None;
                    }
                })
                .or_insert(ty);
        }

        if let BoundExpression::Call(call) = node
            && semantics.types().expression(call.callee()).is_some_and(|entry|
                matches!(request.semantic_values().type_data(entry.ty()).as_ref(), bray_symbols::TypeData::Callable(callable)
                    if callable.constness() == bray_symbols::CallableConstness::Constant)) {
            invalidating.remove(&expression.into());
        }

        if let Some(bray_bound_tree::SemanticSelection::Call(call)) =
            semantics.selections().expression(expression)
            && (super::super::storage_invalidation::call_preserves_storage(call, &copied_types)
                || !matches!(
                    call.resolution().result(),
                    bray_bound_tree::BoundCallResult::Immediate(_)
                )
                || matches!(
                    call.implementation_hook(),
                    Some(
                        bray_compiler_known::ImplementationHook::RawAllocate
                            | bray_compiler_known::ImplementationHook::RawBufferAllocate
                            | bray_compiler_known::ImplementationHook::Allocate
                    )
                ))
        {
            // These closed operations observe existing storage or create a fresh allocation.
            // They cannot change existing owner epochs or initialized contents.
            invalidating.remove(&expression.into());
        }
    }

    let yield_targets = yields
        .into_iter()
        .filter_map(|(id, target)| targets.get(&target).copied().map(|target| (id, target)))
        .collect();

    let mut scope_places = BTreeMap::<_, Vec<_>>::new();

    if request.trusted_contracts().is_some_and(|contracts| {
        !contracts.requirements.is_empty()
            || !contracts.guarantees.is_empty()
            || !contracts.calls.is_empty()
    }) {
        let owners = bray_bound_tree::StorageScopeOwners::collect(request.unit())
            .expect("checked bound unit must have balanced lexical scopes");

        for (target, binding) in storage.bindings() {
            let identity = match binding {
                bray_bound_tree::StorageBinding::Identity(identity) => Some(*identity),
                bray_bound_tree::StorageBinding::Access(access) => storage.root_identity(*access),
            };

            // A borrowed input ends its local binding at return, while its caller retains
            // the reached storage and the authority carried by a returned projection.
            if identity.is_some_and(|identity| {
                matches!(
                    storage.identity(identity),
                    Some(
                        bray_bound_tree::StorageIdentity::Parameter(_)
                            | bray_bound_tree::StorageIdentity::Receiver(_)
                            | bray_bound_tree::StorageIdentity::AnonymousParameter(_)
                    )
                ) && storage.identity_type(identity).is_some_and(|ty| {
                    matches!(
                        request.semantic_values().type_data(ty).as_ref(),
                        bray_symbols::TypeData::Borrow { .. }
                    )
                })
            }) {
                continue;
            }

            if let Some(scope) =
                identity.and_then(|identity| owners.identity_scope(storage, identity))
                && let Some(reference) =
                    crate::execution_guarantees::storage_binding_reference(*target)
            {
                scope_places.entry(scope).or_default().push((
                    crate::ExecutionPlace::from(reference),
                    identity.expect("scoped binding has a storage identity"),
                ));
            }
        }
    }

    let domain = ExecutionFlowDomain {
        copied_types,
        observed_types,
        assigned_places,
        scope_places,
        yield_targets,
        graph,
        request,
        semantics,
        assumptions,
        literals,
        storage,
        contracts,
        cleanup,
        invalidating,
        spatial_predicates: request
            .trusted_contracts()
            .filter(|contracts| !contracts.requirements.is_empty() || !contracts.calls.is_empty())
            .into_iter()
            .flat_map(|_| {
                [
                    "ValidRead",
                    "ValidWrite",
                    "AlignedFor",
                    "DeviceValidRead",
                    "DeviceValidWrite",
                    "DeviceAlignedFor",
                ]
            })
            .filter_map(|name| {
                let key = bray_compiler_known::CompilerKnownDeclarationKey::try_new(name)
                    .expect("closed spatial predicate key must be valid");

                request
                    .context()
                    .available_compiler_known_symbols()
                    .provider()
                    .declaration_symbol::<bray_symbols::PredicateSymbolId>(&key)
                    .map(Into::into)
            })
            .collect(),
        owned_allocation: request
            .context()
            .available_compiler_known_symbols()
            .provider()
            .declaration_symbol::<bray_symbols::PredicateSymbolId>(
                &bray_compiler_known::CompilerKnownDeclarationKey::try_new("OwnedAllocation")
                    .expect("closed allocation predicate key must be valid"),
            )
            .map(Into::into),
        initialized_range: request
            .context()
            .available_compiler_known_symbols()
            .provider()
            .declaration_symbol::<bray_symbols::PredicateSymbolId>(
                &bray_compiler_known::CompilerKnownDeclarationKey::try_new("InitializedRangeAs")
                    .expect("closed initialized range predicate key must be valid"),
            )
            .map(Into::into),
    };

    let states = match solve_fixed_point(graph, &domain, &request) {
        FixedPointOutcome::Complete(states) => states,
        FixedPointOutcome::Cancelled => return CheckerOutcome::Cancelled,
        FixedPointOutcome::ConvergenceInvariantViolated => {
            panic!("execution condition flow exceeded its finite convergence bound");
        }
    };

    CheckerOutcome::complete(ExecutionFlow { domain, states }, diagnostics)
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
    pub(super) copied_types: BTreeSet<bray_symbols::TypeId>,
    observed_types: BTreeMap<crate::ExecutionPlace, Option<bray_symbols::TypeId>>,
    assigned_places: BTreeSet<crate::ExecutionPlace>,
    pub(super) scope_places: BTreeMap<
        bray_bound_tree::BoundBlockId,
        Vec<(crate::ExecutionPlace, bray_bound_tree::StorageIdentityId)>,
    >,
    yield_targets: BTreeMap<BoundExpressionId, BoundExpressionId>,
    pub(super) graph: &'a ControlFlowGraph,
    pub(super) request: CheckerUnitView<'view, C>,
    pub(super) semantics: &'a CheckedExpressionSemantics,
    assumptions: &'a [ExecutionCondition],
    literals: &'a BTreeMap<BoundExpressionId, ExecutionCondition>,
    pub(super) storage: &'a bray_bound_tree::StoragePlan,
    pub(super) contracts: &'a BTreeMap<BoundExpressionId, Vec<crate::ExecutionCompletionContract>>,
    cleanup: Option<&'a bray_bound_tree::CheckedAsync>,
    invalidating: BTreeMap<bray_bound_tree::BoundExecutionSite, StorageInvalidation>,
    pub(super) spatial_predicates: BTreeSet<bray_symbols::PredicateDefinitionSymbolId>,
    pub(super) owned_allocation: Option<bray_symbols::PredicateDefinitionSymbolId>,
    pub(super) initialized_range: Option<bray_symbols::PredicateDefinitionSymbolId>,
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn value(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> ExecutionCondition {
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

    pub(super) fn invocation_value(
        &self,
        state: &ExecutionState,
        invocation: bray_bound_tree::BoundExecutionSite,
    ) -> ExecutionCondition {
        match invocation {
            bray_bound_tree::BoundExecutionSite::Node(
                bray_bound_tree::AnyBoundNodeId::Expression(expression),
            ) => self.value(state, expression),
            bray_bound_tree::BoundExecutionSite::ScopedEnter(expression) => {
                ExecutionCondition::ScopedCapability(expression)
            }
            bray_bound_tree::BoundExecutionSite::ScopedExit(_) => ExecutionCondition::Unknown,
            bray_bound_tree::BoundExecutionSite::Node(node) => {
                panic!("invocation value requires an expression owner, got {node:?}")
            }
        }
    }

    pub(super) fn unsigned_value(&self, value: &ExecutionCondition) -> bool {
        let ty = match value {
            ExecutionCondition::Input(place) | ExecutionCondition::Joined(_, _, place) => {
                self.observed_types.get(place).copied().flatten()
            }
            ExecutionCondition::Expression(expression) => self
                .semantics
                .types()
                .expression(*expression)
                .map(|entry| entry.ty()),
            ExecutionCondition::Literal(value) => Some(value.ty()),
            _ => None,
        };

        ty.and_then(|ty| crate::representation::type_representation(self.request, ty))
            .and_then(bray_compiler_known::RepresentationRole::integer_representation)
            .is_some_and(|representation| {
                matches!(
                    representation,
                    bray_compiler_known::IntegerRepresentation::Unsigned(_)
                        | bray_compiler_known::IntegerRepresentation::TargetUnsigned
                )
            })
    }

    pub(super) fn storage_value(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        access: bray_bound_tree::StorageAccessId,
    ) -> ExecutionCondition {
        let (mut value, scoped) = match self
            .storage
            .root_identity(access)
            .and_then(|identity| self.storage.identity(identity))
        {
            Some(bray_bound_tree::StorageIdentity::ScopedCapability { expression, .. }) => {
                (ExecutionCondition::ScopedCapability(expression), true)
            }
            _ => (self.value(state, expression), false),
        };

        let source = self
            .storage
            .expression_plans(expression)
            .filter(|source| {
                source.node() == AnyBoundNodeId::Expression(expression)
                    && self.storage.access_contains(source.access(), access)
            })
            .max_by_key(|source| {
                self.storage
                    .resolved_projections(source.access())
                    .map_or(0, <[_]>::len)
            });

        // A scoped root names the whole retained capability, even when this occurrence
        // observes one of its fields. Ordinary expression values already include their source path.
        let source_path = if scoped {
            &[][..]
        } else {
            source
                .and_then(|source| self.storage.resolved_projections(source.access()))
                .unwrap_or(&[])
        };

        let path = self
            .storage
            .resolved_projections(access)
            .and_then(|path| path.strip_prefix(source_path));

        if let Some(path) = path {
            for projection in path {
                value = value.project(*projection);
            }
        } else {
            value = ExecutionCondition::Unknown;
        }

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
                    .assume(true, &mut next.assumptions);
            }

            condition.assume(value, &mut next.assumptions);
        }

        if let Some(AnalysisRefinement::PatternOutcome {
            subject,
            pattern,
            value,
        }) = edge.refinement()
            && let Some(expected) = self
                .request
                .trusted_contracts()
                .and_then(|contracts| contracts.pattern_values.get(&pattern))
        {
            let condition = ExecutionCondition::operation(
                bray_bound_tree::BoundOperator::Equal,
                vec![self.value(state, subject), expected.clone()],
            );

            if condition
                .prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT })
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
        if let super::super::model::AnalysisOperationKind::Call {
            invocation:
                invocation @ (bray_bound_tree::BoundExecutionSite::ScopedEnter(_)
                | bray_bound_tree::BoundExecutionSite::ScopedExit(_)),
            phase,
        } = operation
        {
            match phase {
                super::super::model::AnalysisCallPhase::Attempt => {
                    let entry = self.call_entry(state, invocation);

                    if let bray_bound_tree::BoundExecutionSite::ScopedExit(expression) = invocation
                    {
                        // Exit may transfer obligations from its captured input contract. The
                        // entered capability itself grants no authority beyond this boundary.
                        state.invalidate_trusted(&ExecutionCondition::ScopedCapability(expression));
                    }

                    self.invalidate(state, invocation);

                    // The invocation owns its captured inputs after transfer. Invalidation
                    // retires caller authority and earlier computations, not this callee's entry evidence.
                    if let Some(entry) = entry {
                        state.entries.insert(invocation, entry);
                    }
                }
                super::super::model::AnalysisCallPhase::Completion => {
                    let result = self.invocation_value(state, invocation);

                    self.complete_trusted_call(state, invocation, result);

                    if let bray_bound_tree::BoundExecutionSite::ScopedExit(expression) = invocation
                    {
                        // Only guarantees rewritten to surviving values can outlive the consumed capability.
                        state.invalidate_trusted(&ExecutionCondition::ScopedCapability(expression));
                    }
                }
            }

            return;
        }

        if let super::super::model::AnalysisOperationKind::ScopeExit { block, exit, phase } =
            operation
        {
            let invalidates = self.cleanup.is_some_and(|cleanup| {
                cleanup
                    .scope_exit_plan(block, exit)
                    .is_some_and(|plan| match phase {
                        super::super::model::AnalysisScopeExitPhase::TaskCancellationBroadcast => {
                            !plan.cancellation_broadcast().is_empty()
                        }
                        super::super::model::AnalysisScopeExitPhase::LifecycleResolution => plan
                            .lifecycle_resolution()
                            .iter()
                            .any(|access| !self.cleanup_preserves_inputs(state, *access)),
                    })
            });

            if phase == super::super::model::AnalysisScopeExitPhase::LifecycleResolution {
                self.expire_trusted_scope(state, block);
            }

            if invalidates {
                state.invalidate_cleanup(None);
            }

            return;
        }

        match operation.node() {
            AnyBoundNodeId::Expression(expression) => {
                if self.cleanup.is_some_and(|cleanup| {
                    cleanup.replacement(expression).is_some_and(|plan| {
                        matches!(
                            plan.cleanup(),
                            bray_bound_tree::AsyncStorageCleanupRequirement::Cleanup(_)
                        ) && !self.cleanup_preserves_inputs(state, plan.access())
                    })
                }) {
                    state.invalidate_cleanup(None);
                }

                if !matches!(
                    operation,
                    super::super::model::AnalysisOperationKind::Call {
                        phase: super::super::model::AnalysisCallPhase::Completion,
                        ..
                    }
                ) {
                    // This occurrence is executing again; its cached observation belongs
                    // to the prior iteration, even when it contains an immutable input term.
                    state.expressions.remove(&expression);

                    state
                        .witness_carriers
                        .retain(|value| !value.depends_on(expression));

                    state
                        .trusted_assumptions
                        .retain(|(condition, _)| !condition.depends_on(expression));

                    state.allocation_owners.retain(|condition, owner| {
                        !condition.depends_on(expression) && !owner.depends_on(expression)
                    });

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

                    if let Some(entry) = self.call_entry(state, expression.into()) {
                        state.entries.insert(expression.into(), entry);
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

                if self
                    .request
                    .trusted_contracts()
                    .and_then(|contracts| contracts.calls.get(&expression.into()))
                    .is_some_and(|contract| !contract.completes)
                    && let Some(entry) = state.entries.get_mut(&expression.into())
                {
                    entry.pending_execution = true;
                }
            }
            AnyBoundNodeId::Pattern(pattern) => self.pattern(state, pattern),
            _ => {}
        }
    }
    fn expression(&self, state: &mut ExecutionState, expression: BoundExpressionId) {
        state.expressions.remove(&expression);

        let mut value = state
            .pending_results
            .remove(&expression)
            .unwrap_or_else(|| self.value(state, expression));

        if !matches!(value, ExecutionCondition::Expression(_))
            && let Some(BoundExpression::Name(name)) = self.request.view().expression(expression)
            && matches!(
                name.target(),
                BoundReferenceTarget::Local(bray_symbols::AnyLocalSymbolId::Binding(_))
            )
            && let Some(ty) = self.semantics.types().expression(expression)
            && crate::representation::type_representation(self.request, ty.ty())
                .is_some_and(|role| role.numeric_kind().is_some())
        {
            let place = name.target().into();

            if self.assigned_places.contains(&place) && state.current.contains_key(&place) {
                // Re-executing this observation retires its old facts before reading the current value.
                let observed = ExecutionCondition::Expression(expression);

                if value != ExecutionCondition::Unknown {
                    ExecutionCondition::operation(
                        bray_bound_tree::BoundOperator::Equal,
                        vec![observed.clone(), value],
                    )
                    .assume(true, &mut state.assumptions);
                }

                value = observed;
                state.assign(place, value.clone());
            }
        }

        state.expressions.insert(expression, value);
        self.invalidate(state, expression.into());

        match self.request.view().expression(expression) {
            Some(BoundExpression::Assignment(assignment)) => {
                if let [target, source] = assignment.operands() {
                    if let Some(place) = crate::execution_guarantees::expression_place(
                        self.request.unit(),
                        self.semantics,
                        self.request.semantic_values(),
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
            Some(BoundExpression::ControlTransfer(transfer))
                if transfer.kind() == BoundControlTransferKind::Yield =>
            {
                if let Some(target) = self.yield_targets.get(&expression) {
                    let value = transfer
                        .operand()
                        .map(|operand| self.value(state, operand))
                        .unwrap_or(ExecutionCondition::Unknown);

                    state.pending_results.insert(*target, value);
                }
            }
            _ => {}
        }
    }

    fn spare_storage_header(
        &self,
        state: &ExecutionState,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) -> Option<crate::ExecutionPlace> {
        use bray_bound_tree::{SelectedArgument, SemanticSelection};
        use bray_compiler_known::ImplementationHook;

        let SemanticSelection::Call(call) = self
            .semantics
            .selections()
            .expression(occurrence.expression()?)?
        else {
            return None;
        };

        let destination = match call.implementation_hook()? {
            ImplementationHook::MemoryCopy | ImplementationHook::MemoryCopyOverlapping => 1,
            ImplementationHook::RawPointerWrite
            | ImplementationHook::ByteBufferFill
            | ImplementationHook::VolatileStore => 0,
            _ => return None,
        };

        let argument = |call: &bray_bound_tree::SelectedCall, index| {
            call.arguments().iter().find_map(|argument| match argument {
                SelectedArgument::Explicit {
                    expression,
                    ordinal,
                    ..
                } if *ordinal == index => Some(*expression),
                _ => None,
            })
        };

        let destination_value = self.value(state, argument(call, destination)?);

        let ExecutionCondition::Expression(producer) = destination_value else {
            return None;
        };

        let SemanticSelection::Call(producer_call) =
            self.semantics.selections().expression(producer)?
        else {
            return None;
        };

        if producer_call.implementation_hook() != Some(ImplementationHook::RawBufferSparePointer) {
            return None;
        }

        let contract = self.request.trusted_contracts()?.calls.get(&occurrence)?;
        let entry = state.entries.get(&occurrence)?;

        if !entry.proves(&contract.preconditions)
            || !self.trusted_requirements_proven(entry, &contract.requirements)
        {
            return None;
        }

        let buffer = argument(producer_call, 0)?;
        let value = self.value(state, buffer);
        let producer_entry = state.entries.get(&producer.into())?;

        if producer_entry
            .arguments
            .get(&crate::ExecutionPlace::argument(
                bray_symbols::SymbolOrdinal::new(0),
            ))
            != Some(&value)
        {
            return None;
        }

        // This closed accessor names only the live owner's spare allocation. A
        // checked store there can change payload contents, but cannot reach its header.
        match value {
            ExecutionCondition::Input(place) => Some(place),
            ExecutionCondition::Borrowed(value) => match value.as_ref() {
                ExecutionCondition::Input(place) => Some(place.clone()),
                _ => None,
            },
            _ => crate::execution_guarantees::expression_place(
                self.request.unit(),
                self.semantics,
                self.request.semantic_values(),
                buffer,
            ),
        }
    }

    fn invalidate(
        &self,
        state: &mut ExecutionState,
        occurrence: bray_bound_tree::BoundExecutionSite,
    ) {
        let Some(invalidation) = self.invalidating.get(&occurrence) else {
            return;
        };

        // A proven zero extent has no writes, epoch changes, or ownership effects.
        if self.empty_memory_copy(state, occurrence) {
            return;
        }

        if matches!(invalidation, StorageInvalidation::All) {
            if let Some(contract) = self
                .request
                .trusted_contracts()
                .and_then(|contracts| contracts.calls.get(&occurrence))
                && contract.preserves_inputs
                && state.entries.get(&occurrence).is_some_and(|entry| {
                    entry.proves(&contract.preconditions)
                        && (entry.trusted_boundary
                            || self.trusted_requirements_proven(entry, &contract.requirements))
                })
            {
                return;
            }

            let preserves_storage = match occurrence {
                bray_bound_tree::BoundExecutionSite::Node(AnyBoundNodeId::Expression(
                    expression,
                )) => {
                    matches!(self.semantics.selections().expression(expression),
                    Some(bray_bound_tree::SemanticSelection::Call(call)) if matches!(call.implementation_hook(), Some(
                        bray_compiler_known::ImplementationHook::RawPointerWrite
                        | bray_compiler_known::ImplementationHook::RawPointerRead
                        | bray_compiler_known::ImplementationHook::VolatileStore
                        | bray_compiler_known::ImplementationHook::DeviceVolatileStore)))
                }
                _ => false,
            };

            let spatial = if preserves_storage {
                state.trusted_assumptions.iter().filter(|(condition, _)| {
                    let condition = match condition {
                        ExecutionCondition::Trusted(condition) => condition.as_ref(),
                        condition => condition,
                    };

                    matches!(condition, ExecutionCondition::Predicate(predicate, _, _) if self.spatial_predicates.contains(predicate))
                })
                    .cloned().collect()
            } else {
                BTreeSet::new()
            };

            let dependencies = if preserves_storage {
                // Reached addresses keep their owner lifetimes even when their contents change.
                // Retain the complete dependency chain, including intermediate address views.
                std::mem::take(&mut state.witness_dependencies)
            } else {
                BTreeMap::new()
            };

            let preserved_header = self.spare_storage_header(state, occurrence);

            state.invalidate_cleanup(preserved_header.as_ref());
            state.trusted_assumptions = spatial;
            state.witness_dependencies = dependencies;

            for place in self.observed_types.keys() {
                if !place.projections.is_empty()
                    && !preserved_header
                        .as_ref()
                        .is_some_and(|header| header.contains(place))
                {
                    state
                        .current
                        .insert(place.clone(), ExecutionCondition::Unknown);
                }
            }

            return;
        }

        let StorageInvalidation::Accesses(accesses) = invalidation else {
            unreachable!("global invalidation is handled before per-access invalidation");
        };

        let transfers = !self.storage.occurrence_plans(occurrence).any(|plan| {
            matches!(
                plan.purpose(),
                bray_bound_tree::StorageAccessPurpose::Assignment
                    | bray_bound_tree::StorageAccessPurpose::Initialize
            )
        });

        let mut changed_places = BTreeSet::new();
        let mut reset_roots = BTreeSet::new();

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

            for invalidated in accesses {
                if self.storage.relationship(*invalidated, access)
                    == bray_bound_tree::StorageRelationship::Disjoint
                {
                    continue;
                }

                let changed = self
                    .storage
                    .resolved_projections(*invalidated)
                    .and_then(|path| {
                        // Checked constant selectors identify components independently of their occurrences.
                        let path = path
                            .iter()
                            .map(|projection| {
                                if let bray_bound_tree::StorageProjection::Element(selector) =
                                    projection
                                    && let Some(ExecutionCondition::Literal(value)) =
                                        self.literals.get(selector)
                                    && let bray_symbols::ConstantValueKind::Integer(index) =
                                        value.kind()
                                    && let Some(index) =
                                        index.to_u64().and_then(|index| u32::try_from(index).ok())
                                {
                                    return bray_bound_tree::StorageProjection::ElementFromStart(
                                        bray_symbols::SymbolOrdinal::new(index),
                                    );
                                }

                                *projection
                            })
                            .collect::<Vec<_>>();

                        crate::ExecutionPlace::from(reference).project(&path)
                    })
                    .unwrap_or_else(|| reference.into());

                changed_places.insert(changed);
            }

            if !transfers
                || accesses.iter().any(|access| {
                    self.storage
                        .resolved_projections(*access)
                        .is_none_or(|projections| projections.is_empty())
                })
            {
                reset_roots.insert(crate::ExecutionPlace::from(reference));
            }
        }

        let invalidated_values = if transfers {
            Vec::new()
        } else {
            changed_places
                .iter()
                .flat_map(|place| {
                    [
                        place
                            .value_in(&state.current)
                            .unwrap_or_else(|| ExecutionCondition::Input(place.clone())),
                        ExecutionCondition::Input(place.clone()),
                    ]
                })
                .collect()
        };

        // Resolve every affected place before changing observations, and index witness
        // dependencies once for the complete mutation rather than once per destination.
        state.invalidate_trusted_places(&changed_places, invalidated_values);

        for (place, value) in &mut state.current {
            if if transfers {
                place.is_contained_by_any(&changed_places)
            } else {
                place.overlaps_any(&changed_places)
            } {
                *value = ExecutionCondition::Unknown;
            }
        }

        for place in changed_places.into_iter().chain(reset_roots) {
            state.current.insert(place, ExecutionCondition::Unknown);
        }
    }

    fn pattern(&self, state: &mut ExecutionState, pattern: bray_bound_tree::BoundPatternId) {
        let bound = self
            .request
            .view()
            .pattern(pattern)
            .expect("checked pattern must exist");

        let expression = self
            .storage
            .node_plans(pattern.into())
            .next()
            .map(|plan| plan.expression());

        let mut plans = self.storage.node_plans(pattern.into()).filter(|plan| {
            !matches!(
                plan.purpose(),
                bray_bound_tree::StorageAccessPurpose::Projection
                    | bray_bound_tree::StorageAccessPurpose::Initialize
            )
        });

        for binding in bound
            .bindings()
            .iter()
            .copied()
            .chain(bound.entries().iter().filter_map(|entry| entry.binding()))
        {
            let source = match self
                .storage
                .binding(bray_bound_tree::StorageBindingTarget::Local(binding))
            {
                Some(bray_bound_tree::StorageBinding::Access(access)) => {
                    expression.map(|expression| (expression, access))
                }
                Some(bray_bound_tree::StorageBinding::Identity(_)) => {
                    plans.next().map(|plan| (plan.expression(), plan.access()))
                }
                None => None,
            };

            let Some((expression, access)) = source else {
                continue;
            };

            let value = self.storage_value(state, expression, access);

            // Observing bindings use their committed component, independently of borrow and initialization plans.
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

        for place in self.observed_types.keys() {
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
                    .insert(place.clone(), ExecutionCondition::Input(place.clone()));
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
                condition
                    .clone()
                    .assume(true, &mut state.trusted_assumptions);
            }

            for (condition, owner) in self.allocation_subjects(&contracts.requirements) {
                let owner = ExecutionCondition::Input(owner);

                state.allocation_owners.insert(condition, owner.clone());
                state.witness_carriers.insert(owner);
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

fn merge(
    target: &mut Option<ExecutionState>,
    incoming: &Option<ExecutionState>,
    block: Option<AnalysisBlockId>,
) -> bool {
    let Some(incoming) = incoming else {
        return false;
    };

    let Some(target) = target else {
        // The first reachable predecessor supplies an independently owned flow environment.
        *target = Some(incoming.clone());

        return true;
    };

    let mut changed = false;
    let mut incoming_joined = std::borrow::Cow::Borrowed(incoming);

    if let Some(block) = block {
        let mut left = BTreeMap::new();
        let mut right = BTreeMap::new();
        let mut places = Vec::new();

        for (place, value) in &target.current {
            let Some(other) = incoming.current.get(place) else {
                continue;
            };

            if value == other
                || *value == ExecutionCondition::Unknown
                || *other == ExecutionCondition::Unknown
            {
                continue;
            }

            let joined = ExecutionCondition::Joined(
                block.unit(),
                u32::try_from(block.to_index().expect("flow block has an index"))
                    .expect("flow block index fits"),
                place.clone(),
            );

            places.push((place.clone(), joined.clone()));

            for (map, value) in [(&mut left, value), (&mut right, other)] {
                // Literal payloads are shared by unrelated places and contract constants.
                // Joining their slots must not rename the constant itself.
                if *value == joined
                    || matches!(
                        value,
                        ExecutionCondition::Literal(_) | ExecutionCondition::Boolean(_)
                    )
                {
                    continue;
                }

                // A value shared by differently merged aliases has no unique replacement.
                map.entry(value.clone())
                    .and_modify(|prior| {
                        if *prior != joined {
                            *prior = ExecutionCondition::Unknown;
                        }
                    })
                    .or_insert_with(|| joined.clone());
            }
        }

        if !places.is_empty() {
            target.join_values(&left);

            let incoming = incoming_joined.to_mut();

            incoming.join_values(&right);

            for (place, value) in places {
                changed |= target.current.get(&place) != Some(&value);
                target.current.insert(place.clone(), value.clone());
                incoming.current.insert(place, value);
            }
        }
    }

    let incoming = &incoming_joined;
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

    target
        .trust_boundaries
        .retain(|boundary| incoming.trust_boundaries.contains(boundary));

    changed |= boundaries_before != target.trust_boundaries.len();

    target
        .assumptions
        .retain(|condition| incoming.assumptions.contains(condition));

    changed |= before != target.assumptions.len();

    let before = target.witness_carriers.len();

    target
        .witness_carriers
        .retain(|value| incoming.witness_carriers.contains(value));

    changed |= before != target.witness_carriers.len();

    let before = target.trusted_assumptions.len();

    target
        .trusted_assumptions
        .retain(|condition| incoming.trusted_assumptions.contains(condition));

    changed |= before != target.trusted_assumptions.len();

    // Conflicting provenance cannot become an unbound allocation proof at a merge.
    for (condition, owner) in &mut target.allocation_owners {
        if incoming.allocation_owners.get(condition) != Some(owner)
            && *owner != ExecutionCondition::Unknown
        {
            *owner = ExecutionCondition::Unknown;
            changed = true;
        }
    }

    for condition in incoming.allocation_owners.keys() {
        if let std::collections::btree_map::Entry::Vacant(entry) =
            target.allocation_owners.entry(condition.clone())
        {
            entry.insert(ExecutionCondition::Unknown);
            changed = true;
        }
    }

    for (carrier, dependencies) in &incoming.witness_dependencies {
        let retained = target
            .witness_dependencies
            .entry(carrier.clone())
            .or_default();

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
