use std::sync::Arc;

use bray_bound_tree::{BoundExecutionSite, BoundExpressionId, StorageDeferredProducer};

use super::super::ExecutionFlowDomain;
use crate::analysis::guarantee::{deferred, state::ExecutionState};
use crate::execution_guarantees::{
    ExecutionDeferredInvocation, ExecutionFailureRow, ExecutionOutput,
};
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionPlace};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis) fn deferred_behavior(
        &self,
        source: &StorageDeferredProducer,
        candidate: &ExecutionDeferredInvocation,
        execution: BoundExecutionSite,
    ) -> Option<bray_bound_tree::BodyBehaviorCall> {
        use bray_bound_tree::{
            BodyBehaviorCall, BodyBehaviorPhase, BoundCallableTarget, SemanticSelection,
        };

        let (target, anonymous, origin) = match source.local() {
            Some(site) => {
                let expression = site.expression()?;

                let SemanticSelection::Call(call) =
                    self.semantics.selections().expression(expression)?
                else {
                    return None;
                };

                let anonymous = call.resolution().anonymous_body().map(|(unit, _)| unit);

                (call.target(), anonymous, expression)
            }
            None => {
                let callee = self
                    .dependencies
                    .frame_callees()
                    .get(candidate.callee.as_deref()?)?;

                (
                    BoundCallableTarget::Declaration(callee.instance),
                    None,
                    execution.expression()?,
                )
            }
        };

        let mut behavior = BodyBehaviorCall::new(target, BodyBehaviorPhase::DeferredExecution);

        if let Some(unit) = anonymous {
            behavior = behavior.with_anonymous_unit(unit.clone());
        }

        if let Some(expression) = self.request.view().expression(origin) {
            behavior = behavior.with_source(expression.origin().source_anchor());
        }

        Some(behavior)
    }

    pub(in crate::analysis) fn select_frame_failures(
        &self,
        entry: &crate::ExecutionCallEvidence,
        rows: Option<&[bray_symbols::CapturedFailureOutcome]>,
    ) -> Option<Arc<[ExecutionFailureRow]>> {
        entry.select_failure_rows(self.request.semantic_values(), &self.proofs, rows)
    }

    pub(in crate::analysis::guarantee) fn awaited_entry(
        &self,
        state: &ExecutionState,
        invocation: BoundExecutionSite,
    ) -> Option<crate::ExecutionCallEvidence> {
        let expression = invocation.expression()?;

        if invocation != expression.into() {
            return None;
        }

        let Some(bray_bound_tree::BoundExpression::Await(awaited)) =
            self.request.view().expression(expression)
        else {
            return None;
        };

        let value = self.operand_value(state, awaited.operand());

        Some(crate::ExecutionCallEvidence {
            outputs: Default::default(),
            owned_inputs: [bray_symbols::SymbolOrdinal::new(0)].into(),
            trusted_assumptions: state.call_trusted_assumptions([value.clone()]),
            assumptions: state.assumptions.clone(),
            arguments: [(
                ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)),
                value,
            )]
            .into(),
            trusted_boundary: !state.trust_boundaries.is_empty(),
            ..Default::default()
        })
    }

    pub(in crate::analysis) fn capture_deferred_outputs<E>(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        completed: impl FnOnce() -> Result<
            std::collections::BTreeSet<bray_symbols::DependencyRequirement>,
            E,
        >,
        mut report: impl FnMut(
            Option<(BoundExecutionSite, &bray_bound_tree::BoundCallReport)>,
        ) -> Result<bray_bound_tree::ReportDependencies, E>,
    ) -> Result<(), E> {
        let value = self.invocation_value(state, expression.into());

        let Some(frame) = value.deferred_value().cloned() else {
            return Ok(());
        };

        let source = BoundExecutionSite::from(expression);
        let key = source.into();

        let formed = frame
            .producers
            .get(&key)
            .filter(|producer| producer.completed.is_none());

        let opaque = frame
            .unknown
            .as_ref()
            .is_some_and(|producer| producer.reports.is_empty());

        if formed.is_none() && !opaque {
            return Ok(());
        }

        let mut deferred = frame.as_ref().clone();

        if let Some(producer) = deferred.producers.get_mut(&key)
            && formed.is_some()
        {
            producer.completed = Some(Arc::new(completed()?));

            use crate::execution_guarantees::FrameReportOwner;
            use bray_bound_tree::CallReportOutput;

            let reports = self.dependencies.calls()[&source]
                .deferred_reports()
                .expect("formed frame retains its checked execution phase");

            let entry = Arc::make_mut(&mut producer.entry);

            entry.failure_rows = entry.select_failure_rows(
                self.request.semantic_values(),
                &self.proofs,
                reports.failures(),
            );

            let candidates = reports
                .completed()
                .map(|(row, report)| (CallReportOutput::DeferredCompleted, Some(row), report))
                .chain(reports.abnormal().map(|(row, kind, report)| {
                    (CallReportOutput::DeferredAbnormal(kind), Some(row), report)
                }));

            for (output, row, contract) in candidates {
                producer
                    .reports
                    .entry(FrameReportOwner::Boundary(output))
                    .or_default()
                    .insert(row, Arc::new(report(Some((source, contract)))?));
            }

            for (row, destination, contract) in reports.payloads() {
                producer
                    .reports
                    .entry(FrameReportOwner::Payload(destination.clone()))
                    .or_default()
                    .insert(Some(row), Arc::new(report(Some((source, contract)))?));
            }
        }

        if opaque {
            let producer = deferred
                .unknown
                .as_mut()
                .expect("opaque frame retains its alternative");

            for output in [
                bray_bound_tree::CallReportOutput::DeferredCompleted,
                bray_bound_tree::CallReportOutput::DeferredAbnormal(None),
            ] {
                producer
                    .reports
                    .entry(crate::execution_guarantees::FrameReportOwner::Boundary(
                        output,
                    ))
                    .or_default()
                    .insert(None, Arc::new(report(None)?));
            }
        }

        state.replace_deferred_value(&frame, Some(Arc::new(deferred)));

        Ok(())
    }

    pub(in crate::analysis) fn awaited_report_dependencies(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        output: bray_bound_tree::CallReportOutput,
        row: Option<usize>,
    ) -> Option<Arc<bray_bound_tree::ReportDependencies>> {
        let Some(bray_bound_tree::BoundExpression::Await(_)) =
            self.request.view().expression(expression)
        else {
            return None;
        };

        let value = state.admitted_receiver(expression.into());
        let deferred = value.deferred_value()?;

        let output = match output {
            bray_bound_tree::CallReportOutput::Abnormal(kind) => {
                bray_bound_tree::CallReportOutput::DeferredAbnormal(kind)
            }
            _ => bray_bound_tree::CallReportOutput::DeferredCompleted,
        };

        self.captured_frame_reports(state, deferred, output, row)
    }

    pub(in crate::analysis) fn direct_await_body<'state>(
        &self,
        state: &'state ExecutionState,
        invocation: BoundExecutionSite,
    ) -> Option<(
        &'state Arc<crate::ExecutionDeferredValue>,
        &'state StorageDeferredProducer,
        &'state ExecutionDeferredInvocation,
    )> {
        let expression = invocation.expression()?;

        if invocation != expression.into()
            || !matches!(
                self.request.view().expression(expression),
                Some(bray_bound_tree::BoundExpression::Await(_))
            )
        {
            return None;
        }

        let entry = state.admitted_call(invocation)?;

        let frame = entry
            .arguments
            .get(&ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(
                0,
            )))?
            .deferred_value()?;

        let proof = state.ordinary_proof();

        if frame.unknown_possible(&proof) {
            return None;
        }

        let mut feasible = frame
            .producers
            .iter()
            .filter(|(_, producer)| deferred::possible(&producer.selection, &proof));

        let (source, producer) = feasible.next()?;

        (feasible.next().is_none() && !self.is_task_observation(source.clone()))
            .then_some((frame, source, producer))
    }

    pub(in crate::analysis) fn observed_task_reports(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        outcome: bray_bound_tree::StorageTaskOutcome,
    ) -> Option<Arc<bray_bound_tree::ReportDependencies>> {
        use bray_bound_tree::{CallReportOutput, StorageTaskOutcome};

        let (frame, output, row) = match outcome {
            StorageTaskOutcome::Completed { .. } => {
                let (_, _, frame) =
                    self.deferred_body(state, state.admitted_receiver(expression.into()))?;

                (frame, CallReportOutput::DeferredCompleted, None)
            }
            StorageTaskOutcome::Abnormal { depth, kind, row } => (
                self.observed_task_at(state, expression, depth)?,
                CallReportOutput::DeferredAbnormal(Some(kind)),
                row,
            ),
            StorageTaskOutcome::Widened => return None,
        };

        self.captured_frame_reports(state, &frame, output, row)
    }

    pub(in crate::analysis) fn captured_frame_reports(
        &self,
        state: &ExecutionState,
        deferred: &crate::ExecutionDeferredValue,
        output: bray_bound_tree::CallReportOutput,
        selected: Option<usize>,
    ) -> Option<Arc<bray_bound_tree::ReportDependencies>> {
        let mut reports = None;
        let proof = &state.ordinary_proof();

        let matching = |key: &crate::execution_guarantees::FrameReportOwner| {
            matches!(key, crate::execution_guarantees::FrameReportOwner::Boundary(key)
                if *key == output || output.opaque() == Some(*key))
        };

        let formed = deferred
            .producers
            .values()
            .filter(|candidate| deferred::possible(&candidate.selection, proof))
            .flat_map(|candidate| {
                candidate
                    .reports
                    .iter()
                    .filter(move |(key, _)| matching(key))
                    .filter_map(move |(_, reports)| {
                        reports.admitted(|row| {
                            if output.is_abnormal() {
                                selected.is_none_or(|selected| selected == row)
                                    && candidate.entry.admits_failure(row)
                                    && candidate
                                        .entry
                                        .failure_guard(row)
                                        .is_none_or(|guard| proof.can_assume(guard, true))
                            } else {
                                candidate.entry.outputs.is_admitted(row)
                            }
                        })
                    })
            });

        let opaque = deferred
            .unknown
            .iter()
            .filter(|candidate| deferred::possible(&candidate.selection, proof))
            .flat_map(|candidate| {
                candidate
                    .reports
                    .iter()
                    .filter(move |(key, _)| matching(key))
                    .filter_map(|(_, reports)| reports.admitted(|_| true))
            });

        for captured in formed.chain(opaque) {
            if let Some(previous) = &mut reports {
                bray_bound_tree::ReportDependencies::merge(previous, &captured);
            } else {
                reports = Some(captured);
            }
        }

        reports
    }

    pub(in crate::analysis::guarantee::flow) fn activate_task(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
    ) {
        if self.started_future(state, expression).is_none() {
            return;
        }

        let task = self.value(state, expression);

        let carriers = state
            .owned_witnesses([task])
            .map(|(carrier, _)| carrier.clone())
            .collect();

        state.suspend_owned_witnesses(&carriers);
    }

    pub(in crate::analysis) fn frame_execution_value(
        &self,
        state: &ExecutionState,
        operation: crate::analysis::AnalysisOperationKind,
    ) -> Option<ExecutionCondition> {
        if let Some(expression) = operation.awaited_expression() {
            return Some(state.admitted_receiver(expression.into()));
        }

        self.started_future(state, operation.explicit_attempt_expression()?)
    }

    pub(in crate::analysis) fn direct_await_outputs(
        &self,
        state: &ExecutionState,
        operation: crate::analysis::AnalysisOperationKind,
    ) -> Option<(
        Arc<crate::ExecutionDeferredValue>,
        StorageDeferredProducer,
        Vec<usize>,
    )> {
        let (frame, source, producer) =
            self.direct_await_body(state, operation.awaited_expression()?.into())?;

        if producer.entry.outputs.0.len() <= 1 {
            return None;
        }

        let rows = self.feasible_entry_outputs(state, &producer.entry);

        Some((Arc::clone(frame), source.clone(), rows))
    }

    /// Bind the activation against the retained frame and publish its resolved output rows.
    pub(in crate::analysis) fn prepare_frame_execution(
        &self,
        state: &mut ExecutionState,
        point: &crate::analysis::model::AnalysisOperation,
        resolve: Option<(
            &super::super::captured::ResolveWrite<'_>,
            &super::super::captured::CaptureFrameInputs<'_>,
            &[bray_bound_tree::StorageValueCapture],
        )>,
        mut record: impl FnMut(
            &StorageDeferredProducer,
            Vec<bray_bound_tree::StorageValueCapture>,
            Vec<bray_bound_tree::StorageValueTransition>,
        ),
    ) {
        let operation = point.kind();

        if let Some(expression) = operation.awaited_expression() {
            let inputs = if let Some(body) = self.checked_body {
                body.storage_flow()
                    .input_captures(point.id(), None)
                    .unwrap_or_default()
            } else {
                resolve
                    .expect("physical await supplies its evaluated input")
                    .2
            };

            self.admit_inline_invocation(state, expression, inputs);
        }

        let Some(value) = self.frame_execution_value(state, operation) else {
            return;
        };

        let Some(frame) = value.deferred_value() else {
            return;
        };

        let mut reached = self.bind_frame_entry(
            state,
            frame,
            point,
            resolve.map(|(writes, inputs, _)| (writes, inputs)),
            &mut record,
        );

        if operation.awaited_expression().is_some()
            && reached
                .producers
                .values()
                .any(|candidate| candidate.entry.pending_execution)
        {
            for candidate in Arc::make_mut(&mut reached)
                .producers
                .values_mut()
                .filter(|candidate| candidate.entry.pending_execution)
            {
                Arc::make_mut(&mut candidate.entry).pending_execution = false;
            }
        }

        if !Arc::ptr_eq(frame, &reached) {
            state.replace_deferred_value(frame, Some(reached));
        }
    }

    fn bind_frame_entry(
        &self,
        state: &ExecutionState,
        frame: &Arc<crate::ExecutionDeferredValue>,
        point: &crate::analysis::model::AnalysisOperation,
        resolve: Option<(
            &super::super::captured::ResolveWrite<'_>,
            &super::super::captured::CaptureFrameInputs<'_>,
        )>,
        record: &mut impl FnMut(
            &StorageDeferredProducer,
            Vec<bray_bound_tree::StorageValueCapture>,
            Vec<bray_bound_tree::StorageValueTransition>,
        ),
    ) -> Arc<crate::ExecutionDeferredValue> {
        use std::borrow::Cow;

        let execution = point.kind().occurrence();
        let mut started = Arc::clone(frame);
        let mut reached = std::collections::HashMap::new();

        for (source, producer) in &frame.producers {
            let inputs: Cow<'_, [bray_bound_tree::StorageValueCapture]> =
                if !producer.entry.pending_execution {
                    Cow::Borrowed(&[])
                } else if let Some(body) = self.checked_body {
                    Cow::Borrowed(
                        body.storage_flow()
                            .input_captures(point.id(), Some(source.clone()))
                            .unwrap_or_default(),
                    )
                } else {
                    Cow::Owned(resolve
                        .expect("physical frame activation supplies the reached input capture")
                        .1(&producer.entry))
                };

            if !inputs.is_empty() {
                let producer = Arc::make_mut(&mut started)
                    .producers
                    .get_mut(source)
                    .expect("starting preserves the actual selected producer");

                let entry = Arc::make_mut(&mut producer.entry);

                for capture in inputs.iter() {
                    if let Some(kind) = capture.borrow_kind {
                        entry.retain_argument_borrow_kind(capture.input, kind);
                    }
                }
            }

            let paired = inputs
                .iter()
                .filter_map(|capture| {
                    let value = producer
                        .entry
                        .arguments
                        .get(&ExecutionPlace::argument(capture.input))?;

                    let target = value.inline_captured_storage()?.known_address()?;

                    (capture.value.known_address() == Some(target))
                        .then_some((std::ptr::from_ref(value.kind()), capture))
                })
                .collect::<std::collections::HashMap<_, _>>();

            for (slot, value) in &producer.entry.arguments {
                let identity = std::ptr::from_ref(value.kind());

                let current = if let Some(capture) = paired.get(&identity) {
                    self.bind_captured_input(state, value.clone(), capture)
                } else {
                    reached
                        .entry(identity)
                        .or_insert_with(|| {
                            self.refresh_captured_references(state, value.clone(), None)
                        })
                        .clone()
                };

                if current != *value {
                    let producer = Arc::make_mut(&mut started)
                        .producers
                        .get_mut(source)
                        .expect("starting preserves the actual selected producer");

                    Arc::make_mut(&mut producer.entry)
                        .arguments
                        .insert(slot.clone(), current);
                }
            }

            let checked = producer
                .callee
                .as_deref()
                .and_then(|callee| self.dependencies.frame_callees().get(callee));

            let local = source
                .local()
                .and_then(|source| self.dependencies.calls().get(&source));

            let entry = &started.producers[source].entry;

            let (bindings, storage, failures) = match (local, checked) {
                (Some(call), _) => {
                    let reports = call
                        .deferred_reports()
                        .and_then(|reports| reports.failures());

                    let failures = entry.select_failure_rows(
                        self.request.semantic_values(),
                        &self.proofs,
                        reports,
                    );

                    (call.captured_values(), call.storage(), failures)
                }
                (_, Some(body)) => (
                    body.summary.captured_values(),
                    &body.storage,
                    self.select_frame_failures(entry, body.summary.failure_outcomes()),
                ),
                _ => continue,
            };

            if entry.pending_execution {
                let extracted = self.extracted_input_values(storage, bindings, entry);

                if extracted != entry.extracted_values {
                    let reached = Arc::make_mut(&mut started)
                        .producers
                        .get_mut(source)
                        .expect("activation retains its selected body");

                    state.capture_frame_extractions(Arc::make_mut(&mut reached.entry), extracted);
                }
            }

            let entry = &started.producers[source].entry;

            let transitions = if let Some(body) = self.checked_body {
                Cow::Borrowed(
                    body.storage_flow()
                        .attempt_values(point.id(), Some(source.clone()))
                        .unwrap_or_default(),
                )
            } else {
                Cow::Owned(
                    self.entry_value_transitions(
                        Some(entry),
                        bindings,
                        Some(storage),
                        |root| root.input_ordinal(producer.has_receiver()),
                        resolve
                            .expect("physical frame activation supplies its reached write operands")
                            .0,
                    ),
                )
            };

            let count = entry.positional_arguments().count();

            let outputs = self.bind_entry_outputs(
                state,
                entry,
                bray_bound_tree::StorageMutation::Operation(execution),
                count,
                ExecutionCondition::unknown(),
                transitions.iter(),
            );

            if let Cow::Owned(transitions) = transitions {
                record(source, inputs.into_owned(), transitions);
            }

            if entry.outputs != outputs || entry.failure_rows != failures {
                let producer = Arc::make_mut(&mut started)
                    .producers
                    .get_mut(source)
                    .expect("selected body retains its entry");

                let entry = Arc::make_mut(&mut producer.entry);

                entry.outputs = outputs;
                entry.failure_rows = failures;
            }
        }

        started
    }

    pub(in crate::analysis) fn suspend_awaited_execution(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
    ) {
        let value = state.admitted_receiver(expression.into());

        if let Some(frame) = value.deferred_value()
            && frame
                .producers
                .values()
                .any(|candidate| !candidate.entry.pending_execution)
        {
            let mut entries = Arc::clone(frame);

            for candidate in Arc::make_mut(&mut entries)
                .producers
                .values_mut()
                .filter(|candidate| !candidate.entry.pending_execution)
            {
                Arc::make_mut(&mut candidate.entry).pending_execution = true;
            }

            state.replace_deferred_value(frame, Some(entries));
        }
    }

    pub(in crate::analysis::guarantee::flow) fn capture_deferred(
        &self,
        state: &mut ExecutionState,
        invocation: BoundExecutionSite,
        result: ExecutionCondition,
    ) -> ExecutionCondition {
        let Some(entry) = state
            .entries
            .get_mut(&invocation)
            .filter(|entry| entry.pending_execution)
        else {
            return result;
        };

        // The admitted inputs are already immutable evaluated values. Formation retains them directly.
        let captured = Arc::clone(entry);

        Arc::make_mut(entry).pending_execution = false;

        let callee = self
            .dependencies
            .calls()
            .get(&invocation)
            .and_then(|contract| contract.deferred_callee())
            .cloned();

        result.with_deferred_value(Arc::new(deferred::DeferredValue {
            producers: [(
                invocation.into(),
                deferred::DeferredInvocation {
                    callee,
                    transition: None,
                    entry: captured,
                    completed: None,
                    reports: Default::default(),
                    selection: deferred::selection(&state.assumptions),
                },
            )]
            .into(),
            unknown: None,
        }))
    }

    pub(in crate::analysis::guarantee::flow) fn remember_deferred_value(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
    ) {
        let Some(ty) = self.semantics.types().expression(expression) else {
            return;
        };

        if !matches!(
            crate::representation::type_representation(self.request, ty.ty()),
            Some(
                bray_compiler_known::RepresentationRole::Future
                    | bray_compiler_known::RepresentationRole::Task
            )
        ) {
            return;
        }

        let value = self.value(state, expression);

        if !value.is_unknown() && value.deferred_value().is_none() {
            self.publish_expression(
                state,
                expression,
                value.with_deferred_value(Arc::new(deferred::DeferredValue::unknown(
                    &state.assumptions,
                ))),
            );
        }
    }

    pub(in crate::analysis::guarantee::flow) fn complete_deferred(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        observations: &bray_bound_tree::StorageCompletion,
    ) -> bool {
        let Some(bray_bound_tree::BoundExpression::Await(_)) =
            self.request.view().expression(expression)
        else {
            return false;
        };

        // Completion owns its producer alternatives while mutating the flow environment.
        let Some(deferred) = state
            .admitted_receiver(expression.into())
            .deferred_value()
            .cloned()
        else {
            return false;
        };

        if deferred.producers.is_empty() {
            return false;
        }

        let root = ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression));

        let value = self
            .complete_producers(
                state,
                &root,
                self.invocation_result_place(expression.into()).as_ref(),
                expression,
                &deferred,
                observations,
            )
            .expect("reachable deferred completion has a feasible producer");

        self.publish_captured_output(
            state,
            expression.into(),
            &bray_bound_tree::StorageValueDestination::Result,
            value,
        );

        true
    }

    fn complete_producers(
        &self,
        state: &mut ExecutionState,
        root: &ExecutionCondition,
        output: Option<&ExecutionPlace>,
        expression: BoundExpressionId,
        deferred: &deferred::DeferredValue,
        observations: &bray_bound_tree::StorageCompletion,
    ) -> Option<ExecutionCondition> {
        let proof = state.ordinary_proof();

        let opaque = deferred
            .unknown
            .as_ref()
            .filter(|guard| deferred::possible(&guard.selection, &proof));

        let feasible =
            |(_, producer): &(&StorageDeferredProducer, &ExecutionDeferredInvocation)| {
                deferred::possible(&producer.selection, &proof)
            };

        let mut producers = deferred.producers.iter();

        // Reserve the last viable candidate so it can reuse the incoming state.
        // The remaining candidates retain their forward merge order.
        let Some(last) = producers.rfind(&feasible) else {
            state.assumptions.extend(opaque?.selection.iter().cloned());

            return Some(root.clone());
        };

        // An opaque alternative contributes no producer-specific completion evidence.
        let mut completed = opaque.map(|guard| {
            let mut branch = state.clone();

            branch.assumptions.extend(guard.selection.iter().cloned());

            (branch, root.clone())
        });

        let complete = |branch: &mut ExecutionState,
                        (source, candidate): (
            &StorageDeferredProducer,
            &ExecutionDeferredInvocation,
        )| {
            let entry = &candidate.entry;

            branch
                .assumptions
                .extend(candidate.selection.iter().cloned());

            if !self.frame_normal_completion(&branch.assumptions, source, candidate, root) {
                return None;
            }

            let result = if let Some(task) = self.task_observation_body(source, candidate) {
                self.complete_task_observation(
                    branch,
                    task,
                    root.clone(),
                    output,
                    expression,
                    observations,
                )
            } else {
                root.clone()
            };

            let value = if let Some(source) = source.local() {
                self.complete_trusted_call(
                    branch,
                    (source, expression.into()),
                    Some(entry),
                    output,
                    result,
                    observations,
                )
            } else {
                let checked = self.frame_storage_outcomes(source, candidate);

                let ordinal = |root: bray_symbols::DependencySubjectRoot| {
                    root.input_ordinal(candidate.has_receiver())
                };

                let selected = checked.map(|(storage, _)| (storage, ordinal));

                self.complete_entry_outputs(
                    branch,
                    (None, expression.into()),
                    &result,
                    entry,
                    |branch, value, row| {
                        let references = checked.map_or_else(Vec::new, |(storage, rows)| {
                            ExecutionOutput::reference_relations(row, storage, rows)
                        });

                        let preserved =
                            self.preserved_borrowed_inputs(&references, &entry.arguments, ordinal);

                        let empty_ownership = bray_symbols::DependencyOwnedResult::default();
                        let ownership = row.map_or(&empty_ownership, |row| &row.ownership);
                        let carriers = row.map_or(&[][..], |row| row.carriers.as_ref());

                        let value = self.complete_transferred_ownership(
                            branch,
                            expression.into(),
                            checked.map(|(storage, _)| storage),
                            entry,
                            value,
                            (ownership, preserved),
                        );

                        let mutation =
                            bray_bound_tree::StorageMutation::Operation(expression.into());

                        let value = self.complete_returned_references(
                            branch,
                            mutation,
                            output,
                            (&result, value),
                            observations,
                        );

                        let instantiate = |formal: &ExecutionCondition| {
                            entry
                                .bind_condition(formal, &value)
                                .pointer_observations(&self.pointer_predicates)
                        };

                        // This carried frame has only its portable evidence at observation.
                        self.complete_returned_carriers(
                            branch,
                            entry,
                            &value,
                            carriers,
                            (selected, output),
                            (&instantiate, &|_| None),
                        );

                        value
                    },
                )
                .unwrap_or(result)
            };

            Some(branch.publish_frame_owners(value, root))
        };

        let merge_producer = |previous: &mut ExecutionState,
                              previous_value: &ExecutionCondition,
                              branch: &ExecutionState,
                              value: &ExecutionCondition| {
            super::super::merge_returned(
                previous,
                branch,
                previous_value,
                value,
                root,
                expression,
                None,
            )
        };

        for producer in producers.filter(feasible) {
            // Each possible sibling starts with the same incoming observations.
            let mut branch = state.clone();

            let Some(value) = complete(&mut branch, producer) else {
                continue;
            };

            if let Some((previous, previous_value)) = &mut completed {
                *previous_value = merge_producer(previous, previous_value, &branch, &value);
            } else {
                completed = Some((branch, value));
            }
        }

        let result = complete(state, last);

        let Some((mut previous, mut value)) = completed else {
            return result;
        };

        if let Some(result) = result {
            value = merge_producer(&mut previous, &value, state, &result);
        }

        *state = previous;

        Some(value)
    }

    fn complete_task_observation(
        &self,
        state: &mut ExecutionState,
        deferred: &deferred::DeferredValue,
        root: ExecutionCondition,
        output: Option<&ExecutionPlace>,
        expression: BoundExpressionId,
        observations: &bray_bound_tree::StorageCompletion,
    ) -> ExecutionCondition {
        let projection = self.task_completed_payload();
        let variant = self.task_result_variant(None);

        // The nested completion owns its payload path while the outer observation keeps its root.
        let payload_output = output.and_then(|output| output.clone().project(&[projection]));

        let completed = ExecutionCondition::new(crate::ExecutionConditionKind::VariantTest(
            variant,
            Arc::new(root.clone()),
        ));

        match completed.prove_ordinary(&state.assumptions, Some(&state.proofs)) {
            Some(false) => {
                let proof = state.ordinary_proof();

                if let Some(variant) = bray_symbols::DependencyFailureKind::ALL
                    .into_iter()
                    .map(|kind| self.task_result_variant(Some(kind)))
                    .find(|variant| {
                        proof.prove_trusted(&root.clone().matches_variant(*variant)) == Some(true)
                    })
                {
                    return ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(
                        variant,
                        Arc::new(root),
                    ));
                }

                return root;
            }
            Some(true) => {
                let payload = root.clone().project(projection);

                let value = self.complete_producers(state, &payload, payload_output.as_ref(), expression, deferred, observations)
                    .unwrap_or_else(|| panic!("completed task observation has no feasible body producer: {expression:?}, producers={:?}", deferred.producers.keys().collect::<Vec<_>>()));

                let root = ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(
                    variant,
                    Arc::new(root),
                ));

                return ExecutionCondition::constructed(root, [(projection, value)].into());
            }
            None => {}
        }

        // Observation may also return a panic or cancellation. Only Completed owns the body's normal result.
        let mut normal = state.clone();

        completed.clone().assume(true, &mut normal.assumptions);

        let payload = root.clone().project(projection);

        completed.assume(false, &mut state.assumptions);

        let Some(value) = self.complete_producers(
            &mut normal,
            &payload,
            payload_output.as_ref(),
            expression,
            deferred,
            observations,
        ) else {
            return root;
        };

        let tagged = ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(
            variant,
            Arc::new(root.clone()),
        ));

        let value = ExecutionCondition::constructed(tagged, [(projection, value)].into());

        let joined = super::super::merge_returned(
            &mut normal,
            state,
            &value,
            &root,
            &root,
            expression,
            None,
        );

        *state = normal;

        joined
    }
}

impl ExecutionState {
    pub(in crate::analysis) fn select_frame_output(
        &mut self,
        frame: &Arc<crate::ExecutionDeferredValue>,
        source: &StorageDeferredProducer,
        row: usize,
    ) {
        let mut selected = frame.as_ref().clone();

        let producer = selected
            .producers
            .get_mut(source)
            .expect("selected output belongs to the reached frame");

        Arc::make_mut(&mut producer.entry).select_output(row);
        self.replace_deferred_value(frame, Some(Arc::new(selected)));
    }
}
