use crate::execution_guarantees::ExecutionSubstitution;
use std::collections::BTreeMap;

use super::model::ExecutionFlowDomain;

use bray_bound_tree::{
    AnyBoundNodeId, BoundAssignmentOperator, BoundControlTransferKind, BoundExpression,
    BoundExpressionId,
};

use crate::CheckerRequestContext;
use crate::execution_guarantees::ExecutionCondition;

use super::super::state::ExecutionState;

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn observe_move(
        &self,
        state: &mut ExecutionState,
        path: &bray_bound_tree::StoragePath,
        ty: bray_symbols::TypeId,
        observed: &ExecutionCondition,
    ) {
        if let Some(place) = crate::ExecutionPlace::storage_path(self.storage, path) {
            if !matches!(self.request.semantic_values().type_data(ty).as_ref(), bray_symbols::TypeData::Borrow { .. }) {
                // The checked transition can follow semantic consumption. Its
                // evaluated source still owns the value that actually moved.
                let value = observed.clone().into_referent();

                // The successful move owns this transition. A current post-state
                // or another slot's snapshot cannot invent original input ownership.
                if value == ExecutionCondition::Input(place.clone()) && place.is_callable_input() {
                    state.owned_results.entry(value.clone()).or_insert_with(|| bray_symbols::DependencyOwnedResult::Transfer(
                        bray_symbols::DependencyOwnedTransfer::new(value, [])
                            .with_preservation(bray_symbols::DependencyValuePreservation::all()),
                    ));
                }
            }

            state.vacate(place);
        }
    }

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
        self.complete_operation(state, operation, None);

        true
    }

    pub(in crate::analysis) fn transfer_operation(
        &self,
        state: &mut ExecutionState,
        operation: super::super::super::model::AnalysisOperationKind,
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
        assignment_place: Option<&crate::ExecutionPlace>,
    ) {
        if matches!(operation,
            crate::analysis::model::AnalysisOperationKind::Call {
                phase: crate::analysis::model::AnalysisCallPhase::Attempt, ..
            } | crate::analysis::model::AnalysisOperationKind::Bound(_))
            && let Some(expression) = operation.occurrence().expression()
            && let Some(target) = self.checked_body
                .and_then(|body| body.storage_flow().memory_read_target(expression))
        {
            self.observe_memory_read(state, expression, Some(target));
        }

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

                    self.vacate_expression_slot(state, expression);
                    state.expressions.remove(&expression);
                    state.pending_results.remove(&expression);
                }
                super::super::super::model::AnalysisResultPhase::Production => {
                    let value = self.produce_value(state, expression);

                    state.pending_results.insert(expression, ExecutionCondition::scalar_observation(
                        ExecutionCondition::Expression(expression), value.computed_value().clone(),
                    ));
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
                    if matches!(invocation, Site::Iterate(_) | Site::Next(_) | Site::ScopedEnter(_)) {
                        let result = self
                            .input_place(invocation)
                            .expect("implicit invocation has its owned result slot");

                        state.vacate(result);
                    }

                    if let Site::CallInput { .. } = invocation {
                        state.vacate(self.input_place(invocation)
                            .expect("staged input retains its actual slot"));
                    }

                    if let Site::Node(AnyBoundNodeId::Expression(expression)) = invocation {
                        self.vacate_expression_slot(state, expression);
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

                    // The admitted entry now owns its evaluated inputs. Their
                    // source slots cannot remain independent outside owners.
                    let private_inputs = state.entry_witness_inputs(invocation);

                    state.suspend_owned_witnesses(&private_inputs);
                    self.consume_value_slots(state, invocation, bray_bound_tree::StorageValueConsumptionPhase::Admission);

                    if let Site::Node(AnyBoundNodeId::Expression(expression)) = invocation {
                        if let Some(observed) = observed {
                            state.pending_results.insert(expression, observed);
                        }

                        self.expression(state, expression, invalidation, assignment_place, false);

                        if let Some(value) = state.expressions.remove(&expression) {
                            state.pending_results.insert(expression, value);
                        }

                        state.trust_boundaries.remove(&expression);
                    } else {
                        self.invalidate(state, invocation, invalidation);
                    }

                    if let Some(entry) = state.entries.get_mut(&invocation) {
                        entry.pending_execution = deferred;
                    }
                }
                AnalysisCallPhase::Completion => {}
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

        let awaited = operation.occurrence().expression().filter(|_| {
            matches!(
                operation,
                crate::analysis::model::AnalysisOperationKind::Suspension {
                    kind: crate::analysis::model::AnalysisSuspensionKind::Await,
                    ..
                }
            )
        });

        if let Some(expression) = awaited {
            self.set_awaited_execution(state, expression, false);
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

                        self.vacate_expression_slot(state, expression);
                    }

                    if let Some(entry) = self.call_entry(state, expression.into()) {
                        state.entries.insert(expression.into(), entry);
                    }

                    if awaited.is_some() {
                        let private = state.entry_witness_inputs(expression.into());

                        state.suspend_owned_witnesses(&private);
                    }

                    if let Some(observed) = self.observed_call_result(state, expression) {
                        state.pending_results.insert(expression, observed);
                    }

                    self.expression(state, expression, invalidation, assignment_place,
                        matches!(operation, super::super::super::model::AnalysisOperationKind::Bound(_)));
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

        if let Some(expression) = awaited {
            self.set_awaited_execution(state, expression, true);

            // The suspended run owns the evaluated frame. Its original output
            // slot cannot remain an independent cleanup owner on any outcome.
            self.consume_value_slots(state, expression.into(), bray_bound_tree::StorageValueConsumptionPhase::Admission);
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
                    && !matches!(edge.refinement(), Some(
                        crate::analysis::model::AnalysisRefinement::InvocationFailure(failed)
                    ) if failed == occurrence)
                {
                    state.entries.remove(&occurrence);
                }
            }
        }

        if let Some(crate::analysis::model::AnalysisRefinement::InvocationFailure(invocation)) =
            edge.refinement()
        {
            let deferred = invocation.expression().and_then(|expression| {
                let bray_bound_tree::BoundExpression::Await(awaited) = self.request.view().expression(expression)? else {
                    return None;
                };

                Some(self.value(state, awaited.operand()).owner_identity().clone())
            });

            self.retire_failed_call(state, invocation);

            if let Some(owner) = deferred { state.deferred_invocations.remove(&owner); }

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
        if operation.value_consumption_phase() != Some(bray_bound_tree::StorageValueConsumptionPhase::Completion) {
            return;
        }

        self.complete_value(state, operation, observations);

        // The destination has published the actual object before its evaluated
        // source handle ends. Frozen operand snapshots remain readable as data.
        self.consume_value_slots(state, operation.occurrence(), bray_bound_tree::StorageValueConsumptionPhase::Completion);
    }

    pub(super) fn consume_value_slots(
        &self,
        state: &mut ExecutionState,
        occurrence: bray_bound_tree::BoundExecutionSite,
        phase: bray_bound_tree::StorageValueConsumptionPhase,
    ) {
        for path in self.storage.value_retirements(occurrence, phase) {
            let selected = crate::ExecutionPlace::select_storage_value(
                self.storage, path, &state.current,
                |place, projection| match projection {
                    bray_bound_tree::StorageProjection::ElementFromEnd(offset) => {
                        let actual = place.value_in(&state.current);

                        let ty = self.observed_types.get(place).copied().flatten()
                            .or_else(|| if place.projections.is_empty() { self.storage.identity_type(path.root) } else { None })
                            .or_else(|| actual.as_ref().and_then(|value| self.observation_type(value)))
                            .or_else(|| actual.as_ref().and_then(|value| self.observation_type(value.owner_identity())))?;

                        let length = crate::constant::known_array_length(self.request.semantic_values(), ty)?;
                        let index = length.checked_sub(usize::try_from(offset.raw()).ok()?.checked_add(1)?)?;

                        Some(bray_symbols::SymbolOrdinal::new(u32::try_from(index).ok()?))
                    }
                    bray_bound_tree::StorageProjection::Element(expression) => {
                        let mut actual = self.value(state, expression);

                        for _ in 0..ExecutionCondition::WORK_LIMIT {
                            match actual {
                                ExecutionCondition::Observation { contents, kind: crate::execution_guarantees::ExecutionObservationKind::Owned | crate::execution_guarantees::ExecutionObservationKind::Scalar, .. } => actual = contents.as_ref().clone(),
                                ExecutionCondition::Literal(value) => {
                                    let bray_symbols::ConstantValueKind::Integer(value) = value.kind() else { return None; };

                                    let index = crate::constant::integer_to_usize(value)?;

                                    return Some(bray_symbols::SymbolOrdinal::new(u32::try_from(index).ok()?));
                                }
                                _ => return None,
                            }
                        }

                        None
                    }
                    _ => None,
                },
            ).expect("checked evaluated-value consumption retains its produced root identity");

            match selected {
                crate::execution_guarantees::ExecutionStorageSelection::Inline(place) => state.current.retire(&place),
                crate::execution_guarantees::ExecutionStorageSelection::Reached => {}
                crate::execution_guarantees::ExecutionStorageSelection::UnresolvedInline(place) => {
                    // No particular child is established as live after an unnamed
                    // inline selection. Keep its terminal tree, but stop using
                    // the containing evaluated view as a complete owner.
                    state.current.retire(&place);
                }
            }
        }
    }

    fn complete_value(
        &self,
        state: &mut ExecutionState,
        operation: super::super::super::model::AnalysisOperationKind,
        observations: Option<&bray_bound_tree::StorageCompletion>,
    ) {
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

        let completed_expression = match operation {
            super::super::super::model::AnalysisOperationKind::Call {
                invocation: bray_bound_tree::BoundExecutionSite::Node(AnyBoundNodeId::Expression(expression)),
                phase: super::super::super::model::AnalysisCallPhase::Completion,
            } => {
                if let Some(value) = state.pending_results.remove(&expression) {
                    self.publish_expression(state, expression, value);
                }

                Some(expression)
            }
            super::super::super::model::AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression))
            | super::super::super::model::AnalysisOperationKind::Result {
                expression,
                phase: super::super::super::model::AnalysisResultPhase::Completion,
            } => Some(expression),
            _ => None,
        };

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

        // Mutation refreshes only current contents. The completion retains entry
        // layers and returned origins for consumers that can interpret them.
        self.observe_references(
            state,
            observations.references(),
        );

        if let Some(expression) = completed_expression
            && let Some(identity) = self.storage.temporary(expression)
            && observations.references().keys().any(|path| path.root == identity)
            && let Some(place) = crate::ExecutionPlace::storage_identity(self.storage, identity)
            && state.current.availability(&place) != super::super::current::ValueAvailability::Absent
            && let Some(value) = place.value_in(&state.current)
        {
            // Capture this completion's actual reference update once. A moved
            // result keeps its evaluated snapshot after its storage becomes absent.
            state.expressions.insert(expression, value);
        }

        if input {
            let invocation = operation.occurrence();

            let place = self.input_place(invocation)
                .expect("completed input retains its place");

            let result = self.invocation_value(state, invocation);

            let result = self.storage.occurrence_plans(invocation)
                .filter(|plan| self.storage.effective_purpose(*plan) == bray_bound_tree::StorageAccessPurpose::Copy)
                .fold(result, |result, plan| {
                    self.copied_access_value(ExecutionCondition::post_state(
                        bray_bound_tree::StorageMutation::Operation(invocation), ExecutionCondition::Input(place.clone()),
                    ), result, plan)
                });

            let (result, _) = self.complete_trusted_call(state, invocation, result, observations);

            state.assign(place, result, None);

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
            self.complete_implicit_invocation(state, operation.occurrence(), observations);

            return;
        }

        if !self.complete_deferred(state, expression, observations) {
            self.complete_call(state, expression, observations);
            self.capture_deferred(state, expression.into(), &self.value(state, expression));
        }

        self.remember_deferred_value(state, expression);
        self.activate_task(state, expression);
        self.complete_memory_write(state, expression);
        self.preserve_integer_value(state, expression);

        let value = self.value(state, expression);

        self.publish_expression(state, expression, value);
    }

    fn vacate_expression_slot(&self, state: &mut ExecutionState, expression: BoundExpressionId) {
        if let Some(identity) = self.storage.value_slot(expression) {
            let place = crate::ExecutionPlace::storage_identity(self.storage, identity)
                .expect("planned expression value slot has an execution place");

            state.vacate(place);
        }
    }

    /// Current storage and the immutable evaluated snapshot have distinct
    /// lifetimes while sharing the same actually produced symbolic object.
    pub(in crate::analysis::guarantee) fn publish_expression(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        value: ExecutionCondition,
    ) {
        if let Some(identity) = self.storage.value_slot(expression) {
            let place = crate::ExecutionPlace::storage_identity(self.storage, identity)
                .expect("planned expression value slot has an execution place");

            state.assign(place, value.clone(), None);
        }

        self.assume_produced_variant(state, expression, &value);
        state.expressions.insert(expression, value);
    }

    fn publish_result(&self, state: &mut ExecutionState, expression: BoundExpressionId) {
        let value = state
            .pending_results
            .remove(&expression)
            .or_else(|| self.storage.value_slot(expression)
                .and_then(|identity| crate::ExecutionPlace::storage_identity(self.storage, identity))
                .and_then(|place| place.value_in(&state.current)))
            .unwrap_or_else(|| self.value(state, expression));

        self.publish_expression(state, expression, value);
    }

    fn expression(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
        assignment_place: Option<&crate::ExecutionPlace>,
        completes: bool,
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
            .unwrap_or_else(|| self.produce_value(state, expression));

        let value = self.storage.node_plans(expression.into())
            .filter(|plan| self.storage.access_plan_role(*plan) == bray_bound_tree::StorageAccessPlanRole::SourceMaterialization)
            .filter(|plan| plan.expression() == expression)
            .filter(|plan| self.storage.effective_purpose(*plan) == bray_bound_tree::StorageAccessPurpose::Copy)
            .fold(value, |value, plan| {
                self.copied_access_value(ExecutionCondition::Expression(expression), value, plan)
            });

        let transfer = self
            .started_future(state, expression)
            .filter(|_| value != ExecutionCondition::Unknown)
            .map(|source| (source, value.clone()));

        state.expressions.insert(expression, value);

        if let Some((source, target)) = transfer {
            // Starting moves the frame before effects run. Its private captures
            // belong to the Task during invalidation, not to the consumed Future.
            let mut values = BTreeMap::new();
            let mut identities = BTreeMap::new();

            super::merge::result_replacements(&source, &target,
                &Default::default(), &Default::default(), &mut values, &mut identities);

            let substitution = ExecutionSubstitution::new(&values, &identities);

            state.join_values(&substitution);

            if let Some(entry) = state.entries.get_mut(&expression.into())
                && let Some(renamed) = entry.joined_values(&substitution)
            {
                *entry = renamed;
            }
        }

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
                            ExecutionCondition::Unknown => ExecutionCondition::post_state(
                                bray_bound_tree::StorageMutation::Operation(expression.into()),
                                ExecutionCondition::Input(assignment_place.unwrap_or(&place).clone()),
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
                        state.assign(owner.clone(), value.clone(), None);
                    }

                    state.assign(place, value, None);
                }
            }
            Some(BoundExpression::ControlTransfer(transfer))
                if transfer.kind() == BoundControlTransferKind::Return =>
            {
                let value = transfer
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

                self.publish_return(state, value);
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
                        .map(|operand| {
                            self.value(state, operand).coerce(
                                self.request.semantic_values(),
                                self.semantics
                                    .types()
                                    .expression(operand)
                                    .expect("transfer operand retains its checked type")
                                    .ty(),
                                self.semantics
                                    .types()
                                    .expression(*target)
                                    .expect("transfer target retains its checked type")
                                    .ty(),
                            )
                        })
                        .unwrap_or(ExecutionCondition::Unknown);

                    // Yield has produced the private destination before its source
                    // is consumed and before enclosing scope cleanup can run.
                    self.publish_expression(state, *target, value);
                }
            }
            _ => {}
        }

        if completes {
            let value = self.value(state, expression);

            self.publish_expression(state, expression, value);
        }
    }

    fn pattern(&self, state: &mut ExecutionState, pattern: bray_bound_tree::BoundPatternId) {
        for (binding, expression, access) in
            crate::storage::pattern_binding_sources(self.request.view(), self.storage, pattern)
        {
            let mut value = self.storage_value(state, expression, access);

            let place = crate::ExecutionPlace::binding(self.storage, binding)
                .expect("checked pattern binding has an execution place");

            for plan in self.storage.node_plans(pattern.into()).filter(|plan| plan.access() == access) {
                value = self.copied_access_value(ExecutionCondition::Input(place.clone()), value, plan);
            }

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

            // Named and discarded owners retain the same committed value and cleanup evidence.
            state.assign(place, value, None);
        }
    }
}
