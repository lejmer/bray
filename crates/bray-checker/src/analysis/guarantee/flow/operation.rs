use super::model::ExecutionFlowDomain;

use bray_bound_tree::{
    AnyBoundNodeId, BoundAssignmentOperator, BoundControlTransferKind, BoundExpression,
    BoundExpressionId, BoundReferenceTarget,
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
    ) {
        self.transition_cleanup(state, operation, observe);

        let expression = operation.occurrence().expression();

        let target = self
            .checked_flow
            .and_then(|flow| flow.assignment_target(expression?));

        let assignment =
            target.and_then(|target| crate::ExecutionPlace::storage_path(self.storage, target));

        let invalidation = self
            .checked_flow
            .filter(|flow| {
                target.is_some()
                    || expression.is_some_and(|expression| {
                        flow.checked_memory_operation(expression).is_some()
                    })
            })
            .and_then(|_| self.refinements.get(&operation.occurrence()));

        self.transfer_operation(state, operation, invalidation, assignment.as_ref());
    }

    pub(in crate::analysis) fn transfer_operation(
        &self,
        state: &mut ExecutionState,
        operation: super::super::super::model::AnalysisOperationKind,
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
        assignment_place: Option<&crate::ExecutionPlace>,
    ) {
        if let super::super::super::model::AnalysisOperationKind::Call {
            invocation:
                invocation @ (bray_bound_tree::BoundExecutionSite::ScopedEnter(_)
                | bray_bound_tree::BoundExecutionSite::ScopedExit(_)),
            phase,
        } = operation
        {
            match phase {
                super::super::super::model::AnalysisCallPhase::Attempt => {
                    let entry = self.call_entry(state, invocation);

                    if let bray_bound_tree::BoundExecutionSite::ScopedExit(expression) = invocation
                    {
                        // Exit may transfer obligations from its captured input contract. The
                        // entered capability itself grants no authority beyond this boundary.
                        state.invalidate_trusted(&ExecutionCondition::ScopedCapability(expression));
                    }

                    self.invalidate(state, invocation, invalidation);

                    // The invocation owns its captured inputs after transfer. Invalidation
                    // retires caller authority and earlier computations, not this callee's entry evidence.
                    if let Some(entry) = entry {
                        state.entries.insert(invocation, entry);
                    }
                }
                super::super::super::model::AnalysisCallPhase::Completion => {
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
                    if self.values.can_repeat(expression) {
                        state.retire_evaluation(bray_bound_tree::StorageMutation::Operation(
                            expression.into(),
                        ));
                    }

                    if let Some(entry) = self.call_entry(state, expression.into()) {
                        state.entries.insert(expression.into(), entry);
                    }

                    // Split calls apply their effects at entry. Normal completion only
                    // contributes the result and completion contracts.
                    self.expression(state, expression, invalidation, assignment_place);
                }

                if self.checked_flow.is_some() {
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
                    phase: crate::analysis::model::AnalysisCallPhase::Attempt,
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
                phase: super::super::super::model::AnalysisCallPhase::Attempt,
                ..
            } | super::super::super::model::AnalysisOperationKind::Suspension {
                kind: super::super::super::model::AnalysisSuspensionKind::Await,
                ..
            }
        ) {
            return;
        }

        let Some(expression) = operation.occurrence().expression() else {
            return;
        };

        if self
            .checked_flow
            .and_then(|flow| flow.memory_operation_status(expression))
            .is_some_and(|status| status != bray_bound_tree::MemoryOperationStatus::Valid)
        {
            return;
        }

        let empty = Default::default();

        let observations = observations
            .or_else(|| {
                self.checked_flow.and_then(|flow| {
                    flow.completion(bray_bound_tree::StorageMutation::Operation(
                        operation.occurrence(),
                    ))
                })
            })
            .unwrap_or(&empty);

        self.observe_references(state, observations.references());

        if matches!(
            operation.occurrence(),
            bray_bound_tree::BoundExecutionSite::ScopedEnter(_)
                | bray_bound_tree::BoundExecutionSite::ScopedExit(_)
        ) {
            let invocation = operation.occurrence();

            if let Some(entry) = state.entries.get_mut(&invocation) {
                entry.pending_execution = false;
            }

            let result = self.invocation_value(state, invocation);

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
    }

    fn expression(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
        assignment_place: Option<&crate::ExecutionPlace>,
    ) {
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
        self.invalidate(state, expression.into(), invalidation);

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

            let mut value = self.storage_value(state, expression, access);

            if let Some(bray_bound_tree::StorageBinding::Identity(identity)) = self
                .storage
                .binding(bray_bound_tree::StorageBindingTarget::Local(binding))
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

            // Observing bindings use their committed component, independently of borrow and initialization plans.
            state.assign(BoundReferenceTarget::Local(binding.into()).into(), value);
        }
    }
}
