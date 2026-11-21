use std::sync::Arc;

use bray_bound_tree::{BoundExpressionId, StorageDeferredProducer, StorageOutcomeRows, StorageTaskOutcome};
use bray_symbols::DependencyFailureKind;

use super::super::state::ExecutionState;
use super::ExecutionFlowDomain;
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionPlace};

#[derive(Clone)]
pub(in crate::analysis) struct TaskSelection {
    pub(in crate::analysis) outcome: StorageTaskOutcome,
    producer: Option<StorageDeferredProducer>,
}

impl TaskSelection {
    pub(in crate::analysis) fn guard(&self) -> StorageOutcomeRows {
        StorageOutcomeRows { task: Some(self.outcome), producer: self.producer.clone(), rows: None }
    }
}

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn complete_caught_result(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        outcome: crate::analysis::model::AnalysisCaughtResult,
    ) {
        use crate::analysis::model::AnalysisCaughtResult;

        let representation = self.request.available_compiler_known_symbols().result_representation()
            .expect("catch retains its checked Result representation");

        let (variant, field, value) = match outcome {
            AnalysisCaughtResult::Panicked =>
                (representation.error_variant(), representation.error_field(), ExecutionCondition::unknown()),
            AnalysisCaughtResult::Completed(operand) =>
                (representation.success_variant(), representation.success_field(),
                    operand.map_or_else(ExecutionCondition::unknown, |operand| self.value(state, operand))),
        };

        let projection =
            bray_bound_tree::StorageProjection::ActiveUnionPayloadField { variant, field };

        let result = ExecutionCondition::constructed(ExecutionCondition::new(crate::ExecutionConditionKind::Tagged(
                variant,
                Arc::new(ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression))),
            )),
            [(projection, value)].into(),
        );

        self.produce_region_result(state, expression, result);

        if matches!(outcome, AnalysisCaughtResult::Completed(_)) {
            // The result slot owns the Ok payload before joining predecessors.
            // The physical transfer still uses the evaluated operand.
            self.consume_value_slots(state, expression.into(), bray_bound_tree::StorageValueConsumptionPhase::Completion);
        }
    }

    /// Follow the selected frame through Task observations, retaining each result's placement.
    fn deferred_execution<'s>(
        &'s self,
        proof: crate::execution_guarantees::ExecutionProof<'s>,
        value: ExecutionCondition,
        result: ExecutionCondition,
    ) -> impl Iterator<
        Item = (
            bray_bound_tree::StorageDeferredProducer,
            Arc<crate::ExecutionDeferredValue>,
            ExecutionCondition,
        ),
    > + 's {
        let mut next = Some((value, result));
        let mut payload = None;

        std::iter::from_fn(move || {
            let (value, result) = next.take()?;

            let deferred = value.deferred_value()?;

            if deferred.unknown_possible(&proof) {
                return None;
            }

            let mut feasible = deferred.producers.iter().filter(|(_, candidate)| {
                super::super::deferred::possible(&candidate.selection, &proof)
            });

            let (source, candidate) = feasible.next()?;

            if feasible.next().is_some() {
                return None;
            }

            if self.is_task_observation(source.clone()) {
                let task = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0))
                    .value_in(&candidate.entry.arguments)?;

                let projection = *payload.get_or_insert_with(|| self.task_completed_payload());

                next = Some((task, result.clone().project(projection)));
            }

            Some((source.clone(), Arc::clone(deferred), result))
        })
    }

    pub(in crate::analysis) fn deferred_body(
        &self,
        state: &ExecutionState,
        value: ExecutionCondition,
    ) -> Option<(bray_bound_tree::StorageDeferredProducer, usize, Arc<crate::ExecutionDeferredValue>)> {
        self.deferred_execution(
            state.ordinary_proof(),
            value,
            ExecutionCondition::unknown(),
        )
        .enumerate()
        .last()
        .filter(|(_, (source, _, _))| !self.is_task_observation(source.clone()))
        .map(|(depth, (source, frame, _))| (source, depth, frame))
    }

    pub(in crate::analysis) fn awaited_value_dependencies(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        outcome: Option<StorageTaskOutcome>,
    ) -> Option<Arc<std::collections::BTreeSet<bray_symbols::DependencyRequirement>>> {
        let bray_bound_tree::BoundExpression::Await(_) = self.request.view().expression(expression)? else {
            return None;
        };

        match outcome {
            Some(StorageTaskOutcome::Abnormal { kind: DependencyFailureKind::Cancellation, .. }) => {
                return Some(Arc::new(std::collections::BTreeSet::new()));
            }
            Some(StorageTaskOutcome::Abnormal { kind: DependencyFailureKind::Panic, .. }) => return None,
            _ => {}
        }

        let value = state.admitted_receiver(expression.into());

        let (producer, _, frame) = self.deferred_body(state, value)?;

        frame.producers[&producer].completed.clone()
    }

    pub(super) fn task_result_variant(&self, failure: Option<DependencyFailureKind>) -> bray_symbols::UnionVariantSymbolId {
        let representation = self.request.available_compiler_known_symbols().run_result_representation()
            .expect("a task observation retains the checked RunResult representation");

        match failure {
            None => representation.completed_variant(),
            Some(DependencyFailureKind::Panic) => representation.panicked_variant(),
            Some(DependencyFailureKind::Cancellation) => representation.cancelled_variant(),
        }
    }

    pub(in crate::analysis) fn task_completed_payload(&self) -> bray_bound_tree::StorageProjection {
        let representation = self.request.available_compiler_known_symbols().run_result_representation()
            .expect("a task observation retains the checked RunResult representation");

        bray_bound_tree::StorageProjection::ActiveUnionPayloadField {
            variant: representation.completed_variant(),
            field: representation.completed_field(),
        }
    }

    fn task_observation_expression(
        &self,
        state: &ExecutionState,
        operation: crate::analysis::AnalysisOperationKind,
    ) -> Option<BoundExpressionId> {
        let crate::analysis::AnalysisOperationKind::Call {
            invocation, phase: crate::analysis::model::AnalysisCallPhase::Completion,
        } = operation else { return None; };

        let expression = invocation.expression()?;

        self.awaited_task_frame(state, expression)?;

        Some(expression)
    }

    fn task_completion_steps<'s>(
        &'s self,
        state: &'s ExecutionState,
        expression: BoundExpressionId,
    ) -> impl Iterator<Item = (bray_bound_tree::StorageDeferredProducer, Arc<crate::ExecutionDeferredValue>, ExecutionCondition)> + 's {
        self.deferred_execution(
            state.ordinary_proof(),
            state.admitted_receiver(expression.into()),
            ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression)),
        )
    }

    pub(in crate::analysis) fn observed_task_at(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
        depth: usize,
    ) -> Option<Arc<crate::ExecutionDeferredValue>> {
        let (source, frame, _) = self.task_completion_steps(state, expression).nth(depth)?;

        self.task_observation_body(&source, &frame.producers[&source]).cloned()
    }

    pub(in crate::analysis) fn task_observation_outcome(
        &self,
        state: &ExecutionState,
        operation: crate::analysis::AnalysisOperationKind,
    ) -> Option<TaskSelection> {
        let expression = self.task_observation_expression(state, operation)?;
        let proof = state.ordinary_proof();

        let mut steps = self.task_completion_steps(state, expression).enumerate().peekable();

        while let Some((depth, (source, frame, root))) = steps.next() {
            if !self.is_task_observation(source.clone()) {
                let row = match frame.producers[&source].entry.outputs.0.as_ref() {
                    [output] => single_task_output_row(output),
                    _ => None,
                };

                return Some(TaskSelection { outcome: StorageTaskOutcome::Completed { row }, producer: Some(source) });
            }

            if let Some(kind) = DependencyFailureKind::ALL.into_iter().find(|kind| {
                proof.prove_trusted(&root.clone().matches_variant(self.task_result_variant(Some(*kind)))) == Some(true)
            }) {
                let body = steps.peek().filter(|(_, (source, _, _))| !self.is_task_observation(source.clone()));
                let evidence = body.map(|(_, (source, frame, _))| frame.producers[source].entry.as_ref());

                let row = task_failure_rows(evidence, kind, &proof, 1).next().flatten().map(|row| row.index);

                return Some(TaskSelection {
                    outcome: StorageTaskOutcome::Abnormal { depth, kind, row },
                    producer: body.map(|(_, (source, _, _))| source.clone()),
                });
            }

            if proof.prove_trusted(&root.matches_variant(self.task_result_variant(None))) != Some(true) {
                break;
            }
        }

        Some(TaskSelection { outcome: StorageTaskOutcome::Widened, producer: None })
    }

    pub(in crate::analysis) fn widened_task_panics(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Vec<(usize, Arc<bray_bound_tree::ReportDependencies>)> {
        let proof = state.ordinary_proof();
        let mut reports = Vec::new();

        for (depth, (source, frame, result)) in self.task_completion_steps(state, expression).enumerate() {
            let Some(body) = self.task_observation_body(&source, &frame.producers[&source]) else {
                break;
            };

            let panic = result.clone().matches_variant(self.task_result_variant(Some(DependencyFailureKind::Panic)));

            if proof.can_assume(&panic, true)
                && let Some(report) = self.captured_frame_reports(state, body,
                    bray_bound_tree::CallReportOutput::DeferredAbnormal(Some(DependencyFailureKind::Panic)), None)
            {
                reports.push((depth, report));
            }

            let completed = result.clone().matches_variant(self.task_result_variant(None));

            if !proof.can_assume(&completed, true)
                || !self.deferred_normal_completion(&state.assumptions, body, &result.project(self.task_completed_payload()))
            {
                break;
            }
        }

        reports
    }

    /// Refine concrete task result tags within the same alternatives budget as call rows.
    pub(in crate::analysis) fn split_task_completion(
        &self,
        state: &ExecutionState,
        operation: crate::analysis::AnalysisOperationKind,
        maximum: usize,
    ) -> Option<Vec<(TaskSelection, ExecutionState)>> {
        let expression = self.task_observation_expression(state, operation)?;

        let mut completed = state.clone();
        let mut steps = self.task_completion_steps(state, expression).enumerate().peekable();
        let mut alternatives = Vec::new();

        while let Some((depth, (source, frame, root))) = steps.next() {
            if !self.is_task_observation(source.clone()) {
                let entry = &frame.producers[&source].entry;
                let rows = self.feasible_entry_outputs(&completed, entry);
                let remaining = maximum.checked_sub(alternatives.len())?;

                if remaining == 0 { return None; }

                let portable = rows.iter().map(|index| single_task_output_row(&entry.outputs.0[*index]))
                    .collect::<Option<std::collections::BTreeSet<_>>>();

                if !rows.is_empty() && rows.len() <= remaining && portable.is_some_and(|portable| portable.len() == rows.len()) {
                    for index in rows {
                        let row = single_task_output_row(&entry.outputs.0[index]);
                        let mut branch = completed.clone();

                        branch.select_frame_output(&frame, &source, index);

                        alternatives.push((TaskSelection {
                            outcome: StorageTaskOutcome::Completed { row },
                            producer: Some(source.clone()),
                        }, branch));
                    }

                    return Some(alternatives);
                }

                alternatives.push((TaskSelection {
                    outcome: StorageTaskOutcome::Completed { row: None },
                    producer: Some(source),
                }, completed));

                return Some(alternatives);
            }

            let normal = root.clone().matches_variant(self.task_result_variant(None));
            let proof = completed.ordinary_proof();

            let normal_assumptions = proof.can_assume(&normal, true).then(|| {
                let mut assumptions = completed.assumptions.clone();

                normal.clone().assume(true, &mut assumptions);

                assumptions
            }).filter(|assumptions| {
                self.task_observation_body(&source, &frame.producers[&source]).is_none_or(|body| {
                    let payload = root.clone().project(self.task_completed_payload());

                    self.deferred_normal_completion(assumptions, body, &payload)
                })
            });

            let body = steps.peek().filter(|(_, (source, _, _))| !self.is_task_observation(source.clone()));
            let evidence = body.map(|(_, (source, frame, _))| frame.producers[source].entry.as_ref());

            let failures = DependencyFailureKind::ALL.map(|kind| {
                let selected = root.clone().matches_variant(self.task_result_variant(Some(kind)));

                (proof.can_assume(&normal, false) && proof.can_assume(&selected, true)
                    && task_failure_rows(evidence, kind, &proof, 0).next().is_some())
                    .then_some((kind, selected))
            });

            // Keep each feasible result tag representable before retaining extra rows within a tag.
            let mut reserved = failures.iter().flatten().count() + usize::from(normal_assumptions.is_some());

            maximum.checked_sub(alternatives.len() + reserved)?;

            for (kind, selected) in failures.into_iter().flatten() {
                reserved -= 1;

                let limit = maximum - alternatives.len() - reserved;

                for row in task_failure_rows(evidence, kind, &proof, limit) {
                    let mut branch = completed.clone();

                    normal.clone().assume(false, &mut branch.assumptions);
                    selected.clone().assume(true, &mut branch.assumptions);

                    if let Some(guard) = row.and_then(|row| row.guard.as_ref()) {
                        guard.clone().assume(true, &mut branch.assumptions);
                    }

                    let row = row.map(|row| row.index);

                    alternatives.push((TaskSelection {
                        outcome: StorageTaskOutcome::Abnormal { depth, kind, row },
                        producer: body.map(|(_, (source, _, _))| source.clone()),
                    }, branch));
                }
            }

            let Some(assumptions) = normal_assumptions else { return Some(alternatives); };

            completed.assumptions = assumptions;
        }

        // An opaque or merged child can still produce an enclosing Completed tag.
        if alternatives.len() == maximum { return None; }

        alternatives.push((TaskSelection { outcome: StorageTaskOutcome::Widened, producer: None }, completed));

        Some(alternatives)
    }

    pub(in crate::analysis) fn merge_completion(
        &self,
        target: &mut ExecutionState,
        incoming: &ExecutionState,
        expression: BoundExpressionId,
    ) {
        let root = ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression));
        let (published, left, right) = self.invocation_result_place(expression.into()).and_then(|place| {
            let left = target.current.live_value(&place)?;
            let right = incoming.current.live_value(&place)?;

            Some((Some(place), left, right))
        }).unwrap_or_else(|| (None, self.value(target, expression), self.value(incoming, expression)));

        let value = super::merge_returned(target, incoming, &left, &right, &root, expression, published.as_ref());

        self.publish_expression(target, expression, value);
    }
}

fn single_task_output_row(output: &crate::execution_guarantees::ExecutionOutput) -> Option<usize> {
    match output.sources.as_deref()? {
        [row] => Some(*row),
        _ => None,
    }
}

fn task_failure_rows<'row>(
    body: Option<&'row crate::ExecutionCallEvidence>,
    kind: DependencyFailureKind,
    proof: &'row crate::execution_guarantees::ExecutionProof<'_>,
    limit: usize,
) -> impl Iterator<Item = Option<&'row crate::execution_guarantees::ExecutionFailureRow>> + 'row {
    let rows = body.and_then(|entry| entry.failure_rows.as_deref());

    let admitted = rows.into_iter().flatten().filter(move |row| {
        row.covers(kind) && row.guard.as_ref().is_none_or(|guard| proof.can_assume(guard, true))
    });

    let widen = rows.is_none() || admitted.clone().nth(limit).is_some();

    admitted.take(if widen { 0 } else { limit }).map(Some).chain(widen.then_some(None))
}
