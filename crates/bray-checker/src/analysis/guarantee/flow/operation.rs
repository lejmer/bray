use super::model::ExecutionFlowDomain;

use bray_bound_tree::{
    AnyBoundNodeId, BoundAssignmentOperator, BoundControlTransferKind, BoundExpression,
    BoundExpressionId,
};

use crate::CheckerRequestContext;
use crate::execution_guarantees::ExecutionCondition;

use super::super::state::ExecutionState;

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis::guarantee) fn operation(
        &self,
        state: &mut ExecutionState,
        operation: super::super::super::model::AnalysisOperationKind,
        observe: Option<&mut super::super::cleanup::Observer<'_>>,
    ) -> bool {
        if !matches!(
            operation,
            super::super::super::model::AnalysisOperationKind::Result {
                phase: super::super::super::model::AnalysisResultPhase::Begin,
                ..
            }
        ) && let (Some(body), bray_bound_tree::BoundExecutionSite::Node(node)) =
            (self.checked_body, operation.occurrence())
            && !self.assume_patterns(state, body.refinements().refinements_before(node).iter())
        {
            return false;
        }

        self.transition_cleanup(state, operation, observe);

        let expression = operation.occurrence().expression();

        let target = self
            .checked_body
            .map(bray_bound_tree::CheckedBodySemantics::storage_flow)
            .and_then(|flow| flow.assignment_target(expression?))
            .and_then(bray_bound_tree::StorageTarget::storage_path);

        let assignment =
            target.and_then(|target| crate::ExecutionPlace::storage_path(self.storage, target));

        let invalidation = self
            .checked_body
            .map(bray_bound_tree::CheckedBodySemantics::storage_flow)
            .filter(|flow| {
                target.is_some()
                    || expression.is_some_and(|expression| {
                        flow.checked_memory_operation(expression).is_some()
                    })
            })
            .and_then(|_| self.refinements.get(&operation.occurrence()));

        self.transfer_operation(state, operation, invalidation, assignment.as_ref());

        true
    }

    pub(in crate::analysis) fn transfer_operation(
        &self,
        state: &mut ExecutionState,
        operation: super::super::super::model::AnalysisOperationKind,
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
        assignment_place: Option<&crate::ExecutionPlace>,
    ) {
        if let super::super::super::model::AnalysisOperationKind::Result { expression, phase } =
            operation
        {
            match phase {
                super::super::super::model::AnalysisResultPhase::Begin => {
                    if self.values.can_repeat(expression.into()) {
                        state.retire_evaluation(bray_bound_tree::StorageMutation::Operation(
                            expression.into(),
                        ));
                    }

                    state.expressions.remove(&expression);
                    state.pending_results.remove(&expression);
                }
                super::super::super::model::AnalysisResultPhase::Completion => {
                    self.publish_result(state, expression);
                }
            }

            return;
        }

        if let super::super::super::model::AnalysisOperationKind::Call { invocation, phase } =
            operation
        {
            use super::super::super::model::AnalysisCallPhase;
            use bray_bound_tree::BoundExecutionSite as Site;

            match phase {
                AnalysisCallPhase::Input => {
                    if matches!(invocation, Site::Iterate(_) | Site::Next(_)) {
                        let result = self
                            .input_place(invocation)
                            .expect("implicit invocation has its owned result slot");

                        state.current.retain(|place, _| !result.contains(place));
                    }

                    if let Site::Node(AnyBoundNodeId::Expression(expression)) = invocation {
                        state.expressions.remove(&expression);
                        state.pending_results.remove(&expression);
                    }

                    if self.values.can_repeat(invocation) {
                        state.retire_evaluation(bray_bound_tree::StorageMutation::Operation(
                            invocation,
                        ));
                    }

                    if let Some(entry) = self.call_entry(state, invocation) {
                        state.entries.insert(invocation, entry);
                    }
                }
                AnalysisCallPhase::Attempt => {
                    let source = invocation.expression()
                        .and_then(|expression| self.task_effect_source(state, expression));

                    let observed = match invocation {
                        Site::Node(AnyBoundNodeId::Expression(expression)) => {
                            self.observed_call_result(state, expression)
                        }
                        _ => None,
                    };

                    let deferred = state.entries.get_mut(&invocation).is_some_and(|entry| {
                        let deferred = entry.pending_execution;

                        // This invocation's own effects do not revoke its captured entry.
                        entry.pending_execution = false;

                        deferred
                    });

                    if let Site::Node(AnyBoundNodeId::Expression(expression)) = invocation {
                        if let Some(observed) = observed {
                            state.pending_results.insert(expression, observed);
                        }

                        self.expression(state, expression, invalidation, assignment_place);

                        if let Some(value) = state.expressions.remove(&expression) {
                            if let Some(source) = source
                                && value != ExecutionCondition::Unknown
                            {
                                // Both completion publication and latent effects retain
                                // this immutable condition and its shared substructure.
                                state.deferred_invocations.insert(value.clone(), source);
                            }

                            state.pending_results.insert(expression, value);
                        }

                        state.trust_boundaries.remove(&expression);
                    } else {
                        if let Site::ScopedExit(expression) = invocation {
                            state.invalidate_trusted(&ExecutionCondition::ScopedCapability(
                                expression,
                            ));
                        }

                        self.invalidate(state, invocation, invalidation);
                    }

                    if let Some(entry) = state.entries.get_mut(&invocation) {
                        entry.pending_execution = deferred;
                    }
                }
                AnalysisCallPhase::Completion => {
                    self.complete_operation(state, operation, None);
                }
            }

            return;
        }

        if matches!(
            operation,
            super::super::super::model::AnalysisOperationKind::ScopeExit { .. }
                | super::super::super::model::AnalysisOperationKind::Suspension {
                    kind: super::super::super::model::AnalysisSuspensionKind::ScopedCall,
                    ..
                }
        ) {
            return;
        }

        let awaited = operation
            .occurrence()
            .expression()
            .filter(|_| {
                matches!(
                    operation,
                    crate::analysis::model::AnalysisOperationKind::Suspension {
                        kind: crate::analysis::model::AnalysisSuspensionKind::Await,
                        ..
                    }
                )
            })
            .and_then(|expression| self.awaited_invocation(state, expression));

        if let Some(entry) = awaited.and_then(|source| state.entries.get_mut(&source)) {
            // Execution owns this entry evidence now. Its own effects must not erase it.
            entry.pending_execution = false;
        }

        match operation.node() {
            AnyBoundNodeId::Expression(expression) => {
                if !matches!(
                    operation,
                    super::super::super::model::AnalysisOperationKind::Call {
                        phase: super::super::super::model::AnalysisCallPhase::Completion,
                        ..
                    }
                ) {
                    // Only a cyclic occurrence can have an earlier incarnation. Its cache
                    // may already have expired, while values copied from it still survive.
                    if self.values.can_repeat(expression.into()) {
                        state.retire_evaluation(bray_bound_tree::StorageMutation::Operation(
                            expression.into(),
                        ));
                    }

                    if let Some(entry) = self.call_entry(state, expression.into()) {
                        state.entries.insert(expression.into(), entry);
                    }

                    if let Some(observed) = self.observed_call_result(state, expression) {
                        state.pending_results.insert(expression, observed);
                    }

                    let source = self.task_effect_source(state, expression);

                    self.expression(state, expression, invalidation, assignment_place);

                    if let Some(source) = source {
                        let value = self.value(state, expression);

                        if value != ExecutionCondition::Unknown {
                            state.deferred_invocations.insert(value, source);
                        }
                    }
                }

                if self
                    .checked_body
                    .map(bray_bound_tree::CheckedBodySemantics::storage_flow)
                    .is_some()
                {
                    self.complete_operation(state, operation, None);
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

        if let Some(entry) = awaited.and_then(|source| state.entries.get_mut(&source)) {
            // Keep the captured invocation until its normal or abnormal continuation.
            entry.pending_execution = true;
        }
    }
    pub(super) fn retain_successor_values(
        &self,
        state: &mut ExecutionState,
        edge: &crate::analysis::model::AnalysisEdge,
    ) {
        let retained = self.values.incoming(edge.target());

        state
            .expressions
            .retain(|expression, _| retained.contains(expression));

        // Completed call evidence is consumed during block publication. Only an
        // in-flight invocation or deferred execution needs it in the successor.
        for operation in self
            .graph
            .block(edge.source())
            .expect("edge source exists")
            .operations()
            .iter()
            .filter_map(|id| self.graph.operation(*id))
        {
            let kind = operation.kind();

            if !matches!(
                kind,
                crate::analysis::model::AnalysisOperationKind::Call {
                    phase: crate::analysis::model::AnalysisCallPhase::Input
                        | crate::analysis::model::AnalysisCallPhase::Attempt,
                    ..
                } | crate::analysis::model::AnalysisOperationKind::Suspension { .. }
            ) {
                let occurrence = kind.occurrence();

                if state
                    .entries
                    .get(&occurrence)
                    .is_some_and(|entry| !entry.pending_execution)
                {
                    state.entries.remove(&occurrence);
                }
            }
        }

        if let Some(crate::analysis::model::AnalysisRefinement::InvocationFailure(invocation)) =
            edge.refinement()
        {
            if let Some(source) = invocation
                .expression()
                .and_then(|expression| self.awaited_invocation(state, expression))
            {
                state.entries.remove(&source);
            }

            state.entries.remove(&invocation);

            if let bray_bound_tree::BoundExecutionSite::Node(AnyBoundNodeId::Expression(
                expression,
            )) = invocation
            {
                state.pending_results.remove(&expression);
            }
        }

        if let Some(crate::analysis::model::AnalysisRefinement::PreconditionFailure(invocation)) =
            edge.refinement()
        {
            state.entries.remove(&invocation);
        }
    }

    pub(in crate::analysis) fn complete_operation(
        &self,
        state: &mut ExecutionState,
        operation: super::super::super::model::AnalysisOperationKind,
        observations: Option<&bray_bound_tree::StorageCompletion>,
    ) {
        if matches!(
            operation,
            super::super::super::model::AnalysisOperationKind::Call {
                phase: super::super::super::model::AnalysisCallPhase::Input
                    | super::super::super::model::AnalysisCallPhase::Attempt,
                ..
            } | super::super::super::model::AnalysisOperationKind::Suspension { .. }
        ) {
            return;
        }

        if let super::super::super::model::AnalysisOperationKind::Call {
            invocation:
                bray_bound_tree::BoundExecutionSite::Node(AnyBoundNodeId::Expression(expression)),
            phase: super::super::super::model::AnalysisCallPhase::Completion,
        } = operation
            && let Some(value) = state.pending_results.remove(&expression)
        {
            state.expressions.insert(expression, value);
        }

        let input = matches!(
            operation.occurrence(),
            bray_bound_tree::BoundExecutionSite::CallInput { .. }
        );

        if input {
            self.complete_input(state, operation.occurrence());
        }

        let Some(expression) = operation.occurrence().expression() else {
            return;
        };

        if self
            .checked_body
            .map(bray_bound_tree::CheckedBodySemantics::storage_flow)
            .and_then(|flow| flow.memory_operation_status(expression))
            .is_some_and(|status| status != bray_bound_tree::MemoryOperationStatus::Valid)
        {
            return;
        }

        let empty = Default::default();

        let observations = observations
            .or_else(|| {
                self.checked_body
                    .map(bray_bound_tree::CheckedBodySemantics::storage_flow)
                    .and_then(|flow| {
                        flow.completion(bray_bound_tree::StorageMutation::Operation(
                            operation.occurrence(),
                        ))
                    })
            })
            .unwrap_or(&empty);

        self.observe_references(state, observations.references());

        if input {
            let invocation = operation.occurrence();
            let result = self.invocation_value(state, invocation);

            self.complete_trusted_call(state, invocation, result, observations.borrow_origins());
            return;
        }

        if matches!(
            operation,
            super::super::super::model::AnalysisOperationKind::Result { .. }
        ) {
            return;
        }

        if matches!(
            operation.occurrence(),
            bray_bound_tree::BoundExecutionSite::Iterate(_)
                | bray_bound_tree::BoundExecutionSite::Next(_)
                | bray_bound_tree::BoundExecutionSite::ScopedEnter(_)
                | bray_bound_tree::BoundExecutionSite::ScopedExit(_)
        ) {
            let invocation = operation.occurrence();

            if let Some(entry) = state.entries.get_mut(&invocation) {
                entry.pending_execution = false;
            }

            let result = self.invocation_value(state, invocation);

            if matches!(
                invocation,
                bray_bound_tree::BoundExecutionSite::Iterate(_)
                    | bray_bound_tree::BoundExecutionSite::Next(_)
            ) {
                let place = self
                    .input_place(invocation)
                    .expect("completed protocol invocation retains its result slot");

                state.assign(place, result.clone());
            }

            self.complete_trusted_call(state, invocation, result, observations.borrow_origins());

            if let bray_bound_tree::BoundExecutionSite::ScopedExit(expression) = invocation {
                state.invalidate_trusted(&ExecutionCondition::ScopedCapability(expression));
            }

            return;
        }

        if let Some(source) = self.awaited_invocation(state, expression) {
            if let Some(entry) = state.entries.get_mut(&source) {
                entry.pending_execution = false;
            }

            self.complete_trusted_call(
                state,
                source,
                ExecutionCondition::Expression(expression),
                observations.borrow_origins(),
            );
        } else {
            self.complete_call(state, expression, observations.borrow_origins());
        }

        self.complete_memory_write(state, expression);
        self.preserve_integer_value(state, expression);
    }

    fn publish_result(&self, state: &mut ExecutionState, expression: BoundExpressionId) {
        let value = state
            .pending_results
            .remove(&expression)
            .unwrap_or_else(|| self.value(state, expression));

        state.expressions.insert(expression, value);
    }

    fn expression(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
        assignment_place: Option<&crate::ExecutionPlace>,
    ) {
        state.expressions.remove(&expression);

        let machine_value = self.machine_operation_value(state, expression);

        let assignment = matches!(
            self.request.view().expression(expression),
            Some(BoundExpression::Assignment(_))
        );

        let value = state
            .pending_results
            .remove(&expression)
            .or_else(|| (!assignment).then(|| machine_value.clone()).flatten())
            .unwrap_or_else(|| self.value(state, expression));

        state.expressions.insert(expression, value);
        self.invalidate(state, expression.into(), invalidation);

        match self.request.view().expression(expression) {
            Some(BoundExpression::Assignment(assignment)) => {
                if let [target, source] = assignment.operands()
                    && let Some(place) = crate::execution_guarantees::expression_place(
                        self.request.unit(),
                        self.semantics,
                        self.request.semantic_values(),
                        *target,
                    )
                {
                    let value = if assignment.operator() == BoundAssignmentOperator::Assign {
                        state
                            .expressions
                            .get(source)
                            .cloned()
                            .unwrap_or_else(|| self.value(state, *source))
                    } else {
                        match machine_value.clone().unwrap_or(ExecutionCondition::Unknown) {
                            ExecutionCondition::Unknown => ExecutionCondition::PostState(
                                bray_bound_tree::StorageMutation::Operation(expression.into()),
                                assignment_place.unwrap_or(&place).clone(),
                            ),
                            value => value,
                        }
                    };

                    let value = value.coerce(
                        self.request.semantic_values(),
                        self.semantics
                            .types()
                            .expression(*source)
                            .expect("assignment source retains its checked type")
                            .ty(),
                        self.semantics
                            .types()
                            .expression(*target)
                            .expect("assignment target retains its checked type")
                            .ty(),
                    );

                    if let Some(owner) = assignment_place
                        && *owner != place
                    {
                        state.assign(owner.clone(), value.clone());
                    }

                    state.assign(place, value);
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
                            .coerce(
                                self.request.semantic_values(),
                                self.semantics
                                    .types()
                                    .expression(operand)
                                    .expect("return operand retains its checked type")
                                    .ty(),
                                self.semantics
                                    .types()
                                    .callable_result_type()
                                    .expect("return retains its callable result type"),
                            )
                    })
                    .unwrap_or(ExecutionCondition::Unknown);
            }
            Some(BoundExpression::ControlTransfer(transfer))
                if matches!(
                    transfer.kind(),
                    BoundControlTransferKind::Yield | BoundControlTransferKind::Break
                ) =>
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

    fn pattern(&self, state: &mut ExecutionState, pattern: bray_bound_tree::BoundPatternId) {
        for (binding, expression, access) in
            crate::storage::pattern_binding_sources(self.request.view(), self.storage, pattern)
        {
            let mut value = self.storage_value(state, expression, access);

            if let Some(bray_bound_tree::StorageBinding::Identity(identity)) =
                self.storage.binding(binding)
            {
                value = value.coerce(
                    self.request.semantic_values(),
                    self.storage
                        .access(access)
                        .expect("pattern retains its source storage")
                        .reached_type(),
                    self.storage
                        .identity_type(identity)
                        .expect("binding retains its checked type"),
                );
            }

            let place = crate::ExecutionPlace::binding(self.storage, binding)
                .expect("checked pattern binding has an execution place");

            // Named and discarded owners retain the same committed value and cleanup evidence.
            state.assign(place, value);
        }
    }
}
