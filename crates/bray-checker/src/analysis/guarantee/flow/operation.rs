
use super::model::ExecutionFlowDomain;

use bray_bound_tree::{
    AnyBoundNodeId, BoundAssignmentOperator, BoundControlTransferKind, BoundExpression,
    BoundExpressionId, StorageOutcomeRows, StorageStateTarget, StorageValueBoundary,
};

use crate::CheckerRequestContext;
use crate::analysis::model::{AnalysisCallPhase, AnalysisOperation, AnalysisOperationKind};
use crate::execution_guarantees::ExecutionCondition;

use super::super::state::ExecutionState;

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn trace_assignment_source(
        &self,
        state: &ExecutionState,
        operation: AnalysisOperationKind,
        phase: &str,
    ) {
        let Some(expression) = operation.occurrence().expression() else { return; };

        if !super::deferred::frame_value_trace(expression.unit()) { return; }

        let Some(BoundExpression::Assignment(assignment)) = self.request.view().expression(expression) else { return; };

        let [_, source] = assignment.operands() else { return; };

        let place = self.storage.value_slot(*source).and_then(|slot| crate::ExecutionPlace::storage_identity(self.storage, slot));
        let slot = place.as_ref().and_then(|place| state.current.slot(place));
        let reached = slot.as_ref().map(|slot| (slot.value.deferred_values().len(), &slot.availability));
        let cached = state.expressions.get(source).map(|value| (value.deferred_values().len(), value.is_unknown()));

        eprintln!("frame assignment source {phase} {expression:?} rhs={source:?} reached={reached:?} cached={cached:?}");
    }

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
                let input = ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone()));

                // The successful move owns this transition. A current post-state
                // or another slot's snapshot cannot invent original input ownership.
                if value.without_captured_storage() == input && place.is_callable_input() {
                    state.owned_results.insert(value, bray_symbols::DependencyOwnedResult::Transfer(
                        bray_symbols::DependencyOwnedTransfer::new(input, [])
                            .with_preservation(bray_symbols::DependencyValuePreservation::all()),
                    ));
                }
            }

            state.vacate(place);
        }
    }

    pub(super) fn replay_operation(
        &self,
        state: &mut ExecutionState,
        point: &AnalysisOperation,
        observe: Option<&mut super::super::cleanup::Observer<'_>>,
        task: Option<super::TaskSelection>,
    ) -> bool {
        let operation = point.kind();
        let body = self.checked_body.expect("execution replay retains its checked body");
        let flow = body.storage_flow();

        if let AnalysisOperationKind::ScopedResult(transfer) = operation {
            self.transfer_scoped_result(state, transfer);

            return true;
        }

        if !matches!(
            operation,
            super::super::super::model::AnalysisOperationKind::Result {
                phase: super::super::super::model::AnalysisResultPhase::Begin,
                ..
            }
        ) && let bray_bound_tree::BoundExecutionSite::Node(node) = operation.occurrence()
            && !self.assume_patterns(state, body.refinements().refinements_before(node).iter())
        {
            return false;
        }

        self.trace_assignment_source(state, operation, "replay before cleanup");

        if !self.transition_cleanup(state, point, observe) {
            return false;
        }

        self.trace_assignment_source(state, operation, "replay after cleanup");

        let expression = operation.occurrence().expression();

        let captures = flow.input_captures(point.id(), None).unwrap_or_default();

        if let Some(expression) = self.single_phase_invocation(operation) {
            self.admit_inline_invocation(state, expression, captures);

            if let Some(values) = flow.attempt_values(point.id(), None) {
                self.prepare_captured_values(state, expression.into(), values);
            }
        }

        self.prepare_replay_outputs(state, point);

        if let AnalysisOperationKind::Call { invocation, phase: AnalysisCallPhase::Attempt } = operation {
            self.prepare_failure_rows(state, invocation);
        }

        let task = task.or_else(|| self.task_observation_outcome(state, operation));
        let selection = self.operation_outcome_rows(state, operation, task.as_ref());
        let rows = selection.as_ref();
        let invalidation = flow.selected_operation_invalidation(point.id(), selection.as_ref());
        let target = |kind| flow.operation_target(point.id(), kind, rows);

        let assignment = expression.filter(|_| matches!(operation,
            AnalysisOperationKind::Bound(_) | AnalysisOperationKind::AssignmentInstall(_)))
            .and_then(|expression| target(StorageStateTarget::Assignment(expression)))
            .filter(|target| target.is_exact())
            .and_then(bray_bound_tree::StorageTarget::value_observation_path)
            .and_then(|path| crate::ExecutionPlace::storage_path(self.storage, &path));

        if task.is_some() && invalidation.is_none() && flow.operation_invalidation(point.id()).is_some() {
            return false;
        }

        if selection.is_some() && self.trace_authority_site(bray_bound_tree::StorageMutation::Operation(operation.occurrence())) {
            let invocation = operation.occurrence();

            eprintln!("replay source outcome {invocation:?} selection={selection:?} checked={:?}",
                invalidation.as_ref().map(|frame| frame.observation_transition()));
        }

        let invalidation = invalidation.as_deref();

        if let Some(expression) = operation.explicit_attempt_expression()
            && let Some(target) = target(StorageStateTarget::MemoryRead(expression))
        {
            self.observe_memory_read(state, expression, Some(target));
        }

        let captures_failure_entry = matches!(operation, crate::analysis::model::AnalysisOperationKind::Call {
            phase: crate::analysis::model::AnalysisCallPhase::Attempt, ..
        }) && operation.explicit_attempt_expression().and_then(|expression| self.memory.operation(expression))
            .is_none_or(|operation| crate::memory::execution::commits_buffer_cleanup(operation.kind()));

        if let Some(expression) = operation.awaited_expression() {
            self.admit_await_inputs(state, expression);
        } else {
            self.prepare_frame_execution(state, point, None, |_, _, _| {});
        }

        if captures_failure_entry {
            let invocation = operation.occurrence();
            let inputs = self.admit_call_inputs(state, invocation);

            self.replay_storage_moves(state, point, rows);
            self.replay_admission(state, point, selection.as_ref());
            self.produce_call_attempt(state, invocation, inputs, invalidation, assignment.as_ref());
        } else {
            if operation.awaited_expression().is_some() {
                self.replay_admission(state, point, selection.as_ref());
            }

            let ordinary = task.is_none().then_some(invalidation).flatten();

            self.transfer_operation(state, operation, ordinary, assignment.as_ref(), captures);

            if let AnalysisOperationKind::Call { invocation, phase: AnalysisCallPhase::Input } = operation {
                let _ = self.capture_entry_check(state, invocation);
            }

            self.replay_storage_moves(state, point, rows);
        }

        if let (Some(task), Some(invalidation)) = (task, invalidation) {
            let mutation = bray_bound_tree::StorageMutation::TaskObservation {
                occurrence: operation.occurrence(),
                operation: point.id(),
                outcome: task.outcome,
            };

            self.invalidate_observations(state, mutation, invalidation);
        }

        let retirements = flow.operation_retirements(point.id(), StorageValueBoundary::Execution, selection.as_ref());

        state.retire_captured_storage(&retirements);

        let completion = flow.operation_completion(point.id(), selection.as_ref());
        let write_target = expression.and_then(|expression| target(StorageStateTarget::MemoryWrite(expression)));

        self.complete_operation(state, operation, completion.as_deref(), write_target);

        let retirements = flow.operation_retirements(point.id(), StorageValueBoundary::Retirement, selection.as_ref());

        state.retire_captured_storage(&retirements);

        true
    }

    fn replay_storage_moves(
        &self,
        state: &mut ExecutionState,
        operation: &AnalysisOperation,
        rows: Option<&StorageOutcomeRows>,
    ) {
        let flow = self.checked_body.expect("execution replay retains its checked storage transitions").storage_flow();

        for plan in operation.kind().storage_plans(self.storage) {
            if self.storage.effective_purpose(plan) == bray_bound_tree::StorageAccessPurpose::Move
                && let Some(target) = flow.operation_target(operation.id(), StorageStateTarget::Access(plan), rows)
            {
                self.observe_storage_move(state, plan, Some(target));
            }
        }
    }

    pub(in crate::analysis) fn observe_storage_move(
        &self,
        state: &mut ExecutionState,
        plan: bray_bound_tree::StorageAccessPlan,
        reached: Option<&bray_bound_tree::StorageTarget>,
    ) {
        let Some(path) = reached.filter(|target| target.is_exact()).and_then(bray_bound_tree::StorageTarget::value_observation_path) else {
            return;
        };

        let ty = self.storage.access(plan.access()).expect("checked move retains its source access").reached_type();
        let observed = self.value(state, plan.expression());

        self.observe_move(state, &path, ty, &observed);
    }

    pub(in crate::analysis) fn single_phase_invocation(&self, operation: crate::analysis::model::AnalysisOperationKind) -> Option<BoundExpressionId> {
        let crate::analysis::model::AnalysisOperationKind::Bound(AnyBoundNodeId::Expression(expression)) = operation else { return None; };

        (self.dependencies.calls().contains_key(&expression.into()) || self.memory.operation(expression).is_some()).then_some(expression)
    }

    pub(in crate::analysis) fn admit_inline_invocation(
        &self, state: &mut ExecutionState, expression: BoundExpressionId,
        captures: &[bray_bound_tree::StorageValueCapture],
    ) {
        if self.values.can_repeat(expression.into()) {
            state.retire_evaluation(bray_bound_tree::StorageMutation::Operation(expression.into()));
            self.vacate_expression_slot(state, expression);
        }

        if let Some(entry) = self.call_entry(state, expression.into(), captures) {
            state.entries.insert(expression.into(), entry.into());
        }
    }

    pub(in crate::analysis) fn transfer_operation(
        &self,
        state: &mut ExecutionState,
        operation: super::super::super::model::AnalysisOperationKind,
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
        assignment_place: Option<&crate::ExecutionPlace>,
        captures: &[bray_bound_tree::StorageValueCapture],
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

                    self.vacate_expression_slot(state, expression);
                    state.expressions.remove(&expression);
                    state.pending_results.remove(&expression);

                    if self.request.view().expression(expression).is_some_and(BoundExpression::accumulates_generator_result)
                        && self.semantics.types().expression(expression).is_some_and(|ty| matches!(
                            self.request.semantic_values().type_data(ty.ty()).as_ref(), bray_symbols::TypeData::Generator(_)
                        ))
                    {
                        self.produce_region_result(state, expression, ExecutionCondition::unknown());
                    }
                }
                super::super::super::model::AnalysisResultPhase::Production => {
                    let value = self.produce_value(state, expression);

                    state.pending_results.insert(expression, ExecutionCondition::scalar_observation(
                        ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression)), value.computed_value().clone(),
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
                    if matches!(invocation, Site::Iterate(_) | Site::Next(_) | Site::ScopedEnter(_) | Site::ScopedExit(_))
                        && let Some(result) = self.invocation_result_place(invocation)
                    {
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

                    if let Some(entry) = self.call_entry(state, invocation, captures) {
                        state.entries.insert(invocation, entry.into());
                    }

                }
                AnalysisCallPhase::Attempt => {
                    let admitted = self.admit_call_inputs(state, invocation);

                    self.produce_call_attempt(state, invocation, admitted, invalidation, assignment_place);
                }
                AnalysisCallPhase::Completion => {}
            }

            return;
        }

        if let AnalysisOperationKind::AssignmentInstall(expression) = operation {
            let result = self.private_operator_result(expression).expect("assignment retains the selected operator's result");
            let value = result.value_in(&state.current).unwrap_or_else(ExecutionCondition::unknown);

            if let Some(invalidation) = invalidation {
                self.invalidate_observations(state, bray_bound_tree::StorageMutation::AssignmentInstall(expression), invalidation);
            }

            if let Some(place) = assignment_place {
                let source = self.storage.invocation_result(expression.into()).expect("selected operator has its own storage");

                let destination = operation.storage_plans(self.storage)
                    .find(|plan| plan.purpose() == bray_bound_tree::StorageAccessPurpose::Assignment)
                    .expect("assignment has its checked write access");

                let ty = |access| self.storage.access(access).expect("assignment storage has a checked type").reached_type();

                state.assign(place.clone(), value.coerce(self.request.semantic_values(), ty(source), ty(destination.access())), None);
            }

            state.vacate(result);

            let unit = self.produce_value(state, expression);

            self.publish_expression(state, expression, unit);

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

        let awaited = operation.awaited_expression();

        match operation.node() {
            AnyBoundNodeId::Expression(expression) => {
                if !matches!(
                    operation,
                    super::super::super::model::AnalysisOperationKind::Call {
                        phase: super::super::super::model::AnalysisCallPhase::Completion,
                        ..
                    }
                ) {
                    if awaited.is_none() && self.single_phase_invocation(operation).is_none() {
                        self.admit_inline_invocation(state, expression, captures);
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
                    && let Some(entry) = state.admitted_call_mut(expression.into())
                {
                    entry.pending_execution = true;
                }
            }
            AnyBoundNodeId::Pattern(pattern) => self.pattern(state, pattern),
            _ => {}
        }

        if let Some(expression) = awaited {
            self.suspend_awaited_execution(state, expression);
        }
    }

    pub(in crate::analysis) fn admit_await_inputs(&self, state: &mut ExecutionState, expression: BoundExpressionId) {
        state.suspend_admitted_witnesses(expression.into());
        self.consume_value_slots(state, expression.into(), bray_bound_tree::StorageValueConsumptionPhase::Admission);
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
                        crate::analysis::model::AnalysisRefinement::InvocationFailure(failed, _)
                    ) if failed == occurrence)
                {
                    state.entries.remove(&occurrence);
                }
            }
        }

        if let Some(crate::analysis::model::AnalysisRefinement::InvocationFailure(invocation, _)) =
            edge.refinement()
        {
            let deferred = invocation.expression().and_then(|expression| {
                let bray_bound_tree::BoundExpression::Await(_) = self.request.view().expression(expression)? else {
                    return None;
                };

                state.admitted_receiver(expression.into()).deferred_value().cloned()
            });

            self.retire_failed_call(state, invocation);

            if let Some(frame) = deferred { state.replace_deferred_value(&frame, None); }

            if let bray_bound_tree::BoundExecutionSite::Node(AnyBoundNodeId::Expression(
                expression,
            )) = invocation
            {
                state.pending_results.remove(&expression);
            }
        }

        if let Some(
            crate::analysis::model::AnalysisRefinement::InvocationEntry { invocation, admitted: false }
            | crate::analysis::model::AnalysisRefinement::InvocationBackingFailure(invocation),
        ) = edge.refinement()
        {
            state.entries.remove(&invocation);
        }
    }

    pub(in crate::analysis) fn admit_call_inputs(
        &self, state: &mut ExecutionState, invocation: bray_bound_tree::BoundExecutionSite,
    ) -> (Option<ExecutionCondition>, bool) {
        let observed = match invocation {
            bray_bound_tree::BoundExecutionSite::Node(AnyBoundNodeId::Expression(expression)) =>
                self.observed_call_result(state, expression),
            _ => None,
        };

        let deferred = state.admitted_call_mut(invocation).is_some_and(|entry| {
            // This invocation's own effects do not revoke its captured entry.
            std::mem::replace(&mut entry.pending_execution, false)
        });

        // Formation moves the owner into an inactive frame. Its witness remains live until execution can start.
        if !deferred {
            state.suspend_admitted_witnesses(invocation);
        }

        self.consume_value_slots(state, invocation, bray_bound_tree::StorageValueConsumptionPhase::Admission);

        (observed, deferred)
    }

    pub(in crate::analysis) fn produce_call_attempt(
        &self, state: &mut ExecutionState, invocation: bray_bound_tree::BoundExecutionSite,
        admitted: (Option<ExecutionCondition>, bool),
        invalidation: Option<&bray_bound_tree::StorageInvalidation>,
        assignment_place: Option<&crate::ExecutionPlace>,
    ) {
        let (observed, deferred) = admitted;

        if let bray_bound_tree::BoundExecutionSite::Node(AnyBoundNodeId::Expression(expression)) = invocation {
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

        if let Some(entry) = state.admitted_call_mut(invocation) {
            entry.pending_execution = deferred;
        }
    }

    pub(in crate::analysis) fn complete_operation(
        &self,
        state: &mut ExecutionState,
        operation: super::super::super::model::AnalysisOperationKind,
        observations: Option<&bray_bound_tree::StorageCompletion>,
        write_target: Option<&bray_bound_tree::StorageTarget>,
    ) {
        if operation.value_consumption_phase() != Some(bray_bound_tree::StorageValueConsumptionPhase::Completion)
            || matches!(operation, AnalysisOperationKind::AssignmentInstall(_))
        {
            return;
        }

        let empty = bray_bound_tree::StorageCompletion::default();

        self.complete_value(state, operation, observations.unwrap_or(&empty), write_target);

        if let Some(completion) = observations {
            self.replay_captured_publications(state, operation.occurrence(), completion);
        }

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
                            .map(|ty| self.request.semantic_values().unborrowed_type(ty))
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
                            match actual.kind() {
                                crate::ExecutionConditionKind::Observation { contents, kind: crate::execution_guarantees::ExecutionObservationKind::Owned | crate::execution_guarantees::ExecutionObservationKind::Scalar, .. } => actual = contents.as_ref().clone(),
                                crate::ExecutionConditionKind::Literal(value) => {
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
        observations: &bray_bound_tree::StorageCompletion,
        write_target: Option<&bray_bound_tree::StorageTarget>,
    ) {
        let input = matches!(
            operation.occurrence(),
            bray_bound_tree::BoundExecutionSite::CallInput { .. }
        );

        let materialized_inline = input && self.complete_input(state, operation.occurrence());

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
                    if let Some(private) = self.private_operator_result(expression) {
                        state.assign(private, value, None);
                    } else {
                        self.publish_expression(state, expression, value);
                    }
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

        // Mutation refreshes only current contents. The completion retains entry
        // layers and returned origins for consumers that can interpret them.
        self.refresh_storage_observations(
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

        if let Some(expression) = completed_expression {
            self.publish_slice_extent(state, expression);
        }

        if input {
            if materialized_inline {
                return;
            }

            let invocation = operation.occurrence();

            let place = self.input_place(invocation)
                .expect("completed input retains its place");

            let result = self.invocation_value(state, invocation);

            let result = self.complete_trusted_call(state, (invocation, invocation), None, self.invocation_result_place(invocation).as_ref(), result, observations);

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

        if let Some(private) = self.private_operator_result(expression) {
            self.complete_call(state, expression, observations);

            let value = state.expressions.remove(&expression).unwrap_or_else(ExecutionCondition::unknown);

            state.assign(private, value, None);

            return;
        }

        if !self.complete_deferred(state, expression, observations) {
            self.complete_call(state, expression, observations);

            let value = self.capture_deferred(state, expression.into(), self.value(state, expression));

            self.publish_expression(state, expression, value);
        }

        self.remember_deferred_value(state, expression);
        self.activate_task(state, expression);

        if let Some(target) = write_target {
            self.complete_memory_write(state, expression, target);
        }

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
        let pending = state.pending_results.remove(&expression);

        let value = self.storage.value_slot(expression)
            .and_then(|identity| crate::ExecutionPlace::storage_identity(self.storage, identity))
            .and_then(|place| state.current.live_value(&place))
            .or(pending)
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
            .or_else(|| (!assignment || !completes).then(|| machine_value.clone()).flatten())
            .unwrap_or_else(|| {
                if assignment && !completes {
                    ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression))
                } else {
                    self.produce_value(state, expression)
                }
            });

        let value = self.storage.node_plans(expression.into())
            .filter(|plan| self.storage.access_plan_role(*plan) == bray_bound_tree::StorageAccessPlanRole::SourceMaterialization)
            .filter(|plan| plan.expression() == expression)
            .filter(|plan| self.storage.effective_purpose(*plan) == bray_bound_tree::StorageAccessPurpose::Copy)
            .fold(value, |value, plan| {
                self.copied_access_value(ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression)), value, plan)
            });

        let frame = self.started_future(state, expression).and_then(|source| source.deferred_value().cloned());

        let value = match frame {
            Some(frame) => value.with_deferred_value(frame),
            None => value,
        };

        state.expressions.insert(expression, value);

        self.trace_assignment_source(state, AnalysisOperationKind::Bound(expression.into()), "before invalidation");
        self.invalidate(state, expression.into(), invalidation);
        self.trace_assignment_source(state, AnalysisOperationKind::Bound(expression.into()), "after invalidation");

        match self.request.view().expression(expression) {
            Some(BoundExpression::Assignment(assignment)) => {
                if let [target, source] = assignment.operands()
                    && let Some(place) = assignment_place
                {
                    assert_eq!(assignment.operator(), BoundAssignmentOperator::Assign,
                        "a compound assignment installs the completed operator result");

                    let value = self.operand_value(state, *source).coerce(
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

                    if super::deferred::frame_value_trace(expression.unit()) {
                        let text = format!("{:?}", value.kind());

                        eprintln!("frame assignment {expression:?} {place:?}: frames={} value={}", value.deferred_values().len(), text.chars().take(290).collect::<String>());
                    }

                    state.assign(place.clone(), value, None);
                }
            }
            Some(BoundExpression::ControlTransfer(transfer))
                if transfer.kind() == BoundControlTransferKind::Return =>
            {
                let value = transfer
                    .operand()
                    .map(|operand| {
                        self.operand_value(state, operand)
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
                    .unwrap_or_else(ExecutionCondition::unknown);

                self.publish_return(state, value);
            }
            Some(BoundExpression::ControlTransfer(transfer))
                if matches!(
                    transfer.kind(),
                    BoundControlTransferKind::Yield | BoundControlTransferKind::Break
                ) =>
            {
                if let Some(generator) = self.graph.generator_yield(expression) {
                    if let Some(owner) = self.storage.value_slot(generator)
                        && let Some(place) = crate::ExecutionPlace::storage_identity(self.storage, owner)
                        && state.current.availability(&place) == super::super::current::ValueAvailability::Absent
                    {
                        // An initialized prefix owns cleanup before the whole array exists.
                        state.current.write(place, super::super::current::CurrentValue {
                            value: ExecutionCondition::unknown(),
                            availability: super::super::current::ValueAvailability::Unknown,
                        }, None);
                    }
                } else if let Some(target) = self.yield_targets.get(&expression) {
                    let destination = self.storage.scoped_result_payload(expression);

                    let destination_type = destination.and_then(|access| self.storage.access(access))
                        .map_or_else(|| self.semantics.types().expression(*target)
                            .expect("transfer target retains its checked type").ty(), bray_bound_tree::StorageAccess::reached_type);

                    let value = transfer
                        .operand()
                        .map(|operand| {
                            self.operand_value(state, operand).coerce(
                                self.request.semantic_values(),
                                self.semantics
                                    .types()
                                    .expression(operand)
                                    .expect("transfer operand retains its checked type")
                                    .ty(),
                                destination_type,
                            )
                        })
                        .unwrap_or_else(ExecutionCondition::unknown);

                    let value = if let Some([projection @ bray_bound_tree::StorageProjection::ActiveUnionPayloadField { .. }]) =
                        destination.and_then(|access| self.storage.resolved_projections(access))
                    {
                        Self::tagged_region_payload(*target, *projection, value)
                    } else {
                        value
                    };

                    self.produce_region_result(state, *target, value);
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
                value = self.copied_access_value(ExecutionCondition::new(crate::ExecutionConditionKind::Input(place.clone())), value, plan);
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
            if super::deferred::frame_value_trace(expression.unit()) {
                let text = format!("{:?}", value.kind());

                eprintln!("frame pattern {pattern:?} {place:?} from {expression:?}: frames={} value={}", value.deferred_values().len(), text.chars().take(290).collect::<String>());
            }

            state.assign(place, value, None);
        }
    }
}
