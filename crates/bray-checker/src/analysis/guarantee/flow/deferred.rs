use std::sync::Arc;

use bray_bound_tree::{BoundExecutionSite, BoundExpressionId};

use super::super::state::ExecutionState;
use super::ExecutionFlowDomain;
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionPlace};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(in crate::analysis::guarantee) fn awaited_entry(
        &self,
        state: &ExecutionState,
        invocation: BoundExecutionSite,
    ) -> Option<crate::ExecutionCallEvidence> {
        let expression = invocation.expression()?;

        if invocation != expression.into() { return None; }

        let Some(bray_bound_tree::BoundExpression::Await(awaited)) = self.request.view().expression(expression) else {
            return None;
        };

        let value = self.value(state, awaited.operand());

        Some(crate::ExecutionCallEvidence {
            outputs: Default::default(),
            owned_inputs: [bray_symbols::SymbolOrdinal::new(0)].into(),
            trusted_assumptions: state.call_trusted_assumptions([value.clone()]),
            assumptions: state.assumptions.clone(),
            arguments: [(ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)), value)].into(),
            trusted_boundary: !state.trust_boundaries.is_empty(),
            ..Default::default()
        })
    }

    pub(in crate::analysis) fn deferred_alternatives(
        &self,
        state: &ExecutionState,
        operation: crate::analysis::AnalysisOperationKind,
    ) -> Option<(
        Arc<super::super::deferred::DeferredValue>,
        Vec<super::super::deferred::DeferredValue>,
    )> {
        use crate::analysis::{AnalysisCallPhase, AnalysisOperationKind, AnalysisSuspensionKind};

        if !matches!(
            operation,
            AnalysisOperationKind::Call {
                phase: AnalysisCallPhase::Attempt | AnalysisCallPhase::Completion,
                ..
            } | AnalysisOperationKind::Suspension {
                kind: AnalysisSuspensionKind::Await,
                ..
            } | AnalysisOperationKind::Bound(bray_bound_tree::AnyBoundNodeId::Expression(_))
        ) {
            return None;
        }

        let expression = operation.occurrence().expression()?;

        let completion = matches!(operation, AnalysisOperationKind::Call { phase: AnalysisCallPhase::Completion, .. });

        let value = if let Some(bray_bound_tree::BoundExpression::Await(awaited)) =
            self.request.view().expression(expression)
        {
            if completion { state.admitted_receiver(expression.into()) } else { self.value(state, awaited.operand()) }
        } else {
            if completion { return None; }

            let bray_bound_tree::SemanticSelection::Call(call) =
                self.semantics.selections().expression(expression)?
            else {
                return None;
            };

            crate::analysis::AnalysisTaskOperationKind::from_implementation_hook(
                call.implementation_hook()?,
            )?;

            if state.entries.contains_key(&operation.occurrence()) {
                state.admitted_receiver(operation.occurrence())
            } else {
                self.receiver_value(state, expression, call)
            }
        };

        let mut value = value;

        loop {
            let deferred = value.deferred_value()?;

            let mut feasible = deferred.producers.iter().filter(|(_, candidate)| {
                super::super::deferred::possible(&candidate.selection, &state.ordinary_proof())
            });

            let first = feasible.next();
            let second = feasible.next();

            let unknown = deferred.unknown.as_ref().filter(|guard| {
                super::super::deferred::possible(&guard.selection, &state.ordinary_proof())
            });

            if second.is_some() || (first.is_some() && unknown.is_some()) {
                let mut alternatives = first
                    .into_iter()
                    .chain(second)
                    .chain(feasible)
                    .map(
                        |(source, candidate)| super::super::deferred::DeferredValue {
                            producers: [(source.clone(), candidate.clone())].into(),
                            unknown: None,
                        },
                    )
                    .collect::<Vec<_>>();

                if let Some(guard) = unknown {
                    alternatives.push(super::super::deferred::DeferredValue {
                        producers: Default::default(),
                        unknown: Some(guard.clone()),
                    });
                }

                return Some((Arc::clone(deferred), alternatives));
            }

            let (source, candidate) = first?;

            if !self.is_task_observation(source.clone()) {
                return None;
            }

            value = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0))
                .value_in(&candidate.entry.arguments)?;
        }
    }

    pub(in crate::analysis) fn select_deferred(
        &self,
        state: &mut ExecutionState,
        frame: Arc<super::super::deferred::DeferredValue>,
        alternative: super::super::deferred::DeferredValue,
    ) {
        let guard = alternative
            .unknown
            .as_ref()
            .map(|candidate| &candidate.selection)
            .or_else(|| {
                alternative
                    .producers
                    .values()
                    .next()
                    .map(|candidate| &candidate.selection)
            })
            .expect("selected producer retains its ordinary guard");

        state.assumptions.extend(guard.iter().cloned());
        state.replace_deferred_value(&frame, Some(Arc::new(alternative)));
    }

    pub(in crate::analysis) fn is_task_observation(&self, producer: impl Into<crate::execution_guarantees::DeferredProducer>) -> bool {
        use crate::execution_guarantees::DeferredProducer;

        let hook = match producer.into() {
            DeferredProducer::Local(source) => source.expression().filter(|expression| source == (*expression).into()).and_then(|expression|
                match self.semantics.selections().expression(expression)? {
                    bray_bound_tree::SemanticSelection::Call(call) => call.implementation_hook(),
                    _ => None,
                }),
            DeferredProducer::Summary { callee, .. } => callee.as_deref().and_then(|callee| self.dependencies.frame_callees().get(callee)?.hook),
        };

        matches!(hook, Some(bray_compiler_known::ImplementationHook::TaskJoin | bray_compiler_known::ImplementationHook::TaskCancel))
    }

    pub(in crate::analysis) fn awaited_task_frame(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<Arc<crate::ExecutionDeferredValue>> {
        let Some(bray_bound_tree::BoundExpression::Await(_)) = self.request.view().expression(expression) else {
            return None;
        };

        self.observed_task_frame(state, &state.admitted_receiver(expression.into()))
    }

    pub(in crate::analysis) fn observed_task_frame(&self, state: &ExecutionState, value: &ExecutionCondition)
        -> Option<Arc<crate::ExecutionDeferredValue>>
    {
        let frame = value.deferred_value()?;
        let proof = state.ordinary_proof();

        if frame.unknown_possible(&proof) { return None; }

        let mut feasible = frame.producers.iter().filter(|(_, producer)| super::super::deferred::possible(&producer.selection, &proof));

        let (source, producer) = feasible.next()?;

        if feasible.next().is_some() || !self.is_task_observation(source.clone()) { return None; }

        ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)).value_in(&producer.entry.arguments)?
            .deferred_value().cloned()
    }

    pub(in crate::analysis) fn capture_deferred_outputs<E>(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        completed: impl FnOnce() -> Result<std::collections::BTreeSet<bray_symbols::DependencyRequirement>, E>,
        mut report: impl FnMut(Option<(BoundExecutionSite, &bray_bound_tree::BoundCallReport)>)
            -> Result<crate::dependency::ReportDependencies, E>,
    ) -> Result<(), E> {
        let value = self.value(state, expression);

        let Some(frame) = value.deferred_value().cloned() else { return Ok(()); };

        let source = BoundExecutionSite::from(expression);
        let key = source.into();
        let formed = frame.producers.get(&key).filter(|producer| producer.completed.is_none());
        let opaque = frame.unknown.as_ref().is_some_and(|producer| producer.reports.is_empty());

        if formed.is_none() && !opaque { return Ok(()); }

        let mut deferred = frame.as_ref().clone();

        if let Some(producer) = deferred.producers.get_mut(&key)
            && formed.is_some() {
            producer.completed = Some(Arc::new(completed()?));

            use bray_bound_tree::CallReportOutput;

            let reports = self.dependencies.calls()[&source].deferred_reports()
                .expect("formed frame retains its checked execution phase");

            let candidates = reports.completed().map(|(row, report)| (CallReportOutput::DeferredCompleted, Some(row), report))
                .chain(reports.abnormal().map(|report| (CallReportOutput::DeferredAbnormal, None, report)));

            for (output, row, contract) in candidates {
                producer.reports.entry(output).or_default().insert(row, Arc::new(report(Some((source, contract)))?));
            }
        }

        if opaque {
            let producer = deferred.unknown.as_mut().expect("opaque frame retains its alternative");

            for output in [bray_bound_tree::CallReportOutput::DeferredCompleted,
                bray_bound_tree::CallReportOutput::DeferredAbnormal] {
                producer.reports.entry(output).or_default().insert(None, Arc::new(report(None)?));
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
    ) -> Option<Arc<crate::dependency::ReportDependencies>> {
        let Some(bray_bound_tree::BoundExpression::Await(_)) =
            self.request.view().expression(expression)
        else {
            return None;
        };

        let value = state.admitted_receiver(expression.into());
        let deferred = value.deferred_value()?;
        let mut reports = None;

        let output = if output.is_abnormal() {
            bray_bound_tree::CallReportOutput::DeferredAbnormal
        } else {
            bray_bound_tree::CallReportOutput::DeferredCompleted
        };

        let proof = state.ordinary_proof();

        let formed = deferred.producers.values()
            .filter(|candidate| super::super::deferred::possible(&candidate.selection, &proof))
            .filter_map(|candidate| candidate.reports.get(&output)?.admitted(Some(&candidate.entry.outputs)));

        let opaque = deferred.unknown.iter()
            .filter(|candidate| super::super::deferred::possible(&candidate.selection, &proof))
            .filter_map(|candidate| candidate.reports.get(&output)?.admitted(None));

        for captured in formed.chain(opaque) {
            if let Some(previous) = &mut reports {
                crate::dependency::ReportDependencies::merge(previous, &captured);
            } else {
                reports = Some(captured);
            }
        }

        reports
    }

    pub(super) fn activate_task(&self, state: &mut ExecutionState, expression: BoundExpressionId) {
        if self.started_future(state, expression).is_none() {
            return;
        }

        let task = self.value(state, expression);
        let carriers = state.owned_witnesses([task]).map(|(carrier, _)| carrier.clone()).collect();

        state.suspend_owned_witnesses(&carriers);
    }

    pub(super) fn start_frame_execution(
        &self, state: &ExecutionState, frame: &Arc<crate::ExecutionDeferredValue>, execution: BoundExecutionSite,
    ) -> Arc<crate::ExecutionDeferredValue> {
        let mut started = Arc::clone(frame);
        let mut reached = std::collections::HashMap::new();

        for (source, producer) in &frame.producers {
            for (slot, value) in &producer.entry.arguments {
                let current = reached.entry(std::ptr::from_ref(value.kind())).or_insert_with(||
                    self.refresh_captured_references(state, value.clone(), None));

                if current != value {
                    let producer = Arc::make_mut(&mut started).producers.get_mut(source)
                        .expect("starting preserves the actual selected producer");

                    Arc::make_mut(&mut producer.entry).arguments.insert(slot.clone(), current.clone());
                }
            }

            let checked = producer.callee.as_deref().and_then(|callee| self.dependencies.frame_callees().get(callee));
            let summary = checked.map(|checked| self.request.semantic_values().dependency_contract_template_data(checked.summary));
            let local = source.local().and_then(|source| self.dependencies.calls().get(&source));

            let bindings = match (local, &summary) {
                (Some(call), _) => call.captured_values(),
                (_, Some(summary)) => summary.captured_values(),
                _ => continue,
            };

            let entry = &started.producers[source].entry;

            let transitions = self.entry_value_transitions(Some(entry), bindings,
                |root| root.input_ordinal(producer.has_receiver()));

            let count = entry.positional_arguments().count();
            let outputs = self.bind_entry_outputs(state, entry, execution, count, ExecutionCondition::unknown(), &transitions, true);

            if entry.outputs != outputs {
                let producer = Arc::make_mut(&mut started).producers.get_mut(source).expect("selected body retains its entry");

                Arc::make_mut(&mut producer.entry).outputs = outputs;
            }
        }

        started
    }

    pub(super) fn set_awaited_execution(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        pending: bool,
    ) {
        let value = state.admitted_receiver(expression.into());

        if let Some(frame) = value.deferred_value().cloned() {
            let mut entries = if pending { Arc::clone(&frame) } else { self.start_frame_execution(state, &frame, expression.into()) };

            for candidate in Arc::make_mut(&mut entries).producers.values_mut() {
                Arc::make_mut(&mut candidate.entry).pending_execution = pending;
            }

            state.replace_deferred_value(&frame, Some(entries));
        }
    }

    pub(super) fn capture_deferred(
        &self,
        state: &mut ExecutionState,
        invocation: BoundExecutionSite,
        result: ExecutionCondition,
        observations: &bray_bound_tree::StorageCompletion,
    ) -> ExecutionCondition {
        let Some(entry) = state.entries.get_mut(&invocation).filter(|entry| entry.pending_execution) else {
            return result;
        };

        // The admitted inputs are already immutable evaluated values. Formation retains them directly.
        let captured = Arc::new(entry.clone());

        entry.pending_execution = false;

        let callee = self.dependencies.calls().get(&invocation).and_then(|contract| contract.deferred_callee()).cloned();

        result.with_deferred_value(Arc::new(super::super::deferred::DeferredValue {
            producers: [(invocation.into(), super::super::deferred::DeferredInvocation { callee,
                transition: observations.deferred_transition().cloned(),
                entry: captured, completed: None, reports: Default::default(),
                selection: super::super::deferred::selection(&state.assumptions),
            })].into(),
            unknown: None,
        }))
    }

    pub(in crate::analysis) fn awaited_invocations<'s>(
        &self,
        state: &'s ExecutionState,
        expression: BoundExpressionId,
    ) -> impl Iterator<Item = (BoundExecutionSite, Arc<crate::ExecutionCallEvidence>)> + 's {
        let entries = match self.request.view().expression(expression) {
            Some(bray_bound_tree::BoundExpression::Await(_)) => {
                state.admitted_receiver(expression.into()).deferred_value().cloned()
            }
            _ => None,
        };

        entries
            .into_iter()
            .flat_map(move |entries| {
                entries.producers.iter().filter(|(_, candidate)| {
                    super::super::deferred::possible(&candidate.selection, &state.ordinary_proof())
                }).filter_map(|(source, candidate)| Some((source.local()?, Arc::clone(&candidate.entry)))).collect::<Vec<_>>()
            })
    }

    pub(super) fn remember_deferred_value(
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
            self.publish_expression(state, expression, value.with_deferred_value(Arc::new(
                super::super::deferred::DeferredValue::unknown(&state.assumptions))));
        }
    }

    pub(super) fn complete_deferred(
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
        let Some(deferred) = state.admitted_receiver(expression.into()).deferred_value().cloned() else {
            return false;
        };

        if deferred.producers.is_empty() {
            return false;
        }

        let root = ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression));

        let (mut completed, value) = self.complete_producers(state, &root, self.invocation_result_place(expression.into()).as_ref(), expression, &deferred, observations)
            .expect("reachable deferred completion has a feasible producer");

        self.publish_captured_output(&mut completed, expression.into(), &bray_bound_tree::StorageValueDestination::Result, value);
        *state = completed;

        true
    }

    fn complete_producers(
        &self,
        state: &ExecutionState,
        root: &ExecutionCondition,
        output: Option<&ExecutionPlace>,
        expression: BoundExpressionId,
        deferred: &super::super::deferred::DeferredValue,
        observations: &bray_bound_tree::StorageCompletion,
    ) -> Option<(ExecutionState, ExecutionCondition)> {
        // An opaque alternative contributes no producer-specific completion evidence.
        let mut completed = deferred
            .unknown
            .as_ref()
            .filter(|guard| super::super::deferred::possible(&guard.selection, &state.ordinary_proof()))
            .map(|guard| {
                let mut branch = state.clone();

                branch.assumptions.extend(guard.selection.iter().cloned());

                (branch, root.clone())
            });

        for (source, candidate) in &deferred.producers {
            if !super::super::deferred::possible(&candidate.selection, &state.ordinary_proof()) {
                continue;
            }

            let entry = &candidate.entry;
            let mut branch = state.clone();

            branch.assumptions.extend(candidate.selection.iter().cloned());
            branch.assumptions.extend(entry.assumptions.iter().cloned());
            if !self.frame_normal_completion(&branch.assumptions, source, candidate, root) { continue; }

            let observed_task = self.is_task_observation(source.clone()).then(|| {
                ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0)).value_in(&entry.arguments)
                    .and_then(|task| task.deferred_value().cloned())
            }).flatten();

            let result = if let Some(task) = observed_task {
                self.complete_task_observation(&mut branch, &task, root.clone(), output, expression, observations)
            } else {
                root.clone()
            };

            let value = if let Some(source) = source.local() {
                let mut active = entry.as_ref().clone();

                active.pending_execution = false;
                branch.entries.insert(source, active);

                self.complete_trusted_call(&mut branch, source, expression.into(), output, result, observations)
            } else {
                branch.resume_owned_witnesses(entry);
                let selected = candidate.callee.as_deref().and_then(|callee|
                    self.dependencies.frame_callees().get(callee).map(|body| (&body.storage, callee.receiver.is_some())));

                self.complete_entry_outputs(&mut branch, None, expression.into(), result, entry, |branch, value, ownership, carriers| {
                    let value = self.complete_transferred_ownership(branch, expression.into(), None, entry, value, ownership);

                    self.complete_frame_carriers(branch, entry, &value, carriers, selected, output);

                    value
                })
            };

            if let Some((previous, previous_value)) = &mut completed {
                *previous_value = super::merge_returned(
                    previous,
                    &branch,
                    previous_value,
                    &value,
                    root,
                    expression,
                );
            } else {
                completed = Some((branch, value));
            }
        }

        completed
    }

    fn complete_task_observation(
        &self,
        state: &mut ExecutionState,
        deferred: &super::super::deferred::DeferredValue,
        root: ExecutionCondition,
        output: Option<&ExecutionPlace>,
        expression: BoundExpressionId,
        observations: &bray_bound_tree::StorageCompletion,
    ) -> ExecutionCondition {
        let (variant, field) = crate::representation::union_payload(
            self.request,
            "RunResultVariant0Completed",
            "RunResultVariant0CompletedValue",
        );

        let projection =
            bray_bound_tree::StorageProjection::ActiveUnionPayloadField { variant, field };

        // The nested completion owns its payload path while the outer observation keeps its root.
        let payload_output = output.and_then(|output| output.clone().project(&[projection]));
        let completed = ExecutionCondition::new(crate::ExecutionConditionKind::VariantTest(variant, Arc::new(root.clone())));

        match completed.prove_ordinary(&state.assumptions, Some(&state.proofs)) {
            Some(false) => return root,
            Some(true) => {
                let payload = root.clone().project(projection);

                let (normal, value) = self.complete_producers(state, &payload, payload_output.as_ref(), expression, deferred, observations)
                    .unwrap_or_else(|| panic!("completed task observation has no feasible body producer: {expression:?}, producers={:?}", deferred.producers.keys().collect::<Vec<_>>()));

                *state = normal;

                return ExecutionCondition::constructed(root, [(projection, value)].into());
            }
            None => {}
        }

        // Observation may also return a panic or cancellation. Only Completed owns the body's normal result.
        let mut normal = state.clone();

        completed.clone().assume(true, &mut normal.assumptions);

        let payload = root.clone().project(projection);
        let mut abnormal = state.clone();

        completed.assume(false, &mut abnormal.assumptions);

        let Some((mut normal, value)) =
            self.complete_producers(&normal, &payload, payload_output.as_ref(), expression, deferred, observations)
        else {
            *state = abnormal;

            return root;
        };

        let value =
            ExecutionCondition::constructed(root.clone(), [(projection, value)].into());

        let joined =
            super::merge_returned(&mut normal, &abnormal, &value, &root, &root, expression);

        *state = normal;

        joined
    }
}
