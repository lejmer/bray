use std::sync::Arc;

use bray_bound_tree::{BoundExecutionSite, BoundExpressionId};

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
                self.request, "ResultVariant1Error", "ResultVariant1ErrorError",
            )
        } else {
            crate::representation::union_payload(
                self.request, "ResultVariant0Ok", "ResultVariant0OkValue",
            )
        };

        let value = if panicked {
            ExecutionCondition::Unknown
        } else {
            operation.operands().last().map_or(ExecutionCondition::Unknown, |operand| self.value(state, *operand))
        };

        let projection = bray_bound_tree::StorageProjection::ActiveUnionPayloadField { variant, field };
        let result = ExecutionCondition::Constructed(
            Arc::new(ExecutionCondition::Tagged(variant, Arc::new(ExecutionCondition::Expression(expression)))),
            [(projection, value)].into(),
        );

        state.pending_results.insert(expression, result);
    }

    /// Follow the selected frame through Task observations, retaining each result's placement.
    fn deferred_execution<'s>(
        &'s self,
        deferred: &'s super::super::state::DeferredInvocations,
        assumptions: &'s std::collections::BTreeSet<(ExecutionCondition, bool)>,
        value: ExecutionCondition,
        result: ExecutionCondition,
    ) -> impl Iterator<Item = (BoundExecutionSite, &'s Arc<crate::ExecutionCallEvidence>, ExecutionCondition)> + 's {
        let mut next = Some((value, result));
        let mut visited = std::collections::BTreeSet::new();
        let mut payload = None;

        std::iter::from_fn(move || {
            let (value, result) = next.take()?;

            let deferred = deferred.get(value.owner_identity())?;

            if deferred.unknown_possible(assumptions) { return None; }

            let mut feasible = deferred.producers.iter().filter(|(_, candidate)| {
                super::super::deferred::possible(&candidate.selection, assumptions)
            });

            let (source, candidate) = feasible.next()?;

            assert!(feasible.next().is_none(), "operation selects one deferred producer before composing completion");

            if self.is_task_observation(*source) {
                if !visited.insert(value.owner_identity().clone()) { return None; }

                let task = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0))
                    .value_in(&candidate.entry.arguments)?;

                let projection = *payload.get_or_insert_with(|| {
                    let (variant, field) = crate::representation::union_payload(
                        self.request, "RunResultVariant0Completed", "RunResultVariant0CompletedValue",
                    );

                    bray_bound_tree::StorageProjection::ActiveUnionPayloadField { variant, field }
                });

                next = Some((task, result.clone().project(projection)));
            }

            Some((*source, &candidate.entry, result))
        })
    }

    pub(in crate::analysis) fn deferred_body(
        &self,
        state: &ExecutionState,
        value: ExecutionCondition,
    ) -> Option<(BoundExecutionSite, usize)> {
        self.deferred_execution(&state.deferred_invocations, &state.assumptions, value, ExecutionCondition::Unknown)
            .enumerate().last()
            .filter(|(_, (source, _, _))| !self.is_task_observation(*source))
            .map(|(depth, (source, _, _))| (source, depth))
    }

    pub(in crate::analysis) fn task_body(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<BoundExecutionSite> {
        let bray_bound_tree::SemanticSelection::Call(call) = self.semantics.selections().expression(expression)?
            else { return None; };

        crate::analysis::AnalysisTaskOperationKind::from_implementation_hook(call.implementation_hook()?)?;

        self.deferred_body(state, self.receiver_value(state, expression, call)).map(|(source, _)| source)
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
            if !visited.insert(value.owner_identity().clone()) { continue; }

            let Some(deferred) = state.deferred_entries(&value) else { return false; };

            if deferred.unknown_possible(&state.assumptions) { return false; }

            for (source, candidate) in &deferred.producers {
                if !super::super::deferred::possible(&candidate.selection, &state.assumptions) { continue; }

                if self.is_task_observation(*source) {
                    let Some(task) = ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(0))
                        .value_in(&candidate.entry.arguments) else { return false; };

                    pending.push(task);
                } else {
                    if !self.dependencies.calls()[source].deferred_preserves_storage_on_normal_return() {
                        return false;
                    }

                    body = true;
                }
            }
        }

        body
    }

    pub(in crate::analysis) fn awaited_value_producers(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<std::collections::BTreeSet<BoundExpressionId>> {
        let Some(bray_bound_tree::BoundExpression::Await(awaited)) = self.request.view().expression(expression)
            else { return None; };

        let value = self.value(state, awaited.operand());

        state.deferred_entries(&value)?;

        Some(self.deferred_body(state, value).map(|(source, _)| {
            source.expression().expect("formed producer retains its source expression")
        }).into_iter().collect())
    }

    fn task_outcomes<'s>(
        &'s self,
        state: &'s ExecutionState,
        expression: BoundExpressionId,
    ) -> impl Iterator<Item = ExecutionCondition> + 's {
        let value = match self.request.view().expression(expression) {
            Some(bray_bound_tree::BoundExpression::Await(awaited)) => self.value(state, awaited.operand()),
            _ => ExecutionCondition::Unknown,
        };

        let (variant, _) = crate::representation::union_payload(
            self.request, "RunResultVariant0Completed", "RunResultVariant0CompletedValue",
        );

        self.deferred_execution(&state.deferred_invocations, &state.assumptions, value, ExecutionCondition::Expression(expression))
            .take_while(|(source, _, _)| self.is_task_observation(*source))
            .map(move |(_, _, result)| ExecutionCondition::VariantTest(variant, Arc::new(result)))
    }

    pub(in crate::analysis) fn task_completion(
        &self,
        state: &ExecutionState,
        expression: BoundExpressionId,
    ) -> Option<bool> {
        let mut result = None;

        for condition in self.task_outcomes(state, expression) {
            match condition.prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT }) {
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
        if !matches!(operation, crate::analysis::AnalysisOperationKind::Call {
            phase: crate::analysis::model::AnalysisCallPhase::Completion, ..
        }) { return Vec::new(); }

        let Some(expression) = operation.occurrence().expression() else { return Vec::new(); };

        let Some(sources) = self.awaited_task_invocations(state, expression) else { return Vec::new(); };

        let Some(cleanup) = self.task_cleanup_types(&sources, false) else { return Vec::new(); };

        let body_effects = sources.iter().any(|source| {
            self.dependencies.calls()[source].deferred_effects().is_none_or(|effects| !effects.is_empty())
        });

        if sources.is_empty() || (cleanup.is_empty() && !body_effects) { return Vec::new(); }

        let Some(bray_bound_tree::BoundExpression::Await(awaited)) = self.request.view().expression(expression)
            else { unreachable!("Task completion retains its await operation"); };

        let value = self.value(state, awaited.operand());

        // Formation guards select existing frames while this outcome refines ordinary truth.
        let formation = state.assumptions.clone();

        let mut steps = self.deferred_execution(
            &state.deferred_invocations, &formation, value, ExecutionCondition::Expression(expression),
        ).peekable();

        let (variant, _) = crate::representation::union_payload(
            self.request, "RunResultVariant0Completed", "RunResultVariant0CompletedValue",
        );

        let mut alternatives = Vec::new();

        while let Some((source, _, result)) = steps.next() {
            if !self.is_task_observation(source) { break; }

            let condition = ExecutionCondition::VariantTest(variant, Arc::new(result));

            match condition.prove(&state.assumptions, &mut { ExecutionCondition::WORK_LIMIT }) {
                Some(false) => break,
                Some(true) => continue,
                None => {}
            }

            let possible = steps.peek().is_none_or(|(body, entry, result)| {
                self.dependencies.calls()[body].deferred_completion()
                    && self.owned_normal_completion(&state.assumptions, *body, entry, result)
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
        let root = ExecutionCondition::Expression(expression);
        let left = self.value(target, expression);
        let right = self.value(incoming, expression);
        let value = super::merge_returned(target, incoming, &left, &right, &root, expression);

        target.expressions.insert(expression, value);
    }
}
