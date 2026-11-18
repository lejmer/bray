use std::sync::Arc;

use bray_bound_tree::BoundExpressionId;

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

        self.assume_produced_variant(state, expression, &result);
        state.pending_results.insert(expression, result);

        if !panicked {
            // The pending Ok payload owns this value before predecessor joins.
            // Its terminal operand snapshot still feeds the physical commit.
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

            assert!(
                feasible.next().is_none(),
                "operation selects one deferred producer before composing completion"
            );

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

    /// Preservation queries can join producers before an operation selects one.
    pub(in crate::analysis) fn deferred_preserves_storage(
        &self,
        state: &ExecutionState,
        value: ExecutionCondition,
    ) -> bool {
        let mut pending = vec![value];
        let mut visited = std::collections::BTreeSet::new();
        let mut body = false;

        while let Some(value) = pending.pop() {
            let Some(deferred) = value.deferred_value() else {
                return false;
            };

            if !visited.insert(Arc::as_ptr(deferred)) { continue; }

            if deferred.unknown_possible(&state.ordinary_proof()) {
                return false;
            }

            for (source, candidate) in &deferred.producers {
                if !super::super::deferred::possible(&candidate.selection, &state.ordinary_proof()) {
                    continue;
                }

                if self.is_task_observation(source.clone()) {
                    let Some(task) = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0))
                        .value_in(&candidate.entry.arguments)
                    else {
                        return false;
                    };

                    pending.push(task);
                } else {
                    let preserves = source.local().map_or_else(|| {
                        candidate.callee.as_deref().and_then(|callee| self.dependencies.frame_callees().get(callee)).is_some_and(|body|
                            self.request.semantic_values().dependency_contract_template_data(body.summary).effect_outcomes().preserves_storage_on_normal_return())
                    }, |source| self.dependencies.calls()[&source].deferred_preserves_storage_on_normal_return());

                    if !preserves
                    {
                        return false;
                    }

                    body = true;
                }
            }
        }

        body
    }

    pub(in crate::analysis) fn awaited_value_dependencies(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<Arc<std::collections::BTreeSet<bray_symbols::DependencyRequirement>>> {
        let bray_bound_tree::BoundExpression::Await(_) = self.request.view().expression(expression)? else {
            return None;
        };

        let value = state.admitted_receiver(expression.into());

        let (producer, _, frame) = self.deferred_body(state, value)?;

        frame.producers[&producer].completed.clone()
    }

    fn task_outcomes<'s>(
        &'s self,
        state: &'s ExecutionState,
        expression: BoundExpressionId,
    ) -> impl Iterator<Item = ExecutionCondition> + 's {
        let value = match self.request.view().expression(expression) {
            Some(bray_bound_tree::BoundExpression::Await(_)) => {
                state.admitted_receiver(expression.into())
            }
            _ => ExecutionCondition::unknown(),
        };

        let (variant, _) = crate::representation::union_payload(
            self.request,
            "RunResultVariant0Completed",
            "RunResultVariant0CompletedValue",
        );

        self.deferred_execution(
            state.ordinary_proof(),
            value,
            ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression)),
        )
        .take_while(|(source, _, _)| self.is_task_observation(source.clone()))
        .map(move |(_, _, result)| ExecutionCondition::new(crate::ExecutionConditionKind::VariantTest(variant, Arc::new(result))))
    }

    pub(in crate::analysis) fn task_completion(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<bool> {
        let mut result = None;

        for condition in self.task_outcomes(state, expression) {
            match condition.prove_ordinary(&state.assumptions, Some(&state.proofs)) {
                Some(false) => return Some(false),
                None => return None,
                Some(true) => result = Some(true),
            }
        }

        result
    }

    /// Refine the feasible normal prefix and retain its abnormal alternatives.
    pub(in crate::analysis) fn split_task_completion(
        &self,
        state: &mut ExecutionState,
        operation: crate::analysis::AnalysisOperationKind,
    ) -> Vec<ExecutionState> {
        if !matches!(
            operation,
            crate::analysis::AnalysisOperationKind::Call {
                phase: crate::analysis::model::AnalysisCallPhase::Completion,
                ..
            }
        ) {
            return Vec::new();
        }

        let Some(expression) = operation.occurrence().expression() else {
            return Vec::new();
        };

        let Some(task) = self.awaited_task_frame(state, expression) else { return Vec::new(); };

        let proof = state.ordinary_proof();

        let changes_storage = task.unknown_possible(&proof) || task.producers.values()
            .filter(|producer| super::super::deferred::possible(&producer.selection, &proof))
            .any(|producer| producer.transition.as_ref().is_none_or(|transition|
                transition.transfer.captured_cleanup.as_ref().is_none_or(|cleanup| !cleanup.is_empty()) ||
                transition.transfer.deferred_effects.as_ref().is_none_or(|effects| !effects.possible().is_empty())));

        if !changes_storage { return Vec::new(); }

        let Some(bray_bound_tree::BoundExpression::Await(_)) =
            self.request.view().expression(expression)
        else {
            unreachable!("Task completion retains its await operation");
        };

        let value = state.admitted_receiver(expression.into());

        // Formation guards select existing frames while this outcome refines ordinary truth.
        let formation = state.assumptions.clone();

        let mut steps = self
            .deferred_execution(
                state.proofs.ordinary_proof(&formation),
                value,
                ExecutionCondition::new(crate::ExecutionConditionKind::Expression(expression)),
            )
            .peekable();

        let (variant, _) = crate::representation::union_payload(
            self.request,
            "RunResultVariant0Completed",
            "RunResultVariant0CompletedValue",
        );

        let mut alternatives = Vec::new();

        while let Some((source, _, result)) = steps.next() {
            if !self.is_task_observation(source) {
                break;
            }

            let condition = ExecutionCondition::new(crate::ExecutionConditionKind::VariantTest(variant, Arc::new(result)));

            match condition.prove_ordinary(&state.assumptions, Some(&state.proofs)) {
                Some(false) => break,
                Some(true) => continue,
                None => {}
            }

            let possible = steps.peek().is_none_or(|(body, frame, result)| {
                self.frame_normal_completion(&state.assumptions, body, &frame.producers[body], result)
            });

            if !possible {
                condition.assume(false, &mut state.assumptions);
                break;
            }

            // Each outcome owns mutable observations and shares immutable captured entries.
            let mut abnormal = state.clone();

            condition.clone().assume(false, &mut abnormal.assumptions);
            alternatives.push(abnormal);
            condition.assume(true, &mut state.assumptions);
        }

        alternatives
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
