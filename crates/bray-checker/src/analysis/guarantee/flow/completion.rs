use std::sync::Arc;

use bray_bound_tree::{BoundExpressionId, StorageTaskOutcome};
use bray_symbols::DependencyFailureKind;

use super::super::state::ExecutionState;
use super::ExecutionFlowDomain;
use crate::{CheckerRequestContext, ExecutionCondition, ExecutionPlace};

impl<C: CheckerRequestContext + ?Sized> ExecutionFlowDomain<'_, '_, C> {
    pub(super) fn complete_caught_result(
        &self,
        state: &mut ExecutionState,
        expression: BoundExpressionId,
        panicked: bool,
    ) {
        let Some(bray_bound_tree::BoundExpression::Structured(operation)) =
            self.request.view().expression(expression)
        else {
            unreachable!("report boundary retains its catch expression");
        };

        assert_eq!(
            operation.kind(),
            bray_bound_tree::BoundStructuredExpressionKind::Catch,
            "report boundary belongs to catch"
        );

        let (variant, field) = if panicked {
            crate::representation::union_payload(
                self.request,
                "ResultVariant1Error",
                "ResultVariant1ErrorError",
            )
        } else {
            crate::representation::union_payload(
                self.request,
                "ResultVariant0Ok",
                "ResultVariant0OkValue",
            )
        };

        let value = if panicked {
            ExecutionCondition::unknown()
        } else {
            operation
                .operands()
                .last()
                .map_or(ExecutionCondition::unknown(), |operand| {
                    self.value(state, *operand)
                })
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

        if !panicked {
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
            crate::execution_guarantees::DeferredProducer,
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

                let projection = *payload.get_or_insert_with(|| {
                    let (variant, field) = crate::representation::union_payload(
                        self.request,
                        "RunResultVariant0Completed",
                        "RunResultVariant0CompletedValue",
                    );

                    bray_bound_tree::StorageProjection::ActiveUnionPayloadField { variant, field }
                });

                next = Some((task, result.clone().project(projection)));
            }

            Some((source.clone(), Arc::clone(deferred), result))
        })
    }

    pub(in crate::analysis) fn deferred_body(
        &self,
        state: &ExecutionState,
        value: ExecutionCondition,
    ) -> Option<(crate::execution_guarantees::DeferredProducer, usize, Arc<crate::ExecutionDeferredValue>)> {
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
            Some(outcome @ StorageTaskOutcome::Abnormal { kind: DependencyFailureKind::Panic, .. }) => {
                return self.observed_task_reports(state, expression, outcome)
                    .map(|report| Arc::new(report.requirements.clone()));
            }
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
    ) -> impl Iterator<Item = (crate::execution_guarantees::DeferredProducer, Arc<crate::ExecutionDeferredValue>, ExecutionCondition)> + 's {
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

        if !self.is_task_observation(source.clone()) { return None; }

        ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0))
            .value_in(&frame.producers[&source].entry.arguments)?.deferred_value().cloned()
    }

    pub(in crate::analysis) fn task_observation_outcome(
        &self,
        state: &ExecutionState,
        operation: crate::analysis::AnalysisOperationKind,
    ) -> Option<StorageTaskOutcome> {
        let expression = self.task_observation_expression(state, operation)?;
        let proof = state.ordinary_proof();

        for (depth, (source, frame, root)) in self.task_completion_steps(state, expression).enumerate() {
            if !self.is_task_observation(source.clone()) {
                let row = match frame.producers[&source].entry.outputs.0.as_ref() {
                    [output] => single_task_output_row(output),
                    _ => None,
                };

                return Some(StorageTaskOutcome::Completed { row });
            }

            if let Some(kind) = DependencyFailureKind::ALL.into_iter().find(|kind| {
                proof.prove_trusted(&root.clone().matches_variant(self.task_result_variant(Some(*kind)))) == Some(true)
            }) {
                return Some(StorageTaskOutcome::Abnormal { depth, kind });
            }

            if proof.prove_trusted(&root.matches_variant(self.task_result_variant(None))) != Some(true) {
                break;
            }
        }

        Some(StorageTaskOutcome::Widened)
    }

    /// Refine concrete task result tags within the same alternatives budget as call rows.
    pub(in crate::analysis) fn split_task_completion(
        &self,
        state: &ExecutionState,
        operation: crate::analysis::AnalysisOperationKind,
        maximum: usize,
    ) -> Option<Vec<(StorageTaskOutcome, ExecutionState)>> {
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
                        alternatives.push((StorageTaskOutcome::Completed { row }, branch));
                    }

                    return Some(alternatives);
                }

                alternatives.push((StorageTaskOutcome::Completed { row: None }, completed));

                return Some(alternatives);
            }

            let normal = root.clone().matches_variant(self.task_result_variant(None));
            let proof = completed.ordinary_proof();

            for kind in DependencyFailureKind::ALL {
                let selected = root.clone().matches_variant(self.task_result_variant(Some(kind)));

                if !proof.can_assume(&normal, false) || !proof.can_assume(&selected, true) {
                    continue;
                }

                if steps.peek().is_some_and(|(_, (body, frame, _))| {
                    !self.task_body_can_fail(body, &frame.producers[body], kind)
                }) {
                    continue;
                }

                if alternatives.len() == maximum { return None; }

                let mut branch = completed.clone();

                normal.clone().assume(false, &mut branch.assumptions);
                selected.assume(true, &mut branch.assumptions);
                alternatives.push((StorageTaskOutcome::Abnormal { depth, kind }, branch));
            }

            let can_complete = proof.can_assume(&normal, true) && steps.peek().is_none_or(|(_, (body, frame, result))| {
                self.frame_normal_completion(&completed.assumptions, body, &frame.producers[body], result)
            });

            if !can_complete {
                return Some(alternatives);
            }

            normal.assume(true, &mut completed.assumptions);
        }

        // An opaque or merged child can still produce an enclosing Completed tag.
        if alternatives.len() == maximum { return None; }

        alternatives.push((StorageTaskOutcome::Widened, completed));

        Some(alternatives)
    }

    fn task_body_can_fail(
        &self,
        source: &crate::execution_guarantees::DeferredProducer,
        candidate: &crate::ExecutionDeferredInvocation,
        kind: DependencyFailureKind,
    ) -> bool {
        if self.is_task_observation(source.clone()) {
            return true;
        }

        if let Some(site) = source.local() {
            return self.dependencies.calls().get(&site).and_then(|call| call.deferred_reports()?.failures())
                .is_none_or(|rows| rows.iter().enumerate().any(|(index, row)| row.covers(kind) && candidate.entry.admits_failure(index)));
        }

        let Some(body) = candidate.callee.as_deref().and_then(|callee| self.dependencies.frame_callees().get(callee)) else {
            return true;
        };

        let summary = self.request.semantic_values().dependency_contract_template_data(body.summary);
        let rows = summary.failure_outcomes();
        let admitted = self.select_frame_failures(&candidate.entry, rows);

        rows.is_none_or(|rows| rows.iter().enumerate().any(|(index, row)| {
            row.covers(kind) && admitted.as_deref().is_none_or(|rows| rows.binary_search(&index).is_ok())
        }))
    }

    pub(in crate::analysis) fn merge_completion(
        &self,
        target: &mut ExecutionState,
        incoming: &ExecutionState,
        expression: BoundExpressionId,
    ) {
        let root = ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression));
        let left = self.value(target, expression);
        let right = self.value(incoming, expression);
        let value = super::merge_returned(target, incoming, &left, &right, &root, expression);

        self.publish_expression(target, expression, value);
    }
}

fn single_task_output_row(output: &crate::execution_guarantees::ExecutionOutput) -> Option<usize> {
    match output.sources.as_deref()? {
        [row] => Some(*row),
        _ => None,
    }
}
